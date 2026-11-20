#!/usr/bin/env python3
"""PF6: save then reopen of each new example with its design context, against the same file with
the `design` key removed from document.json (made here in a temp folder with zipfile).
Usage: pf6.py CLI EXAMPLES_DIR OUT_JSON [ROUNDS] [N]
Rounds alternate with/without (fresh server each); samples pooled; mean and p95 (nearest rank)."""
import json, math, os, shutil, sys, tempfile, zipfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # mcp_client.py beside this script
from mcp_client import Client  # noqa: E402

CLI, EX, OUT = sys.argv[1:4]
ROUNDS = int(sys.argv[4]) if len(sys.argv) > 4 else 3
N = int(sys.argv[5]) if len(sys.argv) > 5 else 10
FILES = ["signup-flow", "pantry-dashboard", "bike-repair-booking"]


def p95(xs):
    s = sorted(xs)
    return s[max(0, math.ceil(0.95 * len(s)) - 1)]


def without_design(src, dst):
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "document.json":
                d = json.loads(data)
                assert "design" in d, f"{src}: no design key"
                del d["design"]
                data = json.dumps(d).encode()
            zout.writestr(item, data)


def measure(path, work):
    c = Client([CLI, "mcp"])
    try:
        r = c.call("open_file", {"path": path})
        assert r.ok, r.text
        save = [c.call("save_file", {"path": os.path.join(work, f"s{i}.figcraft")}) for i in range(N)]
        assert all(x.ok for x in save), [x.text for x in save if not x.ok][:1]
        reopen = [c.call("open_file", {"path": os.path.join(work, f"s{i}.figcraft")}) for i in range(N)]
        assert all(x.ok for x in reopen), [x.text for x in reopen if not x.ok][:1]
        return [x.ms for x in save], [x.ms for x in reopen]
    finally:
        c.close()


pooled = {}
load = [open("/proc/loadavg").read().strip()]
with tempfile.TemporaryDirectory() as work:
    for name in FILES:
        with_d = os.path.join(work, name + ".figcraft")
        shutil.copy(os.path.join(EX, name + ".figcraft"), with_d)
        without_d = os.path.join(work, name + "-nodesign.figcraft")
        without_design(with_d, without_d)
    for r in range(ROUNDS):
        for name in FILES:
            for label, suffix in (("with", ""), ("without", "-nodesign")):
                d = tempfile.mkdtemp(dir=work)
                s, o = measure(os.path.join(work, name + suffix + ".figcraft"), d)
                pooled.setdefault(f"save_file {name} {label}", []).extend(s)
                pooled.setdefault(f"open_file {name} {label}", []).extend(o)
        print(f"round {r + 1}/{ROUNDS} load {open('/proc/loadavg').read().split()[0]}", flush=True)
load.append(open("/proc/loadavg").read().strip())
stats = {k: {"n": len(v), "mean_ms": sum(v) / len(v), "p95_ms": p95(v)} for k, v in pooled.items()}
json.dump({"load": load, "stats": stats, "samples": pooled}, open(OUT, "w"), indent=1)
print("load", load)
for op in ("save_file", "open_file"):
    for name in FILES:
        a, b = stats[f"{op} {name} with"], stats[f"{op} {name} without"]
        print(f"{op:9} {name:20} without {b['mean_ms']:7.2f}/{b['p95_ms']:7.2f}  with {a['mean_ms']:7.2f}/{a['p95_ms']:7.2f}"
              f"  d_mean {100 * (a['mean_ms'] / b['mean_ms'] - 1):+6.1f}%  d_p95 {100 * (a['p95_ms'] / b['p95_ms'] - 1):+6.1f}%  n={a['n']}")
