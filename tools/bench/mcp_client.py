"""Minimal MCP stdio client for `figcraft-cli mcp` (newline-delimited JSON-RPC 2.0).

Usage:
    from mcp_client import Client
    c = Client(["/path/to/figcraft-cli", "mcp"])       # spawns + initialize + notifications/initialized
    r = c.call("create_node", {"type": "frame", "width": 100, "height": 100})
    r.ok        -> bool (not isError and no JSON-RPC error)
    r.text      -> first text block (str)
    r.json      -> first text block parsed as JSON (or None)
    r.images    -> list of PNG/JPG bytes
    r.raw       -> the whole JSON-RPC reply
    r.ms        -> wall time of the round trip in milliseconds
    c.cmd("node.set", {...})   -> shorthand for run_command
    c.close()

Only the Python standard library is used.
"""
import base64
import json
import subprocess
import threading
import time


class Result:
    def __init__(self, raw, ms):
        self.raw = raw
        self.ms = ms
        self.rpc_error = raw.get("error")
        res = raw.get("result") or {}
        self.result = res
        self.content = res.get("content", []) if isinstance(res, dict) else []
        self.is_error = bool(res.get("isError")) if isinstance(res, dict) else False
        self.ok = self.rpc_error is None and not self.is_error

    @property
    def text(self):
        if self.rpc_error is not None:
            return "RPC ERROR %s: %s" % (self.rpc_error.get("code"), self.rpc_error.get("message"))
        for c in self.content:
            if c.get("type") == "text":
                return c.get("text", "")
        return ""

    @property
    def texts(self):
        return [c.get("text", "") for c in self.content if c.get("type") == "text"]

    @property
    def json(self):
        for t in self.texts:
            try:
                return json.loads(t)
            except Exception:
                continue
        return None

    @property
    def images(self):
        return [base64.b64decode(c["data"]) for c in self.content if c.get("type") == "image"]

    def __repr__(self):
        t = self.text
        return "<Result ok=%s %.1fms %s>" % (self.ok, self.ms, (t[:300] + "...") if len(t) > 300 else t)


class Client:
    def __init__(self, cmd, env=None, cwd=None, init=True, client_name="baseline-probe", protocol="2025-06-18", stderr_path=None):
        self.cmd = cmd
        self._stderr = open(stderr_path, "ab") if stderr_path else subprocess.DEVNULL
        self.proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self._stderr, env=env, cwd=cwd)
        self._id = 0
        self._lock = threading.Lock()
        self.init_result = None
        if init:
            self.init_result = self.request("initialize", {
                "protocolVersion": protocol,
                "capabilities": {},
                "clientInfo": {"name": client_name, "version": "0"},
            })
            self.notify("notifications/initialized")

    # ---- raw JSON-RPC ----
    def send_line(self, line):
        self.proc.stdin.write(line.encode("utf-8") + b"\n")
        self.proc.stdin.flush()

    def read_line(self):
        line = self.proc.stdout.readline()
        if not line:
            raise EOFError("server closed stdout (exit code %s)" % self.proc.poll())
        return json.loads(line)

    def request(self, method, params=None):
        with self._lock:
            self._id += 1
            msg = {"jsonrpc": "2.0", "id": self._id, "method": method}
            if params is not None:
                msg["params"] = params
            t0 = time.perf_counter()
            self.send_line(json.dumps(msg))
            raw = self.read_line()
            ms = (time.perf_counter() - t0) * 1000.0
            return Result(raw, ms)

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self.send_line(json.dumps(msg))

    # ---- MCP ----
    def call(self, tool, args=None):
        return self.request("tools/call", {"name": tool, "arguments": args or {}})

    def cmd_(self, command, params=None, document=None):
        a = {"command": command, "params": params or {}}
        if document is not None:
            a["document"] = document
        return self.call("run_command", a)

    # friendlier alias
    def run(self, command, params=None, document=None):
        return self.cmd_(command, params, document)

    def tools(self):
        return self.request("tools/list").result.get("tools", [])

    def resources(self):
        return self.request("resources/list").result

    def resource_templates(self):
        return self.request("resources/templates/list").result

    def prompts(self):
        return self.request("prompts/list").result

    def read_resource(self, uri):
        return self.request("resources/read", {"uri": uri})

    def close(self, timeout=5):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=timeout)
        except Exception:
            self.proc.kill()
        if self._stderr not in (None, subprocess.DEVNULL):
            self._stderr.close()
        return self.proc.returncode

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.close()


def strip_volatile(v, keys=("dirty", "path", "history", "selection", "id_document", "name_document")):
    """Recursively drop keys that legitimately differ between sessions (helper for diffs)."""
    if isinstance(v, dict):
        return {k: strip_volatile(x, keys) for k, x in v.items() if k not in keys}
    if isinstance(v, list):
        return [strip_volatile(x, keys) for x in v]
    return v


def deep_diff(a, b, path="", out=None, limit=200):
    """List of human-readable differences between two JSON values."""
    if out is None:
        out = []
    if len(out) >= limit:
        return out
    if type(a) != type(b):
        out.append("%s: type %s vs %s (%r vs %r)" % (path, type(a).__name__, type(b).__name__, _short(a), _short(b)))
    elif isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a:
                out.append("%s/%s: only in B = %s" % (path, k, _short(b[k])))
            elif k not in b:
                out.append("%s/%s: only in A = %s" % (path, k, _short(a[k])))
            else:
                deep_diff(a[k], b[k], path + "/" + k, out, limit)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append("%s: list length %d vs %d" % (path, len(a), len(b)))
        for i, (x, y) in enumerate(zip(a, b)):
            deep_diff(x, y, "%s[%d]" % (path, i), out, limit)
    elif isinstance(a, float) or isinstance(b, float):
        if abs(float(a) - float(b)) > 1e-9:
            out.append("%s: %r vs %r" % (path, a, b))
    elif a != b:
        out.append("%s: %r vs %r" % (path, _short(a), _short(b)))
    return out


def _short(v, n=120):
    s = json.dumps(v, ensure_ascii=False) if not isinstance(v, str) else v
    return s if len(s) <= n else s[:n] + "..."
