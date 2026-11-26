"""Cancellation of both owned subprocesses in the Root measurement runner."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

RUNNER = Path(__file__).resolve().parents[1] / 'runtime-remediation-worktrees.py'


class Cancellation(unittest.TestCase):
    def test_sigterm_cleans_app_and_sampler(self):
        with tempfile.TemporaryDirectory(prefix='guild-root-runner-') as folder:
            root = Path(folder)
            for name in ('.owned-runtime-worktrees', 'manifest.json'):
                (root / name).write_text('{}\n')
            fakes = []
            for name in ('app', 'sampler'):
                fake = root / name
                fake.write_text('#!' + sys.executable + '\nimport os,time\n'
                                + f'open({str(root / (name + ".pid"))!r}, "w").write(str(os.getpid()))\n'
                                + 'time.sleep(30)\n')
                fake.chmod(0o700)
                fakes.append(fake)
            runner = subprocess.Popen([sys.executable, str(RUNNER), '--binary', str(fakes[0]),
                                       '--sampler', str(fakes[1]), '--fixture', str(root),
                                       '--roots', '1', '--output', str(root / 'output')],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            owned = []
            try:
                deadline = time.monotonic() + 5
                while not all((root / (n + '.pid')).exists() for n in ('app', 'sampler')):
                    self.assertLess(time.monotonic(), deadline, 'fake startup deadline')
                    time.sleep(.01)
                owned = [int((root / (n + '.pid')).read_text()) for n in ('app', 'sampler')]
                runner.send_signal(signal.SIGTERM)
                runner.communicate(timeout=8)
                self.assertNotEqual(runner.returncode, 0)
                for pid in owned:
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)
                self.assertTrue(json.loads((root / 'output' / 'metadata.json').read_text())['all_children_reaped'])
            finally:
                if runner.poll() is None:
                    runner.kill()
                    runner.communicate(timeout=5)
                for pid in owned:
                    try:
                        os.killpg(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == '__main__':
    unittest.main()
