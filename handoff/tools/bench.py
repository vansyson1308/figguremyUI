#!/usr/bin/env python3
"""Before/after MCP timings for M11 (release builds, headless), run on one machine in one session.

PF1: the operations that existed at the baseline, for the baseline CLI and the new CLI, interleaved
round by round (A, B, A, B, ...) so both see the same machine load. PF2-PF6: the new operations on
the three new example files, new CLI only.

Usage: bench.py BASELINE_CLI NEW_CLI EXAMPLES_DIR OUT_JSON [ROUNDS] [N]
Each round runs every operation N times in a fresh server process; samples from all rounds are
pooled. Mean and p95 (nearest rank) are reported separately.
"""
import json
import math
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, "/tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/baseline")
from mcp_client import Client  # noqa: E402

BASE, NEW, EX, OUT = sys.argv[1:5]
ROUNDS = int(sys.argv[5]) if len(sys.argv) > 5 else 3
N = int(sys.argv[6]) if len(sys.argv) > 6 else 10
OLD_FILE = os.path.join(EX, "ridgeline-studio-dashboard.figcraft")  # largest pre-existing example
NEW_FILES = ["signup-flow.figcraft", "pantry-dashboard.figcraft", "bike-repair-booking.figcraft"]


def p95(xs):
    s = sorted(xs)
    return s[max(0, math.ceil(0.95 * len(s)) - 1)]


def summary(xs):
    return {"n": len(xs), "mean_ms": sum(xs) / len(xs), "p95_ms": p95(xs), "median_ms": sorted(xs)[len(xs) // 2], "min_ms": min(xs), "max_ms": max(xs)}


def must(r, what):
    if not r.ok:
        raise RuntimeError(f"{what}: {r.text[:300]}")
    return r


def old_ops(cli, work):
    """The baseline's operations; returns {name: [ms...]} for N repetitions each."""
    out = {}
    c = Client([cli, "mcp"])
    try:
        f = os.path.join(work, "in.figcraft")
        shutil.copy(OLD_FILE, f)
        r = must(c.call("open_file", {"path": f}), "open")
        frame = r.json["document"]["pages"][0]["children"][0]["id"]
        out["inspect_document {outline:true}"] = [must(c.call("inspect_document", {"outline": True}), "inspect").ms for _ in range(N)]
        out["inspect_document {depth:64}"] = [must(c.call("inspect_document", {"depth": 64}), "inspect").ms for _ in range(N)]
        xs = []
        for i in range(N):
            xs.append(must(c.call("create_node", {"type": "rectangle", "x": 3000 + i * 12, "y": 0, "width": 10, "height": 10, "props": {"fill": "#3366FF"}}), "create").ms)
        out["create_node rectangle"] = xs
        rid = must(c.call("create_node", {"type": "rectangle", "x": 2800, "y": 0, "width": 10, "height": 10}), "create").json["id"]
        out["set_properties"] = [must(c.call("set_properties", {"ids": [rid], "props": {"fill": "#FF0000" if i % 2 else "#00FF00", "width": 10 + i}}), "set").ms for i in range(N)]
        steps = [{"command": "node.create", "params": {"type": "frame", "x": 5000, "y": 0, "width": 300, "height": 10, "props": {"layout": "vertical"}}, "as": "f"}]
        steps += [{"command": "node.create", "params": {"type": "rectangle", "parent": {"$ref": "f"}, "width": 20, "height": 4}} for _ in range(49)]
        out["batch 50 steps"] = [must(c.call("batch", {"commands": steps}), "batch").ms for _ in range(N)]
        out["screenshot one frame"] = [must(c.call("screenshot", {"ids": [frame]}), "shot").ms for _ in range(max(3, N // 2))]
        out["undo"] = [must(c.call("undo", {}), "undo").ms for _ in range(N)]
        save = os.path.join(work, "out.figcraft")
        xs, ys = [], []
        for i in range(N):
            xs.append(must(c.call("save_file", {"path": save + f".{i}.figcraft"}), "save").ms)
        out["save_file"] = xs
        for i in range(N):
            ys.append(must(c.call("open_file", {"path": save + f".{i}.figcraft"}), "open").ms)
        out["open_file"] = ys
    finally:
        c.close()
    return out


def new_ops(cli, work):
    out = {}
    for name in NEW_FILES:
        c = Client([cli, "mcp"])
        try:
            f = os.path.join(work, name)
            shutil.copy(os.path.join(EX, name), f)
            must(c.call("open_file", {"path": f}), "open")
            out[f"proto.check ({name})"] = [must(c.call("run_command", {"command": "proto.check", "params": {}}), "check").ms for _ in range(N)]
            out[f"proto.scenario.run, all ({name})"] = [must(c.call("run_command", {"command": "proto.scenario.run", "params": {}}), "run").ms for _ in range(N)]
            out[f"design.check, all rules ({name})"] = [
                must(c.call("run_command", {"command": "design.check", "params": {"rules": RULES}}), "dcheck").ms for _ in range(N)
            ]
            out[f"design.get ({name})"] = [must(c.call("run_command", {"command": "design.get", "params": {}}), "dget").ms for _ in range(N)]
            save = os.path.join(work, "s-" + name)
            out[f"save_file ({name})"] = [must(c.call("save_file", {"path": save + f".{i}.figcraft"}), "save").ms for i in range(N)]
            out[f"open_file ({name})"] = [must(c.call("open_file", {"path": save + f".{i}.figcraft"}), "open").ms for i in range(N)]
        finally:
            c.close()
    return out


def rules(cli):
    c = Client([cli, "mcp"])
    try:
        r = must(c.call("run_command", {"command": "design.check.rules", "params": {}}), "rules")
        return [x["id"] for x in r.json]
    finally:
        c.close()


RULES = rules(NEW)
machine = {
    "nproc": os.cpu_count(),
    "cpu": [line.split(":", 1)[1].strip() for line in open("/proc/cpuinfo") if line.startswith("model name")][0],
    "mem": subprocess.run(["free", "-g"], capture_output=True, text=True).stdout.splitlines()[1],
    "loadavg_start": open("/proc/loadavg").read().strip(),
    "python": platform.python_version(),
    "rounds": ROUNDS,
    "n_per_round": N,
    "baseline_cli": BASE,
    "new_cli": NEW,
    "old_file": OLD_FILE,
    "rules": RULES,
}
pooled = {"baseline": {}, "new": {}, "new_only": {}}
with tempfile.TemporaryDirectory() as work:
    for r in range(ROUNDS):
        for label, cli in (("baseline", BASE), ("new", NEW)):
            d = tempfile.mkdtemp(dir=work)
            for k, v in old_ops(cli, d).items():
                pooled[label].setdefault(k, []).extend(v)
        d = tempfile.mkdtemp(dir=work)
        for k, v in new_ops(NEW, d).items():
            pooled["new_only"].setdefault(k, []).extend(v)
        print(f"round {r + 1}/{ROUNDS} done, load {open('/proc/loadavg').read().split()[0]}", flush=True)
machine["loadavg_end"] = open("/proc/loadavg").read().strip()
result = {"machine": machine, "stats": {g: {k: summary(v) for k, v in d.items()} for g, d in pooled.items()}, "samples": pooled}
with open(OUT, "w") as fh:
    json.dump(result, fh, indent=1)
print(json.dumps(machine, indent=1))
print(f"{'operation':45} {'base mean':>10} {'new mean':>10} {'Δmean':>7} {'base p95':>10} {'new p95':>10} {'Δp95':>7}")
for k in pooled["baseline"]:
    b, n = result["stats"]["baseline"][k], result["stats"]["new"][k]
    print(f"{k:45} {b['mean_ms']:10.2f} {n['mean_ms']:10.2f} {100 * (n['mean_ms'] / b['mean_ms'] - 1):6.1f}% {b['p95_ms']:10.2f} {n['p95_ms']:10.2f} {100 * (n['p95_ms'] / b['p95_ms'] - 1):6.1f}%")
print()
for k, v in result["stats"]["new_only"].items():
    print(f"{k:60} mean {v['mean_ms']:8.2f}  p95 {v['p95_ms']:8.2f}  (n={v['n']})")
