# Owned terminal refresh pilot

These tools capture **only the sole window of the supplied owned PID**. An absent
or ambiguous window fails closed. They never capture the desktop, answer permission
prompts, inspect credentials, or change display/power settings. Use fresh task-owned
state and the isolated diagnostic Root. Never launch untouched production Root.

`producer.py` emits 240 numbered synchronized ANSI background updates/s (offered
load, not achieved refresh). Output uses a 32-bit sequence, CRC-CCITT-FALSE and two
sentinel cells at either end. A second stripe uses the high bit to distinguish ACKs.
The finite input driver dispatches synthetic digits and Enter at 10 Hz. Producer
receipt, grid-content/generation capture and ScreenCaptureKit displayTime use the
mach absolute clock. Parser generation is a diagnostic revision, distinct from the
adaptive scheduler epoch/generation. Grid witnesses may repeat a content sequence.

`codec.swift` samples 52 cell centers per stripe; initial/recovery location is bounded
by the owned image dimensions. CRC failure is invalid, never a guessed value.
Recovery continues after startup; no fixed sample cutoff truncates later activity.
Separate output/ACK roles avoid treating an adjacent stale row as a second marker.
Records are buffered, with explicit overflow counts. No OCR, image files or per-frame
stdout writes are used. Capture's internal drops remain UNVERIFIED: public frame
attachments expose no lossless sequence counter. Skipped producer sequences reflect
coalescing and cannot be called dropped display opportunities.

Build tools, outside any trial:

```sh
swiftc scripts/terminal-refresh/codec.swift scripts/terminal-refresh/codec-tests.swift -o target/adaptive-refresh/codec-tests
./target/adaptive-refresh/codec-tests
python3 scripts/terminal-refresh/test_analysis.py
swiftc -O scripts/terminal-refresh/codec.swift scripts/terminal-refresh/capture.swift -o target/adaptive-refresh/stripe-capture
clang -O2 scripts/terminal-refresh/usage.c -o target/adaptive-refresh/owned-usage
```

`prepare-builds.py` exports main and candidate into ignored task-owned directories;
it refuses existing destinations. It produces source manifests and the exact common
diagnostic patch, including the already-approved profiler feature's lockfile delta.
It excludes the experimental scheduler from A. Compile A0 with `native-preview`, A
with `native-policy-probe`, B with `native-policy-probe,adaptive-terminal-refresh`,
all `--locked --release -p guild-desktop --example resource-preview`. With a shared
target directory, first `cargo clean -p guild-desktop --release` for EACH archive:
Cargo reused absolute-path dep-info from the previous archive despite changed source.
Verify the compilation path and binary hash. Copy each
result to a distinct binary before building the next variant. B Current/Adaptive
share one binary. No build runs during trials.

Example, using a frozen binary and an unused output directory:

```sh
python3 scripts/terminal-refresh/run-pilot.py target/adaptive-refresh/comparison/binaries/B target/adaptive-refresh/my-pilot --policy adaptive --display 1 --mode stripe-ack --seconds 12
python3 scripts/terminal-refresh/analysis.py target/adaptive-refresh/my-pilot
```

Use display IDs from the actual inventory. `--count 20 --mode quiet` keeps twenty
terminals, one front. `hidden` hides all static terminals; `hidden-output` hides one
busy terminal and the other retained quiet terminals. `--standalone --no-capture`
permits a safe A0/A-instrumented overhead control; both use the original standalone
window geometry and `inactive_frame_interval=None`. Real Root always retains the
production inactive policy. Standalone overhead cannot prove real Root overhead.

The analyzer removes two seconds at each end of process-accounting coverage. It
counts exact marker matches with a preceding grid witness and emission, duplicates,
invalid and unattributed frames, skipped producer IDs, and matched ACK latencies.
ACKs outside the trimmed interval and unmatched receipts remain explicit. A receipt
near the trim end can have its ACK outside the analysis interval; do not label that
as input loss. Process CPU uses delta user+system **mach ticks** divided by wall
mach ticks; physical footprint is not RSS. Historical debug files named these CPU
fields `user_ns`/`sys_ns` incorrectly; they are excluded from release comparisons.

WindowServer displayTime is a **software endpoint**, not photon timing. Observed
visible/active/power/display states can reject a trial but cannot establish lossless
visibility history. Short pilots are not acceptance, confidence intervals, frame-miss
proof, energy calibration, or permission to enable Adaptive by default. A zero GPU
counter is UNVERIFIED. Kernel energy accounting has unverified coverage/calibration.


Deadline diagnosis is independently reproducible without a window:

```sh
clang -O2 scripts/terminal-refresh/deadline-spike.c -o target/adaptive-refresh/deadline-spike
./target/adaptive-refresh/deadline-spike > target/adaptive-refresh/deadline-spike.csv
```

Modes 0/1/2 are dispatch_after, zero-leeway ordinary one-shot source, and strict
zero-leeway one-shot source; each records 150 four-millisecond waits. This is a
scheduler timing experiment, not FPS or energy proof. `GUILD_PROBE_TIMING=1` enables
bounded native deadline-action records in the terminal fixture for causal runs;
leave it absent for comparative trials. The strict pending-only adapter can cost
active power; no claim of battery preservation follows from its timer precision.

### Next authorized interval: 60 Hz correction and interactions

Do not run until another visible-window interval is authorized. Rebuild/freeze a
new B snapshot; comparison-v3/B predates the correction. Keep A-instrumented an
independent exact-base + symmetric common-diagnostics build, not B Current.
For ordinary ACK repeats the existing A remains a valid diagnostic control;
interaction changes must be applied symmetrically and their diff re-frozen.
No builds while a trial is running. Each output directory must be new.

```
python3 scripts/terminal-refresh/run-pilot.py NEW_B target/adaptive-refresh/postfix-60-B --display 4 --mode stripe-ack --policy adaptive --seconds 25
python3 scripts/terminal-refresh/run-pilot.py A_INSTRUMENTED target/adaptive-refresh/postfix-60-A --display 4 --mode stripe-ack --policy current --seconds 25
python3 scripts/terminal-refresh/run-pilot.py NEW_B target/adaptive-refresh/scroll-B --display 1 --mode output --scenario scroll --seconds 16
python3 scripts/terminal-refresh/run-pilot.py NEW_B target/adaptive-refresh/selection-B --display 1 --mode output --scenario selection --seconds 16
python3 scripts/terminal-refresh/run-pilot.py NEW_B target/adaptive-refresh/migration-B --display 1 --mode stripe --scenario migration --seconds 20
```

Repeat/counterbalance the ACK pairs (including unfavorable results); no short-pilot
PASS. Scroll injects Shift-PageUp at 4 s, Shift-PageDown at 6 s, Cmd-End at 8 s
through public GPUI input dispatch. Selection dispatches SelectAll at 4 s then
Escape at 6 s. Synthetic Look explicitly disables copy-on-select, so these do not
read or replace the user's clipboard. Records include actual grid display offset
and selection presence; reject if scroll never leaves bottom, does not return,
or selection fails to appear/clear. These are finite functional keyboard/action
scenarios, not mouse-drag latency or hardware-input acceptance. Existing public
terminal selection/scroll tests cover grid semantics; native scenario execution
remains UNVERIFIED. Counts-only capture does not prove presentation latency.

Migration requests moves of the owned window to display 4 at 4 s, display 1 at
8 s, display 4 at 12 s, via public AppKit on the main queue outside GPUI borrow.
No activation, other-window lookup or system setting changes. IDs describe this
machine only. A missing target returns false; require subsequent environment
samples to confirm each target, visibility/activity and continuing generations.
Same-turn observed_display may precede the asynchronous move. Reject occluded
segments. General analysis intentionally rejects mixed-display trials as steady
A/B; evaluate transition segments separately. Unknown-screen fallback is locally
tested, not manufactured by changing system settings. High-refresh capability is
not the currently delivered VRR rate. No native execution was done for this change.

### Persistent preference builds

Normal `cargo`/`just` builds include the technical scheduler feature. The user
selects Settings -> Terminal -> Adaptive rendering up to 120 Hz; its persisted
`adaptive_terminal_refresh` boolean is false by default. No environment variable
is read by production scheduling. `--no-default-features` builds omit the toggle.

Current diagnostic fixtures use `GUILD_PROBE_REFRESH=adaptive` only to construct
an explicit synthetic Settings override, before creating their terminals. The
runner supplies it from `--policy`; its old GUILD_TERMINAL_REFRESH value remains
solely for compatibility with immutable pre-setting frozen binaries. New code
ignores that old variable. Historical comparison-v4 binaries and measurements are
unchanged. Re-freeze sources/common diagnostics and rebuild before new trials;
prepare-builds removes the candidate-only Settings override from A. No native
trial or full performance matrix was run for the preference change.

For a future separately authorized **functional settings UI** interval, build the
owned `resource-preview` example with `--features native-policy-probe` and use
`--mode quiet --policy current --scenario settings --seconds 60 --no-capture`.
It opens the real Terminal settings page over a synthetic retained terminal,
starting off; manual clicks affect that existing terminal. No native window was
launched for this change. Headless GPUI tests already exercise the actual switch,
persisted on/off and absence of owned adaptive resources after off.


The `settings` scenario now loads the task-owned `settings.json` Store for both
Root and synthetic terminals; it ignores the policy environment override. To test
restart, retain that exact absolute state directory and set
`GUILD_PROBE_SETTINGS_RESTART=1`. The example requires the marker it created on the
first fresh Settings launch, Root mode, and the Settings scenario; other scenarios
still require fresh state. Each launch creates a fresh synthetic project subtree.
Keep the owning launcher alive until the window closes and record its exit code;
a detached subprocess killed with its orchestration command is not a clean-restart
trial. Native interaction evidence is under `target/adaptive-refresh/native-settings`.
