#!/usr/bin/env python3
"""Render the selected Contour icon from its editable SVG master.

Run `python3 assets/icon.py` with rsvg-convert (librsvg) installed.
The committed 1024px PNG is used by `just bundle`; ordinary builds need
neither Python nor librsvg. Keep the SVG as the single drawing source.
"""

from pathlib import Path
import shutil
import subprocess


def main():
    renderer = shutil.which("rsvg-convert")
    if renderer is None:
        raise SystemExit("Install librsvg (rsvg-convert) to regenerate the icon.")
    source = Path(__file__).with_suffix(".svg")
    target = source.with_suffix(".png")
    subprocess.run(
        [renderer, "-w", "1024", "-h", "1024", str(source), "-o", str(target)],
        check=True,
    )
    print(f"done: {target}")


if __name__ == "__main__":
    main()
