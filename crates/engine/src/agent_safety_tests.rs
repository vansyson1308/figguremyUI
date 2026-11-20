//! What an AI agent can't do to a person's work: throw away their unsaved changes or undo history
//! (revert, close, reload, branch and live-session commands, deleting versions), change the
//! document under a gesture or text edit they have in hand (`busy`), or take back with a snapshot
//! undo a change of theirs that has no undo step. And who acts is put back after a panic.
//!
//! The checks go by error codes and texts, not variants, so the same tests can run against the
//! engine from before these rules (where they fail).

use std::sync::Arc;

use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::params::id_of;
use crate::{PointerEvent, PointerKind, Session};

const AGENT: Option<&str> = Some("Claude");

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn rect(s: &mut Session, x: f64) -> NodeId {
    id_of(&ex(s, "shape.rectangle", json!({"x": x, "y": 0, "width": 40, "height": 40}))["id"]).unwrap()
}

fn rev(s: &Session) -> u64 {
    s.ds().unwrap().doc_revision()
}

fn temp_file(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("figcraft-agent-safety-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.figcraft").to_string_lossy().into_owned()
}

/// A saved file with one rectangle, open in `s`.
fn saved(name: &str) -> (Session, String) {
    let mut s = Session::new();
    rect(&mut s, 0.0);
    let path = temp_file(name);
    ex(&mut s, "file.save", json!({"path": path}));
    (s, path)
}

/// The document, its undo labels and its revision: what a refused command must leave alone.
fn state(s: &Session) -> (Arc<figcraft_doc::Document>, Vec<String>, u64, usize) {
    let ds = s.ds().unwrap();
    (ds.doc.clone(), ds.history.undo.iter().map(|h| h.label.clone()).collect(), ds.doc_revision(), s.docs.len())
}

fn unchanged(s: &Session, was: &(Arc<figcraft_doc::Document>, Vec<String>, u64, usize), what: &str) {
    let now = state(s);
    assert!(Arc::ptr_eq(&now.0, &was.0), "{what}: the document was replaced");
    assert_eq!((&now.1, now.2, now.3), (&was.1, was.2, was.3), "{what}: history, revision or tabs changed");
}

/// `cmd` run for the agent is refused with code `otherAuthor`, naming `label` and its author.
fn refused(s: &mut Session, cmd: &str, p: Value, label: &str, author: &str) {
    let was = state(s);
    let e = s.execute_as(AGENT, cmd, &p).expect_err(&format!("{cmd} {p} should be refused"));
    let d = e.detail();
    assert_eq!(d["code"], "otherAuthor", "{cmd}: {e}");
    assert_eq!((d["label"].as_str(), d["author"].as_str()), (Some(label), Some(author)), "{cmd}: {d}");
    let text = e.to_string();
    assert!(text.contains("would discard") && text.contains(&format!("made by {author}")) && text.contains("nothing was changed"), "{text}");
    unchanged(s, &was, cmd);
}

#[test]
fn an_agent_cant_revert_or_close_away_a_persons_unsaved_work() {
    let (mut s, _path) = saved("revert");
    let a = rect(&mut s, 100.0);
    ex(&mut s, "node.set", json!({"ids": [a], "props": {"name": "Mine"}}));
    for (cmd, p) in [("file.revert", json!({})), ("file.close", json!({"force": true})), ("file.close", json!({}))] {
        refused(&mut s, cmd, p, "Set properties", "user");
    }
    // Saved, the person's undo history is still theirs: closing or reverting would drop it.
    ex(&mut s, "file.save", json!({}));
    assert!(!s.ds().unwrap().is_dirty());
    refused(&mut s, "file.close", json!({}), "Set properties", "user");
    refused(&mut s, "file.revert", json!({}), "Set properties", "user");
    // Another agent's step is somebody else's too.
    s.execute_as(Some("Cursor"), "node.set", &json!({"ids": [a], "props": {"x": 7}})).unwrap();
    refused(&mut s, "file.revert", json!({}), "Set properties", "Cursor (agent)");
    // The person reverts as they like.
    ex(&mut s, "file.revert", json!({}));
}

#[test]
fn an_agent_reverts_and_closes_its_own_work() {
    let mut s = Session::new();
    let path = temp_file("own");
    s.execute_as(AGENT, "shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    s.execute_as(AGENT, "file.save", &json!({"path": path})).unwrap();
    s.execute_as(AGENT, "shape.ellipse", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    s.execute_as(AGENT, "file.revert", &json!({})).unwrap();
    s.execute_as(AGENT, "shape.ellipse", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    // A new file only the agent worked on closes, unsaved work and all, when it says so.
    ex(&mut s, "file.new", json!({}));
    s.execute_as(AGENT, "shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
    let n = s.docs.len();
    s.execute_as(AGENT, "file.close", &json!({"force": true})).unwrap();
    assert_eq!(s.docs.len(), n - 1);
}

#[test]
fn unsaved_work_nobody_recorded_is_the_persons() {
    // A document recovered after a crash has unsaved changes and no steps: not the agent's.
    let mut s = Session::new();
    s.docs.push(crate::DocState::new(figcraft_doc::Document::new("Recovered"), None).unsaved());
    s.active = s.docs.len() - 1;
    refused(&mut s, "file.close", json!({"force": true}), "unsaved changes", "user");
    // A change made outside any command (the app edits the document directly) is the user's.
    let mut s = Session::new();
    let n = s.ds().unwrap().doc.pages[0];
    s.doc_mut().unwrap().node_mut(n).unwrap().name = "Renamed in place".into();
    refused(&mut s, "file.close", json!({"force": true}), "Edit", "user");
}

#[test]
fn a_revert_keeps_the_unsaved_work_in_the_version_history() {
    let (mut s, _path) = saved("keep");
    let a = rect(&mut s, 100.0);
    ex(&mut s, "file.saveVersion", json!({"name": "Mine, unsaved"}));
    let b = rect(&mut s, 200.0);
    ex(&mut s, "file.revert", json!({}));
    let versions = ex(&mut s, "version.list", json!({}));
    let names: Vec<&str> = versions.as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Mine, unsaved", "Before revert"], "{versions}");
    assert!(s.doc().unwrap().node(a).is_none() && s.doc().unwrap().node(b).is_none(), "reverted");
    assert!(!s.ds().unwrap().is_dirty(), "the file is as saved; the history is written with the next save");
    // The work is there to go back to.
    ex(&mut s, "version.restore", json!({"index": 1}));
    assert!(s.doc().unwrap().node(a).is_some() && s.doc().unwrap().node(b).is_some());
}

#[test]
fn an_agent_cant_reload_away_a_persons_history_or_delete_versions() {
    let (mut s, path) = saved("reload");
    let refused_open = |s: &mut Session| refused(s, "file.open", json!({"path": path, "reload": true}), "Rectangle", "user");
    refused_open(&mut s);
    ex(&mut s, "file.saveVersion", json!({"name": "Theirs"}));
    let was = state(&s);
    let e = s.execute_as(AGENT, "version.delete", &json!({"index": 0})).unwrap_err();
    assert!(e.to_string().contains("a person deletes versions"), "{e}");
    unchanged(&s, &was, "version.delete");
    assert_eq!(ex(&mut s, "version.list", json!({})).as_array().unwrap().len(), 1);
}

#[test]
fn an_agent_cant_drop_a_branch_tabs_history() {
    let mut s = Session::new();
    rect(&mut s, 0.0);
    let b = ex(&mut s, "branch.create", json!({"name": "Explore"}));
    let id = b["id"].as_str().unwrap().to_string();
    // The person works on the branch.
    rect(&mut s, 200.0);
    for cmd in ["branch.archive", "branch.merge", "branch.updateFromMain"] {
        refused(&mut s, cmd, json!({"id": id}), "Rectangle", "user");
    }
    // Starting a live session starts the tab's undo history over too.
    ex(&mut s, "doc.switch", json!({"index": 0}));
    refused(&mut s, "collab.host", json!({}), "Rectangle", "user");
}

fn ev(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
    PointerEvent { kind, x, y, sx: x, sy: y, shift: false, alt: false, cmd: false, ctrl: false, zoom: 1.0, pressure: None }
}

/// `cmd` run for the agent is refused with code `busy`, the document, revision and gesture as
/// they were.
fn busy(s: &mut Session, cmd: &str, p: Value) {
    let (doc, revision) = (s.ds().unwrap().doc.clone(), rev(s));
    let gesture = s.ds().unwrap().interaction.as_ref().map(|it| it.doc.clone()).unwrap();
    let e = s.execute_as(AGENT, cmd, &p).expect_err(&format!("{cmd} should wait"));
    assert_eq!(e.detail()["code"], "busy", "{cmd}: {e}");
    assert!(e.to_string().contains("in the middle of a gesture or text edit") && e.to_string().contains("nothing was changed"), "{e}");
    let ds = s.ds().unwrap();
    assert!(Arc::ptr_eq(&ds.doc, &doc), "{cmd}: the document is the person's preview still");
    assert_eq!(rev(s), revision, "{cmd}");
    assert!(ds.interaction.as_ref().is_some_and(|it| Arc::ptr_eq(&it.doc, &gesture)), "{cmd}: the gesture goes on");
}

#[test]
fn an_agents_change_waits_while_a_person_drags() {
    let mut s = Session::new();
    let a = rect(&mut s, 0.0);
    ex(&mut s, "select.set", json!({"ids": [a]}));
    // The person drags the layer: the gesture previews, nothing is committed yet.
    s.pointer(&ev(PointerKind::Down, 20.0, 20.0)).unwrap();
    s.pointer(&ev(PointerKind::Move, 60.0, 20.0)).unwrap();
    assert!(s.ds().unwrap().interaction.is_some());
    busy(&mut s, "node.set", json!({"ids": [a], "props": {"name": "Agent's"}}));
    busy(&mut s, "command.batch", json!({"commands": [{"command": "node.set", "params": {"ids": [a], "props": {"y": 9}}}]}));
    // Its own gesture can't join the person's either.
    let e = s.acting_as(AGENT, |s| s.pointer(&ev(PointerKind::Move, 90.0, 90.0))).unwrap_err();
    assert_eq!(e.detail()["code"], "busy", "{e}");
    let e = s.acting_as(AGENT, |s| s.begin_interaction("Move")).unwrap_err();
    assert_eq!(e.detail()["code"], "busy", "{e}");
    // Reading is fine.
    s.execute_as(AGENT, "doc.list", &json!({})).unwrap();
    // The person lets go: their move is one step of theirs, and the agent goes on.
    s.pointer(&ev(PointerKind::Up, 60.0, 20.0)).unwrap();
    assert_eq!(s.undo_steps(1)[0].1, None, "the drag is the person's step");
    s.execute_as(AGENT, "node.set", &json!({"ids": [a], "props": {"name": "Agent's"}})).unwrap();
}

#[test]
fn an_agents_change_waits_while_a_person_types() {
    let mut s = Session::new();
    let t = id_of(&ex(&mut s, "text.create", json!({"x": 0, "y": 0, "text": "Hello"}))["id"]).unwrap();
    crate::textedit::begin(&mut s, t, None, false).unwrap();
    crate::textedit::insert(&mut s, "Their words").unwrap();
    busy(&mut s, "node.set", json!({"ids": [t], "props": {"x": 50}}));
    busy(&mut s, "text.edit.end", json!({}));
    // Typing into the person's text, or moving their caret, waits too.
    for e in [
        s.acting_as(AGENT, |s| crate::textedit::insert(s, "agent")).unwrap_err(),
        s.acting_as(AGENT, |s| crate::textedit::key(s, "Left", false, false, false)).unwrap_err(),
    ] {
        assert_eq!(e.detail()["code"], "busy", "{e}");
    }
    assert!(s.ds().unwrap().text_edit.is_some(), "still editing");
    crate::textedit::end(&mut s).unwrap();
    assert!(matches!(&s.doc().unwrap().node(t).unwrap().kind, figcraft_doc::NodeKind::Text(d) if d.characters == "Their words"));
    s.execute_as(AGENT, "node.set", &json!({"ids": [t], "props": {"x": 50}})).unwrap();
}

#[test]
fn an_agent_finishes_its_own_text_edit() {
    let mut s = Session::new();
    let t = id_of(&ex(&mut s, "text.create", json!({"x": 0, "y": 0, "text": "Hello"}))["id"]).unwrap();
    s.execute_as(AGENT, "text.edit.begin", &json!({"id": t})).unwrap();
    s.acting_as(AGENT, |s| crate::textedit::insert(s, "Hi")).unwrap();
    s.execute_as(AGENT, "text.edit.end", &json!({})).unwrap();
    assert_eq!(s.undo_steps(1)[0].1.as_deref(), Some("Claude (agent)"));
}

#[test]
fn who_acts_comes_back_after_a_panic() {
    let mut s = Session::new();
    let bug = |s: &mut Session| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.acting_as(AGENT, |_| -> u8 { panic!("a bug") })));
    let caught = bug(&mut s);
    assert!(caught.is_err());
    assert_eq!(s.agent, None, "the next request runs as the user again");
    s.agent = Some("Cursor (agent)".into());
    let caught = bug(&mut s);
    assert!(caught.is_err());
    assert_eq!(s.agent.as_deref(), Some("Cursor (agent)"));
}

#[test]
fn an_agents_undo_doesnt_take_back_a_change_that_has_no_step() {
    let mut s = Session::new();
    let a = rect(&mut s, 0.0);
    s.execute_as(AGENT, "node.set", &json!({"ids": [a], "props": {"x": 10}})).unwrap();
    // The person changes the document directly (no undo step of its own).
    let page = s.ds().unwrap().page;
    s.doc_mut().unwrap().node_mut(page).unwrap().name = "Their page".into();
    let was = state(&s);
    let e = s.execute_as(AGENT, "edit.undo", &json!({})).unwrap_err();
    let d = e.detail();
    assert_eq!((d["code"].as_str(), d["author"].as_str()), (Some("otherAuthor"), Some("user")), "{d}");
    assert!(d["label"].as_str().unwrap().contains("with no undo step of its own"), "{d}");
    assert!(Arc::ptr_eq(&was.0, &s.ds().unwrap().doc));
    // Asked to, it does.
    s.execute_as(AGENT, "edit.undo", &json!({"others": true})).unwrap();
    // Its own undo and redo of its own steps go on as before.
    let mut s = Session::new();
    let a = rect(&mut s, 0.0);
    s.execute_as(AGENT, "node.set", &json!({"ids": [a], "props": {"x": 10}})).unwrap();
    s.execute_as(AGENT, "edit.undo", &json!({})).unwrap();
    s.execute_as(AGENT, "edit.redo", &json!({})).unwrap();
    // The person's undo of their own step after it isn't in the way.
    ex(&mut s, "node.set", json!({"ids": [a], "props": {"y": 3}}));
    ex(&mut s, "edit.undo", json!({}));
    s.execute_as(AGENT, "edit.undo", &json!({})).unwrap();
}

/// Review R2 (suspicion): while a person views an earlier version, what an agent reads is that
/// version's layers; the reading says so, beside the document's own revision.
#[test]
fn a_read_during_a_version_preview_says_it_shows_the_version() {
    let mut s = Session::new();
    rect(&mut s, 0.0);
    ex(&mut s, "file.saveVersion", json!({"name": "One rectangle"}));
    rect(&mut s, 100.0);
    assert!(crate::inspect::document(&s, 1, false).get("previewing").is_none());
    ex(&mut s, "version.preview", json!({"index": 0}));
    let read = crate::inspect::document(&s, 1, false);
    assert_eq!(read["previewing"]["name"], "One rectangle", "{read}");
    assert_eq!(read["revision"], json!(rev(&s)));
}

/// The same for one layer read on its own (`node.get`, the command and the inspection behind the
/// control method): the version's layer, and words saying so.
#[test]
fn a_layer_read_during_a_version_preview_says_it_shows_the_version() {
    let mut s = Session::new();
    let a = rect(&mut s, 0.0);
    ex(&mut s, "file.saveVersion", json!({"name": "One rectangle"}));
    let options = crate::inspect::InspectOptions::default();
    assert!(ex(&mut s, "node.get", json!({"id": a})).get("previewing").is_none());
    assert!(crate::inspect::node_in_session(&s, a, 1, &options).unwrap().get("previewing").is_none());
    ex(&mut s, "version.preview", json!({"index": 0}));
    let said = crate::inspect::document(&s, 0, false)["previewing"].clone();
    assert_eq!(said["name"], "One rectangle");
    for read in [ex(&mut s, "node.get", json!({"id": a})), crate::inspect::node_in_session(&s, a, 1, &options).unwrap()] {
        assert_eq!((read["id"].clone(), read["previewing"].clone()), (json!(a), said.clone()), "{read}");
    }
    // A layer that isn't there reads as nothing, with nothing added.
    assert_eq!(crate::inspect::node_in_session(&s, NodeId(999_999), 1, &options).unwrap(), Value::Null);
}
