#!/usr/bin/env python3
"""Run inside a terminal job; record actual input and resize events without models."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import select
import signal
import struct
import sys
import termios
import time
import tty

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=Path)
parser.add_argument("--output-delay", type=float, default=0)
args = parser.parse_args()
args.directory.mkdir(parents=True, exist_ok=True)


def event(kind, **data):
    with (args.directory / "input-events.jsonl").open("a") as log:
        log.write(json.dumps({"kind": kind, **data}, ensure_ascii=False) + "\n")


def resized(*_):
    rows, cols, _, _ = struct.unpack(
        "HHHH", fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
    )
    event("size", rows=rows, cols=cols)


original = termios.tcgetattr(0)
try:
    tty.setraw(0)
    signal.signal(signal.SIGWINCH, resized)
    time.sleep(args.output_delay)
    sys.stdout.write("x" * 1048576)
    sys.stdout.flush()
    (args.directory / "drained").write_text("1 MiB drained\n")
    sys.stdout.write(
        "\x1b[2J\x1b[H\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h"
        "\x1b[36mGuild terminal input fixture\x1b[0m\r\n"
        "Привет · 界 · arrows / Escape / mouse / paste\r\n"
        "Ctrl+C completes with exit 7.\r\n"
    )
    sys.stdout.flush()
    resized()
    event("ready")
    deadline = time.monotonic() + 600
    while time.monotonic() < deadline:
        if not select.select([0], [], [], 1)[0]:
            continue
        data = os.read(0, 4096)
        if not data:
            break
        event("input", hex=data.hex())
        if b"\x03" in data:
            break
finally:
    sys.stdout.write("\x1b[?1000l\x1b[?1002l\x1b[?1006l\x1b[?2004l")
    sys.stdout.flush()
    termios.tcsetattr(0, termios.TCSANOW, original)
sys.exit(7)
