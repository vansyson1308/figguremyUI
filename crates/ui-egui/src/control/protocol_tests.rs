//! What the app's control channel says beyond a result: what kind of failure an error is
//! (`errorDetail`), who it is (`app.info`), whose gestures `ui.pointer`, `ui.key` and `ui.type`
//! make (`agent`), and that a command whose dialog was dropped says so.

use std::sync::mpsc::{Receiver, channel};

use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

use super::{ControlReply, ControlRequest};
use crate::present_tests::{Ui as Harness, id};

fn handle(u: &mut Harness, method: &str, p: Value) -> Result<Value, String> {
    let ctx = u.ctx.clone();
    super::handle(&mut u.app, &ctx, method, &p)
}

/// A request as the transport queues it; the reply arrives once a frame has answered it.
fn queued(u: &mut Harness, method: &str, params: Value) -> ControlReply {
    let (reply, rx): (_, Receiver<ControlReply>) = channel();
    u.app.control_queue.push(ControlRequest { id: json!(9), method: method.into(), params, reply, state: Default::default() });
    for _ in 0..4 {
        u.frame(Vec::new());
        if let Ok(r) = rx.try_recv() {
            return r;
        }
    }
    panic!("{method}: no reply");
}

/// A screen with a 100×60 rectangle at (50, 50).
fn screen() -> (Harness, NodeId) {
    let mut s = Session::new();
    let a = id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 300, "height": 400, "name": "A"})).unwrap());
    let r = id(&s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 60, "parent": a})).unwrap());
    s.execute("select.set", &json!({"ids": []})).unwrap();
    (Harness::new(s), r)
}

#[test]
fn a_failed_reply_says_what_kind_of_failure_it_is() {
    let (mut u, rect) = screen();
    // The engine's own detail: a layer that doesn't exist.
    let r = queued(&mut u, "engine.execute", json!({"command": "node.set", "params": {"ids": [4141], "props": {"x": 1}}}));
    let detail = r.error_detail().expect("a failure has a detail");
    assert_eq!((detail["code"].clone(), detail["node"].clone()), (json!("noSuchNode"), json!(4141)), "{detail}");
    assert_eq!(detail["message"], json!(r.result.as_ref().unwrap_err()), "the message is the error's text");
    // A stale revision: a conflict, with the revisions and what changed in between.
    let revision = handle(&mut u, "document.inspect", json!({"depth": 0})).unwrap()["revision"].as_u64().unwrap();
    handle(&mut u, "engine.execute", json!({"command": "node.set", "params": {"ids": [rect], "props": {"x": 60}}})).unwrap();
    let r = queued(&mut u, "engine.execute", json!({"command": "node.set", "params": {"ids": [rect], "props": {"x": 70}, "ifRevision": revision}}));
    let detail = r.error_detail().unwrap();
    assert_eq!((detail["code"].clone(), detail["expected"].clone()), (json!("conflict"), json!(revision)), "{detail}");
    assert!(detail["since"].as_array().is_some_and(|s| !s.is_empty()), "{detail}");
    // The handler's own: a parameter it checks, a method it doesn't know.
    let r = queued(&mut u, "engine.execute", json!({"params": {}}));
    assert_eq!(r.error_detail().unwrap()["code"], json!("badParams"));
    let r = queued(&mut u, "no.such.method", json!({}));
    assert_eq!(r.error_detail().unwrap()["code"], json!("unknownMethod"));
    // A success has none, and doesn't carry an earlier failure's.
    let r = queued(&mut u, "engine.execute", json!({"command": "select.set", "params": {"ids": [rect]}}));
    assert!(r.result.is_ok() && r.error_detail().is_none() && r.detail.is_none());
    // Called directly (tests, the snapshot tool), the detail of the last failure is on the app.
    assert!(handle(&mut u, "engine.execute", json!({"command": "no.such.command"})).is_err());
    assert_eq!(u.app.control_error.as_ref().map(|d| d["code"].clone()), Some(json!("unknownCommand")));
}

#[test]
fn a_reply_without_a_detail_of_its_own_is_a_plain_failure() {
    let r = ControlReply::new(json!(1), Err("no window 7".into()));
    assert_eq!(r.error_detail(), Some(json!({"code": "failed", "message": "no window 7"})));
    // A detail without a code isn't one.
    let r = ControlReply { id: json!(1), result: Err("x".into()), detail: Some(json!({"node": 3})), revision: None };
    assert_eq!(r.error_detail().unwrap()["code"], json!("failed"));
}

#[test]
fn app_info_says_it_has_a_window_and_what_its_channel_does() {
    let (mut u, _) = screen();
    let info = handle(&mut u, "app.info", json!({})).unwrap();
    assert_eq!(info["window"], json!(true));
    assert_eq!(info["features"], json!(["errorDetail", "revision"]));
}

/// The undo steps of the document, the next to undo first: `(label, author)`.
fn steps(u: &mut Harness) -> Vec<(String, String)> {
    let doc = handle(u, "document.inspect", json!({"depth": 0})).unwrap();
    let list = doc["history"]["steps"].as_array().cloned().unwrap_or_default();
    list.iter().map(|s| (s["label"].as_str().unwrap_or_default().into(), s["author"].as_str().unwrap_or_default().into())).collect()
}

#[test]
fn an_agents_gestures_are_its_own_undo_steps() {
    let (mut u, rect) = screen();
    let agent = |p: Value| {
        let mut p = p;
        p["agent"] = json!("Claude");
        p
    };
    // The agent drags the rectangle: its step, which it may undo.
    handle(&mut u, "ui.pointer", agent(json!({"kind": "drag", "x": 100, "y": 80, "to": [140, 120]}))).unwrap();
    let x = |u: &Harness| u.app.session.doc().unwrap().node(rect).unwrap().transform.translation().x;
    assert_eq!(x(&u), 90.0, "moved");
    assert_eq!(steps(&mut u).first().map(|s| s.1.clone()), Some("Claude (agent)".into()), "{:?}", steps(&mut u));
    handle(&mut u, "engine.execute", agent(json!({"command": "edit.undo"}))).unwrap();
    assert_eq!(x(&u), 50.0, "the agent took back its own drag");
    // The user drags it: the agent can't take that back.
    handle(&mut u, "ui.pointer", json!({"kind": "drag", "x": 100, "y": 80, "to": [110, 80]})).unwrap();
    assert_eq!(steps(&mut u).first().map(|s| s.1.clone()), Some("user".into()));
    let e = handle(&mut u, "engine.execute", agent(json!({"command": "edit.undo"}))).unwrap_err();
    assert!(e.contains("user"), "{e}");
    assert_eq!(u.app.control_error.as_ref().map(|d| d["code"].clone()), Some(json!("otherAuthor")));
    assert_eq!(x(&u), 60.0, "nothing was undone");
    // A key and typed text are the agent's too.
    handle(&mut u, "engine.execute", json!({"command": "select.set", "params": {"ids": [rect]}})).unwrap();
    handle(&mut u, "ui.key", agent(json!({"key": "ArrowRight"}))).unwrap();
    assert_eq!((x(&u), steps(&mut u)[0].1.as_str()), (61.0, "Claude (agent)"));
    // (Typed text runs as the agent too, but a text edit is one step, credited when the edit
    // ends: to whoever ends it. That is the engine's rule, not this channel's.)
    let t = id(&handle(&mut u, "engine.execute", json!({"command": "text.create", "params": {"text": "Hi", "x": 10, "y": 300}})).unwrap());
    handle(&mut u, "engine.execute", json!({"command": "text.edit.begin", "params": {"id": t}})).unwrap();
    handle(&mut u, "ui.type", agent(json!({"text": "!"}))).unwrap();
    assert!(u.app.session.agent.is_none(), "not left acting after typing");
    // Without `agent`, gestures stay the user's, and the agent is not left set.
    assert!(u.app.session.agent.is_none());
}

#[test]
fn a_command_whose_dialog_was_dropped_says_so() {
    let (mut u, _) = screen();
    let r = handle(&mut u, "engine.execute", json!({"command": "file.colorProfile"})).unwrap();
    assert_eq!(r["ui"], json!("dropped (dialog.colorProfile)"), "{r}");
    assert!(r["profile"].is_string(), "the result is still the command's: {r}");
    assert!(u.app.session.ui_requests.iter().all(|q| !q.kind.starts_with("dialog.")));
    // A command that asks for no dialog answers as before.
    let r = handle(&mut u, "engine.execute", json!({"command": "file.colorProfile", "params": {"profile": "srgb"}})).unwrap();
    assert!(r.get("ui").is_none(), "{r}");
}

#[test]
fn whoever_acted_before_is_back_after_an_agents_gesture_fails_or_panics() {
    let (mut u, _) = screen();
    // A gesture that fails: an unknown pointer kind, a key with no shortcut, typing with nothing
    // being edited.
    assert!(handle(&mut u, "ui.pointer", json!({"kind": "wiggle", "x": 1, "y": 1, "agent": "Claude"})).is_err());
    assert!(handle(&mut u, "ui.key", json!({"key": "F19", "agent": "Claude"})).is_err());
    assert!(handle(&mut u, "ui.type", json!({"text": "x", "agent": "Claude"})).is_err());
    assert!(u.app.session.agent.is_none(), "{:?}", u.app.session.agent);
    // One that panics (the frame's guard catches it, outside): the agent is not left acting.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::as_agent(&mut u.app, &json!({"agent": "Claude"}), |app| {
            assert_eq!(app.session.agent.as_deref(), Some("Claude (agent)"), "the name as execute_as forms it");
            panic!("a bug in a gesture")
        })
    }));
    assert!(caught.is_err());
    assert!(u.app.session.agent.is_none(), "{:?}", u.app.session.agent);
}

#[test]
fn the_methods_that_act_as_the_person_refuse_an_agent() {
    let (mut u, _) = screen();
    for (method, params) in
        [("ui.input", json!({"events": [{"move": [10, 10]}]})), ("ui.menu.invoke", json!({"id": "edit.undo"})), ("ui.set", json!({"theme": "dark"}))]
    {
        let mut asked = params.clone();
        asked["agent"] = json!("Claude");
        let e = handle(&mut u, method, asked).unwrap_err();
        assert!(e.contains(&format!("`{method}`")) && e.contains("person") && e.contains("engine.execute"), "{method}: {e}");
        let d = u.app.control_error.clone().unwrap();
        assert_eq!((d["code"].clone(), d["param"].clone()), (json!("badParams"), json!("agent")), "{d}");
        // Without `agent` they work as before.
        assert!(handle(&mut u, method, params).is_ok(), "{method}");
    }
}

#[test]
fn an_executed_commands_reply_carries_the_revision_it_left() {
    let (mut u, rect) = screen();
    let before = handle(&mut u, "document.inspect", json!({"depth": 0})).unwrap()["revision"].as_u64().unwrap();
    let r = queued(&mut u, "engine.execute", json!({"command": "node.set", "params": {"ids": [rect], "props": {"x": 80}}}));
    assert_eq!(r.revision, Some(before + 1), "one change");
    // A read leaves it; a refused command says where the document stands.
    let r = queued(&mut u, "engine.execute", json!({"command": "select.set", "params": {"ids": []}}));
    assert_eq!(r.revision, Some(before + 1));
    let r = queued(&mut u, "engine.execute", json!({"command": "node.set", "params": {"ids": [rect], "props": {"x": 1}, "ifRevision": before}}));
    assert!(r.result.is_err());
    assert_eq!(r.revision, Some(before + 1));
    // Other methods carry none.
    assert_eq!(queued(&mut u, "app.info", json!({})).revision, None);
}
