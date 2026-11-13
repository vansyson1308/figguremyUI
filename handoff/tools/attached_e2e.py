"""Attached mode end to end: the real desktop app (under Xvfb), `figcraft-cli mcp --attach`, and
this MCP client. Usage: attached_e2e.py <worktree>  (writes attached_e2e.log next to this file).

Each step prints PASS/FAIL with what came back; the script goes on after a failure so every step
is reported.
"""
import json
import os
import signal
import socket
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SCRATCH = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(SCRATCH, "baseline"))
sys.path.insert(0, os.path.join(SCRATCH, "tools"))
from mcp_client import Client  # noqa: E402
from ctl import Ctl  # noqa: E402

ROOT = sys.argv[1]
APP = os.path.join(ROOT, "target/debug/figcraft")
CLI = os.path.join(ROOT, "target/debug/figcraft-cli")
RUN_APP = os.path.join(SCRATCH, "tools/run_app.sh")
LOG = open(os.path.join(HERE, "attached_e2e.log"), "w", encoding="utf-8")
results = []


def log(*parts):
    line = " ".join(str(p) for p in parts)
    print(line)
    LOG.write(line + "\n")
    LOG.flush()


def check(name, ok, detail=""):
    results.append((name, bool(ok)))
    log("PASS" if ok else "FAIL", name, "--", detail if isinstance(detail, str) else json.dumps(detail, ensure_ascii=False)[:1500])


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def start_app(port):
    out = open(os.path.join(HERE, "attached_e2e_app.log"), "ab")
    proc = subprocess.Popen(["bash", RUN_APP, str(port), APP], stdout=out, stderr=out, start_new_session=True)
    deadline = time.time() + 90
    while time.time() < deadline:
        try:
            c = Ctl(port, timeout=10)
            info = c.call("app.info", {})
            if info.get("ok"):
                return proc, c, info["result"]
        except Exception:
            time.sleep(1)
    raise RuntimeError("the app did not answer on port %d" % port)


def stop_app(proc):
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except Exception:
        pass
    try:
        proc.wait(timeout=10)
    except Exception:
        os.killpg(proc.pid, signal.SIGKILL)
    subprocess.run(["pkill", "-f", "figcraft --control"], check=False)


def structured(r):
    return (r.result.get("structuredContent") or {}).get("error") or {}


def tool_json(r):
    # The result is the first text block; `structuredContent` is the document it was about.
    j = r.json
    return j if isinstance(j, dict) else {}


port = free_port()
log("port", port)
proc, ctl, info = start_app(port)
log("app.info", json.dumps(info))
check("app.info says window and features", info.get("window") is True and {"errorDetail", "revision"} <= set(info.get("features") or []), info)
CANVAS_IF_REVISION = "canvasIfRevision" in (info.get("features") or [])
r = ctl.execute("file.new", {})
check("file.new through the control channel", r.get("ok") and "revision" in r, r)

mcp = Client([CLI, "mcp", "--attach", str(port)], client_name="Claude", stderr_path=os.path.join(HERE, "attached_e2e_mcp.stderr"))

# 1. Capabilities.
r = mcp.call("get_capabilities")
caps = tool_json(r)
feats = caps.get("features", {})
check("get_capabilities: backend attached, window, engineErrorCodes",
      r.ok and caps.get("backend") == "attached" and caps.get("window") is True and feats.get("engineErrorCodes") is True,
      {k: caps.get(k) for k in ("backend", "window", "player", "app")} | {"engineErrorCodes": feats.get("engineErrorCodes")})

# 2. Screens, an interaction, a scenario.
def node_id(r):
    j = tool_json(r)
    return j.get("id") if isinstance(j, dict) else None

home = node_id(mcp.call("create_node", {"type": "frame", "x": 0, "y": 0, "width": 360, "height": 640, "name": "Home"}))
done = node_id(mcp.call("create_node", {"type": "frame", "x": 460, "y": 0, "width": 360, "height": 640, "name": "Done"}))
go = node_id(mcp.call("create_node", {"type": "rectangle", "x": 40, "y": 520, "width": 280, "height": 56, "parent": home, "name": "Continue", "props": {"fill": "#0D99FF"}}))
check("create_node: two screens and a button", all(isinstance(v, int) for v in (home, done, go)), {"home": home, "done": done, "go": go})
r = mcp.run("proto.interaction.add", {"id": go, "trigger": {"type": "onClick"}, "action": {"type": "navigate", "dest": done}})
check("interaction: Continue navigates to Done", r.ok, r.text[:300])
r = mcp.run("proto.flow.add", {"start": home})
check("flow start on Home", r.ok, r.text[:200])
r = mcp.run("proto.scenario.add", {"name": "Continue reaches Done", "start": home, "steps": [{"click": go, "expect": {"screen": done}}]})
check("scenario added", r.ok, r.text[:300])

# 3. Verify.
r = mcp.call("check_prototype")
j = tool_json(r)
check("check_prototype", r.ok and j.get("prototyped") is True, {"prototyped": j.get("prototyped"), "problems": [p.get("rule") for p in j.get("problems", [])]})
r = mcp.call("run_scenarios")
j = tool_json(r)
check("run_scenarios: passed, simulated", r.ok and j.get("passed") is True and j.get("total") == 1 and j.get("simulated") is True,
      {k: j.get(k) for k in ("passed", "total", "failed", "simulated")})
r = mcp.call("simulate_prototype", {"start": home, "steps": [{"click": go}], "expect": {"screen": done}, "image": True})
j = tool_json(r)
img = r.images[0] if r.images else b""
if img:
    open(os.path.join(HERE, "attached_e2e_simulated.png"), "wb").write(img)
check("simulate_prototype {image: true}", r.ok and j.get("passed") is True and img[:8] == b"\x89PNG\r\n\x1a\n", {"passed": j.get("passed"), "image bytes": len(img)})

# 4. A stale ifRevision after the person's edit through the raw control channel.
r = mcp.call("inspect_document", {"depth": 0})
rev = tool_json(r).get("revision")
user = ctl.execute("node.set", {"ids": [go], "props": {"name": "Next"}})
check("the person renames the button (raw control channel, no agent)", user.get("ok"), user)
r = mcp.call("set_properties", {"ids": [go], "props": {"name": "Continue"}, "ifRevision": rev})
e = structured(r)
check("stale ifRevision refused with conflict", (not r.ok) and e.get("code") == "conflict", e)

# 5. A structured error from the app's own errorDetail.
r = mcp.call("set_properties", {"ids": [987654], "props": {"x": 1}})
e = structured(r)
check("structured error noSuchNode", (not r.ok) and e.get("code") == "noSuchNode" and e.get("node") == 987654, e)

# 6. The agent's gesture is its own step; it can't undo the person's.
r = mcp.call("pointer", {"kind": "drag", "x": 100, "y": 540, "to": [100, 500]})
check("agent pointer drag", r.ok, r.text[:300])
d = tool_json(mcp.call("inspect_document", {"depth": 0}))
steps = d.get("history", {}).get("steps", [])
check("the drag is the agent's step", steps[:1] and steps[0].get("author", "").endswith("(agent)"), steps[:3])
r = mcp.call("undo")
check("agent undoes its own drag", r.ok, r.text[:300])
seen = tool_json(mcp.call("inspect_document", {"depth": 0})).get("revision")
user = ctl.call("ui.pointer", {"kind": "drag", "x": 100, "y": 540, "to": [140, 540]})
check("the person drags (raw control channel)", user.get("ok"), user)
r = mcp.call("undo")
e = structured(r)
check("agent undo of the person's step refused with otherAuthor", (not r.ok) and e.get("code") == "otherAuthor", e)

# 6b. A gesture with the revision from before the person's drag: refused, nothing moves.
check("app.info says canvasIfRevision (the canvas methods check ifRevision)", CANVAS_IF_REVISION, info.get("features"))
raw = ctl.call("ui.pointer", {"kind": "drag", "x": 140, "y": 540, "to": [140, 400], "ifRevision": seen, "agent": "Claude"})
check("raw ui.pointer with a stale ifRevision: conflict, revision and revisionDocument in the reply",
      (not raw.get("ok")) and (raw.get("errorDetail") or {}).get("code") == "conflict" and "revision" in raw and "revisionDocument" in raw, raw)
before = tool_json(mcp.call("get_node", {"id": go}))
r = mcp.call("pointer", {"kind": "drag", "x": 140, "y": 540, "to": [140, 400], "ifRevision": seen})
e = structured(r)
after = tool_json(mcp.call("get_node", {"id": go}))
check("pointer with a stale ifRevision refused with conflict, the button didn't move",
      (not r.ok) and e.get("code") == "conflict" and e.get("expected") == seen and (before.get("x"), before.get("y")) == (after.get("x"), after.get("y")),
      {"error": e, "before": [before.get("x"), before.get("y")], "after": [after.get("x"), after.get("y")]})
now = tool_json(mcp.call("inspect_document", {"depth": 0})).get("revision")
r = mcp.call("pointer", {"kind": "click", "x": 600, "y": 300, "ifRevision": now})
check("pointer with the current ifRevision acts", r.ok, r.text[:300])

# 7. The window, as the person sees it.
shot = os.path.join(HERE, "attached_e2e_window.png")
r = mcp.call("screenshot", {"window": True})
img = r.images[0] if r.images else b""
if img:
    open(shot, "wb").write(img)
check("screenshot {window: true}", r.ok and img[:8] == b"\x89PNG\r\n\x1a\n", {"bytes": len(img), "path": shot})

# 8. Restart on the same port.
first = info.get("instance")
stop_app(proc)
time.sleep(2)
proc, ctl, info2 = start_app(port)
log("restarted app.info", json.dumps(info2))
r = mcp.call("get_capabilities")
e = structured(r)
check("first call after the restart fails once with transport / restarted",
      (not r.ok) and e.get("code") == "transport" and e.get("restarted") is True and info2.get("instance") != first, e)
r = mcp.call("get_capabilities")
caps = tool_json(r)
check("the next call works", r.ok and caps.get("backend") == "attached", {k: caps.get(k) for k in ("backend", "window", "workingDocument")})

mcp.close()
stop_app(proc)
subprocess.run(["pkill", "Xvfb"], check=False)
passed = sum(1 for _, ok in results if ok)
log("SUMMARY %d/%d passed" % (passed, len(results)))
for name, ok in results:
    if not ok:
        log("FAILED:", name)
