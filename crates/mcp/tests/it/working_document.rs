//! What lets an agent work beside a person without guessing: the working document (calls never
//! follow the tab in front), every result naming its document and revision, conditional changes
//! (`ifRevision`), undo that stays in the agent's own work, errors a program can read,
//! capabilities read from what the build has, and no success for what did not happen.

use std::path::PathBuf;

use crate::common::{Client, approx};
use figcraft_engine::file_access::Roots;
use figcraft_mcp::{Backend, Server, TOOL_NAMES};
use serde_json::{Value, json};

/// A folder of its own under the temp dir, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("figcraft-mcp-working-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    fn p(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }

    /// A saved file holding one rectangle per name in `layers`.
    fn saved(&self, file: &str, layers: &[&str]) -> String {
        let mut c = Client::new();
        for name in layers {
            c.create(json!({"type": "rectangle", "name": name, "width": 10, "height": 10}));
        }
        let path = self.p(file);
        c.ok("save_file", json!({"path": path}));
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The document a result says it is about, checked to be said twice the same: for a program
/// (`structuredContent`) and for the model (the last text block).
fn about(r: &Value) -> Value {
    let doc = r["structuredContent"]["document"].clone();
    assert!(doc.is_object(), "the result names no document: {r}");
    let last = r["content"].as_array().unwrap().last().unwrap()["text"].as_str().unwrap();
    let said: Value = serde_json::from_str(last).unwrap();
    assert_eq!(said["document"], doc, "the last block and structuredContent disagree: {r}");
    doc
}

/// The error of a failed call, checked the same way: the first block is the text people read, the
/// last one `{"error": …}` for the model, `structuredContent.error` the same for a program.
fn error_of(r: &Value) -> Value {
    assert_eq!(r["isError"], true, "should have failed: {r}");
    let e = r["structuredContent"]["error"].clone();
    assert!(e["code"].is_string() && e["message"].is_string(), "no structured error: {r}");
    let blocks = r["content"].as_array().unwrap();
    assert_eq!(blocks[0]["text"], e["message"], "the text block is the message: {r}");
    let said: Value = serde_json::from_str(blocks.last().unwrap()["text"].as_str().unwrap()).unwrap();
    assert_eq!(said["error"], e, "{r}");
    e
}

fn first_json(r: &Value) -> Value {
    let text = r["content"].as_array().unwrap().iter().find_map(|c| c.get("text").and_then(Value::as_str)).unwrap();
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

/// A change the user makes in the app, behind the agent's back: a command run without an agent.
fn as_user(c: &mut Client, command: &str, params: Value, document: &Value) -> Value {
    c.server.backend().call("engine.execute", json!({"command": command, "params": params, "document": document})).unwrap()
}

fn layer_names(c: &mut Client, document: &Value) -> Vec<String> {
    let doc = c.ok("inspect_document", json!({"depth": 1, "fields": ["name"], "document": document}));
    doc["pages"][0]["children"].as_array().map(|a| a.iter().map(|n| n["name"].as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

/// F-02, the investigator's probe: file A is read, then file B is opened and is in front. An edit
/// by an id read from A, sent without `document`, used to change whatever layer has that id in B
/// and answer as if nothing was odd. It still acts on B (the working document since `open_file`),
/// but now says so, so a wrong assumption shows; and nothing follows the tab in front any more.
#[test]
fn an_id_based_edit_names_the_document_it_changed() {
    let dir = Dir::new("ids");
    let (a, b) = (dir.saved("alpha.figcraft", &["InA"]), dir.saved("beta.figcraft", &["InB"]));
    let mut c = Client::new();
    let ra = c.call("open_file", json!({"path": a}));
    let da = about(&ra);
    assert_eq!((da["name"].clone(), da["front"].clone()), (json!("alpha"), json!(true)), "{da}");
    let opened = first_json(&ra);
    assert_eq!(
        (opened["opened"]["id"].clone(), opened["document"]["id"].clone()),
        (da["id"].clone(), da["id"].clone()),
        "open_file says which document"
    );
    assert_eq!(opened["document"]["revision"], da["revision"]);
    let node = opened["document"]["pages"][0]["children"][0]["id"].clone();
    let rb = c.call("open_file", json!({"path": b}));
    let db = about(&rb);
    assert_ne!(da["id"], db["id"]);
    assert_eq!((db["name"].clone(), db["front"].clone()), (json!("beta"), json!(true)));

    // The agent still thinks of A. The call lands in B, and the result says B.
    let r = c.call("set_properties", json!({"ids": [node], "props": {"name": "Renamed"}}));
    assert_eq!(r["isError"], false, "{r}");
    let said = about(&r);
    assert_eq!((said["id"].clone(), said["name"].clone()), (db["id"].clone(), json!("beta")), "the result tells which file was changed");
    assert_eq!(said["revision"], json!(db["revision"].as_u64().unwrap() + 1));
    assert_eq!(layer_names(&mut c, &da["id"]), ["InA"], "A was not touched");
    assert_eq!(layer_names(&mut c, &db["id"]), ["Renamed"]);

    // Naming A acts on A and makes it the working document; B stays the tab in front.
    c.ok("use_document", json!({"document": db["id"]}));
    let r = c.call("set_properties", json!({"ids": [node], "props": {"name": "InA2"}, "document": da["id"]}));
    let said = about(&r);
    assert_eq!((said["id"].clone(), said["front"].clone()), (da["id"].clone(), json!(false)), "{said}");
    let r = c.call("get_node", json!({"id": node, "depth": 0}));
    assert_eq!(about(&r)["id"], da["id"], "later calls stay on the document named last");
    assert_eq!(first_json(&r)["name"], "InA2");

    // use_document: by "front", by id and by path.
    let front = c.ok("use_document", json!({"document": "front"}));
    assert_eq!(front["workingDocument"]["id"], db["id"], "{front}");
    assert_eq!(front["documents"].as_array().unwrap().len(), 3, "the untitled one, alpha and beta: {front}");
    assert_eq!(about(&c.call("get_node", json!({"id": node, "depth": 0})))["id"], db["id"]);
    assert_eq!(c.ok("use_document", json!({"document": a}))["workingDocument"]["id"], da["id"]);
    assert_eq!(c.ok("use_document", json!({"document": db["id"]}))["workingDocument"]["name"], "beta");
    // The document resource is the working document's too.
    let read = c.rpc("resources/read", json!({"uri": "figcraft://document"}));
    let doc: Value = serde_json::from_str(read["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(doc["id"], db["id"]);
}

/// The other half of F-02: the front changes (here by the agent's own `file.new`; in the app, by
/// the user clicking a tab) and calls without `document` used to follow it.
#[test]
fn calls_do_not_follow_the_tab_in_front() {
    let mut c = Client::new();
    let first = about(&c.call("create_node", json!({"type": "rectangle", "name": "One"})));
    assert_eq!(first["front"], true);
    // A new file comes to the front. A file the agent makes is its working document from then on,
    // as one it opens is, and the result says so (review R2-11).
    let made = c.call("run_command", json!({"command": "file.new", "params": {"name": "Second"}}));
    let said = about(&made);
    let second = first_json(&made)["id"].clone();
    assert!(second.is_u64() && second != first["id"], "file.new says which document it made: {made}");
    assert_eq!((said["id"].clone(), said["front"].clone()), (second.clone(), json!(true)), "{said}");
    assert_eq!(made["structuredContent"]["workingDocumentChanged"], json!({"from": first["id"], "to": second}), "{made}");
    // Back to the first, which stays the working document whatever is in front.
    c.ok("use_document", json!({"document": first["id"]}));
    let r = c.call("create_node", json!({"type": "rectangle", "name": "Two"}));
    assert_eq!(about(&r)["id"], first["id"]);
    assert_eq!(layer_names(&mut c, &first["id"]), ["One", "Two"]);
    assert!(layer_names(&mut c, &second).is_empty());
    // doc.list says which tab is in front even when it is asked for a file behind it.
    c.ok("use_document", json!({"document": first["id"]}));
    let list = c.run("doc.list", json!({}));
    assert_eq!((list[0]["active"].clone(), list[1]["active"].clone()), (json!(false), json!(true)), "{list}");
    assert_eq!((list[0]["revision"].clone(), list[1]["revision"].clone()), (json!(3), json!(1)), "{list}");
    // Switching tabs is not choosing the working document; use_document is.
    c.run("doc.switch", json!({"index": 0}));
    assert_eq!(about(&c.call("inspect_document", json!({"depth": 0})))["front"], true);
    c.run("doc.switch", json!({"index": 1}));
    assert_eq!(about(&c.call("inspect_document", json!({"depth": 0})))["id"], first["id"]);
    assert_eq!(c.ok("use_document", json!({"document": "front"}))["workingDocument"]["id"], second);
}

/// Reverting gives the file a new id in the engine; to the agent it is still the document it works
/// on, at its next revision.
#[test]
fn a_reverted_file_stays_the_working_document() {
    let dir = Dir::new("revert");
    let path = dir.saved("alpha.figcraft", &["InA"]);
    let mut c = Client::new();
    let opened = about(&c.call("open_file", json!({"path": path})));
    let edited = about(&c.call("create_node", json!({"type": "rectangle", "name": "Unsaved"})));
    assert_eq!((edited["id"].clone(), edited["revision"].clone()), (opened["id"].clone(), json!(2)));
    for (tool, args) in [("run_command", json!({"command": "file.revert"})), ("open_file", json!({"path": path, "reload": true}))] {
        let before = about(&c.call("inspect_document", json!({"depth": 0})));
        let r = c.call(tool, args);
        assert_eq!(r["isError"], false, "{tool}: {r}");
        let after = about(&r);
        assert_ne!(after["id"], before["id"], "{tool}: the engine gives the reverted file a new id");
        assert_eq!((after["name"].clone(), after["front"].clone()), (json!("alpha"), json!(true)), "{tool}");
        assert_eq!(after["revision"], json!(before["revision"].as_u64().unwrap() + 1), "{tool}: its next revision, not 1 again");
        // The next call goes to it without being told.
        let next = c.call("inspect_document", json!({"depth": 1}));
        assert_eq!(about(&next)["id"], after["id"], "{tool}");
        assert_eq!(layer_names(&mut c, &after["id"]), ["InA"], "{tool}: as saved");
    }
}

#[test]
fn a_closed_working_document_fails_the_call_and_lists_the_open_ones() {
    let dir = Dir::new("closed");
    let (a, b) = (dir.saved("alpha.figcraft", &["InA"]), dir.saved("beta.figcraft", &["InB"]));
    let mut c = Client::new();
    let da = about(&c.call("open_file", json!({"path": a})));
    let db = about(&c.call("open_file", json!({"path": b})));
    // The working document is closed (by the agent here; the user closing its tab is the same).
    let closed = c.call("run_command", json!({"command": "file.close"}));
    assert_eq!(closed["isError"], false, "{closed}");
    assert_eq!(about(&closed), json!({"id": db["id"], "name": "beta", "closed": true}));
    for (tool, args) in [
        ("create_node", json!({"type": "rectangle", "name": "Lost"})),
        ("set_properties", json!({"ids": [2], "props": {"name": "Lost"}})),
        ("inspect_document", json!({})),
        ("undo", json!({})),
        ("pointer", json!({"x": 1, "y": 1})),
    ] {
        let r = c.call(tool, args);
        let e = error_of(&r);
        let text = e["message"].as_str().unwrap();
        assert!(
            text.contains("the working document \"beta\"") && text.contains("no longer open") && text.contains("nothing was done"),
            "{tool}: {text}"
        );
        assert!(text.contains(&format!("\"alpha\" (id {})", da["id"])) && text.contains("use_document"), "{tool}: {text}");
        assert_eq!(e["code"], "noDocument", "{tool}");
        assert_eq!(e["document"], json!({"id": db["id"], "name": "beta", "closed": true}), "{tool}");
        assert!(e["documents"].as_array().unwrap().iter().any(|d| d["id"] == da["id"]), "{tool}: {e}");
    }
    // Nothing was applied anywhere: not to the file left in front either.
    assert_eq!(layer_names(&mut c, &da["id"]), ["InA"]);
    // Naming a document that is not open says the same, and changes nothing about the working one.
    c.ok("use_document", json!({"document": da["id"]}));
    for missing in [json!(424_242), json!(dir.p("nope.figcraft"))] {
        for (tool, args) in [("inspect_document", json!({"document": missing})), ("use_document", json!({"document": missing}))] {
            let e = error_of(&c.call(tool, args));
            assert_eq!(e["code"], "noDocument", "{tool} {missing}: {e}");
            let text = e["message"].as_str().unwrap();
            assert!(text.contains("no open document") && text.contains("\"alpha\""), "{tool} {missing}: {text}");
        }
    }
    let e = error_of(&c.call("inspect_document", json!({"document": true})));
    assert_eq!((e["code"].clone(), e["param"].clone()), (json!("badParams"), json!("document")), "{e}");
    assert_eq!(about(&c.call("inspect_document", json!({"depth": 0})))["id"], da["id"]);
}

#[test]
fn every_result_about_a_document_names_it_and_its_revision() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "text", "text": "Hello", "name": "Title"}));
    let doc = about(&c.call("inspect_document", json!({"depth": 0})));
    let mut revision = doc["revision"].as_u64().unwrap();
    assert_eq!(doc["name"], "Untitled");
    // (tool, arguments, whether it changes the document)
    let calls = [
        ("get_node", json!({"id": id}), false),
        ("inspect_document", json!({"depth": 1}), false),
        ("select", json!({"ids": [id]}), false),
        ("export", json!({"ids": [id], "format": "svg"}), false),
        ("export", json!({"ids": [id], "format": "png"}), false),
        ("screenshot", json!({}), false),
        ("set_properties", json!({"ids": [id], "props": {"x": 40}}), true),
        ("create_node", json!({"type": "rectangle"}), true),
        ("run_command", json!({"command": "node.set", "params": {"ids": [id], "props": {"y": 9}}}), true),
        ("run_command", json!({"command": "doc.list"}), false),
        (
            "batch",
            json!({"commands": [{"command": "node.set", "params": {"ids": [id], "props": {"x": 1}}}, {"command": "node.set", "params": {"ids": [id], "props": {"y": 2}}}]}),
            true,
        ),
        ("undo", json!({}), true),
        ("redo", json!({}), true),
        ("key", json!({"key": "Escape"}), false),
        ("pointer", json!({"x": 900, "y": 900}), false),
        ("type_text", json!({"id": id, "text": "Bye"}), true),
    ];
    for (tool, args, changes) in calls {
        let r = c.call(tool, args.clone());
        assert_eq!(r["isError"], false, "{tool} {args}: {r}");
        let said = about(&r);
        revision += u64::from(changes);
        assert_eq!(said, json!({"id": doc["id"], "name": "Untitled", "revision": revision, "front": true}), "{tool} {args}");
        // What the result held before is still its first block.
        assert!(r["content"].as_array().unwrap().len() >= 2, "{tool}");
    }
    // Tools that are not about a document don't name one.
    for tool in ["list_commands", "inspect_ui", "wait_for_changes"] {
        let r = c.call(tool, json!({}));
        assert!(r.get("structuredContent").is_none() && r["content"].as_array().unwrap().len() == 1, "{tool}: {r}");
    }
    // A long or odd name can't push a response past its size limit.
    c.run("file.rename", json!({"name": format!("{}\n\u{7}", "é".repeat(500))}));
    let said = about(&c.call("inspect_document", json!({"depth": 0})));
    assert_eq!(said["name"], "é".repeat(48));
}

#[test]
fn a_change_is_refused_when_someone_else_changed_the_document_since() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    let seen = about(&c.call("get_node", json!({"id": id})));
    let (doc, rev) = (seen["id"].clone(), seen["revision"].as_u64().unwrap());
    // The user moves the layer in the app while the agent thinks.
    as_user(&mut c, "node.set", json!({"ids": [id], "props": {"x": 300}}), &doc);
    let stale = [
        ("set_properties", json!({"ids": [id], "props": {"x": 5}, "ifRevision": rev})),
        ("create_node", json!({"type": "rectangle", "ifRevision": rev})),
        ("run_command", json!({"command": "node.set", "params": {"ids": [id], "props": {"x": 5}}, "ifRevision": rev})),
        ("batch", json!({"commands": [{"command": "node.set", "params": {"ids": [id], "props": {"x": 5}}}], "ifRevision": rev})),
        ("undo", json!({"ifRevision": rev, "others": true})),
        ("redo", json!({"ifRevision": rev})),
    ];
    for (tool, args) in stale {
        let r = c.call(tool, args.clone());
        let e = error_of(&r);
        assert_eq!((e["code"].clone(), e["expected"].clone(), e["actual"].clone()), (json!("conflict"), json!(rev), json!(rev + 1)), "{tool}: {e}");
        assert_eq!(e["since"], json!([{"revision": rev + 1, "label": "Set properties", "author": "user"}]), "{tool}");
        assert_eq!(e["document"], json!({"id": doc, "name": "Untitled", "revision": rev + 1, "front": true}), "{tool}");
        let text = e["message"].as_str().unwrap();
        assert!(text.contains("conflict") && text.contains("Set properties (user)") && text.contains(&format!("ifRevision {}", rev + 1)), "{text}");
    }
    assert!(approx(&c.node(&id)["x"], 300.0), "the user's edit is as it was");
    assert_eq!(about(&c.call("get_node", json!({"id": id})))["revision"], rev + 1, "refused changes changed nothing");
    // With the revision the error named, the change goes through.
    let r = c.call("set_properties", json!({"ids": [id], "props": {"y": 5}, "ifRevision": rev + 1}));
    assert_eq!(about(&r)["revision"], rev + 2, "{r}");
    // It is a number, and only the changing tools take it.
    let e = error_of(&c.call("set_properties", json!({"ids": [id], "props": {"y": 5}, "ifRevision": "now"})));
    assert_eq!((e["code"].clone(), e["param"].clone()), (json!("badParams"), json!("ifRevision")), "{e}");
    let e = error_of(&c.call("select", json!({"ids": [id], "ifRevision": rev})));
    assert!(e["message"].as_str().unwrap().contains("unknown argument `ifRevision`"), "{e}");
}

#[test]
fn undo_stays_in_the_agents_own_work() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    let doc = about(&c.call("get_node", json!({"id": id})))["id"].clone();
    as_user(&mut c, "node.set", json!({"ids": [id], "props": {"name": "The user's name"}}), &doc);
    let steps = c.ok("inspect_document", json!({"depth": 0}))["history"]["steps"].clone();
    assert_eq!(steps, json!([{"label": "Set properties", "author": "user"}, {"label": "Create layer", "author": "test (agent)"}]));
    // The agent's undo would take back the user's rename: refused, naming the step and whose it is.
    let e = error_of(&c.call("undo", json!({})));
    assert_eq!((e["code"].clone(), e["label"].clone(), e["author"].clone()), (json!("otherAuthor"), json!("Set properties"), json!("user")), "{e}");
    // `others` is for when the person asked (review R2-03).
    assert!(e["message"].as_str().unwrap().contains("pass `others: true` only if the person asked you to undo their step"), "{e}");
    assert_eq!(c.node(&id)["name"], "The user's name");
    // Asked to, it does; and its own step it undoes without asking.
    assert_eq!(c.ok("undo", json!({"others": true})), json!({"undone": true}));
    assert_eq!(c.ok("undo", json!({})), json!({"undone": true}));
    assert_eq!(c.ok("redo", json!({})), json!({"redone": true}));
    assert_eq!(error_of(&c.call("redo", json!({})))["code"], "otherAuthor");
    // A gesture the agent makes is its own step too.
    let moved = c.ok("pointer", json!({"kind": "drag", "x": 5, "y": 5, "to": [80, 60]}));
    assert_eq!(moved["selection"], json!([id]), "{moved}");
    let steps = c.ok("inspect_document", json!({"depth": 0}))["history"]["steps"].clone();
    assert_eq!(steps[0]["author"], "test (agent)", "{steps}");
    assert_eq!(c.ok("undo", json!({})), json!({"undone": true}));
}

#[test]
fn a_failed_call_says_what_failed_for_a_program_too() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    let e = error_of(&c.call("run_command", json!({"command": "nope.nope"})));
    assert_eq!((e["code"].clone(), e["command"].clone()), (json!("unknownCommand"), json!("nope.nope")), "{e}");
    let e = error_of(&c.call("get_node", json!({"id": 424_242})));
    assert_eq!((e["code"].clone(), e["node"].clone()), (json!("noSuchNode"), json!(424_242)), "{e}");
    let e = error_of(&c.call("set_properties", json!({"ids": [424_242], "props": {"x": 1}})));
    assert_eq!((e["code"].clone(), e["node"].clone()), (json!("noSuchNode"), json!(424_242)), "{e}");
    let e = error_of(&c.call("screenshot", json!({"ids": [424_242]})));
    assert_eq!(e["code"], "noSuchNode", "{e}");
    let e = error_of(&c.call("run_command", json!({"command": "file.rename", "params": {}})));
    assert_eq!((e["code"].clone(), e["command"].clone(), e["param"].clone()), (json!("badParams"), json!("file.rename"), json!("name")), "{e}");
    let e = error_of(&c.call("run_command", json!({"command": "file.revert"})));
    assert_eq!(e["code"], "notAvailable", "{e}");
    // The tool's own checks of its arguments.
    let e = error_of(&c.call("set_properties", json!({"ids": [id], "props": {}})));
    assert_eq!((e["code"].clone(), e["param"].clone()), (json!("badParams"), json!("props")), "{e}");
    let e = error_of(&c.call("create_node", json!({"type": "rectangle", "nope": 1})));
    assert_eq!(e["code"], "badParams", "{e}");
    let e = error_of(&c.call("type_text", json!({"text": "x"})));
    assert_eq!(e["code"], "notAvailable", "not editing text: {e}");
    // Every error about a document names it.
    assert_eq!(e["document"]["name"], "Untitled", "{e}");
    // An unknown tool is a protocol error, with the same object as its data.
    let r = c.rpc("tools/call", json!({"name": "nope", "arguments": {}}));
    assert_eq!(
        (r["error"]["code"].clone(), r["error"]["data"]["code"].clone(), r["error"]["data"]["tool"].clone()),
        (json!(-32602), json!("unknownTool"), json!("nope")),
        "{r}"
    );
    assert_eq!(figcraft_mcp::call_tool(c.server.backend(), "nope", &json!({})).structured.unwrap()["error"]["code"], "unknownTool");
}

/// F-06: a batch that aborted on "no such node" or "unknown command" didn't say which step.
#[test]
fn a_failed_batch_names_the_step_and_its_command() {
    let mut c = Client::new();
    let id = c.create(json!({"type": "rectangle", "name": "Card"}));
    let ok = json!({"command": "node.set", "params": {"ids": [id], "props": {"x": 7}}});
    for (bad, code) in [
        (json!({"command": "node.set", "params": {"ids": [424_242], "props": {"x": 1}}}), "noSuchNode"),
        (json!({"command": "nope.nope"}), "unknownCommand"),
        (json!({"command": "node.set", "params": {"ids": [id], "props": {"nope": 1}}}), "badParams"),
        (json!({"command": "node.set", "params": {"ids": [{"$ref": "missing"}], "props": {"x": 1}}}), "badParams"),
        (json!({"command": "proto.present"}), "unsupportedHere"),
    ] {
        let command = bad["command"].clone();
        let e = error_of(&c.call("batch", json!({"commands": [ok, ok, bad]})));
        assert_eq!((e["code"].clone(), e["step"].clone(), e["command"].clone()), (json!(code), json!(2), command.clone()), "{e}");
        let text = e["message"].as_str().unwrap();
        assert!(text.contains("step 2") && text.contains(command.as_str().unwrap()), "{text}");
        assert!(approx(&c.node(&id)["x"], 0.0), "the steps before it were rolled back");
    }
    // What the tool refuses before anything runs names the step too.
    let e = error_of(&c.call("batch", json!({"commands": [ok, {"command": "node.set", "params": 3}]})));
    assert_eq!((e["code"].clone(), e["step"].clone(), e["command"].clone()), (json!("badParams"), json!(1), json!("node.set")), "{e}");
    let e = error_of(&c.call("batch", json!({"commands": [{"command": 3}]})));
    assert_eq!((e["code"].clone(), e["step"].clone()), (json!("badParams"), json!(0)), "{e}");
}

/// `maxBytes` is the whole message: naming the document after the tree must not push a response
/// that was cut to fit back over the limit, whatever the document is called.
#[test]
fn naming_the_document_keeps_a_cut_response_within_its_size_limit() {
    let mut c = Client::new();
    let frame = c.create(json!({"type": "frame", "width": 400, "height": 400}));
    for i in 0..80 {
        c.create(json!({"type": "rectangle", "parent": frame, "name": format!("Layer {i}"), "width": 10, "height": 10}));
    }
    // Characters that are escaped once in the JSON and again in the text block around it, then
    // the widest ones.
    for name in ["\"\\".repeat(60), "😀".repeat(120)] {
        c.run("file.rename", json!({"name": name}));
        for max in [6_000, 12_000, 40_000] {
            let reply = c.rpc("tools/call", json!({"name": "inspect_document", "arguments": {"depth": 4, "maxBytes": max}}));
            let wire = serde_json::to_string(&reply).unwrap().len() + 1;
            assert!(wire <= max, "the message takes {wire} bytes, over maxBytes {max}");
            let said = about(&reply["result"]);
            assert_eq!(said["name"].as_str().unwrap().chars().count(), 48, "{said}");
        }
    }
}

/// F-03: presenting answered success without a window and did nothing.
#[test]
fn without_a_window_the_player_commands_fail_and_say_what_to_use() {
    let mut c = Client::new();
    c.create(json!({"type": "frame", "width": 100, "height": 100}));
    for (command, params) in [
        ("proto.present", json!({})),
        ("proto.preview", json!({})),
        ("proto.closePreview", json!({})),
        ("proto.presentOptions", json!({"scale": "fit"})),
    ] {
        let e = error_of(&c.call("run_command", json!({"command": command, "params": params})));
        assert_eq!((e["code"].clone(), e["command"].clone()), (json!("unsupportedHere"), json!(command)), "{e}");
        let text = e["message"].as_str().unwrap();
        assert!(text.contains("proto.simulate") && text.contains("proto.scenario.run"), "{text}");
    }
    // A command that only asks for some other UI succeeds, and says the request went nowhere.
    let r = c.run("doc.home", json!({}));
    assert_eq!(r, json!({"ok": true, "ui": "dropped (home): no window in this backend"}));
    // One that does its work and asks for UI besides keeps its result (here a list).
    assert_eq!(c.run("file.versionHistory", json!({})), json!([]));
    // A command that asks for no UI says nothing about it.
    assert!(c.run("select.none", json!({})).get("ui").is_none());
}

#[test]
fn capabilities_say_what_this_build_really_has() {
    let mut c = Client::new();
    let has = |id: &str| figcraft_engine::all_commands().iter().any(|c| c.id == id);
    let r = c.call("get_capabilities", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    let caps = first_json(&r);
    assert_eq!((caps["backend"].clone(), caps["window"].clone(), caps["player"].clone()), (json!("headless"), json!(false), json!(false)), "{caps}");
    assert_eq!((caps["product"].clone(), caps["protocol"].clone()), (json!("FigCraft"), json!(figcraft_mcp::PROTOCOL_VERSION)));
    assert_eq!(caps["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(caps["roots"], Value::Null);
    assert_eq!(caps["commands"], figcraft_engine::all_commands().len());
    // The working document is settled by the answer: it is what later calls act on.
    let working = caps["workingDocument"].clone();
    assert_eq!(working, about(&r));
    assert_eq!(caps["documents"], json!([working]));
    assert_eq!(about(&c.call("inspect_document", json!({"depth": 0}))), working);
    // Features are read from the commands that are registered, not written down as true.
    let f = &caps["features"];
    for (feature, command) in [
        ("prototypeSimulator", "proto.simulate"),
        ("prototypeSnapshot", "proto.snapshot"),
        ("scenarios", "proto.scenario.run"),
        ("flowCheck", "proto.check"),
        ("designContext", "design.get"),
    ] {
        assert_eq!(f[feature], has(command), "{feature} is there exactly when `{command}` is");
        // This build has the simulator, the flow check and the scenarios beside the design context.
        assert_eq!(f[feature], true, "{feature}: {f}");
    }
    let rules = c.run("design.check.rules", json!({}));
    let ids: Vec<Value> = rules.as_array().unwrap().iter().map(|r| r["id"].clone()).collect();
    assert_eq!(f["checks"], json!(ids));
    // The design rules and, with the flow check in the build, the flow and scenario rules.
    assert!(ids.len() >= 17, "{ids:?}");
    for rule in [
        "contrast.text",
        "text.clipped",
        "target.small",
        "context.missingLink",
        "flow.brokenTarget",
        "flow.deadEnd",
        "flow.trappedOverlay",
        "flow.unreachable",
        "flow.noStart",
        "scenario.fails",
        "scenario.none",
        "scenario.unvisited",
    ] {
        assert!(ids.contains(&json!(rule)), "`{rule}` is not among the checks: {ids:?}");
    }
    for on in ["workingDocument", "conflictDetection", "historyAuthors", "structuredErrors", "engineErrorCodes", "appIdentity"] {
        assert_eq!(f[on], true, "{on}: {f}");
    }
    assert_eq!(caps["limits"]["maxLineBytes"], 32 * 1024 * 1024);
    assert!(
        caps["formats"]["export"].as_array().unwrap().contains(&json!("png"))
            && caps["formats"]["open"].as_array().unwrap().contains(&json!("figcraft"))
    );
    // With automation roots it says which folders.
    let dir = Dir::new("caps");
    for sub in ["in", "out"] {
        std::fs::create_dir_all(dir.0.join(sub)).unwrap();
    }
    let mut confined = Server::headless().with_roots(Some(Roots::new(&[dir.p("in")], &[dir.p("out")]).unwrap()));
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_capabilities", "arguments": {}}}).to_string();
    let reply: Value = serde_json::from_str(&confined.handle_line(&line).unwrap()).unwrap();
    let roots = first_json(&reply["result"])["roots"].clone();
    assert!(roots["read"][0].as_str().unwrap().ends_with("in") && roots["write"][0].as_str().unwrap().ends_with("out"), "{roots}");
    // A path outside them is refused with its own code.
    let line =
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "open_file", "arguments": {"path": dir.p("outside.figcraft")}}})
            .to_string();
    let reply: Value = serde_json::from_str(&confined.handle_line(&line).unwrap()).unwrap();
    assert_eq!(error_of(&reply["result"])["code"], "pathRejected");
}

/// An app from before all of this: its command list has none of the new commands, its doc.list no
/// revisions, and it says nothing about itself.
struct OldApp;

impl Backend for OldApp {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match (method, params["command"].as_str()) {
            ("engine.commands", _) => Ok(json!([
                {"id": "doc.list", "label": "List documents", "params": "{}  → [{index, id, name, path?, dirty, active}]"},
                {"id": "edit.undo", "label": "Undo", "params": "{}"},
                {"id": "proto.present", "label": "Present", "params": "{start?}"},
            ])),
            ("engine.execute", Some("doc.list")) => Ok(json!([{"index": 0, "id": 7, "name": "Old", "dirty": false, "active": true}])),
            other => Err(format!("unexpected call {other:?}")),
        }
    }

    fn has_ui(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        "an old app".into()
    }
}

#[test]
fn capabilities_of_an_older_app_are_not_claimed() {
    let r = figcraft_mcp::call_tool(&mut OldApp, "get_capabilities", &json!({}));
    assert!(!r.is_error, "{r:?}");
    let caps: Value = serde_json::from_str(r.first_text()).unwrap();
    assert_eq!((caps["backend"].clone(), caps["window"].clone(), caps["player"].clone()), (json!("attached"), json!(true), json!(true)));
    assert_eq!(caps["commands"], 3);
    assert_eq!(caps["workingDocument"], json!({"id": 7, "name": "Old", "front": true}), "no revision where the app has none");
    let f = &caps["features"];
    for off in [
        "prototypeSimulator",
        "prototypeSnapshot",
        "scenarios",
        "flowCheck",
        "designContext",
        "conflictDetection",
        "historyAuthors",
        "engineErrorCodes",
        "appIdentity",
    ] {
        assert_eq!(f[off], false, "{off}: {f}");
    }
    assert_eq!(f["checks"], json!([]));
    // What the server does itself is there whatever the app is.
    assert_eq!((f["workingDocument"].clone(), f["structuredErrors"].clone()), (json!(true), json!(true)));
}

#[test]
fn the_instructions_start_with_capabilities_the_working_document_and_revisions() {
    let mut s = Server::headless();
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}).to_string();
    let r: Value = serde_json::from_str(&s.handle_line(&init).unwrap()).unwrap();
    let ins = r["result"]["instructions"].as_str().unwrap();
    assert!(ins.starts_with("Call get_capabilities first"), "{ins}");
    assert!(ins.contains("working document") && ins.contains("`ifRevision`") && ins.contains("conflict"), "{ins}");
    assert_eq!(TOOL_NAMES[..2], ["get_capabilities", "use_document"]);
    // The tools that change the document take `ifRevision`; the others don't.
    let tools: Value = serde_json::from_str(&s.handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap()).unwrap();
    let conditional: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["inputSchema"]["properties"].get("ifRevision").is_some())
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        conditional,
        ["run_command", "batch", "create_node", "set_properties", "check_design", "save_file", "undo", "redo", "pointer", "key", "type_text"]
    );
}
