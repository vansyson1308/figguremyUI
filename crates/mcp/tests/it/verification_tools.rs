//! The tools an agent verifies with (`design_context`, `check_design`, `check_prototype`,
//! `simulate_prototype`, `run_scenarios`) and the resources beside them: what they read, which
//! document they say they read, what they refuse, and that they answer the same attached to an
//! app as headless. The prototype they are tried on is built here with tool calls, as an agent
//! would build it, and `full_loop.rs` walks the whole working loop on the same one.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use crate::common::{Client, approx, png_dims};
use figcraft_mcp::{Attached, Backend, Headless, Server};
use serde_json::{Value, json};

/// An app to attach to: a session of its own behind the desktop app's control protocol (JSON
/// lines on a loopback port), as `attached.rs` has one.
fn fake_app() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut app = Headless::new();
        for stream in listener.incoming().flatten() {
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                let reply = match app.call(v["method"].as_str().unwrap_or(""), v["params"].clone()) {
                    Ok(r) => json!({"id": v["id"], "ok": true, "result": r}),
                    Err(e) => json!({"id": v["id"], "ok": false, "error": e}),
                };
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    });
    port
}

/// The document a result says it is about.
pub(crate) fn about(r: &Value) -> Value {
    let doc = r["structuredContent"]["document"].clone();
    assert!(doc.is_object(), "the result names no document: {r}");
    doc
}

pub(crate) fn first_json(r: &Value) -> Value {
    let text = r["content"].as_array().unwrap().iter().find_map(|c| c.get("text").and_then(Value::as_str)).unwrap();
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

/// The error of a failed call.
pub(crate) fn error_of(r: &Value) -> Value {
    assert_eq!(r["isError"], true, "should have failed: {r}");
    let e = r["structuredContent"]["error"].clone();
    assert!(e["code"].is_string() && e["message"].is_string(), "no structured error: {r}");
    e
}

/// The PNG a result carries as image content: its size in pixels.
pub(crate) fn picture(r: &Value) -> (u32, u32) {
    let block = r["content"].as_array().unwrap().iter().find(|b| b["type"] == "image").unwrap_or_else(|| panic!("no image in {r}"));
    assert_eq!(block["mimeType"], "image/png");
    png_dims(block["data"].as_str().unwrap())
}

/// The ids the loop works with.
pub(crate) struct Built {
    pub home: Value,
    pub details: Value,
    pub done: Value,
    pub menu: Value,
    pub title: Value,
    pub hint: Value,
    pub toggle: Value,
    pub toggle_on: Value,
    pub go_on: Value,
    pub open_menu: Value,
    pub close_menu: Value,
    pub back: Value,
    pub confirm: Value,
    pub again: Value,
}

/// A text layer in `parent`.
pub(crate) fn text(c: &mut Client, parent: &Value, name: &str, characters: &str, props: Value) -> Value {
    c.create(json!({"type": "text", "parent": parent, "name": name, "text": characters, "props": props}))
}

/// A plain button: an auto-layout frame named `name` holding its label.
pub(crate) fn plain_button(c: &mut Client, parent: &Value, name: &str) -> Value {
    let b = c.create(json!({"type": "frame", "parent": parent, "name": name, "height": 48,
        "props": {"fill": "#E9ECEF", "cornerRadius": 8, "layout": "horizontal", "mainAlign": "center", "crossAlign": "center", "padding": [12, 20, 12, 20], "sizingH": "fill"}}));
    text(c, &b, "Label", name, json!({"fill": "#212529"}));
    b
}

/// A component set of two variants (`State=<a>`, `State=<b>`), each a frame of `size` filled as
/// given. Returns the set and the two variants.
pub(crate) fn variants(c: &mut Client, y: f64, size: (f64, f64), states: [(&str, &str); 2], label: Option<&str>) -> (Value, [Value; 2]) {
    let made: Vec<Value> = states
        .iter()
        .enumerate()
        .map(|(k, (state, fill))| {
            let f = c.create(json!({"type": "frame", "name": format!("State={state}"), "x": k as f64 * 240.0, "y": y, "width": size.0, "height": size.1,
                "props": {"fill": fill, "cornerRadius": 8, "layout": "horizontal", "mainAlign": "center", "crossAlign": "center", "padding": [12, 20, 12, 20]}}));
            if let Some(label) = label {
                text(c, &f, "Label", label, json!({"fill": "#FFFFFF"}));
            }
            c.run("component.create", json!({"ids": [f]}))["id"].clone()
        })
        .collect();
    let set = c.run("component.combineVariants", json!({"ids": made}))["id"].clone();
    (set, [made[0].clone(), made[1].clone()])
}

/// Stages 2 and 3: the screens, the components, the variables, the interactions and the flow
/// start, with what each must look like once made.
pub(crate) fn build(c: &mut Client) -> Built {
    // Three screens and an overlay, each a frame that lays its children out itself.
    let frame = |c: &mut Client, name: &str, x: f64, size: (f64, f64)| {
        c.create(json!({"type": "frame", "name": name, "x": x, "y": 0, "width": size.0, "height": size.1,
            "props": {"fill": "#FFFFFF", "layout": "vertical", "padding": 24, "gap": 16}}))
    };
    let home = frame(c, "Home", 0.0, (360.0, 640.0));
    let details = frame(c, "Details", 460.0, (360.0, 640.0));
    let done = frame(c, "Done", 920.0, (360.0, 640.0));
    let menu = frame(c, "Menu", 1380.0, (240.0, 160.0));
    c.ok("set_properties", json!({"ids": [menu], "props": {"fill": "#F1F3F5"}}));

    // Variables: whether the terms are agreed to, and the hint the first screen shows.
    c.run("variable.createCollection", json!({"name": "State"}));
    c.run("variable.create", json!({"collection": "State", "name": "agreed", "type": "boolean", "value": false}));
    let hint_variable =
        c.run("variable.create", json!({"collection": "State", "name": "hint", "type": "string", "value": "Tick the box to go on"}))["id"].clone();
    let listed = c.run("variable.list", json!({})).to_string();
    assert!(listed.contains("agreed") && listed.contains("hint"), "{listed}");

    // A button with two states and a toggle whose states change to each other and keep the
    // variable in step.
    let (button_set, [button_default, _]) = variants(c, 900.0, (160.0, 48.0), [("Default", "#3B5BDB"), ("Disabled", "#ADB5BD")], Some("Continue"));
    let (toggle_set, [toggle_off, toggle_on]) = variants(c, 1100.0, (56.0, 32.0), [("Off", "#ADB5BD"), ("On", "#2F9E44")], None);
    assert_eq!(c.node(&button_set)["children"].as_array().unwrap().len(), 2, "two variants in the set");
    for (from, to, value) in [(&toggle_off, &toggle_on, true), (&toggle_on, &toggle_off, false)] {
        c.run(
            "proto.interaction.add",
            json!({"id": from, "trigger": {"type": "onClick"}, "actions": [{"type": "changeTo", "dest": to}, {"type": "setVariable", "variable": "agreed", "value": value}]}),
        );
    }

    // Home: a title, a hint bound to the variable, the toggle, "Continue" and a menu button.
    let title = text(c, &home, "Title", "Collect your parcel", json!({"fontSize": 24, "fill": "#212529"}));
    let hint = text(c, &home, "Hint", "Tick the box to go on", json!({"fontSize": 14, "fill": "#495057"}));
    c.run("variable.bind", json!({"ids": [hint], "field": "characters", "variable": hint_variable}));
    let toggle = c.run("instance.create", json!({"component": toggle_set, "parent": home}))["id"].clone();
    let go_on = c.run("instance.create", json!({"component": button_set, "parent": home}))["id"].clone();
    let open_menu = plain_button(c, &home, "Menu");
    let home_node = c.node(&home);
    assert_eq!(home_node["layout"]["mode"], "vertical");
    assert_eq!(home_node["children"].as_array().unwrap().len(), 5);
    let title_node = c.node(&title);
    assert!(approx(&title_node["x"], 24.0) && approx(&title_node["y"], 24.0), "the layout places the title at the padding: {title_node}");
    let hint_node = c.node(&hint);
    assert!(approx(&hint_node["y"], 24.0 + title_node["height"].as_f64().unwrap() + 16.0), "and the hint one gap below it: {hint_node}");
    let (toggle_node, go_on_node) = (c.node(&toggle), c.node(&go_on));
    assert_eq!(
        (toggle_node["type"].as_str().map(str::to_lowercase), toggle_node["mainComponent"].clone()),
        (Some("instance".into()), toggle_off.clone()),
        "{toggle_node}"
    );
    assert_eq!(go_on_node["mainComponent"], button_default, "an instance of a set shows its default variant");
    assert!(approx(&c.node(&open_menu)["width"], 312.0), "a child that fills takes the frame's width less its padding");

    // The overlay with its way out; Details with a way back and a confirmation; Done with a way
    // to start again, made with its interaction in one undo step.
    let close_menu = plain_button(c, &menu, "Close");
    text(c, &details, "Title", "Check the details", json!({"fontSize": 24, "fill": "#212529"}));
    let back = plain_button(c, &details, "Back");
    let confirm = c.run("instance.create", json!({"component": button_set, "parent": details}))["id"].clone();
    text(c, &done, "Title", "Your parcel is ready", json!({"fontSize": 24, "fill": "#212529"}));
    let made = c.ok(
        "batch",
        json!({"label": "Start again", "commands": [
            {"command": "node.create", "as": "again", "params": {"type": "frame", "parent": done, "name": "Start again", "height": 48,
                "props": {"fill": "#E9ECEF", "cornerRadius": 8, "layout": "horizontal", "mainAlign": "center", "crossAlign": "center", "sizingH": "fill"}}},
            {"command": "node.create", "params": {"type": "text", "parent": {"$ref": "again"}, "text": "Start again"}},
            {"command": "proto.interaction.add", "params": {"id": {"$ref": "again"}, "trigger": {"type": "onClick"}, "action": {"type": "navigate", "dest": home}}},
        ]}),
    );
    let again = made[0]["id"].clone();

    // Interactions: an overlay opened and closed, a conditional on the variable that either
    // navigates or sets another variable, back, navigate.
    let on_click = |c: &mut Client, layer: &Value, action: Value| {
        c.run("proto.interaction.add", json!({"id": layer, "trigger": {"type": "onClick"}, "action": action}));
    };
    on_click(c, &open_menu, json!({"type": "openOverlay", "dest": menu, "position": "center", "background": "#00000066"}));
    on_click(c, &close_menu, json!({"type": "closeOverlay"}));
    on_click(
        c,
        &go_on,
        json!({"type": "conditional", "blocks": [
            {"expression": "agreed", "actions": [{"type": "navigate", "dest": details}]},
            {"actions": [{"type": "setVariable", "variable": "hint", "value": "Tick the box first"}]},
        ]}),
    );
    on_click(c, &back, json!({"type": "back"}));
    on_click(c, &confirm, json!({"type": "navigate", "dest": done}));
    c.run("proto.flow.add", json!({"start": home, "name": "Collect a parcel"}));
    for (layer, kind) in [(&open_menu, "openOverlay"), (&go_on, "conditional"), (&back, "back"), (&confirm, "navigate"), (&again, "navigate")] {
        let node = c.node(layer);
        assert_eq!(node["interactions"][0]["actions"][0]["type"], kind, "{} stores its interaction: {}", node["name"], node["interactions"]);
    }
    Built { home, details, done, menu, title, hint, toggle, toggle_on, go_on, open_menu, close_menu, back, confirm, again }
}

/// Stage 5: the walks worth keeping, each one person in one situation.
pub(crate) fn store_scenarios(c: &mut Client, b: &Built) -> Vec<Value> {
    let agree_and_go = json!([
        {"click": b.toggle, "expect": {"variables": {"State/agreed": true}, "variants": [{"instance": b.toggle, "variant": "On"}]}, "note": "ticks the box"},
        {"click": b.go_on, "expect": {"screen": b.details}},
    ]);
    let mut to_done = agree_and_go.as_array().unwrap().clone();
    to_done.push(json!({"click": b.confirm, "expect": {"screen": b.done}}));
    let mut and_back = agree_and_go.as_array().unwrap().clone();
    and_back.push(json!({"click": b.back, "expect": {"screen": b.home}}));
    let scenarios = [
        json!({"name": "Collects a parcel", "purpose": "Someone with the code at hand gets from the first screen to the confirmation",
            "start": b.home, "steps": to_done, "expect": {"screen": b.done, "overlays": []}}),
        json!({"name": "Goes on without ticking the box", "purpose": "The first screen says what is missing instead of moving on",
            "start": b.home, "steps": [{"click": b.go_on, "expect": {"screen": b.home, "variables": {"State/hint": "Tick the box first"}}}],
            "expect": {"text": [{"node": b.hint, "equals": "Tick the box first"}]}}),
        json!({"name": "Opens the menu and closes it", "purpose": "The overlay has a way out",
            "start": b.home, "steps": [{"click": b.open_menu, "expect": {"overlays": [b.menu]}}, {"click": b.close_menu, "expect": {"overlays": []}}],
            "expect": {"screen": b.home}}),
        json!({"name": "Changes their mind on the details", "purpose": "Back returns to the first screen",
            "start": b.home, "steps": and_back, "expect": {"screen": b.home}}),
    ];
    scenarios.iter().map(|s| c.run("proto.scenario.add", s.clone())["id"].clone()).collect()
}

/// The resources an agent reads the same things from.
#[test]
fn the_design_and_prototype_resources_read_the_working_document() {
    let mut c = Client::new();
    let listed = c.rpc("resources/list", json!({}));
    let uris: Vec<&str> = listed["result"]["resources"].as_array().unwrap().iter().map(|r| r["uri"].as_str().unwrap()).collect();
    assert_eq!(
        uris,
        [
            "figcraft://document",
            "figcraft://commands",
            "figcraft://comments",
            "figcraft://design",
            "figcraft://design/findings",
            "figcraft://prototype/flow",
            "figcraft://prototype/scenarios"
        ]
    );
    for r in listed["result"]["resources"].as_array().unwrap() {
        assert!(r["description"].as_str().unwrap().len() > 30 && r["mimeType"] == "application/json", "{r}");
    }
    c.run("design.brief.set", json!({"audience": "Couriers"}));
    let b = build(&mut c);
    store_scenarios(&mut c, &b);
    c.run("design.finding.add", json!({"kind": "judgement", "rule": "wording", "message": "Say which box"}));
    let read = |c: &mut Client, uri: &str| -> Value {
        let r = c.rpc("resources/read", json!({"uri": uri}));
        let contents = &r["result"]["contents"][0];
        assert_eq!((&contents["uri"], &contents["mimeType"]), (&json!(uri), &json!("application/json")), "{r}");
        serde_json::from_str(contents["text"].as_str().unwrap()).unwrap()
    };
    // Each is what the tool or command gives.
    assert_eq!(read(&mut c, "figcraft://design"), c.ok("design_context", json!({})));
    assert_eq!(read(&mut c, "figcraft://design/findings"), c.run("design.finding.list", json!({})));
    assert_eq!(read(&mut c, "figcraft://prototype/flow"), c.ok("check_prototype", json!({})));
    assert_eq!(read(&mut c, "figcraft://prototype/scenarios"), c.run("proto.scenario.list", json!({})));
    assert_eq!(read(&mut c, "figcraft://design")["brief"]["audience"], "Couriers");
    assert_eq!(read(&mut c, "figcraft://design/findings")["findings"][0]["message"], "Say which box");
    assert_eq!(read(&mut c, "figcraft://prototype/flow")["starts"][0]["frame"], b.home);
    assert_eq!(read(&mut c, "figcraft://prototype/scenarios")["scenarios"].as_array().unwrap().len(), 4);
    // They follow the working document, not the tab in front.
    let first = about(&c.call("inspect_document", json!({"depth": 0})))["id"].clone();
    let second = c.run("file.new", json!({"name": "Other"}))["id"].clone();
    assert_eq!(read(&mut c, "figcraft://design")["brief"]["audience"], "Couriers", "a new tab in front changes nothing");
    c.ok("use_document", json!({"document": second}));
    assert_eq!(read(&mut c, "figcraft://design")["brief"], json!({}));
    assert_eq!(read(&mut c, "figcraft://prototype/scenarios")["scenarios"], json!([]));
    c.ok("use_document", json!({"document": first}));
    assert_eq!(read(&mut c, "figcraft://design")["counts"]["scenarios"], 4);
    assert_eq!(c.rpc("resources/read", json!({"uri": "figcraft://design/nope"}))["error"]["code"], -32002);
}

/// The verification tools take the document they are told, say which one they read, and refuse
/// what they don't take.
#[test]
fn the_verification_tools_name_their_document_and_check_their_arguments() {
    let mut c = Client::new();
    let b = build(&mut c);
    store_scenarios(&mut c, &b);
    let second = c.run("file.new", json!({"name": "Empty"}))["id"].clone();
    let first = about(&c.call("inspect_document", json!({"depth": 0})));
    assert_eq!(first["front"], false, "the new file is in front; the working document is the one behind it");
    let calls = [
        ("design_context", json!({})),
        ("check_design", json!({})),
        ("check_prototype", json!({})),
        ("simulate_prototype", json!({"start": b.home, "steps": [{"click": b.open_menu}], "image": true})),
        ("run_scenarios", json!({})),
    ];
    for (tool, args) in &calls {
        // Read-only: the working document, at the revision it had.
        let r = c.call(tool, args.clone());
        assert_eq!(r["isError"], false, "{tool}: {r}");
        assert_eq!(about(&r), first, "{tool} reads the working document and leaves its revision");
        // Named: the other file, which has no prototype (a run from a frame it doesn't have fails
        // there, and the failure names that file too).
        let mut named = args.clone();
        named["document"] = second.clone();
        let r = c.call(tool, named);
        let said = if r["isError"] == true { &r["structuredContent"]["error"]["document"] } else { &r["structuredContent"]["document"] };
        assert_eq!(said["id"], second, "{tool}: {r}");
        c.ok("use_document", json!({"document": first["id"]}));
        // An argument it doesn't take is refused, not ignored.
        let mut wrong = args.clone();
        wrong["nope"] = json!(1);
        let e = error_of(&c.call(tool, wrong));
        assert!(e["message"].as_str().unwrap().contains("unknown argument `nope`"), "{tool}: {e}");
        // None of them is a conditional change.
        let mut conditional = args.clone();
        conditional["ifRevision"] = json!(1);
        assert!(error_of(&c.call(tool, conditional))["message"].as_str().unwrap().contains("unknown argument `ifRevision`"), "{tool}");
    }
    // The engine's own refusals come through with their code and the parameter they are about.
    let e = error_of(&c.call("check_design", json!({"rules": ["no.such.rule"]})));
    assert_eq!((&e["code"], &e["command"]), (&json!("badParams"), &json!("design.check")), "{e}");
    assert!(e["message"].as_str().unwrap().contains("contrast.text"), "an unknown rule lists the rules: {e}");
    let e = error_of(&c.call("simulate_prototype", json!({"steps": [{"type": "swipe"}]})));
    assert_eq!((&e["code"], &e["command"]), (&json!("badParams"), &json!("proto.simulate")), "{e}");
    let e = error_of(&c.call("simulate_prototype", json!({"start": b.home})));
    assert!(e["message"].as_str().unwrap().contains("`steps`"), "{e}");
    let e = error_of(&c.call("simulate_prototype", json!({"start": b.home, "steps": [], "image": "yes"})));
    assert_eq!((&e["code"], &e["param"]), (&json!("badParams"), &json!("image")), "{e}");
    let e = error_of(&c.call("simulate_prototype", json!({"start": b.home, "steps": [], "image": true, "scale": -1})));
    assert_eq!(e["param"], "scale", "{e}");
    let e = error_of(&c.call("run_scenarios", json!({"id": "S99"})));
    assert_eq!(e["command"], "proto.scenario.run", "{e}");
    let e = error_of(&c.call("check_design", json!({"record": "yes"})));
    assert_eq!(e["param"], "record", "{e}");
    assert_eq!(error_of(&c.call("check_prototype", json!({"page": 424_242})))["command"], "proto.check");
    // `tools/list` describes each for a model that has never seen this product.
    let tools = c.rpc("tools/list", json!({}));
    let described = |name: &str| -> String {
        let t = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap();
        assert!(t["inputSchema"]["properties"].get("document").is_some(), "{name} takes `document`");
        t["description"].as_str().unwrap().to_string()
    };
    for must in [
        "animation and timing",
        "scroll position",
        "drag direction",
        "device frame and responsive resizing",
        "hit-testing",
        "does not mean a person used it",
    ] {
        assert!(described("simulate_prototype").contains(must), "simulate_prototype doesn't say `{must}`");
    }
    for must in ["deterministic", "standard", "stage", "`skipped`", "not a quality score"] {
        assert!(described("check_design").contains(must), "check_design doesn't say `{must}`");
    }
    for must in ["proposed", "only by a person", "untested", "refuses"] {
        assert!(described("design_context").contains(must), "design_context doesn't say `{must}`");
    }
    assert!(described("run_scenarios").contains("an empty run is not a pass"));
    assert!(described("check_prototype").contains("not that a person will find it"));
    // The canvas tools say what they don't reach.
    for name in ["pointer", "key"] {
        let t = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap();
        let text = t["description"].as_str().unwrap();
        assert!(text.contains("editor canvas") && text.contains("simulate_prototype") && text.contains("run_scenarios"), "{name}: {text}");
    }
}

/// Attached to an app the same tools run the same commands, so they answer the same; what
/// differs is what capabilities say: there is a window and a player, which these tools don't
/// drive.
#[test]
fn the_verification_tools_answer_the_same_attached_to_an_app() {
    let mut headless = Client::new();
    let mut attached = Client::of(Server::new(Box::new(Attached::port(fake_app()).unwrap())));
    let caps = attached.ok("get_capabilities", json!({}));
    assert_eq!((&caps["backend"], &caps["window"], &caps["player"]), (&json!("attached"), &json!(true), &json!(true)), "{caps}");
    for feature in ["designContext", "flowCheck", "scenarios", "prototypeSimulator", "prototypeSnapshot"] {
        assert_eq!(caps["features"][feature], true, "{feature}: {}", caps["features"]);
    }
    // The same file, built the same way in each: the same ids.
    let (h, a) = (build(&mut headless), build(&mut attached));
    assert_eq!((&h.home, &h.go_on, &h.menu), (&a.home, &a.go_on, &a.menu));
    for c in [&mut headless, &mut attached] {
        c.run("design.brief.set", json!({"audience": "People collecting a parcel"}));
        store_scenarios(c, &h);
        c.run("design.stage.set", json!({"stage": "converging"}));
    }
    let steps = json!([{"click": h.toggle}, {"click": h.go_on, "expect": {"screen": h.details}}, {"click": h.back}, {"click": h.open_menu, "expect": {"overlays": [h.menu]}}]);
    let calls = [
        ("design_context", json!({})),
        ("check_prototype", json!({})),
        ("run_scenarios", json!({})),
        ("check_design", json!({})),
        ("simulate_prototype", json!({"start": h.home, "steps": steps, "image": true})),
        ("check_design", json!({"record": true})),
        ("design_context", json!({})),
    ];
    for (tool, args) in calls {
        let (here, there) = (headless.call(tool, args.clone()), attached.call(tool, args.clone()));
        assert_eq!(there["isError"], false, "{tool} {args}: {there}");
        assert_eq!(first_json(&there), first_json(&here), "{tool} {args} answers the same in both backends");
        assert_eq!(about(&there)["revision"], about(&here)["revision"], "{tool}");
        if args.get("image").is_some() {
            assert_eq!((picture(&there), picture(&here)), ((360, 640), (360, 640)));
            assert_eq!(first_json(&there)["state"]["overlays"][0]["id"], h.menu);
        }
    }
    let run = attached.ok("run_scenarios", json!({}));
    assert_eq!((&run["passed"], &run["total"]), (&json!(true), &json!(4)), "{run}");
}
