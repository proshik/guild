"""Owned subprocess cancellation, without a window or provider."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

RUNNER = Path(__file__).resolve().parents[1] / 'runtime-remediation-diff.py'


class Cancellation(unittest.TestCase):
    def test_sigterm_reaps_owned_session(self):
        self.cancel_owned("diff")

    def test_idle_sigterm_reaps_owned_session(self):
        self.cancel_owned("idle")

    def test_transport_sigterm_reaps_owned_session(self):
        self.cancel_owned("transport")

    def cancel_owned(self, protocol):
        with tempfile.TemporaryDirectory(prefix='guild-runner-cancel-') as folder:
            root = Path(folder)
            fixture = root / 'fixture'
            fixture.mkdir()
            for name in ('.owned-runtime-' + protocol, 'old.txt', 'new.txt', 'manifest.json'):
                (fixture / name).write_text('fixture\n')
            pid_file = root / 'child.pid'
            fake = root / 'fake'
            fake.write_text('#!' + sys.executable + '\nimport os,time\n'
                            + f'open({str(pid_file)!r}, "w").write(str(os.getpid()))\n'
                            + 'time.sleep(30)\n')
            fake.chmod(0o700)
            runner = subprocess.Popen([sys.executable, str(RUNNER), '--binary', str(fake),
                                       '--fixture', str(fixture), '--state', str(root / 'state'),
                                       '--output', str(root / 'output'), '--protocol', protocol,
                                       '--test-filter', 'pressure_tests'],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            child_pid = None
            try:
                deadline = time.monotonic() + 5
                while not pid_file.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(pid_file.exists(), 'owned fake did not start')
                child_pid = int(pid_file.read_text())
                runner.send_signal(signal.SIGTERM)
                out, err = runner.communicate(timeout=6)
                self.assertEqual(runner.returncode, 128 + signal.SIGTERM, err.decode())
                with self.assertRaises(ProcessLookupError):
                    os.kill(child_pid, 0)
                result = json.loads((root / 'output' / 'cancellation.json').read_text())
                self.assertTrue(result['owned_child_reaped'])
                self.assertEqual(result['signal'], signal.SIGTERM)
            finally:
                if runner.poll() is None:
                    runner.kill()
                    runner.communicate(timeout=5)
                if child_pid is not None:
                    try:
                        os.killpg(child_pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == '__main__':
    unittest.main()
