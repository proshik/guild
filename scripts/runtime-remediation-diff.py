#!/usr/bin/env python3
"""Finite hidden-window release comparison; no providers or UI activation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Terminated(BaseException):
    def __init__(self, number):
        self.number = number


_spawning = False
_pending_signal = None


def terminate(number, _frame):
    global _pending_signal
    if _spawning:
        _pending_signal = number
    else:
        raise Terminated(number)


def main():
    global _spawning
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--protocol', choices=('diff', 'idle', 'transport'), default='diff')
    parser.add_argument('--test-filter')
    args = parser.parse_args()
    for path in (args.binary, args.fixture, args.state, args.output):
        if not path.is_absolute():
            parser.error('absolute task-owned paths required')
    if not (args.fixture / ('.owned-runtime-' + args.protocol)).is_file():
        parser.error('marked fixture required')
    args.output.mkdir(exist_ok=False)
    args.state.mkdir(exist_ok=False)
    env = os.environ.copy()
    env.update(GUILD_STATE_DIR=str(args.state), GUILD_NO_GLOBAL_HOOKS='1',
               GUILD_DIFF_FIXTURE=str(args.fixture), GUILD_DIFF_NATIVE='0')
    if args.protocol == 'idle':
        env['GUILD_IDLE_FIXTURE'] = str(args.fixture)
    deadline = 180 if args.protocol != 'diff' else 45
    env.pop('NO_COLOR', None)
    metadata = dict(binary=str(args.binary), binary_sha256=digest(args.binary),
                    fixture_sha256={name: digest(args.fixture / name)
                                    for name in (('manifest.json',) if args.protocol != 'diff' else ('old.txt', 'new.txt'))},
                    state=str(args.state), visibility=('headless owned tests' if args.protocol == 'transport' else 'hidden, no activation'),
                    cwd='/', deadline_seconds=deadline,
                    limits=dict(rss_mib=1800, numeric_fds=800, descendants=45, threads=400),
                    guild_provenance_preserved=True)
    (args.output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    signal.signal(signal.SIGTERM, terminate)
    start = time.monotonic()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    with (args.output / 'stdout.jsonl').open('w') as out, (args.output / 'stderr.log').open('w') as err:
        _spawning = True
        command = [str(args.binary)]
        if args.protocol == 'transport':
            if args.test_filter != 'pressure_tests':
                parser.error('transport protocol requires the finite pressure_tests filter')
            command += [args.test_filter, '--nocapture', '--test-threads=1']
        child = subprocess.Popen(command, cwd='/', env=env, stdout=out,
                                 stderr=err, start_new_session=True)
        samples = []
        try:
            metadata['owned_pid'] = child.pid
            birth = subprocess.run(['/bin/ps', '-p', str(child.pid), '-o', 'lstart='],
                                   capture_output=True, text=True, timeout=2)
            metadata['owned_birth_observed'] = birth.stdout.strip()
            (args.output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
            _spawning = False
            if _pending_signal is not None:
                raise Terminated(_pending_signal)
            while True:
                try:
                    code = child.wait(timeout=0.5)
                    break
                except subprocess.TimeoutExpired:
                    if time.monotonic()-start > deadline:
                        raise RuntimeError('owned diff trial exceeded its deadline')
                    rss = subprocess.run(['/bin/ps', '-p', str(child.pid), '-o', 'rss='],
                                         capture_output=True, text=True, timeout=2)
                    fds = subprocess.run(['/usr/sbin/lsof', '-p', str(child.pid), '-Ff'],
                                         capture_output=True, text=True, timeout=2)
                    if not rss.stdout.strip():
                        continue
                    rss_kib = int(rss.stdout.strip())
                    fd_count = sum(line.startswith('f') and line[1:].isdigit()
                                   for line in fds.stdout.splitlines())
                    inventory = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid='],
                                               capture_output=True, text=True, timeout=2, check=True)
                    parents = [tuple(map(int, line.split())) for line in inventory.stdout.splitlines()]
                    descendants = {child.pid}
                    while True:
                        expanded = descendants | {pid for pid, ppid in parents if ppid in descendants}
                        if expanded == descendants:
                            break
                        descendants = expanded
                    threads = subprocess.run(['/bin/ps', '-M', '-p', str(child.pid), '-o', 'pid='],
                                             capture_output=True, text=True, timeout=2)
                    thread_count = len(threads.stdout.splitlines())
                    samples.append(dict(seconds=time.monotonic()-start, rss_kib=rss_kib,
                                        numeric_fds=fd_count, descendants=len(descendants)-1, threads=thread_count))
                    if rss_kib > 1800*1024 or fd_count > 800 or len(descendants)-1 > 45 or thread_count > 400:
                        raise RuntimeError('owned trial exceeded its sampled resource envelope')
        except BaseException as failure:
            # Further SIGTERM must not interrupt the bounded owned-child cleanup.
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            # Popen still owns the unreaped leader. Never signal an archived PID.
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
            if isinstance(failure, Terminated):
                (args.output / 'cancellation.json').write_text(json.dumps(dict(
                    signal=failure.number, owned_child_reaped=True,
                    elapsed_seconds=time.monotonic()-start), indent=2) + '\n')
                raise SystemExit(128 + failure.number)
            raise
        finally:
            (args.output / 'resource-samples.json').write_text(json.dumps(samples, indent=2) + '\n')
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    records = [json.loads(line) for line in (args.output / 'stdout.jsonl').read_text().splitlines()
               if line.startswith('{')]
    trials = [r for r in records if 'trial' in r and not r['warmup']]
    summary = dict(exit=code, elapsed_seconds=time.monotonic()-start,
                   child_user_seconds=after.ru_utime-before.ru_utime,
                   child_system_seconds=after.ru_stime-before.ru_stime,
                   measured_trials=len(trials),
                   correct=code == 0 and len(trials) == 5 and all(r['correct'] for r in trials),
                   endpoint='public update return / model matching captured source, not presented frame',
                   owned_child_reaped=True)
    summary['runner_child_cpu_includes_external_ps_lsof_probes'] = True
    if args.protocol == 'idle':
        subjects = [r for r in records if 'subject' in r]
        summary.update(measured_subjects=len(subjects),
                       correct=code == 0 and len(subjects) == 12 and any(r.get('completed') for r in records),
                       endpoint='actual Root flush and Book/LiveJobs read-consume-drop; model only')
    if args.protocol == 'transport':
        output_text = (args.output / 'stdout.jsonl').read_text()
        summary.update(correct=code == 0 and '6 passed; 0 failed' in output_text,
                       endpoint='actual bounded Client/Emulator IO tests, 6 finite cases, no UI')
    (args.output / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary))
    if not summary['correct']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
