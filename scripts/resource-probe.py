#!/usr/bin/env python3
"""Measure an isolated resource-preview binary, never an installed/user process."""
import argparse
import datetime
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import time


def cpu_seconds(value):
    parts = value.split(':')
    result = 0.0
    for part in parts:
        result = result * 60 + float(part)
    return result


# Darwin SDK sys/resource.h: rusage_info_v0, with a 16-byte UUID and ten u64s.
class Usage(ctypes.Structure):
    _fields_ = [('uuid', ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            'user_time', 'system_time', 'idle_wakeups', 'interrupt_wakeups',
            'pageins', 'wired_size', 'resident_size', 'phys_footprint',
            'start_abstime', 'exit_abstime')]


def footprint(pid):
    usage = Usage()
    library = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    function = library.proc_pid_rusage
    function.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    function.restype = ctypes.c_int
    if function(pid, 0, ctypes.byref(usage)) != 0:
        raise OSError(ctypes.get_errno(), 'Cannot read owned probe footprint')
    return usage.phys_footprint


def validate_latency_records(records, expected_count=120):
    """Reject incomplete ACK evidence before computing paint-only statistics."""
    sent = [row['sent_sequence'] for row in records if 'sent_sequence' in row]
    acknowledged = [row['input_sequence'] for row in records if 'input_sequence' in row]
    if len(sent) != expected_count or set(sent) != set(range(1, expected_count + 1)):
        raise ValueError(f'Invalid latency trial: expected {expected_count} unique sent sequences')
    if len(acknowledged) != len(set(acknowledged)):
        raise ValueError('Invalid latency trial: duplicate ACK')
    missing = set(sent) - set(acknowledged)
    unmatched = set(acknowledged) - set(sent)
    if missing or unmatched:
        raise ValueError(f'Invalid latency trial: missing ACKs {sorted(missing)}, unmatched ACKs {sorted(unmatched)}')


def observed_visible_during(records, start, end):
    """Summarize delivered observations only; GPUI may coalesce hide/reveal."""
    events = sorted((row['observed_visibility_seconds'], row['visible'])
                    for row in records if 'observed_visibility_seconds' in row)
    initial = [visible for at, visible in events if at <= start]
    return bool(initial and initial[-1] and all(
        visible for at, visible in events if start < at <= end))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--mode', choices=['quiet', 'hidden', 'output', 'completed', 'cycles', 'one-row', 'latency', 'flood'], default='hidden')
    parser.add_argument('--count', type=int, choices=[1, 5, 20], default=1)
    parser.add_argument('--seconds', type=int, default=85)
    parser.add_argument('--require-visible', action='store_true',
                        help='Fail closed: continuous visibility has no validated lossless source yet')
    args = parser.parse_args()
    if args.require_visible:
        parser.error("continuous visibility UNVERIFIED: GPUI callbacks coalesce transitions; no validated lossless source")
    assert 5 <= args.seconds <= 300 and args.seconds % 5 == 0
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.pop('NO_COLOR', None)
    env.update(GUILD_STATE_DIR=str(output / 'state'), GUILD_NO_GLOBAL_HOOKS='1',
               GUILD_PROBE_COUNT=str(args.count), GUILD_PROBE_MODE=args.mode,
               GUILD_PROBE_SECONDS=str(args.seconds))
    metadata = dict(binary=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                    count=args.count, mode=args.mode, seconds=args.seconds,
                    started=datetime.datetime.now().astimezone().isoformat())
    (output / 'metadata.json').write_text(json.dumps(metadata, indent=2))
    with (output / 'native.jsonl').open('w') as log, (output / 'process.jsonl').open('w') as metrics:
        process = subprocess.Popen([str(binary)], cwd='/', env=env, stdout=log, stderr=subprocess.STDOUT)
        metadata['pid'] = process.pid
        (output / 'metadata.json').write_text(json.dumps(metadata, indent=2))
        print(json.dumps(dict(pid=process.pid, output=str(output))), flush=True)
        started = time.monotonic()
        try:
            time.sleep(1)
            subprocess.run(['osascript', '-e', 'tell application "System Events" to set frontmost of first application process whose unix id is ' + str(process.pid) + ' to true'], check=True)
            while process.poll() is None:
                elapsed = time.monotonic() - started
                if elapsed > args.seconds + 20:
                    raise TimeoutError('Owned probe exceeded its shutdown deadline')
                result = subprocess.run(['ps', '-p', str(process.pid), '-o', 'time=,rss='], capture_output=True, text=True, check=True)
                fields = result.stdout.split()
                if len(fields) == 2:
                    row = dict(at=datetime.datetime.now().astimezone().isoformat(), elapsed=elapsed,
                               cpu_seconds=cpu_seconds(fields[0]), rss_kib=int(fields[1]),
                               physical_footprint_bytes=footprint(process.pid))
                    if row['physical_footprint_bytes'] == 0:
                        break
                    children = subprocess.run(['pgrep', '-P', str(process.pid)], capture_output=True, text=True)
                    row['child_count'] = len(children.stdout.split())
                    metrics.write(json.dumps(row) + '\n')
                    metrics.flush()
                time.sleep(1 if args.mode == 'cycles' else 5)
            if process.returncode:
                raise RuntimeError(f'Owned probe exited {process.returncode}')
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
    native = []
    for line in (output / 'native.jsonl').read_text().splitlines():
        try:
            native.append(json.loads(line))
        except ValueError:
            pass
    if args.mode == 'latency':
        validate_latency_records(native)
    rows = [json.loads(line) for line in (output / 'process.jsonl').read_text().splitlines()]
    rows = [row for row in rows if 15 <= row['elapsed'] < args.seconds - 4]
    if len(rows) >= 2:
        first, last = rows[0], rows[-1]
        summary = dict(continuous_visibility='UNVERIFIED',
                       interval_seconds=last['elapsed']-first['elapsed'],
                       cpu_one_core_percent=100*(last['cpu_seconds']-first['cpu_seconds'])/(last['elapsed']-first['elapsed']),
                       rss_kib_last=last['rss_kib'])
        summary['physical_footprint_bytes_last'] = last['physical_footprint_bytes']
        summary['child_count_last'] = last['child_count']
        counters = [row for row in native if 15 <= row.get('elapsed', -1) < args.seconds - 4]
        if len(counters) >= 2:
            start, finish = counters[0], counters[-1]
            interval = finish['elapsed'] - start['elapsed']
            for field in ['polls', 'paints', 'rows']:
                summary[field + '_per_second'] = (finish[field] - start[field]) / interval
            summary['observed_visible_counter_interval'] = observed_visible_during(native, start['elapsed'], finish['elapsed'])
            summary['active_entire_counter_interval'] = all(row['active'] for row in counters)
        samples = sorted(row['input_to_paint_ms'] for row in native if 'input_to_paint_ms' in row)
        if args.mode == 'latency':
            sent = sum('sent_sequence' in row for row in native)
            summary.update(input_samples=len(samples), sent_inputs=sent,
                           missed_inputs=sent-len(samples),
                           input_p95_ms=samples[math.ceil(.95 * len(samples)) - 1] if samples else None,
                           input_max_ms=samples[-1] if samples else None)
        for field in ['max_grid_hold_ms', 'max_grid_wait_ms']:
            values = [row[field] for row in native if field in row]
            if values:
                summary[field] = max(values)
        parsed = [row for row in native if 'parsed_bytes' in row]
        if parsed:
            summary['throughput'] = parsed
            summary['final_grid_marker'] = any(row.get('final_grid_marker') for row in native)
        (output / 'summary.json').write_text(json.dumps(summary, indent=2))
        print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
