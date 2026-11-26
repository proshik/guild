#!/usr/bin/env python3
"""Finite task-owned hidden production Root trial and separate resource sampler."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('binary', 'sampler', 'fixture', 'output'):
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--roots', type=int, choices=(1, 10, 33, 100), required=True)
    a = p.parse_args()
    assert all(v.is_absolute() for v in (a.binary, a.sampler, a.fixture, a.output))
    assert (a.fixture / '.owned-runtime-worktrees').is_file()
    a.output.mkdir(exist_ok=False)
    env = os.environ.copy()
    env.update(GUILD_STATE_DIR=str(a.output / 'state'), GUILD_NO_GLOBAL_HOOKS='1',
               GUILD_WORKTREES_FIXTURE=str(a.fixture), GUILD_WORKTREES_ROOTS=str(a.roots))
    env.pop('NO_COLOR', None)
    cancelled = []
    signal.signal(signal.SIGTERM, lambda number, _: cancelled.append(number))
    children = []
    samples = []
    start = time.monotonic()
    metadata = dict(binary_sha256=sha(a.binary), sampler_sha256=sha(a.sampler),
                    fixture_manifest_sha256=sha(a.fixture / 'manifest.json'), roots=a.roots,
                    deadline_seconds=330, limits=dict(footprint_mib=1800, fds=850, descendants=50, threads=400),
                    subject_cpu='owned application only, including its reaped children; excludes external samplers',
                    sampling_limits='1s usage, 3s FDs/children; peaks are lower bounds; live child CPU excluded',
                    visibility='hidden/no activation', provenance_preserved=True)
    try:
        with (a.output / 'stdout.jsonl').open('w') as out, (a.output / 'stderr.log').open('w') as err, (a.output / 'usage.jsonl').open('w') as usage:
            app = subprocess.Popen([str(a.binary)], env=env, cwd='/', stdout=out,
                                   stderr=err, start_new_session=True)
            children.append(app)
            metadata['owned_pid'] = app.pid
            sampler = subprocess.Popen([str(a.sampler), str(app.pid)], stdout=usage,
                                       stderr=err, start_new_session=True)
            children.append(sampler)
            while app.poll() is None:
                if cancelled:
                    raise RuntimeError('owned trial cancelled by signal')
                if time.monotonic()-start > 330:
                    raise RuntimeError('owned trial deadline')
                time.sleep(3)
                if app.poll() is not None:
                    break
                fd = subprocess.run(['/usr/sbin/lsof', '-p', str(app.pid), '-Ff'],
                                    capture_output=True, text=True, timeout=3)
                table = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid='],
                                       capture_output=True, text=True, timeout=3, check=True)
                pairs = [tuple(map(int, line.split())) for line in table.stdout.splitlines()]
                descendants = {app.pid}
                while True:
                    more = descendants | {pid for pid, parent in pairs if parent in descendants}
                    if more == descendants:
                        break
                    descendants = more
                fds = sum(s.startswith('f') and s[1:].isdigit() for s in fd.stdout.splitlines())
                usage.flush()
                records = [json.loads(s) for s in (a.output / 'usage.jsonl').read_text().splitlines(keepends=True) if s.endswith('\n')]
                footprint = records[-1]['footprint'] if records else 0
                samples.append(dict(seconds=time.monotonic()-start, numeric_fds=fds,
                                    descendants=len(descendants)-1, footprint=footprint, threads=records[-1]['threads'] if records else -1))
                if fds > 850 or len(descendants)-1 > 50 or footprint > 1800*1024**2 or (records and records[-1]['threads'] > 400):
                    raise RuntimeError('owned trial resource envelope')
            metadata['app_exit'] = app.returncode
            sampler.wait(timeout=5)
            metadata['sampler_exit'] = sampler.returncode
    finally:
        for child in reversed(children):
            # Popen owns this generation until reaped; never signal an archived PID.
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
        metadata.update(elapsed_seconds=time.monotonic()-start, all_children_reaped=True)
        (a.output / 'metadata.json').write_text(json.dumps(metadata, indent=2)+'\n')
        (a.output / 'resources.json').write_text(json.dumps(samples, indent=2)+'\n')
    assert metadata['app_exit'] == 0 and metadata['sampler_exit'] == 0
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()
