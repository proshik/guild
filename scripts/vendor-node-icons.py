#!/usr/bin/env python3
"""Bring the IntelliJ platform's declaration icons over.

M8.13. A `.kt` or a `.java` file draws what is **declared in it** — a
class, an interface, an enum — and the artwork is the original rather
than a likeness of it. The icons are the IntelliJ platform's,
Apache-2.0, from `intellij-community` at a pinned tag.

Each icon has a `_dark` variant, which is the one Guild's five dark
palettes take; the plain file is for the light one.

Run from the repository root:

    python3 scripts/vendor-node-icons.py [tag]

Maintenance only: it is not part of the build, and `just check` does not
run it. A test walks `assets/nodes/` against `icons::NODES`, so a file
that is here and unwired, or wired and absent, fails the build.
"""

from __future__ import annotations

import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NODES = ROOT / "crates" / "desktop" / "assets" / "nodes"

DEFAULT_TAG = "idea/243.21565.193"
RAW = "https://raw.githubusercontent.com/JetBrains/intellij-community/{tag}/{path}"
ICONS = "platform/icons/src/expui/nodes"

# What a declaration can be, as far as the two languages Guild reads go.
# `record` stands in for Kotlin's `data class`, which is the same idea and
# the same shape, and `class` for its `object`: the Kotlin plugin's own
# icons are in neither the open repository nor the installed application's
# jars, and a likeness drawn by hand would be the one thing the person
# asked not to have.
WANTED = [
    "class", "classAbstract", "interface", "enum", "annotation", "record",
    # External Libraries' rows — M8.14.
    "library", "jdk",
]


def fetch(tag: str, path: str) -> bytes:
    with urllib.request.urlopen(RAW.format(tag=tag, path=path)) as answer:
        return answer.read()


def main() -> None:
    tag = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_TAG

    NODES.mkdir(parents=True, exist_ok=True)
    for old in NODES.glob("*.svg"):
        old.unlink()

    names = [f"{stem}{suffix}" for stem in WANTED for suffix in ("", "_dark")]
    for name in names:
        (NODES / f"{name}.svg").write_bytes(fetch(tag, f"{ICONS}/{name}.svg"))
    (NODES / "LICENSE").write_bytes(fetch(tag, "LICENSE.txt"))

    (NODES / "PROVENANCE").write_text(
        f"""IntelliJ IDEA's declaration icons, from the IntelliJ platform.

    source:  https://github.com/JetBrains/intellij-community
    tag:     {tag}
    path:    {ICONS}/
    licence: Apache-2.0, LICENSE beside this file

{len(names)} files: {', '.join(WANTED)}, each with its `_dark`
variant. The dark one is what Guild's five dark palettes take and the
plain one is for the light palette — the opposite of the Material set's
`_light` rule, and the reason `icons::for_declaration` spells it out
rather than reusing `themed`.

`record` stands in for Kotlin's `data class` and `class` for its
`object`: the Kotlin plugin's own icons are in neither the open
repository nor the installed application's jars, and drawing a likeness
is the one thing the person asked not to do. Where they turn up, they
are two more rows here and two more arms in `icons::for_declaration`.

Regenerate with `scripts/vendor-node-icons.py`; a test walks this
directory against `icons::NODES`.
"""
    )

    print(f"{len(names)} icons, the LICENSE and a PROVENANCE → {NODES.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
