#!/usr/bin/env python3
"""A cargo build with a status row per crate, redrawn in place.

**Why not a process per crate.** The obvious dispatcher is one `cargo`
per row, and it would be a lie: cargo takes an exclusive
lock on the target directory, so N of them queue up and the table shows a
parallelism that is not happening. Giving each its own `--target-dir`
would make the parallelism real and the build far slower — every crate
would compile the whole dependency graph again.

So there is one cargo, which already compiles crates in parallel, and the
rows are drawn from its own event stream: `Compiling <name>` on stderr is
a start, a `compiler-artifact` on stdout is a finish.

**Dependencies are one row, not four hundred.** They are most of the wall
clock and none of the interest; the row carries whichever is compiling
now, so the table says what it is waiting for.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import threading
import time

TICK = 0.2
COMPILING = re.compile(r"^\s+Compiling ([A-Za-z0-9_.\-]+) v")
DEPS = "dependencies"


class Row:
    """One line of the table."""

    def __init__(self, name: str) -> None:
        self.name = name
        self.state = "queued"
        self.started: float | None = None
        self.ended: float | None = None
        self.note = ""

    def begin(self, at: float) -> None:
        if self.started is None:
            self.started = at
        self.state = "running"

    def finish(self, at: float, ok: bool = True) -> None:
        if self.state in ("done", "failed"):
            return
        self.ended = at
        self.state = "done" if ok else "failed"
        self.note = ""

    def seconds(self, now: float) -> int:
        if self.started is None:
            return 0
        return int((self.ended or now) - self.started)


def members() -> list[str]:
    """The workspace's own crates, in the order cargo lists them."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    data = json.loads(out.stdout)
    return [package["name"] for package in data["packages"]]


def render(rows: dict[str, Row], first: bool, width: int) -> None:
    now = time.monotonic()
    if not first:
        sys.stdout.write(f"\033[{len(rows)}A")

    longest = max(len(name) for name in rows)
    for row in rows.values():
        mark, shown = {
            "queued": ("  ", "   "),
            "running": (" *", f"{row.seconds(now):3d}s"),
            "done": (" ✓", f"{row.seconds(now):3d}s"),
            "failed": (" ✗", f"{row.seconds(now):3d}s"),
        }[row.state]
        line = f"{mark} {row.name:<{longest}} {row.state:<8} {shown}  {row.note}"
        sys.stdout.write(f"\033[2K{line[:width]}\n")
    sys.stdout.flush()


def main() -> int:
    argv = sys.argv[1:] or ["build", "--workspace"]
    profile = os.environ.get("BUILD_PROFILE", "dev")

    ours = members()
    rows: dict[str, Row] = {name: Row(name) for name in ours}
    rows[DEPS] = Row(DEPS)

    print(f"Build profile: {profile}")
    print(f"Building {len(ours)} crates and their dependencies")

    started = time.monotonic()
    cargo = subprocess.Popen(
        ["cargo", *argv, "--message-format", "json-render-diagnostics"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )

    lock = threading.Lock()
    trouble: list[str] = []

    def watch_stderr() -> None:
        for line in cargo.stderr:
            found = COMPILING.match(line)
            with lock:
                if found:
                    name = found.group(1)
                    if name in rows:
                        rows[name].begin(time.monotonic())
                    else:
                        rows[DEPS].begin(time.monotonic())
                        rows[DEPS].note = name
                elif line.strip() and not line.startswith(("    Finished", "   Compiling")):
                    trouble.append(line.rstrip())

    def watch_stdout() -> None:
        for line in cargo.stdout:
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if message.get("reason") != "compiler-artifact":
                continue
            name = message.get("target", {}).get("name", "")
            with lock:
                if name in rows:
                    rows[name].finish(time.monotonic())

    threads = [threading.Thread(target=f, daemon=True) for f in (watch_stderr, watch_stdout)]
    for thread in threads:
        thread.start()

    tty = sys.stdout.isatty()
    width = os.get_terminal_size().columns if tty else 200
    first = True
    # Without a terminal there is nowhere to redraw, and a dashboard that
    # prints nothing into a log file is worse than no dashboard: the run
    # looks hung. So off a terminal the same events are printed as they
    # happen, one line each.
    seen: dict[str, str] = {name: "queued" for name in rows}
    if tty:
        sys.stdout.write("\033[?25l")

    try:
        while cargo.poll() is None:
            with lock:
                if tty:
                    render(rows, first, width)
                else:
                    for row in rows.values():
                        if row.state != seen[row.name]:
                            seen[row.name] = row.state
                            print(f"[{row.name}] {row.state}", flush=True)
            first = False
            time.sleep(TICK)
        for thread in threads:
            thread.join(timeout=2)

        code = cargo.returncode
        with lock:
            for row in rows.values():
                if row.state == "running":
                    row.finish(time.monotonic(), ok=code == 0)
            if tty:
                render(rows, first, width)
            else:
                for row in rows.values():
                    if row.state != seen[row.name]:
                        print(f"[{row.name}] {row.state}", flush=True)
    finally:
        if tty:
            sys.stdout.write("\033[?25h")
            sys.stdout.flush()

    total = int(time.monotonic() - started)
    if code == 0:
        touched = sum(1 for row in rows.values() if row.started is not None)
        if touched == 0:
            print(f"\nNothing to do: everything was already built ({total}s).")
        else:
            print(f"\nAll {len(ours)} crates built in {total}s.")
    else:
        print(f"\nFailed after {total}s.")
        for line in trouble[-40:]:
            print(line)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
