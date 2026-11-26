"""Evidence integrity tests; paint records never establish presentation latency."""
import importlib.util
from pathlib import Path
import unittest
import subprocess
import sys
import tempfile

spec = importlib.util.spec_from_file_location('resource_probe', Path(__file__).with_name('resource-probe.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class LatencyEvidenceTests(unittest.TestCase):
    def test_complete_ack_set(self):
        probe.validate_latency_records([
            {'sent_sequence': 1}, {'sent_sequence': 2},
            {'input_sequence': 2, 'input_to_paint_ms': 2},
            {'input_sequence': 1, 'input_to_paint_ms': 3},
        ], expected_count=2)

    def test_missing_ack_rejects_survivor_only_statistics(self):
        with self.assertRaisesRegex(ValueError, 'missing'):
            probe.validate_latency_records([
                {'sent_sequence': 1}, {'sent_sequence': 2},
                {'input_sequence': 2, 'input_to_paint_ms': 2},
            ], expected_count=2)

    def test_unmatched_and_duplicate_ack_rejected(self):
        for ids in [[1, 3], [1, 1]]:
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                probe.validate_latency_records([
                    {'sent_sequence': 1}, {'sent_sequence': 2},
                    *({'input_sequence': i, 'input_to_paint_ms': 2} for i in ids),
                ], expected_count=2)

    def test_truncated_trial_rejected_even_if_all_sent_inputs_acknowledged(self):
        with self.assertRaisesRegex(ValueError, 'expected'):
            probe.validate_latency_records([
                {'sent_sequence': 1}, {'input_sequence': 1, 'input_to_paint_ms': 2},
            ], expected_count=120)


class VisibilityEvidenceTests(unittest.TestCase):
    def test_startup_observation_is_not_continuous_visibility(self):
        self.assertTrue(probe.observed_visible_during([
            {'observed_visibility_seconds': 0, 'visible': False},
            {'observed_visibility_seconds': 1, 'visible': True},
        ], 15, 60))

    def test_delivered_occlusion_rejects_observed_visibility(self):
        self.assertFalse(probe.observed_visible_during([
            {'observed_visibility_seconds': 0, 'visible': True},
            {'elapsed': 15, 'visible': True},
            {'observed_visibility_seconds': 16, 'visible': False},
            {'observed_visibility_seconds': 17, 'visible': True},
            {'elapsed': 20, 'visible': True},
        ], 15, 20))

    def test_missing_initial_state_rejects_observed_visibility(self):
        self.assertFalse(probe.observed_visible_during([
            {'elapsed': 15, 'visible': True},
            {'elapsed': 20, 'visible': True},
        ], 15, 20))


class RequiredVisibilityTests(unittest.TestCase):
    def test_require_visible_fails_before_launch_without_lossless_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'trial'
            result = subprocess.run([
                sys.executable, str(Path(__file__).with_name('resource-probe.py')),
                '/nonexistent-owned-probe', str(output), '--require-visible',
            ], text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('continuous visibility UNVERIFIED', result.stderr)
            self.assertFalse(output.exists(), 'Do not launch or create a trial on an unprovable gate')


if __name__ == '__main__':
    unittest.main()
