#!/usr/bin/env python3
"""Vendor the faces the window is drawn in.

Inter and JetBrains Mono — the interface and code pair until M12.11 — and
Lilex in three families: Lilex for code,
Lilex Duo beside it, Lilex Nerd Font for the terminal. All are OFL 1.1,
which asks that the licence travel with the files; every licence is
written beside them.

Run by hand, like `vendor-icons.py` and `vendor-grammars.py`:

    python3 scripts/vendor-fonts.py

It downloads the releases, takes exactly the faces the window uses,
and writes them into `crates/desktop/assets/fonts/`. A face missing from a
release is an error rather than a silent absence: a font that fails to
load draws in something else and says nothing about it.
"""

import io
import pathlib
import sys
import urllib.request
import zipfile

HERE = pathlib.Path(__file__).resolve().parent.parent
FONTS = HERE / "crates/desktop/assets/fonts"

# What is taken, and what it is called here. The names are the file names
# the crate's `include_bytes!` uses, so they are part of the code.
RELEASES = [
    {
        "name": "Inter 4.1",
        "url": "https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip",
        "licence": ("LICENSE.txt", "LICENSE-inter"),
        # 400 body, 500 what is chosen, 600 headings, and a true italic —
        # a sheared upright is the thing to avoid.
        "faces": {
            "extras/ttf/Inter-Regular.ttf": "Inter-Regular.ttf",
            "extras/ttf/Inter-Medium.ttf": "Inter-Medium.ttf",
            "extras/ttf/Inter-SemiBold.ttf": "Inter-SemiBold.ttf",
            "extras/ttf/Inter-Italic.ttf": "Inter-Italic.ttf",
        },
    },
    {
        "name": "JetBrains Mono 2.304",
        "url": "https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip",
        "licence": ("OFL.txt", "LICENSE-jetbrains-mono"),
        # The terminal needs all four: SGR 1, SGR 3 and SGR 1;3 are three
        # of them, and no upright can stand in for an italic in a grid.
        "faces": {
            "fonts/ttf/JetBrainsMono-Regular.ttf": "JetBrainsMono-Regular.ttf",
            "fonts/ttf/JetBrainsMono-Bold.ttf": "JetBrainsMono-Bold.ttf",
            "fonts/ttf/JetBrainsMono-Italic.ttf": "JetBrainsMono-Italic.ttf",
            "fonts/ttf/JetBrainsMono-BoldItalic.ttf": "JetBrainsMono-BoldItalic.ttf",
        },
    },
    {
        # Slate's code and terminal face since M12.6, `LilexNF-*` 3.4.0. The
        # plain variant rather than `Mono`, because that is the family the
        # terminal is drawn with.
        "name": "Lilex Nerd Font 3.4.0",
        "url": "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/Lilex.zip",
        # Its copyright line is Nerd Fonts' copy of upstream's, and not
        # word for word upstream's: two families, two licence files.
        "licence": ("LICENSE.txt", "LICENSE-lilex-nerd-font"),
        "faces": {
            "LilexNerdFont-Regular.ttf": "LilexNerdFont-Regular.ttf",
            "LilexNerdFont-Bold.ttf": "LilexNerdFont-Bold.ttf",
            "LilexNerdFont-Italic.ttf": "LilexNerdFont-Italic.ttf",
            "LilexNerdFont-BoldItalic.ttf": "LilexNerdFont-BoldItalic.ttf",
        },
    },
    {
        # The code face since M12.11: the plain
        # family, which draws a file, not the Nerd one, which draws a
        # prompt. The release carries no licence in its archive, so the
        # OFL is read from the same tag.
        "name": "Lilex 2.700",
        "url": "https://github.com/mishamyrt/Lilex/releases/download/2.700/Lilex.zip",
        "licence": (
            "https://raw.githubusercontent.com/mishamyrt/Lilex/2.700/OFL.txt",
            "LICENSE-lilex",
        ),
        "faces": {
            "ttf/Lilex-Regular.ttf": "Lilex-Regular.ttf",
            "ttf/Lilex-Bold.ttf": "Lilex-Bold.ttf",
            "ttf/Lilex-Italic.ttf": "Lilex-Italic.ttf",
            "ttf/Lilex-BoldItalic.ttf": "Lilex-BoldItalic.ttf",
        },
    },
    {
        # Lilex's *duospaced* sibling, experimental upstream and marked *Beta*
        # here. It has no italic at all; the two weights code is drawn
        # in are what is taken. Its licence is Lilex's, one project, one
        # tag, written by the entry above.
        "name": "Lilex Duo 2.700",
        "url": "https://github.com/mishamyrt/Lilex/releases/download/2.700/LilexDuo.zip",
        "licence": None,
        "faces": {
            "ttf/LilexDuo-Regular.ttf": "LilexDuo-Regular.ttf",
            "ttf/LilexDuo-Bold.ttf": "LilexDuo-Bold.ttf",
        },
    },
]


def vendor(release):
    print(f"{release['name']}: fetching")
    with urllib.request.urlopen(release["url"]) as response:
        archive = zipfile.ZipFile(io.BytesIO(response.read()))

    licence = release["licence"]
    # A licence is a file in the archive, or a URL when the archive has
    # none — or nothing, when another entry of the same project writes it.
    from_archive = licence is not None and not licence[0].startswith("https://")
    inside = set(archive.namelist())
    for wanted in list(release["faces"]) + ([licence[0]] if from_archive else []):
        if wanted not in inside:
            sys.exit(f"{release['name']} has no {wanted} — the release moved")

    for source, name in release["faces"].items():
        (FONTS / name).write_bytes(archive.read(source))
        print(f"  {name}: {(FONTS / name).stat().st_size // 1024} KB")

    if licence is None:
        return
    source, name = licence
    if from_archive:
        text = archive.read(source)
    else:
        with urllib.request.urlopen(source) as response:
            text = response.read()
    (FONTS / name).write_bytes(text)
    print(f"  {name}")


def main():
    FONTS.mkdir(parents=True, exist_ok=True)
    for release in RELEASES:
        vendor(release)
    total = sum(f.stat().st_size for f in FONTS.iterdir())
    print(f"{FONTS.relative_to(HERE)}: {total // 1024} KB in {len(list(FONTS.iterdir()))} files")


if __name__ == "__main__":
    main()
