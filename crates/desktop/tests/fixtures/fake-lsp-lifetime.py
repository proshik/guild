#!/usr/bin/env python3
"""Bounded LSP with a lifetime socket; ignores stdin EOF until owner cleanup."""
import argparse
import json
import os
import signal
import socket
import stat
import subprocess
import sys
import threading

def devices():
    found = []
    for name in os.listdir("/dev/fd"):
        try:
            fd = int(name)
            info = os.fstat(fd)
            if fd > 2 and stat.S_ISCHR(info.st_mode):
                found.append(info.st_rdev)
        except (ValueError, OSError):
            pass
    return sorted(set(found))

def watch_lifetime(lifetime):
    # Closing the test's socket is failure cleanup, not the tested owner stop.
    lifetime.recv(1)
    os._exit(0)

def send(message):
    body = json.dumps(message).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    sys.stdout.buffer.flush()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("socket")
    parser.add_argument("--document-events", action="store_true")
    parser.add_argument("--descendant", action="store_true")
    parser.add_argument("--child", type=int)
    parser.add_argument("--ready-fd", type=int)
    args = parser.parse_args()
    signal.alarm(30)  # Broken tests cannot leave this fixture alive indefinitely.
    if args.child is not None:
        lifetime = socket.socket(fileno=args.child)
        os.write(args.ready_fd, b"1")
        os.close(args.ready_fd)
        watch_lifetime(lifetime)
        return

    lifetime = socket.socket(socket.AF_UNIX)
    lifetime.connect(args.socket)
    descendant = None
    if args.descendant:
        read, write = os.pipe()
        descendant = subprocess.Popen(
            [sys.executable, __file__, args.socket, "--child", str(lifetime.fileno()),
             "--ready-fd", str(write)],
            pass_fds=(lifetime.fileno(), write), stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(write)
        assert os.read(read, 1) == b"1"
        os.close(read)
    lifetime.sendall(b"ready")
    threading.Thread(target=watch_lifetime, args=(lifetime,), daemon=True).start()
    opened = []
    document_events = []
    while True:
        header = b""
        while b"\r\n\r\n" not in header:
            byte = sys.stdin.buffer.read(1)
            if not byte:
                # Stubborn EOF is intentional: only ownership may end the server.
                signal.pause()
                continue
            header += byte
        length = next(int(line.split(b":", 1)[1]) for line in header.split(b"\r\n")
                      if line.lower().startswith(b"content-length:"))
        message = json.loads(sys.stdin.buffer.read(length))
        if args.document_events and message.get("method", "").startswith("textDocument/did"):
            assert len(document_events) < 64, "finite document fixture exhausted"
            document_events.append(message)
        if message.get("method") == "textDocument/didOpen":
            opened.append(message["params"]["textDocument"])
        if "id" in message and message.get("method") == "textDocument/references":
            send({"jsonrpc": "2.0", "id": message["id"], "result": [{
                "uri": message["params"]["textDocument"]["uri"],
                "range": {"start": {"line": 0, "character": 0},
                          "end": {"line": 0, "character": 1}}}]})
        elif "id" in message:
            send({"jsonrpc": "2.0", "id": message["id"], "result": {
                "capabilities": {}, "serverInfo": {"name": "lifetime-fake"},
                "pid": os.getpid(), "pgid": os.getpgrp(),
                "descendant": descendant.pid if descendant else None,
                "inheritedDevices": devices(), "opened": opened, **({"documentEvents": document_events} if args.document_events else {})}})

if __name__ == "__main__":
    main()
