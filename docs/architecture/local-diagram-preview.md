# Local diagram preview

Mermaid and PlantUML render offline inside Guild, through the system's WebKit in
a small helper beside the main executable.

## Current contract

Install Guild, open .mmd/.mermaid/.puml/.plantuml or Markdown diagram fences,
and use Source/Split/Preview immediately offline. Official Mermaid 11.12.0,
@plantuml/core 1.2026.9 and its Viz.js browser runtime are separate app resources.
A small arm64 macOS 11 helper lives beside the main executable and alone links
SYSTEM WebKit/AppKit. No Chromium, Node, Java, npm runtime/scripts or browser is
bundled or required. JS is not compiled into Rust's main binary.

There is no component Settings section, file picker, download, version/update or
removal flow. Components update with Guild.app. Production discovers only its
own exact bundle paths, ignores developer overrides inside an app and never
reads/adopts/deletes prior app-support components. Missing/corrupt bundle files
produce an actionable packaging error next to Preview, retaining source editing;
there is no runtime network/compiler/PATH fallback.

Development uses an explicit prepared directory in this checkout. Preparation
compiles the helper with the installed Apple toolchain and copies committed,
verified runtime resources. No preparation or downloads are part of Cargo or
just check. Desktop/dev/bundle entry points explicitly prepare as needed.
Vendoring is a separate explicit developer operation from pinned official
archives, exact length/SHA512 and extracted hashes; no npm scripts/install run.
Only selected runtime JS plus full applicable licenses/notices/provenance ship.

Keep the accepted single FileTab buffer, save/unsaved/undo behavior, modes,
selection/path routing, native fenced PNG blocks, adjacent syntax errors and
recovery. Keep natural/logical sizing, automatic deferred fitting frames,
per-occurrence fence layout with engine/source work deduplication, bounded
queue/decoded/GPU cache, cancellation, close and quit ownership, and helper
network/file/stdlib restrictions. No WebView overlays or dependency patches.

The helper's build, verification and pixel probes are in
`tools/diagrams/renderer/README.md`.
