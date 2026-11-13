//! Review R2, seen from a client: the revision a result names belongs to what it reports (R2-01),
//! an agent waits while a person is in the middle of a gesture (R2-02), the refusal of another's
//! step says `others` is only for when the person asked (R2-03), the key tool acts as the agent
//! (R2-05), the simulator's known differences are said everywhere (R2-08), every changing tool
//! takes `ifRevision` (R2-10), results name the document they acted on (R2-11), and capabilities
//! come from what the build has (R2-12).

use figcraft_engine::{PointerEvent, PointerKind};
use figcraft_mcp::{Backend, Headless, INSTRUCTIONS, PRODUCT_NAME, Server};
use serde_json::{Value, json};

use crate::common::Client;

/// A headless session that plays a person beside the agent: armed with a change, it makes it as
/// the user right after the agent's next command, before the tool's next request.
struct Beside {
    inner: Headless,
    /// After this many more agent commands, run `change` as the user.
    armed: Option<(usize, Value)>,
    /// The revision the agent's command left, read before the person's change.
    revision_kept: Option<(u64, u64)>,
}

impl Beside {
    fn user(&mut self, command: &str, params: &Value) {
        self.inner.session.execute(command, params).unwrap_or_else(|e| panic!("{command}: {e}"));
    }

    fn pointer(&mut self, kind: PointerKind, x: f64, y: f64) {
        let ev = PointerEvent { kind, x, y, sx: x, sy: y, shift: false, alt: false, cmd: false, ctrl: false, zoom: 1.0, pressure: None };
        self.inner.session.pointer(&ev).unwrap();
    }
}

impl Backend for Beside {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            // `test.arm {after, command, params}`: the person's change, after `after` more agent
            // commands.
            "test.arm" => {
                let after = params["after"].as_u64().unwrap_or(1) as usize;
                self.armed = Some((after, json!({"command": params["command"], "params": params["params"]})));
                return Ok(Value::Null);
            }
            "test.user" => {
                self.user(params["command"].as_str().unwrap(), &params["params"]);
                return Ok(Value::Null);
            }
            // The person presses on the canvas and drags, without letting go (`test.drag`), or lets
            // go (`test.release`).
            "test.drag" => {
                let (x, y) = (params["x"].as_f64().unwrap(), params["y"].as_f64().unwrap());
                self.pointer(PointerKind::Down, x, y);
                self.pointer(PointerKind::Move, x + 40.0, y);
                return Ok(Value::Null);
            }
            "test.release" => {
                self.pointer(PointerKind::Up, params["x"].as_f64().unwrap(), params["y"].as_f64().unwrap());
                return Ok(Value::Null);
            }
            _ => {}
        }
        let for_agent = method == "engine.execute" && params.get("agent").is_some();
        let r = self.inner.call(method, params);
        if for_agent && let Some((left, change)) = self.armed.take() {
            if left <= 1 {
                // Its change is the user's: the revision the agent's call left is kept for the
                // tool to read, as the app reads it in the same request.
                let kept = self.inner.take_revision();
                self.user(change["command"].as_str().unwrap(), &change["params"]);
                self.revision_kept = kept;
            } else {
                self.armed = Some((left - 1, change));
            }
        }
        r
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "headless, with a person beside the agent".into()
    }

    fn take_error_detail(&mut self) -> Option<Value> {
        self.inner.take_error_detail()
    }

    fn take_revision(&mut self) -> Option<(u64, u64)> {
        self.revision_kept.take().or_else(|| self.inner.take_revision())
    }
}

fn beside() -> Client {
    Client::of(Server::new(Box::new(Beside { inner: Headless::new(), armed: None, revision_kept: None })))
}

fn about(r: &Value) -> Value {
    r["structuredContent"]["document"].clone()
}

fn error(r: &Value) -> Value {
    assert_eq!(r["isError"], true, "should have failed: {r}");
    r["structuredContent"]["error"].clone()
}

#[test]
fn a_change_made_while_a_tool_runs_is_not_taken_as_seen() {
    let mut c = beside();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    // The person renames the layer right after the agent's move (the tool's second command, after
    // the list of documents it reads first), before the tool reads the layer.
    c.server.backend().call("test.arm", json!({"after": 2, "command": "node.set", "params": {"ids": [id], "props": {"name": "Theirs"}}})).unwrap();
    let r = c.call("set_properties", json!({"ids": [id], "props": {"x": 5}}));
    let seen = about(&r)["revision"].as_u64().unwrap();
    // The result names the revision the agent's move left: the rename came after it.
    let e = error(&c.call("set_properties", json!({"ids": [id], "props": {"y": 5}, "ifRevision": seen})));
    assert_eq!((e["code"].clone(), e["actual"].clone()), (json!("conflict"), json!(seen + 1)), "{e}");
    assert_eq!(e["since"][0]["author"], "user");
    // A read that a change lands in the middle of (here right after its first request) names the
    // revision from before it began.
    let now = about(&c.call("get_node", json!({"id": id})))["revision"].as_u64().unwrap();
    c.server.backend().call("test.arm", json!({"after": 1, "command": "node.set", "params": {"ids": [id], "props": {"name": "Again"}}})).unwrap();
    let read = c.call("inspect_document", json!({"depth": 2}));
    assert_eq!(about(&read)["revision"], json!(now), "{read}");
    let e = error(&c.call("set_properties", json!({"ids": [id], "props": {"y": 9}, "ifRevision": now})));
    assert_eq!(e["code"], "conflict");
}

#[test]
fn an_agent_waits_while_a_person_drags() {
    let mut c = beside();
    let id = c.create(json!({"type": "rectangle", "name": "Card", "x": 0, "y": 0, "width": 100, "height": 100}));
    c.server.backend().call("test.user", json!({"command": "select.set", "params": {"ids": [id]}})).unwrap();
    c.server.backend().call("test.drag", json!({"x": 50, "y": 50})).unwrap();
    let e = error(&c.call("set_properties", json!({"ids": [id], "props": {"name": "Mine"}})));
    assert_eq!((e["code"].clone(), e["by"].clone()), (json!("busy"), json!("user")), "{e}");
    assert!(e["message"].as_str().unwrap().contains("try again shortly"), "{e}");
    let e = error(&c.call("pointer", json!({"kind": "drag", "x": 10, "y": 10, "to": [60, 60]})));
    assert_eq!(e["code"], "busy", "{e}");
    // Reading goes on; once the person lets go, so does the agent.
    c.ok("get_node", json!({"id": id}));
    c.server.backend().call("test.release", json!({"x": 90, "y": 50})).unwrap();
    c.ok("set_properties", json!({"ids": [id], "props": {"name": "Mine"}}));
}

#[test]
fn others_is_only_for_when_the_person_asked() {
    let must = "pass `others: true` only if the person asked you to";
    let mut c = Client::new();
    let tools = c.rpc("tools/list", json!({}));
    for name in ["undo", "redo"] {
        let t = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap();
        assert!(t["description"].as_str().unwrap().contains(must), "{name}: {t}");
        let others = t["inputSchema"]["properties"]["others"]["description"].as_str().unwrap();
        assert!(others.starts_with("Only if the person asked you to take back their step"), "{name}: {others}");
    }
    assert!(INSTRUCTIONS.contains("pass `others: true` only if the person asked you to take back their step"), "{INSTRUCTIONS}");
    // And the refusal itself.
    let id = c.create(json!({"type": "rectangle"}));
    let doc = about(&c.call("get_node", json!({"id": id})))["id"].clone();
    c.server.backend().call("engine.execute", json!({"command": "node.set", "params": {"ids": [id], "props": {"x": 3}}, "document": doc})).unwrap();
    let e = error(&c.call("undo", json!({})));
    assert!(e["message"].as_str().unwrap().contains(&format!("{must} undo their step")), "{e}");
}

#[test]
fn the_key_tool_acts_as_the_agent() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    let doc = about(&c.call("get_node", json!({"id": id})))["id"].clone();
    // The person moves the layer; the agent's Cmd+Z would take that back.
    c.server.backend().call("engine.execute", json!({"command": "node.set", "params": {"ids": [id], "props": {"x": 70}}, "document": doc})).unwrap();
    let r = c.call("key", json!({"key": "Z", "cmd": true}));
    assert_eq!(error(&r)["code"], "otherAuthor", "{r}");
    assert!(crate::common::approx(&c.node(&id)["x"], 70.0));
    // Delete is the agent's step.
    c.ok("select", json!({"ids": [id]}));
    c.ok("key", json!({"key": "Delete"}));
    let steps = c.ok("inspect_document", json!({"depth": 0}))["history"]["steps"].clone();
    assert_eq!(steps[0]["author"], "test (agent)", "{steps}");
}

#[test]
fn every_tool_that_changes_the_document_takes_if_revision() {
    let dir = std::env::temp_dir().join(format!("figcraft-mcp-r2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("r2.figcraft").to_string_lossy().into_owned();
    let mut c = Client::new();
    let text = c.create(json!({"type": "text", "text": "Hello", "name": "Title"}));
    let rev = about(&c.call("get_node", json!({"id": text})))["revision"].as_u64().unwrap();
    let doc = about(&c.call("get_node", json!({"id": text})))["id"].clone();
    c.server
        .backend()
        .call("engine.execute", json!({"command": "node.set", "params": {"ids": [text], "props": {"x": 70}}, "document": doc}))
        .unwrap();
    let before = c.ok("inspect_document", json!({"depth": 3}));
    for (tool, args) in [
        ("pointer", json!({"kind": "drag", "x": 75, "y": 5, "to": [200, 200], "ifRevision": rev})),
        ("key", json!({"key": "Delete", "ifRevision": rev})),
        ("type_text", json!({"id": text, "text": "Bye", "ifRevision": rev})),
        ("check_design", json!({"record": true, "ifRevision": rev})),
        ("save_file", json!({"path": path, "ifRevision": rev})),
    ] {
        let e = error(&c.call(tool, args.clone()));
        assert_eq!((e["code"].clone(), e["expected"].clone()), (json!("conflict"), json!(rev)), "{tool}: {e}");
    }
    assert_eq!(c.ok("inspect_document", json!({"depth": 3})), before, "nothing changed");
    assert!(!std::path::Path::new(&path).exists(), "nothing was saved");
    // A check that records nothing changes nothing: it takes no `ifRevision`.
    assert_eq!(error(&c.call("check_design", json!({"ifRevision": rev + 1})))["param"], "ifRevision");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn results_name_the_document_they_acted_on() {
    let mut c = Client::new();
    let first = about(&c.call("inspect_document", json!({"depth": 0})));
    let second = c.run("file.new", json!({"name": "Second"}))["id"].clone();
    c.ok("use_document", json!({"document": first["id"]}));
    // Closing another tab by its place names the tab that closed; the working document stays.
    let r = c.call("run_command", json!({"command": "file.close", "params": {"index": 1}}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(about(&r), json!({"id": second, "name": "Second", "closed": true}), "{r}");
    assert_eq!(r["structuredContent"]["workingDocument"]["id"], first["id"], "{r}");
    // Switching the tab in front names the tab it switched to.
    let third = c.run("file.new", json!({"name": "Third"}))["id"].clone();
    c.ok("use_document", json!({"document": first["id"]}));
    let r = c.call("run_command", json!({"command": "doc.switch", "params": {"index": 1}}));
    assert_eq!((about(&r)["id"].clone(), about(&r)["front"].clone()), (third, json!(true)), "{r}");
    assert_eq!(about(&c.call("inspect_document", json!({"depth": 0})))["id"], first["id"], "still the working document");
}

#[test]
fn a_resource_read_first_settles_the_working_document() {
    let mut c = Client::new();
    let r = c.rpc("resources/read", json!({"uri": "figcraft://design"}));
    assert!(r["result"].is_object(), "{r}");
    // A new file in front afterwards (made by the user) doesn't move it.
    c.server.backend().call("engine.execute", json!({"command": "file.new", "params": {"name": "The user's"}})).unwrap();
    let doc = about(&c.call("inspect_document", json!({"depth": 0})));
    assert_eq!((doc["name"].clone(), doc["front"].clone()), (json!("Untitled"), json!(false)), "{doc}");
}

#[test]
fn capabilities_come_from_what_the_build_has() {
    let mut c = Client::new();
    let caps = c.ok("get_capabilities", json!({}));
    assert_eq!(caps["product"], PRODUCT_NAME);
    assert_eq!(caps["formats"]["open"], json!(figcraft_engine::cmd::file::OPEN_FORMATS));
    assert_eq!(caps["formats"]["save"], json!(figcraft_engine::cmd::file::SAVE_FORMATS));
    assert_eq!(caps["formats"]["export"], json!(figcraft_engine::cmd::file::export_formats()));
    assert_eq!((caps["window"].clone(), caps["player"].clone()), (json!(false), json!(false)), "headless: no window, no player");
    // A design-context section that doesn't exist is refused, naming those that do.
    let r = c.call("run_command", json!({"command": "view.designContext", "params": {"section": "nope"}}));
    assert!(error(&r)["message"].as_str().unwrap().contains("the sections are brief, assumptions"), "{r}");
}

#[test]
fn the_simulators_known_differences_are_said_wherever_it_is_used() {
    let known = figcraft_engine::proto_verify::KNOWN_DIFFERENCES;
    // One per parity case of the app's simulator-player tests (`difference_<key>`).
    assert_eq!(known.len(), 5);
    let mut keys: Vec<&str> = known.iter().map(|(k, _)| *k).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), known.len(), "each case once");
    let mut c = Client::new();
    let tools = c.rpc("tools/list", json!({}));
    let described = |name: &str| {
        tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap()["description"].as_str().unwrap().to_string()
    };
    let p = figcraft_mcp::PROMPTS.iter().find(|p| p.name == "verify_prototype").unwrap();
    let args: serde_json::Map<String, Value> = p.args.iter().map(|a| (a.name.to_string(), json!("x"))).collect();
    let prompt = c.rpc("prompts/get", json!({"name": "verify_prototype", "arguments": args}))["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    for (_, says) in known {
        for (place, text) in [
            ("simulate_prototype", described("simulate_prototype")),
            ("run_scenarios", described("run_scenarios")),
            ("verify_prototype", prompt.clone()),
        ] {
            assert!(text.contains(says), "{place} doesn't say `{says}`");
        }
    }
    // Every simulated result lists them.
    let frame = c.create(json!({"type": "frame", "name": "A", "width": 100, "height": 100}));
    let sim = c.ok("simulate_prototype", json!({"start": frame, "steps": []}));
    let said: Vec<&str> = known.iter().map(|(_, s)| *s).collect();
    assert_eq!(sim["knownDifferences"], json!(said), "{sim}");
    assert_eq!(c.ok("run_scenarios", json!({}))["knownDifferences"], json!(said));
    assert_eq!(c.run("proto.snapshot", json!({"start": frame}))["knownDifferences"], json!(said));
}
