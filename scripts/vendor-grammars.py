#!/usr/bin/env python3
"""Bring in the grammar pieces that cannot be dependencies.

Four of the grammars M8.1.1 needs cannot simply be listed in
`Cargo.toml`, and they fail in three different ways — the third arrived
with M8.8.6:

  * **kotlin** and **dockerfile** publish crates that ask for
    `tree-sitter` 0.20 or 0.21. Cargo refuses outright — only one package
    in the graph may specify the same `links` value — so the parser is
    vendored and built by our own `build.rs` instead. The grammar itself
    is version-agnostic; it is the crate's manifest that is stale.
  * **proto** publishes a usable parser with its query constant commented
    out. Only that query is vendored. Just now exposes its query through
    the codebook-tree-sitter-just package, compatible with tree-sitter 0.26.
  * **graphql** and **hcl** publish a parser and **no query at all** —
    not commented out, not behind a `cfg`: there is no `queries/`
    directory in either package. The query comes from nvim-treesitter,
    pinned to a commit, and never from Helix: Helix writes its captures
    in its own vocabulary, and `keyword.control.conditional` would
    degrade through `paint_of_capture` to `keyword` rather than reach
    `conditional`, throwing away exactly the distinctions the capture
    table exists to make. nvim's vocabulary is the one `RENAMED` was
    built against.

Everything copied is MIT or Apache-2.0, and each directory keeps the
licence it came with beside it — for a query taken from
nvim-treesitter that is nvim-treesitter's, not the parser crate's, and
the two are different documents.

Run from the repository root:

    python3 scripts/vendor-grammars.py

The sources are the crate packages cargo has already downloaded, which
are the published artefacts themselves. Maintenance only: it is not part
of the build, and `just check` does not run it. A test checks that every
vendored file the code names is there.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = Path.home() / ".cargo/registry/src"
VENDOR = ROOT / "crates/desktop/vendor"
QUERIES = ROOT / "crates/desktop/assets/queries"

# Where a package ships no licence text of its own. MIT requires the
# notice to travel with the code, and a package that omits it is not a
# permission to omit it — so it comes from the repository the package's
# own manifest names.
LICENCES = {
    "tree-sitter-dockerfile": "https://raw.githubusercontent.com/camdencheek/tree-sitter-dockerfile/main/LICENSE",
    "tree-sitter-kotlin": "https://raw.githubusercontent.com/fwcd/tree-sitter-kotlin/main/LICENSE",
}

# (crate, version, what to take)
PARSERS = [
    ("tree-sitter-dockerfile", "0.2.0"),
    ("tree-sitter-kotlin", "0.3.5"),
]
QUERY_ONLY = [
    ("tree-sitter-proto", "0.6.0", "queries/highlights.scm", "proto"),
]

# A query with no package to take it from. Pinned to a commit rather than
# to `main`: a branch is not a version, and a re-run that quietly brings a
# different query is the same trap as a `*` in a manifest.
NVIM_REF = "41416e81a6c7f4af4984395bfe0cd8d174e70976"
NVIM_RAW = f"https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/{NVIM_REF}"

# (the grammar's name here, the crate the parser comes from, its version)
QUERY_FROM_NVIM = [
    ("graphql", "tree-sitter-graphql", "0.2.1"),
    ("hcl", "tree-sitter-hcl", "1.1.0"),
]


def package(name: str, version: str) -> Path:
    """Where cargo unpacked a published crate."""
    for index in REGISTRY.iterdir():
        candidate = index / f"{name}-{version}"
        if candidate.is_dir():
            return candidate
    raise SystemExit(f"{name} {version} is not unpacked; run `cargo fetch` first")


def licence(name: str, source: Path, into: Path) -> None:
    """The licence text beside the code, from the package or from the
    repository the package names when the package ships none."""
    for file in ("LICENSE", "LICENSE-MIT", "LICENSE.md"):
        if (source / file).is_file():
            shutil.copy2(source / file, into / "LICENSE")
            return
    url = LICENCES.get(name)
    if url is None:
        raise SystemExit(f"{source}: no licence file, and no URL for one")
    text = subprocess.run(
        ["curl", "-fsSL", url], capture_output=True, text=True, check=True
    ).stdout
    kind = kind_of(text)
    if kind is None:
        raise SystemExit(f"{url}: reads as neither MIT nor Apache-2.0")
    (into / "LICENSE").write_text(text)
    declared = declared_licence(source)
    note = f"{url}\n{kind} by its text.\n"
    if declared and kind not in declared:
        note += (
            f"The crate's manifest says `license = \"{declared}\"`, which is not\n"
            f"what the repository it names carries. The text beside this file is\n"
            "the repository's, because that is the one that can be read.\n"
        )
    (into / "LICENSE.source").write_text(note)


def kind_of(text: str) -> str | None:
    """Which permissive licence a text reads as, or `None`."""
    if "MIT License" in text or "Permission is hereby granted, free of charge" in text:
        return "MIT"
    if "Apache License" in text and "Version 2.0" in text:
        return "Apache-2.0"
    return None


def declared_licence(source: Path) -> str | None:
    """What the crate's own manifest claims."""
    for line in (source / "Cargo.toml").read_text().splitlines():
        if line.startswith("license ="):
            return line.split("=", 1)[1].strip().strip('"')
    return None


def vendor_parser(name: str, version: str) -> None:
    source = package(name, version)
    into = VENDOR / name
    if into.exists():
        shutil.rmtree(into)
    (into / "src/tree_sitter").mkdir(parents=True)
    for file in ("parser.c", "scanner.c"):
        if (source / "src" / file).is_file():
            shutil.copy2(source / "src" / file, into / "src" / file)
    shutil.copy2(source / "src/tree_sitter/parser.h", into / "src/tree_sitter/parser.h")
    shutil.copy2(source / "queries/highlights.scm", into / "highlights.scm")
    licence(name, source, into)
    (into / "PROVENANCE").write_text(
        f"{name} {version}, the published crate package.\n"
        "Vendored because the crate's manifest pins an incompatible\n"
        "tree-sitter; the grammar itself is version-agnostic.\n"
        "Regenerate with scripts/vendor-grammars.py — never hand-edit.\n"
    )
    print(f"  {name}: parser vendored")


def vendor_query(name: str, version: str, inside: str, as_name: str) -> None:
    source = package(name, version)
    into = QUERIES / as_name
    into.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / inside, into / "highlights.scm")
    licence(name, source, into)
    (into / "PROVENANCE").write_text(
        f"{name} {version}, {inside}.\n"
        "Vendored because the crate ships the parser without a way to\n"
        "reach this query from Rust.\n"
        "Regenerate with scripts/vendor-grammars.py — never hand-edit.\n"
    )
    print(f"  {as_name}: query vendored")


def fetch(url: str) -> str:
    """A file from the network, or a refusal that names it."""
    result = subprocess.run(
        ["curl", "-fsSL", url], capture_output=True, text=True, check=False
    )
    if result.returncode != 0 or not result.stdout:
        raise SystemExit(f"{url}: not fetched ({result.returncode})")
    return result.stdout


def vendor_nvim_query(as_name: str, crate: str, version: str) -> None:
    """A query from nvim-treesitter, for a parser whose package carries
    none. The licence beside it is nvim-treesitter's, because the query is
    nvim-treesitter's work — the parser's own licence travels with the
    crate and is cargo's business, not ours."""
    into = QUERIES / as_name
    into.mkdir(parents=True, exist_ok=True)
    query = fetch(f"{NVIM_RAW}/runtime/queries/{as_name}/highlights.scm")
    (into / "highlights.scm").write_text(query)
    text = fetch(f"{NVIM_RAW}/LICENSE")
    kind = kind_of(text)
    if kind is None:
        raise SystemExit("nvim-treesitter's LICENSE reads as neither MIT nor Apache-2.0")
    (into / "LICENSE").write_text(text)
    (into / "PROVENANCE").write_text(
        f"nvim-treesitter {NVIM_REF}, runtime/queries/{as_name}/highlights.scm.\n"
        f"{kind}; the licence beside this file is nvim-treesitter's own.\n"
        f"The parser is the crate {crate} {version}, which ships no query\n"
        "at all — not commented out, not behind a cfg: there is no\n"
        "queries/ directory in the package.\n"
        "Not Helix's flavour of the same query: its captures are written\n"
        "in Helix's vocabulary and would degrade through paint_of_capture\n"
        "into something coarser than the table can use.\n"
        "Regenerate with scripts/vendor-grammars.py — never hand-edit.\n"
    )
    print(f"  {as_name}: query vendored from nvim-treesitter")


def main() -> None:
    print("parsers:")
    for name, version in PARSERS:
        vendor_parser(name, version)
    print("queries:")
    for name, version, inside, as_name in QUERY_ONLY:
        vendor_query(name, version, inside, as_name)
    for as_name, crate, version in QUERY_FROM_NVIM:
        vendor_nvim_query(as_name, crate, version)


if __name__ == "__main__":
    sys.exit(main())
