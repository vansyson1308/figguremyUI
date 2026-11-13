"""Tiny client for the app's JSON-lines control channel (docs/control-protocol.md).

Usage as a module: c = Ctl(port); c.call("ui.inspect", {}); c.execute("shape.rectangle", {...}).
Usage as a script: ctl.py <port> <method> ['<json params>']
"""
import json
import socket
import sys


class Ctl:
    def __init__(self, port, timeout=90):
        self.s = socket.create_connection(("127.0.0.1", int(port)), timeout=timeout)
        self.f = self.s.makefile("rw", encoding="utf-8")
        self.n = 0

    def call(self, method, params=None):
        self.n += 1
        self.f.write(json.dumps({"id": self.n, "method": method, "params": params or {}}) + "\n")
        self.f.flush()
        line = self.f.readline()
        if not line:
            raise RuntimeError("control channel closed")
        return json.loads(line)

    def execute(self, command, params=None):
        return self.call("engine.execute", {"command": command, "params": params or {}})


if __name__ == "__main__":
    port, method = sys.argv[1], sys.argv[2]
    params = json.loads(sys.argv[3]) if len(sys.argv) > 3 else {}
    out = Ctl(port).call(method, params)
    text = json.dumps(out, ensure_ascii=False)
    print(text if len(text) < 4000 else text[:4000] + " …")
