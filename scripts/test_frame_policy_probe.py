"""Reject incomplete native scheduling evidence; never accept presentation metrics."""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location("policy", pathlib.Path(__file__).with_name("frame-policy-report.py"))
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)

class EvidenceTests(unittest.TestCase):
    def fixture(self):
        rows = []
        for mode in policy.MODES:
            for generation in range(1, 25):
                rows.append(dict(mode=mode, generation=generation, native_key=False,
                                 native_visible=True, native_occluded=False, lost=0))
            rows.append(dict(mode=mode, event=dict(kind="submission")))
            if mode not in ("current", "candidate"):
                rows.append(dict(mode=mode, event=dict(kind="skip", reason="InactiveFrameRateLimit")))
        rows.append(dict(visibility_trial=True, journal_lost=0, visibility_lost=0))
        return rows

    def test_missing_negative_control_is_not_a_pass(self):
        rows = [r for r in self.fixture() if r.get("mode") != "fenced"]
        self.assertRaises(ValueError, policy.summarize, rows)

    def test_lost_entries_reject_trial(self):
        rows = self.fixture(); rows[-1]["journal_lost"] = 1
        self.assertRaises(ValueError, policy.summarize, rows)

    def test_occlusion_rejects_trial(self):
        rows = self.fixture(); rows[0]["native_occluded"] = True
        self.assertRaises(ValueError, policy.summarize, rows)

    def test_new_inactive_throttle_rejects_candidate(self):
        rows = self.fixture() + [dict(mode="candidate", event=dict(kind="skip", reason="InactiveFrameRateLimit"))]
        self.assertRaises(ValueError, policy.summarize, rows)

    def test_scheduling_does_not_accept_visibility_or_presentation(self):
        report = policy.summarize(self.fixture())
        self.assertEqual(report["presentation"], "UNVERIFIED")
        self.assertEqual(report["continuous_visibility"], "UNVERIFIED")

if __name__ == "__main__":
    unittest.main()
