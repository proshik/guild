"""Finite raw PTY native input fixture; no providers, only marked owned state."""
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import termios
import time
import tty

root = Path(os.environ['GUILD_STATE_DIR'])
assert root.is_absolute()
assert (root / '.owned-runtime-input').read_text() == 'Guild owned transport input\n'
signal.alarm(60)
original = termios.tcgetattr(0)
data = bytearray()
try:
    tty.setraw(0)
    os.write(1, b'\x1b[?2004h\x1b[2J\x1b[HOwned transport input fixture\r\n'
             + 'Unicode paste / arrows / Enter / Ctrl-C\r\nЖ · 界 · 🦀\r\n'.encode())
    (root / 'input-ready').write_text(str(os.getpid()))
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        if not select.select([0], [], [], .2)[0]:
            continue
        chunk = os.read(0, 65536)
        if not chunk:
            break
        if len(data) + len(chunk) > 1024 * 1024:
            raise RuntimeError('owned native fixture input limit')
        data.extend(chunk)
        (root / 'input-captured.bin').write_bytes(data)
        (root / 'input-receipt.json').write_text(json.dumps(dict(
            bytes=len(data), sha256=hashlib.sha256(data).hexdigest())))
        os.write(1, f'\r\nACK {len(data)} input bytes\r\n'.encode())
        if b'\x03' in chunk:
            break
finally:
    os.write(1, b'\x1b[?2004l')
    termios.tcsetattr(0, termios.TCSANOW, original)
