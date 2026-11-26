# Bundled system WebKit renderer

Guild.app ships separate browser JS resources and a small native helper. Only the
helper links system AppKit/WebKit. There is no installer, runtime network fetch,
Node, Java, Chromium or compiler fallback. Renderer resources update with Guild.

Production always uses `Contents/MacOS/guild-diagram-renderer` and
`Contents/Resources/diagrams`; even an explicit development override cannot
replace missing app resources. No app-support component directory is read or
modified. Worker-side bounded regular-file reads verify committed JS hashes and
the helper digest sealed with the app before execution. Errors explain how to
repair the bundle and keep source editing available.

```
just prepare-diagrams
GUILD_DIAGRAM_RESOURCES="$PWD/target/diagram-runtime" cargo run -p guild-desktop
# just desktop / desktop-release / desktop-dev do this preparation explicitly.
just bundle
```

Preparation uses the installed Apple Swift toolchain, arm64-apple-macos11.0,
and committed JS only. Cargo, just check and user flows never invoke it. It
writes only the owned target/diagram-runtime build directory. Bundle assembly
signs executables ad hoc using the existing trust route, writes the final helper
digest, seals the app, then verifies signatures and resources. No signing keys,
service or configuration is added. SOURCE_DATE_EPOCH optionally fixes metadata
and file timestamps plus a content-derived build UUID for reproducible assembly
from the same compiled inputs; normal builds retain the existing UUID/time
policy. The assembler replaces output only under this checkout target/. The
existing ditto ZIP route adds times to synthetic __MACOSX directory headers;
archive payloads are repeatable, while whole ZIP hashes can differ.

Assets live in crates/desktop/assets/diagrams. PROVENANCE.json pins the official
Mermaid 11.12.0 and @plantuml/core 1.2026.9 full npm archives (length/SHA512), every
selected extracted file (size/SHA256), and supplementary Viz.js/Graphviz/Expat
license sources. JS bytes remain unchanged with bundled notices preserved.
Full applicable licenses and provenance ship in the app. files.sha256 is generated.

```
python3 tools/diagrams/renderer/vendor.py
# Only when immutable inputs are absent, explicitly authorize fetching:
python3 tools/diagrams/renderer/vendor.py --fetch
```

Vendoring prefers retained target/webkit-spike archives. It admits exact bounded
regular archive files, verifies integrity before extraction and never runs npm
scripts/install. No build/CI/check/user flow invokes this operation.

```
python3 tools/diagrams/renderer/test_helper.py
cargo test --offline -p guild-desktop --lib diagrams::runtime::tests -- --nocapture
cargo test --offline -p guild-desktop --lib markdown::pane::diagram_geometry_tests -- --ignored --nocapture
GUILD_DIAGRAM_TEST_APP="$PWD/target/release/Guild.app" cargo test --offline -p guild-desktop --test all diagrams -- --include-ignored --test-threads=4
GUILD_DIAGRAM_TEST_APP="$PWD/target/release/Guild.app" python3 tools/diagrams/renderer/test_helper.py
```

The explicit helper test compiles its separate AppKit pixel probe with installed
tools. Real PNGs must contain painted content and dark text in both Mermaid
foreignObject label interiors; white and label-erased PNG controls must fail.
Results stay in target/diagram-runtime-results. Neither probe nor tests ship.

The window keeps serial work/engine-source deduplication, independent fence-span
layout, logical dimensions distinct from physical pixels, bounded cache/GPU
ownership and cancellation/quit joins. JSON source is bounded to 256 KiB decoded,
wire to worst-case expansion, output to 4 Mi pixels. CSP/network rules, disabled
file/stdlib loading and no navigation bridge keep render execution offline.
Native OS gestures/capture remain unverified without host permissions.
