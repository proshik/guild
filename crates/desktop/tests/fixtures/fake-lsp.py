#!/usr/bin/env python3
"""A language server that answers, so the client can be tested without one.

M8.5's tests need a server that starts in milliseconds, has no opinion
about Rust and cannot index a workspace for two minutes. This one speaks
the framing and answers the six messages the client sends, with fixed
answers a test can assert on.

What it does:
  initialize            -> capabilities, and the name "fake"
  initialized           -> nothing (a notification)
  textDocument/didOpen  -> publishDiagnostics: one error per line
                           containing the word BAD, at that word
  textDocument/didChange-> the same, against the new text
  textDocument/hover    -> "hover at L:C"
  textDocument/definition -> a link to definition.rs, line 4
  textDocument/completion -> two items: `alpha` and `beta`
  shutdown / exit       -> stops
Anything else is answered with an empty result, so a client that sends
more than this does not hang waiting for a reply that never comes.
"""

import json
import sys

texts = {}


def send(message):
    body = json.dumps(message).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    sys.stdout.buffer.flush()


def diagnostics_for(uri, version):
    found = []
    for line, text in enumerate(texts.get(uri, "").splitlines()):
        column = text.find("BAD")
        if column >= 0:
            found.append({
                "range": {"start": {"line": line, "character": column},
                          "end": {"line": line, "character": column + 3}},
                "severity": 1,
                "source": "fake",
                "message": "BAD is not allowed here",
            })
    send({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
          "params": {"uri": uri, "version": version, "diagnostics": found}})


def main():
    while True:
        header = b""
        while b"\r\n\r\n" not in header:
            byte = sys.stdin.buffer.read(1)
            if not byte:
                return
            header += byte
        length = 0
        for line in header.decode().split("\r\n"):
            if line.lower().startswith("content-length"):
                length = int(line.split(":")[1])
        message = json.loads(sys.stdin.buffer.read(length))

        method, id = message.get("method"), message.get("id")
        params = message.get("params", {})

        if method == "initialize":
            send({"jsonrpc": "2.0", "id": id, "result": {
                "capabilities": {
                    "textDocumentSync": 1,
                    "hoverProvider": True,
                    "definitionProvider": True,
                    "completionProvider": {"triggerCharacters": ["."]},
                },
                "serverInfo": {"name": "fake", "version": "1"},
            }})
        elif method == "textDocument/didOpen":
            document = params["textDocument"]
            texts[document["uri"]] = document["text"]
            diagnostics_for(document["uri"], document.get("version", 0))
        elif method == "textDocument/didChange":
            document = params["textDocument"]
            texts[document["uri"]] = params["contentChanges"][-1]["text"]
            diagnostics_for(document["uri"], document.get("version", 0))
        elif method == "textDocument/didClose":
            texts.pop(params["textDocument"]["uri"], None)
        elif method == "textDocument/hover":
            at = params["position"]
            send({"jsonrpc": "2.0", "id": id, "result": {
                "contents": {"kind": "plaintext",
                             "value": f"hover at {at['line']}:{at['character']}"}}})
        elif method == "textDocument/definition":
            send({"jsonrpc": "2.0", "id": id, "result": [{
                "targetUri": "file:///definition.rs",
                "targetRange": {"start": {"line": 4, "character": 0},
                                "end": {"line": 4, "character": 5}},
                "targetSelectionRange": {"start": {"line": 4, "character": 0},
                                         "end": {"line": 4, "character": 5}}}]})
        elif method == "textDocument/completion":
            send({"jsonrpc": "2.0", "id": id, "result": {
                "isIncomplete": False,
                "items": [{"label": "alpha", "kind": 3}, {"label": "beta", "kind": 3}]}})
        elif method == "shutdown":
            send({"jsonrpc": "2.0", "id": id, "result": None})
        elif method == "exit":
            return
        elif id is not None:
            # Never leave a request unanswered: a client waiting on an id
            # that will not come is the bug this fixture exists to rule out.
            send({"jsonrpc": "2.0", "id": id, "result": None})


if __name__ == "__main__":
    main()
