#!/usr/bin/env python3
"""Build the example prototypes from nothing, through the headless MCP server.

    python3 examples/build/build_examples.py --cli target/debug/figcraft-cli --out examples
    python3 examples/build/build_examples.py --cli ... --out /tmp/x --only pantry-dashboard

Each example is one function below (`signup_flow`, `pantry_dashboard`, `bike_repair_booking`) that
starts `figcraft-cli --no-system-fonts mcp` (built-in fonts only, so the layout is the same on
every machine), builds the file with `run_command`, `batch`, `create_node` and `set_properties` as
an AI agent would, checks it, and saves it. The same binary gives the same bytes: nothing here
reads the clock, random numbers or the machine's fonts.

Over MCP every command runs as the client's AI agent ("Example builder (agent)"), so the end
states it declares and the decisions it records are *proposals* that wait for a person. The
separate step `accept_as_maintainer()` (on by default; `--no-accept-as-maintainer` leaves the
proposals in place) then runs `figcraft-cli run`, which acts as the user, to accept them. The
maintainers' acceptance step stands in for a person's review of these example files; in your own
files an agent's end states and decisions stay proposals until you accept them.

Any tool error stops the build with a non-zero exit, naming the call and the error. Standard
library only.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile

CLIENT = {"name": "examples-build", "title": "Example builder", "version": "1"}


class BuildError(Exception):
    pass


# ---------------------------------------------------------------------------------------------
# MCP stdio client (newline-delimited JSON-RPC 2.0)
# ---------------------------------------------------------------------------------------------


class Mcp:
    def __init__(self, cli):
        self.proc = subprocess.Popen(
            [cli, "--no-system-fonts", "mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.next_id = 0
        self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": CLIENT})
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _send(self, msg):
        self.proc.stdin.write(json.dumps(msg, ensure_ascii=False).encode("utf-8") + b"\n")
        self.proc.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self._send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        line = self.proc.stdout.readline()
        if not line:
            raise BuildError("the MCP server closed its output during %s (exit code %s)" % (method, self.proc.poll()))
        reply = json.loads(line)
        if "error" in reply:
            raise BuildError("%s: JSON-RPC error %s" % (method, json.dumps(reply["error"], ensure_ascii=False)))
        return reply.get("result", {})

    def call(self, tool, args):
        """A tool call that must succeed: the first text block, read as JSON when it is JSON."""
        res = self.request("tools/call", {"name": tool, "arguments": args})
        texts = [c.get("text", "") for c in res.get("content", []) if c.get("type") == "text"]
        if res.get("isError"):
            raise BuildError("%s %s\n  failed: %s" % (tool, json.dumps(args, ensure_ascii=False)[:1500], texts[0] if texts else res))
        if not texts:
            return res
        try:
            return json.loads(texts[0])
        except ValueError:
            return texts[0]

    def run(self, command, params=None):
        return self.call("run_command", {"command": command, "params": params or {}})

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=30)
        except Exception:
            self.proc.kill()


# ---------------------------------------------------------------------------------------------
# A small vocabulary over the commands: theme variables, text styles, layout frames and texts
# ---------------------------------------------------------------------------------------------

# Colours, per mode. Text colours pass WCAG AA (4.5:1) on the surfaces they are used on:
# text on surface 15.6:1, muted on surface 6.9:1 (6.4:1 on canvas), on_accent on accent >= 5.8:1,
# danger on surface 6.5:1 (5.5:1 on danger_soft), success on success_soft 4.65:1,
# muted on disabled 5.4:1.
BASE_THEME = {
    "canvas": "#F4F6F8",
    "surface": "#FFFFFF",
    "text": "#1B2430",
    "muted": "#4D5B6A",
    "border": "#8A96A3",
    "divider": "#D5DBE1",
    "on_accent": "#FFFFFF",
    "danger": "#B3261E",
    "danger_soft": "#FCE8E6",
    "success": "#1E7B45",
    "success_soft": "#E6F4EA",
    "disabled": "#DDE3E8",
    "scrim": "#1B243066",
}

TEXT_STYLES = {
    # name: (size, style, line height)
    "Display": (28, "Bold", 34),
    "Title": (22, "Bold", 28),
    "Heading": (17, "SemiBold", 24),
    "Body": (15, "Regular", 22),
    "Label": (14, "SemiBold", 20),
    "Caption": (13, "Regular", 18),
}
FONT = "Inter"


class Builder:
    def __init__(self, mcp, accent):
        self.m = mcp
        self.styles = {}
        theme = dict(BASE_THEME)
        theme.update(accent)
        self.theme = theme
        # What the agent proposed and the maintainers' step accepts (see accept_as_maintainer).
        self.proposed_end_states = []
        self.proposed_decisions = []

    # -- setup ------------------------------------------------------------------------------
    def setup_theme(self):
        styles = self.m.run("text.fonts", {"family": FONT})
        for _, (_, style, _) in TEXT_STYLES.items():
            if style not in styles:
                raise BuildError("this figcraft-cli has no %s %s: build it with craft-fonts (CRAFT_FONTS_DIR), "
                                 "or the text would lay out in nothing" % (FONT, style))
        self.m.call("batch", {"label": "Theme colours", "commands": [
            {"command": "variable.createCollection", "params": {"name": "Theme", "modes": ["Light"]}}] + [
            {"command": "variable.create", "params": {"collection": "Theme", "name": name, "type": "color", "value": self.theme[name]}}
            for name in sorted(self.theme)]})
        for name, (size, style, lh) in TEXT_STYLES.items():
            r = self.m.run(
                "style.create",
                {"type": "text", "name": name, "value": {"fontFamily": FONT, "fontStyle": style, "fontSize": size, "lineHeight": lh}},
            )
            self.styles[name] = r["id"] if isinstance(r, dict) and "id" in r else r

    def collection(self, name, variables):
        """A collection of prototype state: [(name, type, value)]."""
        self.m.call("batch", {"label": name + " variables", "commands": [
            {"command": "variable.createCollection", "params": {"name": name, "modes": ["Default"]}}] + [
            {"command": "variable.create", "params": {"collection": name, "name": vname, "type": vtype, "value": value}}
            for vname, vtype, value in variables]})

    # -- layers -----------------------------------------------------------------------------
    def node(self, kind, parent=None, name=None, props=None, **kw):
        args = {"type": kind}
        if parent is not None:
            args["parent"] = parent
        if name is not None:
            args["name"] = name
        args.update(kw)
        if props:
            args["props"] = props
        return self.m.call("create_node", args)["id"]

    def frame(self, parent=None, name="Frame", layout=None, gap=None, padding=None, fill=None, stroke=None, radius=None,
              w=None, h=None, x=None, y=None, sizing=None, align=None, cross=None, wrap=None, cross_gap=None, clip=None,
              extra=None):
        props = {}
        if fill is not None:
            props["fill"] = {"$var": "Theme/" + fill}
        else:
            props["fill"] = "none"
        if stroke is not None:
            props["stroke"] = {"$var": "Theme/" + stroke}
            props["strokeWeight"] = 1
            props["strokeAlign"] = "inside"
        if radius is not None:
            props["cornerRadius"] = radius
        if layout is not None:
            props["layout"] = layout
            if gap is not None:
                props["gap"] = gap
            if padding is not None:
                props["padding"] = padding
            if align is not None:
                props["mainAlign"] = align
            if cross is not None:
                props["crossAlign"] = cross
            if wrap is not None:
                props["wrap"] = wrap
            if cross_gap is not None:
                props["crossGap"] = cross_gap
        if clip is not None:
            props["clip"] = clip
        kw = {}
        for k, v in (("x", x), ("y", y), ("width", w), ("height", h)):
            if v is not None:
                kw[k] = v
        nid = self.node("frame", parent, name, props, **kw)
        later = {}
        if sizing is not None:
            later["sizingH"], later["sizingV"] = sizing
        if extra:
            later.update(extra)
        if later:
            self.set(nid, later)
        return nid

    def text(self, parent, chars, style="Body", color="text", name=None, fill_width=False, align=None, extra=None):
        props = {"textStyle": self.styles[style], "fill": {"$var": "Theme/" + color}}
        nid = self.node("text", parent, name or chars[:40], props, text=chars)
        later = {}
        if fill_width:
            later["sizingH"] = "fill"
            later["textAutoResize"] = "height"
        if align is not None:
            later["textAlign"] = align
        if extra:
            later.update(extra)
        if later:
            self.set(nid, later)
        return nid

    def set(self, ids, props):
        if not isinstance(ids, list):
            ids = [ids]
        return self.m.call("set_properties", {"ids": ids, "props": props})

    def bind(self, ids, field, variable):
        if not isinstance(ids, list):
            ids = [ids]
        return self.m.run("variable.bind", {"ids": ids, "field": field, "variable": variable})

    def instance(self, component, parent, name=None, props=None, sizing=None):
        nid = self.m.run("instance.create", {"component": component, "parent": parent})["id"]
        later = {}
        if name:
            later["name"] = name
        if sizing:
            later["sizingH"], later["sizingV"] = sizing
        if later:
            self.set(nid, later)
        if props:
            self.m.run("instance.setProperties", {"ids": [nid], "properties": props})
        return nid

    def child(self, root, name):
        """The id of the first layer called `name` under `root` (instance children included)."""
        r = self.m.run("node.find", {"root": root, "name": name, "limit": 50})
        for i in r["ids"]:
            n = self.m.call("get_node", {"id": i, "depth": 0, "fields": ["name"]})
            if n.get("name") == name:
                return i
        raise BuildError("no layer named %r under %s" % (name, root))

    def on(self, node, trigger, actions, **extra):
        trig = trigger if isinstance(trigger, dict) else {"type": trigger}
        p = {"id": node, "trigger": trig}
        if isinstance(actions, list):
            p["actions"] = actions
        else:
            p["action"] = actions
        p.update(extra)
        return self.m.run("proto.interaction.add", p)



    def override(self, inst, layer, chars):
        """Override the text of layer `layer` (by name) inside instance `inst`."""
        self.m.run("text.set", {"id": self.child(inst, layer), "characters": chars})

    def variant_set(self, name, x, y, variants, gap=40):
        """A component set `name` from `variants`: [(variant name, fn(frame id)), ...]. Each fn
        builds the variant's frame (made by `make(name, x, y)`), returning its id. Returns the set
        id and {variant name: component id}."""
        ids = []
        cx = x
        for vname, build in variants:
            fid = build(vname, cx, y)
            comp = self.m.run("component.create", {"ids": [fid]})["id"]
            ids.append(comp)
            w = self.m.call("get_node", {"id": comp, "depth": 0, "fields": ["width"]})["width"]
            cx += w + gap
        s = self.m.run("component.combineVariants", {"ids": ids})["id"]
        self.set(s, {"name": name})
        names = {}
        for comp in ids:
            names[self.m.call("get_node", {"id": comp, "depth": 0, "fields": ["name"]})["name"]] = comp
        return s, names

    # -- design context and flow ------------------------------------------------------------
    def end_state(self, frame, reason):
        r = self.m.run("proto.endState.set", {"id": frame, "reason": reason})
        if r.get("status") == "proposed":
            self.proposed_end_states.append(frame)

    def decision(self, **kw):
        r = self.m.run("design.decision.add", kw)
        if r.get("status") == "proposed":
            self.proposed_decisions.append(r["id"])
        return r["id"]

    def assumption(self, text, links):
        return self.m.run("design.assumption.add", {"text": text, "links": links})["id"]

    def scenario(self, name, start, purpose, steps, expect=None):
        p = {"name": name, "start": start, "purpose": purpose, "steps": steps}
        if expect is not None:
            p["expect"] = expect
        return self.m.run("proto.scenario.add", p)["id"]


def set_var(variable, value):
    return {"type": "setVariable", "variable": variable, "value": value}


def set_expr(variable, expression):
    return {"type": "setVariable", "variable": variable, "expression": expression}


def nav(dest):
    return {"type": "navigate", "dest": dest}


def back():
    return {"type": "back"}


def when(*blocks):
    """A conditional: blocks of (expression, [actions]); expression None is the else."""
    out = []
    for cond, acts in blocks:
        b = {"actions": acts}
        if cond is not None:
            b["expression"] = cond
        out.append(b)
    return {"type": "conditional", "blocks": out}


def click(node, note=None, expect=None, gesture=None):
    s = {"type": "gesture", "node": node}
    if gesture:
        s["gesture"] = gesture
    if note:
        s["note"] = note
    if expect is not None:
        s["expect"] = expect
    return s


def typing(variable, value, note, expect=None):
    s = {"type": "setVariable", "variable": variable, "value": value, "note": note}
    if expect is not None:
        s["expect"] = expect
    return s


def wait(note=None, expect=None):
    s = {"type": "wait"}
    if note:
        s["note"] = note
    if expect is not None:
        s["expect"] = expect
    return s


# ---------------------------------------------------------------------------------------------
# Components the examples share (each file gets its own copy, built from scratch)
# ---------------------------------------------------------------------------------------------


def make_button(b, x, y, width=342, name="Button", hug=False):
    """Button: State=Default|Pressed|Disabled. Default shows Pressed while pressed. `hug`: as wide
    as its label (desktop toolbars), else `width` wide (phone screens stretch it)."""

    def variant(fill, color):
        def build(vname, cx, cy):
            pad = [12, 16, 12, 16] if hug else [15, 20, 15, 20]
            f = b.frame(name=vname, layout="horizontal", padding=pad, fill=fill, radius=12 if not hug else 10, w=width, h=52,
                        x=cx, y=cy, align="center", cross="center", sizing=("hug" if hug else "fixed", "hug"),
                        extra={"minHeight": 44 if hug else 52, "minWidth": 44})
            b.text(f, "Button", "Label", color, name="label", align="center")
            return f
        return build

    s, v = b.variant_set(name, x, y, [
        ("State=Default", variant("accent", "on_accent")),
        ("State=Pressed", variant("accent_pressed", "on_accent")),
        ("State=Disabled", variant("disabled", "muted")),
    ])
    b.on(v["State=Default"], "onPress", {"type": "changeTo", "dest": v["State=Pressed"]})
    return s, v


def make_text_button(b, x, y):
    """Text button: a link-like action with a 44 px tall target. No left padding, so its text lines
    up with the text above and below it."""
    f = b.frame(name="Text button", layout="horizontal", padding=[12, 12, 12, 0], w=80, h=44, x=x, y=y,
                align="center", cross="center", sizing=("hug", "hug"), extra={"minHeight": 44, "minWidth": 44})
    b.text(f, "Action", "Label", "accent", name="label")
    return b.m.run("component.create", {"ids": [f]})["id"]


def make_field(b, x, y, width=342):
    """Field: State=Empty|Filled|Error, with a label, a placeholder, a value and an error message."""

    def variant(kind):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="vertical", gap=6, w=width, h=100, x=cx, y=cy, sizing=("fixed", "hug"))
            b.text(f, "Label", "Label", "text", name="label", fill_width=True)
            box = b.frame(f, "box", layout="horizontal", padding=[13, 14, 13, 14], fill="surface",
                          stroke="danger" if kind == "Error" else "border", radius=10, cross="center",
                          sizing=("fill", "hug"), extra={"minHeight": 48, "strokeWeight": 2 if kind == "Error" else 1})
            ph = b.text(box, "Placeholder", "Body", "muted", name="placeholder", fill_width=True)
            val = b.text(box, "Value", "Body", "text", name="value", fill_width=True)
            err = b.text(f, "Error message", "Caption", "danger", name="error", fill_width=True)
            if kind == "Filled":
                b.set([ph, err], {"visible": False})
            elif kind == "Error":
                b.set([val], {"visible": False})
            else:
                b.set([val, err], {"visible": False})
            return f
        return build

    return b.variant_set("Field", x, y, [("State=" + k, variant(k)) for k in ("Empty", "Filled", "Error")])


def make_checkbox(b, x, y):
    """Checkbox: Checked=No|Yes, a 24 px box in the top-left corner of a 44 px target (level with the
    first line of a label beside it, and with the left edge of the fields above it)."""

    def variant(on):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="horizontal", w=44, h=44, x=cx, y=cy, align="min", cross="min")
            box = b.frame(f, "box", fill="accent" if on else "surface", stroke=None if on else "border", radius=6, w=24, h=24,
                          extra={"strokeWeight": 2} if not on else None)
            if on:
                b.node("vector", box, "tick", {"fill": "none", "stroke": {"$var": "Theme/on_accent"}, "strokeWeight": 2.5,
                                                "strokeCap": "round", "strokeJoin": "round"},
                       path="M6 12.5 L10.5 17 L18 8", x=0, y=0)
            return f
        return build

    return b.variant_set("Checkbox", x, y, [("Checked=No", variant(False)), ("Checked=Yes", variant(True))])


def make_chip(b, x, y, name="Chip", width=110):
    """A choice: Selected=No|Yes (segments, date and time chips, filters)."""

    def variant(on):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="horizontal", padding=[12, 12, 12, 12], fill="accent" if on else "surface",
                        stroke=None if on else "border", radius=10, w=width, h=44, x=cx, y=cy, align="center", cross="center",
                        sizing=("fixed", "hug"), extra={"minHeight": 44})
            b.text(f, "Option", "Label", "on_accent" if on else "text", name="label", align="center")
            return f
        return build

    return b.variant_set(name, x, y, [("Selected=No", variant(False)), ("Selected=Yes", variant(True))])


# ---------------------------------------------------------------------------------------------
# 1. Sign-up flow: "Lớp bơi cộng đồng: đăng ký"
# ---------------------------------------------------------------------------------------------

SIGNUP_PARAGRAPH = (
    "Khóa học dành cho người mới bắt đầu từ sáu tuổi trở lên, học tại bể bơi của phường vào các buổi tối "
    "trong tuần. Mỗi buổi kéo dài bốn mươi lăm phút, có huấn luyện viên hướng dẫn và nhân viên cứu hộ trực "
    "suốt giờ học. Đăng ký chỉ mất khoảng hai phút; sau đó lớp sẽ gửi mã xác nhận đến số điện thoại hoặc "
    "email của bạn."
)
CONSENT = (
    "Tôi đồng ý để lớp bơi liên lạc với tôi qua số điện thoại hoặc email ở trên về lịch học, và tôi đã đọc "
    "nội quy an toàn của bể bơi cộng đồng."
)


def signup_flow(b):
    m = b.m
    m.run("page.rename", {"name": "Sign-up flow"})
    b.setup_theme()
    b.collection("Sign-up", [
        ("name_filled", "boolean", False),
        ("contact_filled", "boolean", False),
        ("consent", "boolean", False),
        ("needs_consent", "boolean", True),
        ("attempts", "number", 0),
        ("error_message", "string", ""),
        ("show_error", "boolean", False),
        ("name_state", "string", "Empty"),
        ("contact_state", "string", "Empty"),
        ("code_filled", "boolean", False),
        ("code_state", "string", "Empty"),
        ("age_group", "string", "6–11 tuổi"),
        ("age_child", "string", "Yes"),
        ("age_teen", "string", "No"),
        ("age_adult", "string", "No"),
        ("consent_box", "string", "No"),
    ])
    m.run("proto.settings", {"device": "iPhone 16"})

    # Components, in a row under the screens.
    button, bv = make_button(b, 0, 1000)
    text_button = make_text_button(b, 1240, 1000)
    field, fv = make_field(b, 0, 1120)
    checkbox, cv = make_checkbox(b, 1240, 1120)
    chip, chv = make_chip(b, 1460, 1120, name="Segment")

    W, H = 390, 844
    xs = [0, 470, 940, 1410]

    def screen(name, x, padding=(24, 24, 32, 24)):
        return b.frame(name=name, layout="vertical", gap=24, padding=list(padding), fill="surface", w=W, h=H, x=x, y=0,
                       align="spaceBetween", clip=True)

    def column(parent, name, gap=16):
        return b.frame(parent, name, layout="vertical", gap=gap, sizing=("fill", "hug"))

    def btn(parent, label, name, state=None):
        i = b.instance(button, parent, name=name, sizing=("fill", "hug"), props={"State": state} if state else None)
        b.override(i, "label", label)
        return i

    def link(parent, label, name):
        i = b.instance(text_button, parent, name=name)
        b.override(i, "label", label)
        return i

    def a_field(parent, name, label, placeholder, value, error, state_var):
        i = b.instance(field, parent, name=name, sizing=("fill", "hug"))
        b.override(i, "label", label)
        b.override(i, "placeholder", placeholder)
        b.override(i, "value", value)
        b.override(i, "error", error)
        b.bind(i, "componentProperties/State", state_var)
        return i

    # -- Welcome
    welcome = screen("Welcome", xs[0], padding=(56, 24, 32, 24))
    intro = column(welcome, "Intro")
    b.text(intro, "Phường Sông Mây · Khóa học mùa hè", "Caption", "muted", name="Eyebrow", fill_width=True)
    b.text(intro, "Lớp bơi cộng đồng", "Display", "text", name="Title", fill_width=True)
    b.text(intro, SIGNUP_PARAGRAPH, "Body", "text", name="About the class", fill_width=True)
    facts = b.frame(intro, "Facts", layout="vertical", gap=8, padding=[16, 16, 16, 16], fill="canvas", radius=12,
                    sizing=("fill", "hug"))
    for line in ("Hai buổi mỗi tuần: tối thứ Ba và tối thứ Năm", "Tối đa 12 học viên mỗi lớp", "Ba nhóm tuổi: 6–11, 12–17 và từ 18 tuổi"):
        b.text(facts, "• " + line, "Body", "text", name="Fact", fill_width=True)
    wfoot = column(welcome, "Footer", gap=12)
    start = btn(wfoot, "Đăng ký ngay", "Start sign-up")
    b.text(wfoot, "Bản mẫu: dữ liệu là giả lập, không có máy chủ.", "Caption", "muted", name="Prototype note",
           fill_width=True, align="center")

    # -- Sign-up form
    form = screen("Sign-up form", xs[1], padding=(12, 24, 32, 24))
    fbody = column(form, "Content")
    form_back = link(fbody, "‹ Quay lại", "Back to welcome")
    b.text(fbody, "Thông tin người học", "Title", "text", name="Title", fill_width=True)
    banner = b.frame(fbody, "Error summary", layout="horizontal", padding=[12, 14, 12, 14], fill="danger_soft", radius=10,
                     sizing=("fill", "hug"))
    err_text = b.text(banner, "Còn thiếu thông tin.", "Body", "danger", name="Error message", fill_width=True)
    b.bind(err_text, "characters", "error_message")
    b.bind(banner, "visible", "show_error")
    name_f = a_field(fbody, "Name field", "Họ và tên người học", "Chạm để nhập họ tên", "Lê Minh Khang",
                     "Vui lòng nhập họ và tên người học.", "name_state")
    contact_f = a_field(fbody, "Contact field", "Số điện thoại hoặc email", "Chạm để nhập số điện thoại hoặc email",
                        "0900 000 123", "Cần số điện thoại hoặc email để gửi mã xác nhận.", "contact_state")
    age = column(fbody, "Age group", gap=8)
    b.text(age, "Nhóm tuổi", "Label", "text", name="Label", fill_width=True)
    seg_row = b.frame(age, "Segments", layout="horizontal", gap=8, sizing=("fill", "hug"))
    segs = []
    for label, var in (("6–11 tuổi", "age_child"), ("12–17 tuổi", "age_teen"), ("Từ 18 tuổi", "age_adult")):
        i = b.instance(chip, seg_row, name="Age " + label, sizing=("fill", "hug"))
        b.override(i, "label", label)
        b.bind(i, "componentProperties/Selected", var)
        segs.append((i, label, var))
    for i, label, var in segs:
        acts = [set_var("age_group", label)] + [set_var(v, "Yes" if v == var else "No") for _, _, v in segs]
        b.on(i, "onClick", acts)
    consent_row = b.frame(fbody, "Consent", layout="horizontal", gap=0, sizing=("fill", "hug"))
    box = b.instance(checkbox, consent_row, name="Consent checkbox")
    b.bind(box, "componentProperties/Checked", "consent_box")
    b.text(consent_row, CONSENT, "Body", "text", name="Consent text", fill_width=True)
    ffoot = column(form, "Footer", gap=8)
    cont_off = btn(ffoot, "Tiếp tục", "Continue (disabled)", state="Disabled")
    cont = btn(ffoot, "Tiếp tục", "Continue")
    hint = b.text(ffoot, "Hãy đánh dấu ô đồng ý để bật nút Tiếp tục.", "Caption", "muted", name="Consent hint",
                  fill_width=True, align="center")
    b.bind(cont_off, "visible", "needs_consent")
    b.bind(hint, "visible", "needs_consent")
    b.bind(cont, "visible", "consent")

    # -- Verify code
    verify = screen("Verify code", xs[2], padding=(12, 24, 32, 24))
    vbody = column(verify, "Content")
    verify_back = link(vbody, "‹ Quay lại", "Back to form")
    b.text(vbody, "Nhập mã xác nhận", "Title", "text", name="Title", fill_width=True)
    b.text(vbody, "Mã gồm 6 chữ số đã được gửi đến số điện thoại hoặc email bạn vừa nhập. Trong bản mẫu này, chạm vào ô "
                  "để điền một mã giả lập.", "Body", "muted", name="Explanation", fill_width=True)
    code_f = a_field(vbody, "Code field", "Mã xác nhận", "Chạm để nhập mã", "482 913",
                     "Mã còn trống. Hãy nhập 6 chữ số trong tin nhắn.", "code_state")
    resend = link(vbody, "Gửi lại mã", "Resend code")
    vfoot = column(verify, "Footer", gap=8)
    confirm = btn(vfoot, "Xác nhận", "Confirm code")

    # -- Done
    done = b.frame(name="Done", layout="vertical", gap=16, padding=[120, 24, 32, 24], fill="surface", w=W, h=H, x=xs[3], y=0,
                   cross="center", clip=True)
    badge = b.frame(done, "Success badge", layout="horizontal", fill="success_soft", radius=40, w=80, h=80, align="center",
                    cross="center")
    b.node("vector", badge, "tick", {"fill": "none", "stroke": {"$var": "Theme/success"}, "strokeWeight": 4,
                                     "strokeCap": "round", "strokeJoin": "round"}, path="M4 18 L14 28 L34 6", x=0, y=0)
    b.text(done, "Đăng ký thành công!", "Title", "text", name="Title", fill_width=True, align="center")
    b.text(done, "Lớp sẽ gọi điện hoặc gửi email trước buổi học đầu tiên để xác nhận giờ học. Bạn có thể đóng trang này.",
           "Body", "text", name="Next steps", fill_width=True, align="center")
    b.text(done, "Mã đăng ký (giả lập): SM-0427", "Caption", "muted", name="Reference", fill_width=True, align="center")

    # -- Toast overlay
    toast = b.frame(name="Toast: code sent again", layout="horizontal", padding=[14, 16, 14, 16], fill="text", radius=12,
                    w=342, h=60, x=xs[3] + 470, y=0, sizing=("fixed", "hug"))
    b.text(toast, "Đã gửi lại mã, có hiệu lực trong 10 phút.", "Body", "surface", name="Message", fill_width=True)

    # -- Interactions
    b.on(start, "onClick", nav(form))
    b.on(form_back, "onClick", back())
    b.on(name_f, "onClick", [set_var("name_filled", True), set_var("name_state", "Filled")])
    b.on(contact_f, "onClick", [set_var("contact_filled", True), set_var("contact_state", "Filled")])
    b.on(box, "onClick", when(
        ("consent", [set_var("consent", False), set_var("needs_consent", True), set_var("consent_box", "No")]),
        (None, [set_var("consent", True), set_var("needs_consent", False), set_var("consent_box", "Yes")]),
    ))
    b.on(cont_off, "onClick", [set_var("show_error", True),
                               set_var("error_message", "Hãy đánh dấu ô đồng ý ở cuối trang trước khi tiếp tục.")])
    b.on(cont, "onClick", [
        when(("not name_filled", [set_var("name_state", "Error")])),
        when(("not contact_filled", [set_var("contact_state", "Error")])),
        when(
            ("name_filled and contact_filled", [set_var("show_error", False), nav(verify)]),
            (None, [
                set_expr("attempts", "attempts + 1"),
                set_var("show_error", True),
                when(
                    ("not name_filled and not contact_filled",
                     [set_var("error_message", "Còn thiếu họ tên và cách liên lạc. Hãy điền hai ô được đánh dấu đỏ.")]),
                    ("not name_filled", [set_var("error_message", "Còn thiếu họ và tên người học.")]),
                    (None, [set_var("error_message", "Còn thiếu số điện thoại hoặc email.")]),
                ),
            ]),
        ),
    ])
    b.on(verify_back, "onClick", back())
    b.on(code_f, "onClick", [set_var("code_filled", True), set_var("code_state", "Filled")])
    b.on(confirm, "onClick", when(("code_filled", [nav(done)]), (None, [set_var("code_state", "Error")])))
    b.on(resend, "onClick", {"type": "openOverlay", "dest": toast, "position": "bottomCenter", "close_on_outside": True})
    b.on(toast, {"type": "afterDelay", "ms": 2500}, {"type": "closeOverlay"})
    m.run("proto.flow.add", {"start": welcome, "name": "Đăng ký lớp bơi"})
    b.end_state(done, "The sign-up is complete: the class contacts the person, so there is nothing left to do here "
                      "and the viewer closes the page.")

    # -- Scenarios (one person, one situation each)
    fill_name = typing("name_filled", True, "Typing a name into “Họ và tên người học” (a text input in the real app; "
                                            "tapping the field fills a sample name in the prototype).")
    fill_name_state = typing("name_state", "Filled", "The field shows what was typed.")
    fill_contact = typing("contact_filled", True, "Typing a phone number into “Số điện thoại hoặc email” (a text input "
                                                  "in the real app).")
    fill_contact_state = typing("contact_state", "Filled", "The field shows what was typed.")
    b.scenario("Happy path", welcome,
               "A parent signing up a child for the first time gets from the poster's QR code to a confirmed place.",
               [
                   click(start, "Starts on the welcome screen and taps “Đăng ký ngay”.", {"screen": form}),
                   fill_name, fill_name_state, fill_contact, fill_contact_state,
                   click(segs[0][0], "Keeps the age group 6–11.", {"variables": {"age_group": "6–11 tuổi"},
                                                                    "variants": [{"instance": segs[0][0], "variant": "Yes"}]}),
                   click(box, "Ticks the consent box.", {"variables": {"consent": True}, "visible": [cont],
                                                         "hidden": [cont_off, hint]}),
                   click(cont, "Taps “Tiếp tục”.", {"screen": verify, "variables": {"attempts": 0}}),
                   typing("code_filled", True, "Types the 6-digit code from the text message (a text input in the real app)."),
                   typing("code_state", "Filled", "The code field shows the code."),
                   click(confirm, "Taps “Xác nhận”.", {"screen": done}),
               ],
               {"screen": done, "endState": True, "overlays": []})
    b.scenario("Missing contact shows an error", welcome,
               "Someone who skips the contact field is told, in words, what is missing and stays on the form.",
               [
                   click(start, "Opens the form.", {"screen": form}),
                   click(name_f, "Taps the name field (the prototype fills a sample name).",
                         {"variants": [{"instance": name_f, "variant": "Filled"}]}),
                   click(box, "Ticks the consent box.", {"variables": {"consent": True}}),
                   click(cont, "Taps “Tiếp tục” without a phone number or email.", {
                       "screen": form,
                       "variants": [{"instance": contact_f, "variant": "Error"}, {"instance": name_f, "variant": "Filled"}],
                       "visible": [err_text],
                       "text": [{"node": err_text, "equals": "Còn thiếu số điện thoại hoặc email."}],
                       "variables": {"attempts": 1, "show_error": True},
                   }),
               ],
               {"screen": form, "overlays": []})
    b.scenario("Back keeps what was typed", welcome,
               "A person who goes back from the code screen to check their number finds the form as they left it.",
               [
                   click(start, "Opens the form.", {"screen": form}),
                   click(name_f, "Taps the name field (fills a sample name)."),
                   click(contact_f, "Taps the contact field (fills a sample number)."),
                   click(segs[2][0], "Chooses “Từ 18 tuổi”: they are signing up themselves.",
                         {"variables": {"age_group": "Từ 18 tuổi"}}),
                   click(box, "Ticks the consent box."),
                   click(cont, "Taps “Tiếp tục”.", {"screen": verify}),
                   click(verify_back, "Taps “‹ Quay lại” to check the number.", {"screen": form}),
               ],
               {"screen": form,
                "variables": {"name_filled": True, "contact_filled": True, "consent": True, "age_group": "Từ 18 tuổi"},
                "variants": [{"instance": name_f, "variant": "Filled"}, {"instance": contact_f, "variant": "Filled"},
                             {"instance": segs[2][0], "variant": "Yes"}, {"instance": segs[0][0], "variant": "No"},
                             {"instance": box, "variant": "Yes"}],
                "visible": [cont], "hidden": [cont_off, banner]})
    b.scenario("Resend toast closes by itself", verify,
               "Someone whose code hasn't arrived asks for it again and sees a short confirmation that goes away on its own.",
               [
                   click(resend, "Taps “Gửi lại mã”.", {"overlays": [toast]}),
                   wait("Waits a few seconds without touching anything.", {"overlays": []}),
               ],
               {"screen": verify, "overlays": []})
    b.scenario("Continue explains itself while consent is off", form,
               "Someone who fills the form but misses the consent box learns why “Tiếp tục” doesn't go on.",
               [
                   click(name_f, "Taps the name field."),
                   click(contact_f, "Taps the contact field."),
                   click(cont_off, "Taps the greyed-out “Tiếp tục”.", {
                       "screen": form, "visible": [err_text, hint],
                       "text": [{"node": err_text, "equals": "Hãy đánh dấu ô đồng ý ở cuối trang trước khi tiếp tục."}],
                   }),
                   click(box, "Ticks the consent box.", {"visible": [cont], "hidden": [cont_off, hint]}),
                   click(cont, "Taps “Tiếp tục”.", {"screen": verify}),
               ],
               {"screen": verify})

    # -- Design context
    m.run("design.brief.set", {
        "audience": "Residents of a fictional ward (Phường Sông Mây) signing up an adult or a child for a beginners' swimming "
                    "class at the ward's community pool; many use older Android phones and rarely fill in forms online.",
        "sponsor": "The ward's sports and culture office, which wants class lists it can call back without paper forms, "
                   "and fewer people turning up without a place.",
        "job": "Put one learner's name, a way to reach them and an age group on next term's list, and know the place is held.",
        "context": "On a phone, in the evening or at the pool gate after seeing the poster; mobile data comes and goes.",
        "content": "What the class is, the three age groups, one contact for the confirmation code, consent to be contacted "
                   "and to the pool's safety rules.",
        "format": "A four-screen phone flow (390×844) opened from a QR code on the pool's poster. A prototype: the data is "
                  "simulated and there is no backend; tapping a field stands in for typing, and scenarios set the "
                  "variables typing would.",
        "tone": "Warm, plain Vietnamese; short sentences; no jargon.",
        "constraints": ["Vietnamese UI copy with diacritics", "Primary buttons at least 44×44 px",
                        "Text contrast at least WCAG AA", "No account or password"],
        "success": ["A first-time user finishes in under two minutes in a hallway test",
                    "Every missing field is named in words, not only in colour",
                    "Fewer than one in ten attempts stop at the same error twice"],
    })
    a1 = b.assumption("Most sign-ups are made by a parent for a child, so the 6–11 group is the right default.", [seg_row])
    a2 = b.assumption("One contact (a phone number or an email) is enough; nobody needs to give an address to hold a place.",
                      [contact_f])
    b.assumption("The confirmation code arrives within a minute, so a resend link with a short toast is enough help.",
                 [resend, toast])
    b.decision(title="Ask for a phone number or an email in one field",
               rationale="One box is less to skip on a small screen, and either reaches the person for the code.",
               rejected="Separate phone and email fields (two boxes, twice the errors); an account with a password "
                        "(forgotten by next term).",
               assumptions=[a2], links=[contact_f])
    b.decision(title="“Tiếp tục” looks disabled until consent is ticked, and a tap on it says why",
               rationale="The hint under the button says what unlocks it, and a tap on the greyed button names the box, so "
                         "nobody is left pressing a button that does nothing.",
               rejected="An enabled button that fails only on consent (the error comes after the effort); an inert disabled "
                        "button (taps do nothing and people don't know why).",
               links=[cont_off, cont, hint])
    b.decision(title="Default the age group to 6–11",
               rationale="If most sign-ups are for children, one tap fewer for most people; the choice stays visible.",
               rejected="No default (one more required choice and one more error).",
               assumptions=[a1], links=[seg_row])
    m.run("design.stage.set", {"stage": "converging"})
    return {"screens": [welcome, form, verify, done], "overlays": [toast]}




# ---------------------------------------------------------------------------------------------
# 2. Pantry dashboard: "Community pantry stock"
# ---------------------------------------------------------------------------------------------

NB = "\u00a0"  # a no-break space keeps a number with its unit
LONG_ITEM = ("Rolled oats, wholegrain, 1" + NB + "kg bags (donated in bulk by the Riverside Bakery co-op; repack into 500" + NB
             + "g portions before shelving)")
LOW_STOCK = [
    # (name in the list, short name for the detail, left, usual, next delivery)
    (LONG_ITEM, "Rolled oats, wholegrain", "4 bags left", "Usually 30 bags", "Next delivery: Thursday morning"),
    ("Tinned chickpeas, 400" + NB + "g", "Tinned chickpeas", "7 tins left", "Usually 60 tins", "Next delivery: none booked"),
    ("Long-life milk, 1" + NB + "L", "Long-life milk", "5 cartons left", "Usually 40 cartons", "Next delivery: Friday afternoon"),
    ("Nappies, size" + NB + "4", "Nappies, size" + NB + "4", "3 packs left", "Usually 12 packs", "Next delivery: none booked"),
    ("Rice, 2" + NB + "kg", "Rice, 2" + NB + "kg", "6 bags left", "Usually 25 bags", "Next delivery: Thursday morning"),
]
KPIS = [
    ("Items in stock", "412", "across 58 products"),
    ("Low on stock", "5", "below a week's need"),
    ("Families served this week", "138", "up 9 on last week"),
    ("Volunteers on shift today", "6", "2 more needed at 4 pm"),
]


def pantry_dashboard(b):
    m = b.m
    m.run("page.rename", {"name": "Dashboard"})
    b.setup_theme()
    b.collection("Dashboard", [
        ("status", "string", "loading"),
        ("is_loading", "boolean", True),
        ("is_ready", "boolean", False),
        ("is_empty", "boolean", False),
        ("is_error", "boolean", False),
        ("show_list", "boolean", False),
        ("filter_on", "boolean", False),
        ("filter_chip", "string", "No"),
        ("fail_next", "boolean", False),
        ("fail_switch", "string", "No"),
        ("updated_text", "string", "Syncing with the stock sheet…"),
        ("detail_name", "string", LOW_STOCK[0][1]),
        ("detail_left", "string", LOW_STOCK[0][2]),
        ("detail_usual", "string", LOW_STOCK[0][3]),
        ("detail_next", "string", LOW_STOCK[0][4]),
    ])
    m.run("proto.settings", {"device": "Desktop"})

    # Components, under the dashboard.
    button, _ = make_button(b, 0, 1400, hug=True)
    chip, _ = make_chip(b, 600, 1400, name="FilterChip", width=170)

    def toggle(on):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="horizontal", gap=10, padding=[12, 14, 12, 12], fill="surface", stroke="border",
                        radius=10, w=200, h=44, x=cx, y=cy, cross="center", sizing=("hug", "hug"), extra={"minHeight": 44})
            track = b.frame(f, "Track", layout="horizontal", padding=[2, 2, 2, 2], fill="accent" if on else "border", radius=10,
                            w=36, h=20, align="max" if on else "min", cross="center")
            b.frame(track, "Knob", fill="surface", radius=8, w=16, h=16)
            b.text(f, "Toggle", "Label", "text", name="label")
            return f
        return build

    switch, _ = b.variant_set("Toggle", 1100, 1400, [("On=No", toggle(False)), ("On=Yes", toggle(True))])

    kpi_main = b.frame(name="KpiCard", layout="vertical", gap=4, padding=[20, 20, 20, 20], fill="surface", stroke="divider",
                       radius=12, w=240, h=120, x=0, y=1560, sizing=("fixed", "hug"))
    b.text(kpi_main, "Label", "Caption", "muted", name="label", fill_width=True)
    b.text(kpi_main, "0", "Display", "text", name="value", fill_width=True)
    b.text(kpi_main, "Note", "Caption", "muted", name="note", fill_width=True)
    kpi = m.run("component.create", {"ids": [kpi_main]})["id"]

    row_main = b.frame(name="StockRow", layout="horizontal", gap=12, padding=[12, 12, 12, 12], fill="surface", radius=8,
                       w=640, h=48, x=320, y=1560, cross="center", wrap=True, cross_gap=4, sizing=("fixed", "hug"),
                       extra={"minHeight": 48})
    b.text(row_main, "Item", "Body", "text", name="name", fill_width=True, extra={"minWidth": 160})
    status = b.frame(row_main, "Status", layout="horizontal", gap=8, cross="center", sizing=("hug", "hug"))
    b.text(status, "0 left", "Label", "text", name="left")
    badge = b.frame(status, "Badge", layout="horizontal", padding=[4, 10, 4, 10], fill="warn_soft", radius=12,
                    sizing=("hug", "hug"))
    b.text(badge, "Low", "Caption", "warn", name="badge", extra={"fontStyle": "SemiBold"})
    row = m.run("component.create", {"ids": [row_main]})["id"]

    def btn(parent, label, name):
        i = b.instance(button, parent, name=name)
        b.override(i, "label", label)
        return i

    # -- The dashboard: one frame, responsive by auto layout (wrapping rows, fill children with min widths)
    dash = b.frame(name="Pantry dashboard", layout="vertical", gap=24, padding=[32, 32, 40, 32], fill="canvas", w=1440, h=900,
                   x=0, y=0, sizing=("fixed", "hug"), clip=True, extra={"minHeight": 900})
    header = b.frame(dash, "Header", layout="horizontal", gap=16, wrap=True, cross_gap=12, cross="center",
                     sizing=("fill", "hug"))
    titles = b.frame(header, "Titles", layout="vertical", gap=4, sizing=("fill", "hug"), extra={"minWidth": 280})
    b.text(titles, "Community pantry stock", "Display", "text", name="Title", fill_width=True)
    b.text(titles, "Hillside Community Pantry (fictional) · simulated data, no backend", "Caption", "muted", name="Subtitle",
           fill_width=True)
    updated = b.text(titles, "Syncing with the stock sheet…", "Caption", "muted", name="Updated", fill_width=True)
    b.bind(updated, "characters", "updated_text")
    actions = b.frame(header, "Actions", layout="horizontal", gap=8, wrap=True, cross_gap=8, cross="center", align="max",
                      sizing=("fill", "hug"), extra={"minWidth": 280})
    filt = b.instance(chip, actions, name="Only out of stock")
    b.override(filt, "label", "Only out of stock")
    b.bind(filt, "componentProperties/Selected", "filter_chip")
    fail = b.instance(switch, actions, name="Simulate a failed sync")
    b.override(fail, "label", "Simulate a failed sync")
    b.bind(fail, "componentProperties/On", "fail_switch")
    refresh = btn(actions, "Refresh", "Refresh")

    banner = b.frame(dash, "Error banner", layout="horizontal", gap=12, wrap=True, cross_gap=8, padding=[12, 16, 12, 16],
                     fill="danger_soft", radius=12, cross="center", sizing=("fill", "hug"))
    b.text(banner, "Couldn't reach the stock sheet. The numbers below are from the last good sync.", "Body", "danger",
           name="Message", fill_width=True, extra={"minWidth": 200})
    retry = btn(banner, "Retry", "Retry")
    b.bind(banner, "visible", "is_error")

    kpis = b.frame(dash, "KPIs", layout="horizontal", gap=16, wrap=True, cross_gap=16, sizing=("fill", "hug"))
    for label, value, note in KPIS:
        i = b.instance(kpi, kpis, name="KPI " + label, sizing=("fill", "hug"))
        b.set(i, {"minWidth": 240})
        b.override(i, "label", label)
        b.override(i, "value", value)
        b.override(i, "note", note)

    main = b.frame(dash, "Main", layout="horizontal", gap=24, wrap=True, cross_gap=24, sizing=("fill", "hug"))
    content = b.frame(main, "Low stock", layout="vertical", gap=12, padding=[20, 20, 20, 20], fill="surface", radius=12,
                      sizing=("fill", "hug"), extra={"minWidth": 280})
    b.text(content, "Low on stock", "Heading", "text", name="Heading", fill_width=True)
    b.text(content, "Below a week's need at the current pace. Select a row for details.", "Caption", "muted", name="Hint",
           fill_width=True)
    skeleton = b.frame(content, "Loading", layout="vertical", gap=8, sizing=("fill", "hug"))
    for n in range(3):
        b.frame(skeleton, "Placeholder row", fill="canvas", radius=8, h=48, sizing=("fill", "fixed"))
    b.text(skeleton, "Loading the latest count…", "Caption", "muted", name="Loading text", fill_width=True)
    b.bind(skeleton, "visible", "is_loading")
    listing = b.frame(content, "List", layout="vertical", gap=4, sizing=("fill", "hug"))
    rows = []
    for name, short, left, usual, nxt in LOW_STOCK:
        i = b.instance(row, listing, name="Row " + short, sizing=("fill", "hug"))
        b.override(i, "name", name)
        b.override(i, "left", left)
        rows.append(i)
    b.bind(listing, "visible", "show_list")
    empty = b.frame(content, "Empty state", layout="vertical", gap=8, padding=[24, 16, 24, 16], fill="canvas", radius=8,
                    cross="center", sizing=("fill", "hug"))
    b.text(empty, "Nothing is out of stock", "Heading", "text", name="Title", fill_width=True, align="center")
    b.text(empty, "Every low item still has a few left. Clear the filter to see them.", "Body", "muted", name="Message",
           fill_width=True, align="center")
    clear = btn(empty, "Clear filter", "Clear filter")
    b.bind(empty, "visible", "is_empty")

    side = b.frame(main, "Side panel", layout="vertical", gap=16, sizing=("fill", "hug"),
                   extra={"minWidth": 280, "maxWidth": 440})
    deliveries = b.frame(side, "Deliveries", layout="vertical", gap=8, padding=[20, 20, 20, 20], fill="surface", radius=12,
                         sizing=("fill", "hug"))
    b.text(deliveries, "Deliveries this week", "Heading", "text", name="Heading", fill_width=True)
    for line in ("Tue · Riverside Bakery co-op · 40 loaves", "Thu · Weekly food bank run · oats, rice, chickpeas",
                 "Fri · School drive · milk and nappies"):
        b.text(deliveries, line, "Body", "text", name="Delivery", fill_width=True)
    notes = b.frame(side, "Shift notes", layout="vertical", gap=8, padding=[20, 20, 20, 20], fill="surface", radius=12,
                    sizing=("fill", "hug"))
    b.text(notes, "Shift notes", "Heading", "text", name="Heading", fill_width=True)
    b.text(notes, "Fresh produce goes out first. Check dates on the bottom shelf before restocking, and log anything "
                  "past its date in the waste sheet.", "Body", "muted", name="Note", fill_width=True)

    # -- Overlays: the sync toast and an item's details
    toast = b.frame(name="Sync toast", layout="horizontal", gap=10, padding=[14, 18, 14, 18], fill="text", radius=12, w=320, h=52,
                    x=1540, y=0, cross="center", sizing=("hug", "hug"))
    b.text(toast, "Syncing with the stock sheet…", "Body", "surface", name="Message")
    detail = b.frame(name="Item detail", layout="vertical", gap=12, padding=[24, 24, 24, 24], fill="surface", radius=16,
                     w=420, h=300, x=1540, y=120, sizing=("fixed", "hug"))
    d_name = b.text(detail, LOW_STOCK[0][1], "Title", "text", name="Name", fill_width=True)
    d_left = b.text(detail, LOW_STOCK[0][2], "Heading", "warn", name="Left", fill_width=True)
    d_usual = b.text(detail, LOW_STOCK[0][3], "Body", "muted", name="Usual", fill_width=True)
    d_next = b.text(detail, LOW_STOCK[0][4], "Body", "text", name="Next delivery", fill_width=True)
    for t, var in ((d_name, "detail_name"), (d_left, "detail_left"), (d_usual, "detail_usual"), (d_next, "detail_next")):
        b.bind(t, "characters", var)
    b.text(detail, "Simulated: these numbers aren't read from a real stock sheet.", "Caption", "muted", name="Note",
           fill_width=True)
    close = btn(detail, "Close", "Close")

    # -- Interactions
    def state(status, loading=False, ready=False, empty_=False, error=False, show=False, updated=None):
        acts = [set_var("status", status), set_var("is_loading", loading), set_var("is_ready", ready),
                set_var("is_empty", empty_), set_var("is_error", error), set_var("show_list", show)]
        if updated:
            acts.append(set_var("updated_text", updated))
        return acts

    finish = when(
        ("fail_next", state("error", error=True, show=True, updated="Last sync failed · showing the 7:40 count (simulated)")
         + [set_var("fail_next", False), set_var("fail_switch", "No")]),
        ("filter_on", state("empty", empty_=True, updated="Updated just now (simulated)")),
        (None, state("ready", ready=True, show=True, updated="Updated just now (simulated)")),
    )
    start_sync = state("loading", loading=True, updated="Syncing with the stock sheet…") + [
        {"type": "openOverlay", "dest": toast, "position": "bottomCenter", "close_on_outside": False}]
    b.on(dash, {"type": "afterDelay", "ms": 1200}, [finish])
    b.on(toast, {"type": "afterDelay", "ms": 1200}, [finish, {"type": "closeOverlay"}])
    b.on(refresh, "onClick", start_sync)
    b.on(retry, "onClick", start_sync)
    b.on(fail, "onClick", when(("fail_next", [set_var("fail_next", False), set_var("fail_switch", "No")]),
                               (None, [set_var("fail_next", True), set_var("fail_switch", "Yes")])))
    b.on(filt, "onClick", when(
        ("filter_on", [set_var("filter_on", False), set_var("filter_chip", "No"),
                       when(("is_empty", state("ready", ready=True, show=True)))]),
        (None, [set_var("filter_on", True), set_var("filter_chip", "Yes"),
                when(("is_ready", state("empty", empty_=True)))]),
    ))
    b.on(clear, "onClick", [set_var("filter_on", False), set_var("filter_chip", "No")] + state("ready", ready=True, show=True))
    for i, (name, short, left, usual, nxt) in zip(rows, LOW_STOCK):
        b.on(i, "onClick", [set_var("detail_name", short), set_var("detail_left", left), set_var("detail_usual", usual),
                            set_var("detail_next", nxt),
                            {"type": "openOverlay", "dest": detail, "position": "center", "close_on_outside": True,
                             "background": "#1B243066"}])
    b.on(close, "onClick", {"type": "closeOverlay"})
    m.run("proto.flow.add", {"start": dash, "name": "Pantry stock"})

    # -- Scenarios
    load = wait("Waits while the count loads.", {"visible": [listing], "hidden": [skeleton], "variables": {"status": "ready"}})
    b.scenario("Load, then ready", dash,
               "A volunteer opening the dashboard at the start of a shift sees a loading state, then the low-stock list.",
               [load],
               {"variables": {"status": "ready", "is_ready": True}, "visible": rows, "hidden": [skeleton, empty, banner]})
    b.scenario("Filter to empty and clear", dash,
               "A coordinator checking for anything completely gone finds nothing and gets back to the list in one tap.",
               [
                   load,
                   click(filt, "Turns on “Only out of stock”.", {
                       "variables": {"status": "empty", "filter_on": True}, "visible": [empty, clear], "hidden": [listing],
                       "variants": [{"instance": filt, "variant": "Yes"}]}),
                   click(clear, "Taps “Clear filter”.", {"variants": [{"instance": filt, "variant": "No"}]}),
               ],
               {"variables": {"status": "ready", "filter_on": False}, "visible": [listing], "hidden": [empty]})
    b.scenario("Error, retry, ready", dash,
               "When the stock sheet can't be reached, the volunteer sees why, keeps the last numbers and retries.",
               [
                   load,
                   click(fail, "Turns on “Simulate a failed sync” (stands in for the network failing).",
                         {"variants": [{"instance": fail, "variant": "Yes"}]}),
                   click(refresh, "Taps “Refresh”.", {"overlays": [toast], "visible": [skeleton],
                                                      "variables": {"status": "loading"}}),
                   wait("The sync fails.", {"overlays": [], "visible": [banner, listing],
                                            "variables": {"status": "error", "fail_next": False},
                                            "variants": [{"instance": fail, "variant": "No"}]}),
                   click(retry, "Taps “Retry” on the banner.", {"overlays": [toast], "variables": {"status": "loading"}}),
                   wait("The retry goes through."),
               ],
               {"overlays": [], "variables": {"status": "ready", "is_error": False}, "hidden": [banner], "visible": [listing]})
    b.scenario("Row detail closes on an outside click", dash,
               "A volunteer checks when oats are next delivered, then clicks beside the panel to get back to the list.",
               [
                   load,
                   click(rows[0], "Selects the oats row.", {"overlays": [detail],
                                                           "text": [{"node": d_name, "equals": LOW_STOCK[0][1]},
                                                                    {"node": d_next, "equals": LOW_STOCK[0][4]}]}),
                   {"type": "clickOutside", "note": "Clicks on the dimmed page beside the panel.", "expect": {"overlays": []}},
               ],
               {"overlays": [], "visible": [listing]})
    b.scenario("Row detail closes with its button", dash,
               "A volunteer looks up the chickpeas and closes the panel with its Close button.",
               [
                   load,
                   click(rows[1], "Selects the chickpeas row.", {"overlays": [detail],
                                                                "text": [{"node": d_name, "equals": LOW_STOCK[1][1]},
                                                                         {"node": d_left, "equals": LOW_STOCK[1][2]}]}),
                   click(close, "Taps “Close”.", {"overlays": []}),
               ],
               {"overlays": []})

    # -- Design context
    m.run("design.brief.set", {
        "audience": "Volunteers and the two part-time coordinators of a fictional neighbourhood food pantry, checking stock "
                    "before and during a shift, on the pantry's laptop or their own phones.",
        "sponsor": "The pantry's coordinators, who want shelves restocked before families arrive and fewer emergency "
                   "shopping runs.",
        "job": "See at a glance what is running low, why, and when more is coming, and spot when the numbers can't be trusted.",
        "context": "Start of a shift at the storeroom laptop, or on a phone between the shelves; the stock sheet sync is "
                   "sometimes slow or fails on the pantry's wifi.",
        "content": "Four headline numbers, the low-stock list with how many are left, deliveries this week, shift notes, and "
                   "what to do when the sync fails.",
        "format": "One responsive desktop frame (designed at 1440×900, checked from 360 to 1440 wide) shown in a browser. A "
                  "prototype: the data is simulated and there is no backend; a toggle stands in for a failing network.",
        "tone": "Calm and practical; plain English, no exclamation marks.",
        "constraints": ["Works from 360 px to 1440 px wide without a second layout", "Text contrast at least WCAG AA",
                        "Loading, empty and error states are real states with words, not blank space"],
        "success": ["A new volunteer names the three lowest items within 10 seconds", "Nobody restocks from numbers a failed "
                    "sync left stale without noticing", "The layout holds at phone width with nothing cut off"],
    })
    a1 = b.assumption("Volunteers trust the list more when a failed sync keeps the last numbers with a clear banner than "
                      "when the list is cleared.", [banner, listing])
    b.assumption("Most checks happen on the storeroom laptop, so the desktop width is the one to design first.", [dash])
    b.assumption("Five or so low items at a time is normal; the list doesn't need search or paging yet.", [listing])
    b.decision(title="One responsive frame, not a separate phone layout",
               rationale="Wrapping rows with minimum widths let the KPI cards stack (four, two, then one per row) and the "
                         "side panel move under the list when the frame is narrower than 648 px (two 280 px columns, the "
                         "24 px gap and the padding), so one frame covers 360 to 1440 px and nothing is kept in step by "
                         "hand. Between 648 and about 1000 px the list and the panel share the width.",
               rejected="A second compact frame (two copies to keep in step); a fixed 1440 px canvas (unusable on phones).",
               links=[kpis, main, side])
    b.decision(title="A failed sync keeps the last numbers under a banner with Retry",
               rationale="Stale numbers with a clear warning are more useful on the floor than an empty list, and Retry is "
                         "where the eye already is.",
               rejected="Clearing the list on error (nothing to work from); a modal error (blocks the whole page).",
               assumptions=[a1], links=[banner, retry])
    b.decision(title="Item details open in a panel over the list",
               rationale="The volunteer keeps their place in the list; a click beside the panel or Close returns to it.",
               rejected="A separate item page (loses the place in the list); expanding the row in place (pushes the rest "
                        "of the list down at phone width).",
               links=[detail])
    m.run("design.stage.set", {"stage": "converging"})
    return {"screens": [dash], "overlays": [toast, detail]}


# ---------------------------------------------------------------------------------------------
# 3. Bike repair booking: "Đặt lịch sửa xe đạp"
# ---------------------------------------------------------------------------------------------

SERVICES = [
    # (key, Vietnamese title, English line, duration and price)
    ("tune", "Bảo dưỡng tổng quát", "Tune-up: gears, brakes, chain and wheels", "60 phút · 250.000 ₫"),
    ("flat", "Vá hoặc thay săm", "Flat tyre: patch or new inner tube", "20 phút · 60.000 ₫"),
    ("brake", "Kiểm tra phanh", "Brake check: pads, cables and adjustment", "30 phút · 90.000 ₫"),
]
DAYS = ["Thứ Năm", "Thứ Sáu", "Thứ Bảy"]
TIMES = ["08:30", "10:00", "14:00", "16:30"]


def make_service_card(b, x, y, width=342):
    """ServiceCard: Selected=No|Yes, a title, an English line, the time and price, and a radio mark."""

    def variant(on):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="horizontal", gap=12, padding=[14, 16, 14, 16], fill="accent_soft" if on else "surface",
                        stroke="accent" if on else "border", radius=12, w=width, h=80, x=cx, y=cy, cross="center",
                        sizing=("fixed", "hug"), extra={"strokeWeight": 2 if on else 1, "minHeight": 72})
            col = b.frame(f, "Text", layout="vertical", gap=2, sizing=("fill", "hug"))
            b.text(col, "Service", "Heading", "text", name="title", fill_width=True)
            b.text(col, "English description", "Caption", "muted", name="subtitle", fill_width=True)
            b.text(col, "Time · price", "Caption", "text", name="meta", fill_width=True)
            mark = b.frame(f, "Radio", layout="horizontal", fill="accent" if on else "surface", stroke=None if on else "border",
                           radius=12, w=24, h=24, align="center", cross="center", extra=None if on else {"strokeWeight": 2})
            if on:
                b.frame(mark, "Dot", fill="on_accent", radius=5, w=10, h=10)
            return f
        return build

    return b.variant_set("ServiceCard", x, y, [("Selected=No", variant(False)), ("Selected=Yes", variant(True))])


def make_steps(b, x, y, width=342):
    """Steps: Step=1|2|3, a three-part progress bar with labels."""
    labels = ["1. Dịch vụ", "2. Thời gian", "3. Xác nhận"]

    def variant(step):
        def build(vname, cx, cy):
            f = b.frame(name=vname, layout="horizontal", gap=8, w=width, h=40, x=cx, y=cy, sizing=("fixed", "hug"))
            for i, label in enumerate(labels, start=1):
                seg = b.frame(f, "Step %d" % i, layout="vertical", gap=6, sizing=("fill", "hug"))
                b.frame(seg, "Bar", fill="accent" if i <= step else "divider", radius=2, h=4, sizing=("fill", "fixed"))
                b.text(seg, label, "Caption", "text" if i == step else "muted", name="label", fill_width=True,
                       extra={"fontStyle": "SemiBold"} if i == step else None)
            return f
        return build

    return b.variant_set("Steps", x, y, [("Step=%d" % s, variant(s)) for s in (1, 2, 3)])


def bike_repair_booking(b):
    m = b.m
    m.run("page.rename", {"name": "Booking flow"})
    b.setup_theme()
    variables = [
        ("service", "string", "Chưa chọn dịch vụ"),
        ("service_en", "string", "No service chosen yet"),
        ("price", "string", "—"),
        ("has_service", "boolean", False),
        ("needs_service", "boolean", True),
        ("choose_hint", "string", "Chọn một dịch vụ để tiếp tục."),
    ]
    variables += [("sel_" + key, "string", "No") for key, _, _, _ in SERVICES]
    variables += [("day", "string", ""), ("time", "string", ""), ("has_day", "boolean", False), ("has_time", "boolean", False),
                  ("slot_ready", "boolean", False), ("needs_slot", "boolean", True),
                  ("details_hint", "string", "Chọn ngày và giờ để tiếp tục.")]
    variables += [("day_%d" % i, "string", "No") for i in range(1, len(DAYS) + 1)]
    variables += [("time_%d" % i, "string", "No") for i in range(1, len(TIMES) + 1)]
    variables += [("slot", "string", "Chưa chọn giờ"), ("notes_state", "string", "Empty"),
                  ("notes_text", "string", "Không có ghi chú")]
    b.collection("Booking", variables)
    m.run("proto.settings", {"device": "iPhone 16"})

    button, _ = make_button(b, 0, 1000)
    text_button = make_text_button(b, 1240, 1000)
    field, _ = make_field(b, 0, 1120)
    chip, _ = make_chip(b, 1240, 1120, width=104)
    card, _ = make_service_card(b, 0, 1300)
    steps, _ = make_steps(b, 800, 1300)

    W, H = 390, 844
    xs = [0, 470, 940, 1410]

    def screen(name, x, padding=(12, 24, 32, 24)):
        return b.frame(name=name, layout="vertical", gap=24, padding=list(padding), fill="surface", w=W, h=H, x=x, y=0,
                       align="spaceBetween", clip=True)

    def column(parent, name, gap=16):
        return b.frame(parent, name, layout="vertical", gap=gap, sizing=("fill", "hug"))

    def btn(parent, label, name, state=None):
        i = b.instance(button, parent, name=name, sizing=("fill", "hug"), props={"State": state} if state else None)
        b.override(i, "label", label)
        return i

    def link(parent, label, name):
        i = b.instance(text_button, parent, name=name)
        b.override(i, "label", label)
        return i

    def step_bar(parent, n):
        return b.instance(steps, parent, name="Steps", sizing=("fill", "hug"), props={"Step": str(n)})

    def heading(parent, vi, en):
        b.text(parent, vi, "Title", "text", name="Title", fill_width=True)
        b.text(parent, en, "Caption", "muted", name="Subtitle", fill_width=True)

    # -- Choose service
    choose = screen("Choose service", xs[0], padding=(24, 24, 32, 24))
    cbody = column(choose, "Content")
    step_bar(cbody, 1)
    head = column(cbody, "Heading", gap=4)
    heading(head, "Chọn dịch vụ", "Choose a service · Xưởng xe đạp Cây Bàng (fictional)")
    cards = column(cbody, "Services", gap=12)
    card_ids = {}
    for key, vi, en, meta in SERVICES:
        i = b.instance(card, cards, name="Card " + key, sizing=("fill", "hug"))
        b.override(i, "title", vi)
        b.override(i, "subtitle", en)
        b.override(i, "meta", meta)
        b.bind(i, "componentProperties/Selected", "sel_" + key)
        card_ids[key] = i
    cfoot = column(choose, "Footer", gap=8)
    choose_hint = b.text(cfoot, "Chọn một dịch vụ để tiếp tục.", "Caption", "muted", name="Hint", fill_width=True, align="center")
    b.bind(choose_hint, "characters", "choose_hint")
    b.bind(choose_hint, "visible", "needs_service")
    choose_off = btn(cfoot, "Tiếp tục", "Continue (disabled)", state="Disabled")
    choose_next = btn(cfoot, "Tiếp tục", "Continue")
    b.bind(choose_off, "visible", "needs_service")
    b.bind(choose_next, "visible", "has_service")

    # -- Details
    details = screen("Details", xs[1])
    dbody = column(details, "Content")
    details_back = link(dbody, "‹ Quay lại", "Back")
    step_bar(dbody, 2)
    head = column(dbody, "Heading", gap=4)
    heading(head, "Chọn ngày và giờ", "Pick a day and a time")
    chosen = column(dbody, "Chosen service", gap=2)
    b.text(chosen, "Dịch vụ đã chọn", "Caption", "muted", name="Label", fill_width=True)
    chosen_value = b.text(chosen, "Chưa chọn dịch vụ", "Label", "text", name="Service", fill_width=True)
    b.bind(chosen_value, "characters", "service")

    def chip_row(parent, title, options, prefix, var, has):
        col = column(parent, title, gap=8)
        b.text(col, title, "Label", "text", name="Label", fill_width=True)
        row = b.frame(col, "Options", layout="horizontal", gap=8, sizing=("fill", "hug"))
        ids = []
        for n, label in enumerate(options, start=1):
            i = b.instance(chip, row, name="%s %s" % (title, label), sizing=("fill", "hug"))
            b.override(i, "label", label)
            b.bind(i, "componentProperties/Selected", "%s_%d" % (prefix, n))
            ids.append(i)
        for n, (i, label) in enumerate(zip(ids, options), start=1):
            acts = [set_var(var, label), set_var(has, True)]
            acts += [set_var("%s_%d" % (prefix, k), "Yes" if k == n else "No") for k in range(1, len(options) + 1)]
            acts.append(when(("has_day and has_time", [set_var("slot_ready", True), set_var("needs_slot", False)])))
            b.on(i, "onClick", acts)
        return ids

    day_ids = chip_row(dbody, "Ngày", DAYS, "day", "day", "has_day")
    time_ids = chip_row(dbody, "Giờ", TIMES, "time", "time", "has_time")
    notes = b.instance(field, dbody, name="Notes field", sizing=("fill", "hug"))
    b.override(notes, "label", "Ghi chú cho thợ (không bắt buộc)")
    b.override(notes, "placeholder", "Chạm để thêm ghi chú")
    b.override(notes, "value", "Phanh sau kêu khi xuống dốc.")
    b.override(notes, "error", "Ghi chú quá dài.")
    b.bind(notes, "componentProperties/State", "notes_state")
    dfoot = column(details, "Footer", gap=8)
    details_hint = b.text(dfoot, "Chọn ngày và giờ để tiếp tục.", "Caption", "muted", name="Hint", fill_width=True, align="center")
    b.bind(details_hint, "characters", "details_hint")
    b.bind(details_hint, "visible", "needs_slot")
    details_off = btn(dfoot, "Tiếp tục", "Continue (disabled)", state="Disabled")
    details_next = btn(dfoot, "Tiếp tục", "Continue")
    b.bind(details_off, "visible", "needs_slot")
    b.bind(details_next, "visible", "slot_ready")

    # -- Confirm
    confirm = screen("Confirm", xs[2])
    fbody = column(confirm, "Content")
    confirm_back = link(fbody, "‹ Quay lại", "Back")
    step_bar(fbody, 3)
    head = column(fbody, "Heading", gap=4)
    heading(head, "Xác nhận lịch hẹn", "Check your booking")
    summary = b.frame(fbody, "Summary", layout="vertical", gap=4, padding=[8, 16, 8, 16], fill="canvas", radius=12,
                      sizing=("fill", "hug"))

    def summary_row(label, var, sub_var=None, change=None):
        row = b.frame(summary, label, layout="horizontal", gap=8, padding=[8, 0, 8, 0], cross="center", sizing=("fill", "hug"))
        col = b.frame(row, "Text", layout="vertical", gap=2, sizing=("fill", "hug"))
        b.text(col, label, "Caption", "muted", name="Label", fill_width=True)
        value = b.text(col, "—", "Label", "text", name="Value", fill_width=True)
        b.bind(value, "characters", var)
        sub = None
        if sub_var:
            sub = b.text(col, "—", "Caption", "muted", name="Detail", fill_width=True)
            b.bind(sub, "characters", sub_var)
        link_id = link(row, "Đổi", "Change " + label) if change else None
        return value, sub, link_id

    service_value, service_en_value, change_service = summary_row("Dịch vụ · Service", "service", "service_en", True)
    slot_value, _, change_slot = summary_row("Thời gian · Time", "slot", None, True)
    price_value, _, _ = summary_row("Giá dự kiến · Estimated price", "price")
    notes_value, _, _ = summary_row("Ghi chú · Notes", "notes_text")
    b.text(fbody, "Trả tiền tại tiệm sau khi sửa xong. Bản mẫu: không có máy chủ, không có lịch hẹn nào được gửi đi.",
           "Caption", "muted", name="Payment note", fill_width=True)
    ffoot = column(confirm, "Footer", gap=8)
    book = btn(ffoot, "Đặt lịch", "Book")

    # -- Booked
    booked = b.frame(name="Booked", layout="vertical", gap=16, padding=[120, 24, 32, 24], fill="surface", w=W, h=H, x=xs[3], y=0,
                     cross="center", clip=True)
    badge = b.frame(booked, "Success badge", layout="horizontal", fill="success_soft", radius=40, w=80, h=80, align="center",
                    cross="center")
    b.node("vector", badge, "tick", {"fill": "none", "stroke": {"$var": "Theme/success"}, "strokeWeight": 4,
                                     "strokeCap": "round", "strokeJoin": "round"}, path="M4 18 L14 28 L34 6", x=0, y=0)
    b.text(booked, "Đã đặt lịch!", "Title", "text", name="Title", fill_width=True, align="center")
    b.text(booked, "Booked", "Caption", "muted", name="Subtitle", fill_width=True, align="center")
    booked_service = b.text(booked, "—", "Label", "text", name="Service", fill_width=True, align="center")
    b.bind(booked_service, "characters", "service")
    booked_slot = b.text(booked, "—", "Body", "text", name="Time", fill_width=True, align="center")
    b.bind(booked_slot, "characters", "slot")
    b.text(booked, "Tiệm sẽ nhắn tin nhắc bạn một giờ trước giờ hẹn. Mã lịch hẹn (giả lập): XD-1162.", "Body", "muted",
           name="Next steps", fill_width=True, align="center")

    # -- Interactions
    for key, vi, en, meta in SERVICES:
        acts = [set_var("service", vi), set_var("service_en", en), set_var("price", meta.split(" · ")[1]),
                set_var("has_service", True), set_var("needs_service", False)]
        acts += [set_var("sel_" + k, "Yes" if k == key else "No") for k, _, _, _ in SERVICES]
        b.on(card_ids[key], "onClick", acts)
    b.on(choose_off, "onClick", set_var("choose_hint", "Bạn chưa chọn dịch vụ: chạm vào một thẻ ở trên."))
    b.on(choose_next, "onClick", nav(details))
    b.on(details_back, "onClick", back())
    b.on(notes, "onClick", [set_var("notes_state", "Filled"), set_var("notes_text", "Phanh sau kêu khi xuống dốc.")])
    b.on(details_off, "onClick", set_var("details_hint", "Hãy chọn cả ngày và giờ ở trên."))
    b.on(details_next, "onClick", [set_expr("slot", 'day + ", " + time'), nav(confirm)])
    b.on(confirm_back, "onClick", back())
    b.on(change_service, "onClick", nav(choose))
    b.on(change_slot, "onClick", nav(details))
    b.on(book, "onClick", nav(booked))
    m.run("proto.flow.add", {"start": choose, "name": "Đặt lịch sửa xe"})
    b.end_state(booked, "The booking is made: the shop texts a reminder, so the flow ends here and the viewer closes "
                        "the page.")

    # -- Scenarios
    to_confirm = [
        click(card_ids["tune"], "Taps “Bảo dưỡng tổng quát”.",
              {"variants": [{"instance": card_ids["tune"], "variant": "Yes"}], "visible": [choose_next]}),
        click(choose_next, "Taps “Tiếp tục”.", {"screen": details}),
        click(day_ids[1], "Picks Thứ Sáu.", {"variants": [{"instance": day_ids[1], "variant": "Yes"}]}),
        click(time_ids[1], "Picks 10:00.", {"visible": [details_next], "hidden": [details_off]}),
        click(details_next, "Taps “Tiếp tục”.", {"screen": confirm}),
    ]
    b.scenario("Book a tune-up", choose,
               "Someone whose gears skip books a tune-up for Friday morning and sees what they booked.",
               to_confirm + [
                   click(book, "Checks the summary and taps “Đặt lịch”.", {"screen": booked}),
               ],
               {"screen": booked, "endState": True,
                "text": [{"node": booked_service, "equals": "Bảo dưỡng tổng quát"}, {"node": booked_slot, "equals": "Thứ Sáu, 10:00"}]})
    b.scenario("Change the service from Confirm", choose,
               "Someone who remembers on the summary that it's the brakes, not a tune-up, changes it and sees the new choice.",
               to_confirm + [
                   click(change_service, "Taps “Đổi” next to the service.", {
                       "screen": choose, "variants": [{"instance": card_ids["tune"], "variant": "Yes"}]}),
                   click(card_ids["brake"], "Taps “Kiểm tra phanh”.", {
                       "variants": [{"instance": card_ids["brake"], "variant": "Yes"},
                                    {"instance": card_ids["tune"], "variant": "No"}]}),
                   click(choose_next, "Taps “Tiếp tục”.", {
                       "screen": details, "variants": [{"instance": day_ids[1], "variant": "Yes"},
                                                       {"instance": time_ids[1], "variant": "Yes"}]}),
                   click(details_next, "Keeps Friday at 10:00 and taps “Tiếp tục”.", {"screen": confirm}),
               ],
               {"screen": confirm,
                "text": [{"node": service_value, "equals": "Kiểm tra phanh"}, {"node": slot_value, "equals": "Thứ Sáu, 10:00"},
                         {"node": price_value, "equals": "90.000 ₫"}]})
    b.scenario("Back from Details keeps the service highlighted", choose,
               "Someone who goes back from the time step to reread the services still sees their choice marked.",
               [
                   click(card_ids["flat"], "Taps “Vá hoặc thay săm”."),
                   click(choose_next, "Taps “Tiếp tục”.", {"screen": details}),
                   click(details_back, "Taps “‹ Quay lại”.", {"screen": choose}),
               ],
               {"screen": choose, "visible": [choose_next], "hidden": [choose_off],
                "variants": [{"instance": card_ids["flat"], "variant": "Yes"}, {"instance": card_ids["tune"], "variant": "No"},
                             {"instance": card_ids["brake"], "variant": "No"}]})
    b.scenario("Booked is where the flow ends", choose,
               "The shortest booking, a flat tyre this Thursday afternoon, ends on Booked, which is an end state.",
               [
                   click(card_ids["flat"], "Taps “Vá hoặc thay săm”."),
                   click(choose_next, "Taps “Tiếp tục”."),
                   click(day_ids[0], "Picks Thứ Năm."),
                   click(time_ids[2], "Picks 14:00."),
                   click(notes, "Adds a note (the prototype fills a sample note).",
                         {"variants": [{"instance": notes, "variant": "Filled"}]}),
                   click(details_next, "Taps “Tiếp tục”.",
                         {"screen": confirm, "text": [{"node": notes_value, "equals": "Phanh sau kêu khi xuống dốc."}]}),
                   click(book, "Taps “Đặt lịch”."),
               ],
               {"screen": booked, "endState": True, "overlays": []})
    b.scenario("Continue waits for a day and a time", choose,
               "Someone who taps “Tiếp tục” before picking a time is told what is still missing.",
               [
                   click(choose_off, "Taps the greyed-out “Tiếp tục” before choosing.",
                         {"screen": choose, "text": [{"node": choose_hint, "equals": "Bạn chưa chọn dịch vụ: chạm vào một thẻ ở trên."}]}),
                   click(card_ids["brake"], "Taps “Kiểm tra phanh”."),
                   click(choose_next, "Taps “Tiếp tục”.", {"screen": details}),
                   click(day_ids[2], "Picks Thứ Bảy but no time."),
                   click(details_off, "Taps the greyed-out “Tiếp tục”.", {
                       "screen": details, "visible": [details_hint],
                       "text": [{"node": details_hint, "equals": "Hãy chọn cả ngày và giờ ở trên."}]}),
               ],
               {"screen": details})

    # -- Design context
    m.run("design.brief.set", {
        "audience": "Everyday cyclists in a fictional neighbourhood (commuters, students, delivery riders) booking a repair at "
                    "a small two-mechanic shop; most read Vietnamese first, some prefer English.",
        "sponsor": "The shop owner, who wants fewer phone calls during repairs and a day plan that doesn't overbook the stand.",
        "job": "Book one repair at a time that suits them and know what it will roughly cost.",
        "context": "On a phone, often right after something broke (a flat tyre on the way to work), sometimes standing next "
                   "to the bike.",
        "content": "Three common services with time and price, a day and a time slot, an optional note for the mechanic, and a "
                   "summary to check before booking.",
        "format": "A four-step phone flow (390×844) linked from the shop's chat page. A prototype: the data is simulated, "
                  "there is no backend and no booking is sent; tapping the note field stands in for typing.",
        "tone": "Friendly and brief; Vietnamese titles with English secondary lines.",
        "constraints": ["Vietnamese first, English secondary text", "Primary buttons at least 44×44 px",
                        "Text contrast at least WCAG AA", "Payment happens at the shop, not in the flow"],
        "success": ["A first-time customer books in under a minute in a hallway test",
                    "People can change the service or the time from the summary without starting over",
                    "Calls asking 'did my booking go through?' drop after launch"],
    })
    a1 = b.assumption("Three services cover most walk-in repairs, so a short list beats a search.", [cards])
    a2 = b.assumption("People know which day suits them but not the exact hour, so day and time are separate choices.",
                      day_ids + time_ids)
    b.assumption("Showing an estimated price before booking reduces no-shows more than it scares people off.", [price_value])
    b.decision(title="“Đổi” links on the summary go back to the step that owns the choice",
               rationale="Changing the service or the time keeps everything else, so a late change costs one step, not "
                         "the whole flow.",
               rejected="Editing in place on the summary (a second set of controls to keep in step); a single 'Start over' "
                        "button (loses the day and time).",
               links=[change_service, change_slot])
    b.decision(title="Day and time are chips, not a calendar",
               rationale="The shop opens three days a week with four slots; chips show every option at once and take "
                         "one tap each.",
               rejected="A month calendar (mostly closed days); a single list of day-and-time slots (12 rows to scroll).",
               assumptions=[a2], links=day_ids + time_ids)
    b.decision(title="Services are three cards with time and price on each",
               rationale="Time and price decide most choices, so they sit on the card rather than on a details page.",
               rejected="A dropdown of services (hides the price); a free-text 'what's wrong' box (the shop can't plan from it).",
               assumptions=[a1], links=[cards])
    m.run("design.stage.set", {"stage": "converging"})
    return {"screens": [choose, details, confirm, booked], "overlays": []}


# ---------------------------------------------------------------------------------------------
# Building, the maintainers' acceptance step, and checks
# ---------------------------------------------------------------------------------------------

EXAMPLES = {
    "signup-flow": (signup_flow, {"accent": "#0B6E8A", "accent_pressed": "#08566C", "accent_soft": "#E3F2F7"}),
    "pantry-dashboard": (pantry_dashboard, {"accent": "#2F6B4F", "accent_pressed": "#24543E", "accent_soft": "#E4F1EA",
                                            "warn": "#8A4B00", "warn_soft": "#FFF1DC"}),
    "bike-repair-booking": (bike_repair_booking, {"accent": "#A34700", "accent_pressed": "#7F3700", "accent_soft": "#FFF1DC"}),
}


def accept_as_maintainer(cli, path, end_states, decisions):
    """The maintainers' acceptance step. It stands in for a person's review of these example
    files: `figcraft-cli run` acts as the user (not as an agent), so it may accept the end states
    and decisions the builder proposed. In your own files an agent's end states and decisions
    stay proposals until you accept them."""
    steps = [{"command": "file.open", "params": {"path": path}}]
    steps += [{"command": "proto.endState.accept", "params": {"id": i}} for i in end_states]
    steps += [{"command": "design.decision.accept", "params": {"id": d}} for d in decisions]
    steps += [{"command": "file.save", "params": {"path": path}}]
    with tempfile.TemporaryDirectory() as tmp:
        script = os.path.join(tmp, "accept.json")
        with open(script, "w", encoding="utf-8") as f:
            json.dump(steps, f, ensure_ascii=False)
        out = subprocess.run([cli, "--no-system-fonts", "run", script], capture_output=True, text=True)
    if out.returncode != 0:
        raise BuildError("accept_as_maintainer (%s) failed:\n%s%s" % (path, out.stdout, out.stderr))
    return {"endStates": end_states, "decisions": decisions}


def check(cli, path, shots=None, name=None):
    """Open the saved file over MCP and report the flow check (`check_prototype`), the stored
    scenarios (`run_scenarios`) and the design check at the file's stage (`check_design`). With
    `shots`, also write a PNG of every top-level frame and, with `proto.snapshot`, of the state each
    scenario ends in."""
    m = Mcp(cli)
    try:
        m.call("open_file", {"path": path})
        flow = m.call("check_prototype", {})
        runs = m.call("run_scenarios", {})
        design = m.call("check_design", {})
        report = {
            "flowOk": flow.get("ok"),
            "problems": [(p["rule"], p["severity"], p["message"]) for p in flow.get("problems", [])],
            "scenarios": [(r["id"], r["name"], r["passed"], r.get("failures") or [s for st in r.get("steps", []) for s in st.get("failures", [])])
                          for r in runs.get("results", [])],
            "findings": [(f["rule"], f["severity"], f["message"]) for f in design.get("findings", [])],
        }
        if shots:
            os.makedirs(shots, exist_ok=True)
            page = m.call("inspect_document", {"depth": 1, "fields": ["name", "type"]})
            cur = page.get("currentPage")
            for p in page.get("pages", []):
                if p.get("id") != cur:
                    continue
                for n in p.get("children", []):
                    fname = "%s--%s.png" % (name, n["name"].replace("/", "-").replace(" ", "_").replace(":", ""))
                    m.call("export", {"ids": [n["id"]], "format": "png", "scale": 1, "path": os.path.join(shots, fname)})
            for sc in m.run("proto.scenario.list", {})["scenarios"]:
                fname = "%s--%s-end.png" % (name, sc["id"])
                m.run("proto.snapshot", {"start": sc["start"], "steps": sc["steps"], "path": os.path.join(shots, fname)})
        return report
    finally:
        m.close()


def build(cli, out, name, accept=True, shots=None):
    fn, accent = EXAMPLES[name]
    path = os.path.abspath(os.path.join(out, name + ".figcraft"))
    m = Mcp(cli)
    try:
        b = Builder(m, accent)
        fn(b)
        m.call("select", {"ids": []})
        m.call("save_file", {"path": path})
    finally:
        m.close()
    accepted = None
    if accept:
        accepted = accept_as_maintainer(cli, path, b.proposed_end_states, b.proposed_decisions)
    report = check(cli, path, shots, name)
    return path, accepted, report


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--cli", required=True, help="path to figcraft-cli")
    ap.add_argument("--out", required=True, help="folder to write the .figcraft files to")
    ap.add_argument("--only", choices=sorted(EXAMPLES), help="build one example")
    ap.add_argument("--accept-as-maintainer", dest="accept", action="store_true", default=True,
                    help="accept the end states and decisions the builder proposed, as the user (default)")
    ap.add_argument("--no-accept-as-maintainer", dest="accept", action="store_false",
                    help="leave the agent's end states and decisions as proposals")
    ap.add_argument("--shots", help="also write a PNG of every top-level frame to this folder")
    args = ap.parse_args(argv)
    os.makedirs(args.out, exist_ok=True)
    failed = False
    for name in sorted(EXAMPLES):
        if args.only and name != args.only:
            continue
        try:
            path, accepted, report = build(args.cli, args.out, name, args.accept, args.shots)
        except (BuildError, OSError) as e:
            print("error: %s: %s" % (name, e), file=sys.stderr)
            return 1
        print("built %s" % path)
        if accepted:
            print("  accepted as maintainer: end states %s, decisions %s" % (accepted["endStates"], accepted["decisions"]))
        print("  proto.check ok: %s" % report["flowOk"])
        for p in report["problems"]:
            print("  problem: %s (%s) %s" % p)
        for sid, sname, passed, fails in report["scenarios"]:
            print("  %s %s: %s%s" % (sid, sname, "pass" if passed else "FAIL", "" if passed else " " + "; ".join(fails)))
            failed |= not passed
        for f in report["findings"]:
            print("  finding: %s (%s) %s" % f)
            failed |= f[1] in ("blocker", "major")
        failed |= args.accept and not report["flowOk"]
        if not args.accept:
            print("  left as proposals: the agent's end states and decisions; flow.deadEnd and the scenarios that "
                  "expect an end state fail until a person accepts them")
    if failed and args.accept:
        print("error: an example doesn't pass its checks (see above)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
