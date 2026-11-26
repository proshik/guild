#!/usr/bin/env python3
"""Bring the file icons over from the Material Icon Theme itself.

Until M8.13 the set was a vendored subset: 191
files and 92 stems out of the theme's eleven hundred. Eleven of Guild's
own thirty-seven languages were not in it — kotlin, java, ruby, swift,
c#, elixir, php, lua, nix, hcl, just — so every file of theirs drew the
blank default page, which is what a person with a Kotlin project saw.

The source now is the **published extension**, pinned by version. It
carries `dist/material-icons.json`, which the theme's own build
generates from its TypeScript sources: the associations already
resolved, no transcription and nothing to parse. The icons travel in the
same package, minified, and the licence with them.

Run from the repository root:

    python3 scripts/vendor-icons.py [version] [--vsix path.vsix]

Maintenance only: it is not part of the build, and `just check` does not
run it. A test checks that `map.rs` and the directory agree, so a
regenerated table cannot point at an icon that is not there.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "crates" / "desktop" / "assets" / "icons"
MAP = ROOT / "crates" / "desktop" / "src" / "icons" / "map.rs"
EMBED = ROOT / "crates" / "desktop" / "src" / "icons" / "embed.rs"

DEFAULT_VERSION = "5.29.0"
MARKETPLACE = (
    "https://marketplace.visualstudio.com/_apis/public/gallery/publishers/"
    "PKief/vsextensions/material-icon-theme/{version}/vspackage"
)


def fetch(version: str, vsix: Path | None) -> tuple[Path, str]:
    """The unpacked extension, and the package's sha256.

    A `.vsix` is a zip; the marketplace serves it gzipped, and the
    header is what says which, because the endpoint answers
    `Content-Encoding: gzip` for a body that is already a zip.
    """
    if vsix is not None:
        data = vsix.read_bytes()
    else:
        url = MARKETPLACE.format(version=version)
        request = urllib.request.Request(url, headers={"Accept-Encoding": "gzip"})
        with urllib.request.urlopen(request) as answer:
            data = answer.read()
            if answer.headers.get("Content-Encoding") == "gzip":
                import gzip

                data = gzip.decompress(data)

    digest = hashlib.sha256(data).hexdigest()
    into = Path(tempfile.mkdtemp(prefix="material-icons-"))
    package = into / "package.vsix"
    package.write_bytes(data)
    with zipfile.ZipFile(package) as archive:
        archive.extractall(into)
    return into / "extension", digest


def rust_table(name: str, rows: list[tuple[str, str]], doc: str) -> str:
    body = "\n".join(f'    ("{k}", "{v}"),' for k, v in rows)
    return f"/// {doc}\npub(super) const {name}: &[(&str, &str)] = &[\n{body}\n];\n"


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    version = args[0] if args else DEFAULT_VERSION
    vsix = None
    if "--vsix" in sys.argv:
        vsix = Path(sys.argv[sys.argv.index("--vsix") + 1])

    extension, digest = fetch(version, vsix)
    theme = json.loads((extension / "dist" / "material-icons.json").read_text())
    icons = extension / "icons"

    # An icon's key is not always its file name: a clone is generated as
    # `<key>.clone.svg`. The key is what the tables carry, so the file is
    # copied under the key and `icons::icon` can build its path from a
    # stem alone.
    paths = {
        key: icons / Path(value["iconPath"]).name
        for key, value in theme["iconDefinitions"].items()
    }

    def pairs(section: str) -> list[tuple[str, str]]:
        return sorted((k.lower(), v) for k, v in theme[section].items())

    by_extension = pairs("fileExtensions")
    by_filename = pairs("fileNames")
    by_folder = pairs("folderNames")
    # **The theme's host editor resolves some types by language id and not
    # by suffix** —
    # `.ts`, `.js`, `.yml` and `.php` are in none of the tables above —
    # so the rule asks Guild's own language table last.
    by_language = pairs("languageIds")

    light = set()
    for section in theme["light"]:
        for key, variant in theme["light"][section].items():
            plain = theme.get(section, {}).get(key)
            if plain is not None and variant == f"{plain}_light":
                light.add(plain)
    light = sorted(light)

    wanted = {theme["file"], theme["folder"], theme["folderExpanded"]}
    for table in (by_extension, by_filename, by_folder, by_language):
        wanted |= {stem for _, stem in table}
    wanted |= {f"{stem}-open" for stem in {v for _, v in by_folder} | {theme["folder"]}}
    wanted |= {"folder-src", "folder-test", "folder-resource"}
    wanted |= {f"{stem}-open" for stem in ("folder-src", "folder-test", "folder-resource")}
    wanted |= {f"{stem}_light" for stem in light}
    wanted |= {f"{stem}_light" for stem in light if f"{stem}_light" in paths}

    absent = sorted(stem for stem in wanted if stem not in paths)
    if absent:
        raise SystemExit(f"the theme has no icon for {absent[:10]}")

    ASSETS.mkdir(parents=True, exist_ok=True)
    for old in ASSETS.glob("*.svg"):
        old.unlink()
    for stem in sorted(wanted):
        shutil.copyfile(paths[stem], ASSETS / f"{stem}.svg")
    shutil.copyfile(extension / "LICENSE.txt", ASSETS / "LICENSE")

    header = f"""//! GENERATED by `scripts/vendor-icons.py` — do not hand-edit.
//!
//! The Material Icon Theme's own associations, from the published
//! extension `PKief.material-icon-theme` {version} (MIT —
//! `assets/icons/LICENSE`), sha256 {digest[:16]}…, as its build
//! generated them in `dist/material-icons.json`. The rule that turns a
//! name into one of these is `icons::for_file` and `icons::for_dir`, the
//! one hand-written half; regenerate this file rather than editing it,
//! because a row added by hand is an association nobody can trace.

"""
    out = header
    out += rust_table(
        "BY_EXTENSION",
        by_extension,
        "Extension → icon stem. Keys are lowercase and carry no leading dot; the longest suffix wins.",
    )
    out += "\n" + rust_table(
        "BY_FILENAME",
        by_filename,
        "Whole file name → icon stem. Lowercase. A whole name outranks any extension.",
    )
    out += "\n" + rust_table(
        "BY_LANGUAGE",
        by_language,
        "Language name → icon stem, for what VS Code resolves by language id rather than by suffix.",
    )
    out += "\n" + rust_table(
        "BY_FOLDER",
        by_folder,
        "Directory name → folder icon stem, without the `-open` suffix.",
    )
    out += "\n/// Stems that have a `_light` variant in the set, for the light palette.\n"
    out += (
        "pub(super) const HAS_LIGHT_VARIANT: &[&str] = &[\n"
        + "\n".join(f'    "{s}",' for s in light)
        + "\n];\n"
    )
    MAP.parent.mkdir(parents=True, exist_ok=True)
    MAP.write_text(out)

    names = sorted(svg.name for svg in ASSETS.glob("*.svg"))
    arms = "\n".join(
        f'        "{n}" => Some(include_bytes!("../../assets/icons/{n}")),' for n in names
    )
    EMBED.write_text(
        f"""//! GENERATED by `scripts/vendor-icons.py` — do not hand-edit.
//!
//! Every icon in `assets/icons/`, compiled into the binary: the desktop
//! is one file, and a file that looks for its icons next to itself is a
//! file that draws nothing when moved.

/// The bytes of one icon by its file name, or `None` for a name the set
/// does not have.
pub(super) fn bytes(name: &str) -> Option<&'static [u8]> {{
    match name {{
{arms}
        _ => None,
    }}
}}

/// Every file name in the set, for the test that checks the tables
/// against it.
pub(super) const NAMES: &[&str] = &[
{chr(10).join(f'    "{n}",' for n in names)}
];
"""
    )
    print(f"{len(names)} icons and the LICENSE → {ASSETS.relative_to(ROOT)}")
    print(
        f"{len(by_extension)} extensions, {len(by_filename)} names, "
        f"{len(by_language)} languages, {len(by_folder)} folders, "
        f"{len(light)} light variants → {MAP.relative_to(ROOT)}"
    )


if __name__ == "__main__":
    main()
