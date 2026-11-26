"""Summarize the bounded native policy probe, without FPS/latency acceptance."""
import collections
import json
import sys

MODES = ("current", "fenced", "candidate", "current-unrelated", "candidate-unrelated")

def summarize(rows):
    receipts = [r for r in rows if r.get("visibility_trial")]
    if len(receipts) != 1 or receipts[0].get("journal_lost") != 0 or receipts[0].get("visibility_lost") != 0:
        raise ValueError("Missing final receipt or lost observations")
    counts = {}
    for mode in MODES:
        samples = [r for r in rows if r.get("mode") == mode and "generation" in r]
        if [r["generation"] for r in samples] != list(range(1, 25)):
            raise ValueError(f"Incomplete demand sequence: {mode}")
        if any(r.get("native_key") is not False or r.get("native_visible") is not True
               or r.get("native_occluded") is not False or r.get("lost") != 0 for r in samples):
            raise ValueError(f"Invalid native state or loss: {mode}")
        events = [r["event"] for r in rows if r.get("mode") == mode and "event" in r]
        count = collections.Counter(e.get("reason", e["kind"]) for e in events)
        if count["submission"] == 0:
            raise ValueError(f"No platform submissions: {mode}")
        if mode in ("current", "candidate"):
            if count["InactiveFrameRateLimit"]:
                raise ValueError(f"Direct notify introduced inactive throttling: {mode}")
        elif not count["InactiveFrameRateLimit"]:
            raise ValueError(f"Native negative/matched control did not exercise throttle: {mode}")
        counts[mode] = dict(count)
    return dict(scheduling="observed expected native policy", counts=counts,
                presentation="UNVERIFIED", continuous_visibility="UNVERIFIED",
                freshness_non_regression_vs_A="UNVERIFIED",
                note="Candidate is the direct-notify shape, not an implemented adaptive scheduler. Native states are sampled; physical occlusion between samples is not excluded.")

if __name__ == "__main__":
    with open(sys.argv[1]) as source:
        print(json.dumps(summarize([json.loads(line) for line in source]), indent=2))
