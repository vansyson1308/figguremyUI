//! A document's revision, conditional changes (`ifRevision`), who made each undo step, and an
//! agent's undo staying in its own work: what lets a person and AI agents edit one file without
//! one silently writing over, or taking back, what the other just did.

use std::sync::Arc;

use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::params::id_of;
use crate::{EngineError, Session};

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn rect(s: &mut Session) -> NodeId {
    id_of(&ex(s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 40, "height": 40}))["id"]).unwrap()
}

fn rev(s: &Session) -> u64 {
    s.ds().unwrap().doc_revision()
}

fn temp_file(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("figcraft-revision-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.figcraft").to_string_lossy().into_owned()
}

#[test]
fn every_committed_change_is_one_revision_and_reading_is_none() {
    let mut s = Session::new();
    assert_eq!(rev(&s), 1, "a document starts at revision 1");
    let a = rect(&mut s);
    assert_eq!(rev(&s), 2, "an edit");
    // Reading, selecting and moving the view change nothing in the document.
    ex(&mut s, "select.set", json!({"ids": [a]}));
    ex(&mut s, "select.none", json!({}));
    ex(&mut s, "doc.list", json!({}));
    ex(&mut s, "view.zoomFit", json!({}));
    crate::inspect::document(&s, 4, false);
    assert_eq!(rev(&s), 2);
    // A command that fails, and a batch that fails after a step that worked, leave it too.
    assert!(s.execute("node.set", &json!({"ids": [999_999], "props": {"x": 1}})).is_err());
    let failing = json!({"commands": [
        {"command": "node.set", "params": {"ids": [a], "props": {"x": 5}}},
        {"command": "node.set", "params": {"ids": [999_999], "props": {"x": 1}}},
    ]});
    assert!(s.execute("command.batch", &failing).is_err());
    assert_eq!(rev(&s), 2);
    // A batch is one change, however many steps it has.
    let batch = json!({"commands": [
        {"command": "node.set", "params": {"ids": [a], "props": {"x": 5}}},
        {"command": "node.set", "params": {"ids": [a], "props": {"y": 6}}},
        {"command": "shape.rectangle", "params": {"x": 100, "y": 0, "width": 10, "height": 10}},
    ]});
    ex(&mut s, "command.batch", batch);
    assert_eq!(rev(&s), 3);
    // Undo and redo are changes of their own, and nothing to undo is none.
    assert_eq!(ex(&mut s, "edit.undo", json!({})), json!({"undone": true}));
    assert_eq!(rev(&s), 4);
    assert_eq!(ex(&mut s, "edit.redo", json!({})), json!({"redone": true}));
    assert_eq!(rev(&s), 5);
    assert_eq!(ex(&mut s, "edit.redo", json!({})), json!({"redone": false}));
    assert_eq!(rev(&s), 5);
}

#[test]
fn the_revision_is_in_doc_list_inspect_and_what_opening_returns() {
    let mut s = Session::new();
    rect(&mut s);
    assert_eq!(ex(&mut s, "doc.list", json!({}))[0]["revision"], 2);
    assert_eq!(crate::inspect::document(&s, 0, false)["revision"], 2);
    let new = ex(&mut s, "file.new", json!({"name": "Second"}));
    let front = s.ds().unwrap().uid;
    assert_eq!((new["id"].clone(), new["revision"].clone()), (json!(front), json!(1)), "{new}");
    // Each document counts its own.
    let list = ex(&mut s, "doc.list", json!({}));
    assert_eq!((list[0]["revision"].clone(), list[1]["revision"].clone()), (json!(2), json!(1)), "{list}");
}

#[test]
fn a_gesture_is_one_revision_when_it_ends_and_none_when_cancelled() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let before = rev(&s);
    s.begin_interaction("Move").unwrap();
    for dx in [1.0, 2.0, 3.0] {
        s.preview(|doc, _| {
            doc.get_mut(a)?.transform = figcraft_geom::Affine::translate((dx, 0.0));
            Ok(())
        })
        .unwrap();
        assert_eq!(rev(&s), before, "a preview is not a change yet");
    }
    s.commit_interaction().unwrap();
    assert_eq!(rev(&s), before + 1);
    s.begin_interaction("Move").unwrap();
    s.preview(|doc, _| {
        doc.get_mut(a)?.transform = figcraft_geom::Affine::translate((50.0, 0.0));
        Ok(())
    })
    .unwrap();
    s.cancel_interaction().unwrap();
    assert_eq!(rev(&s), before + 1);
    // The gesture is the user's step.
    assert_eq!(s.undo_steps(1), [("Move".to_string(), None)]);
}

#[test]
fn reading_the_file_again_is_the_next_revision_not_a_restart() {
    let path = temp_file("revert");
    let mut s = Session::new();
    rect(&mut s);
    // The first save names the document after its file: that is a change. Saving again is none.
    ex(&mut s, "file.save", json!({"path": path}));
    assert_eq!((rev(&s), s.doc().unwrap().name.as_str()), (3, "doc"));
    ex(&mut s, "file.save", json!({}));
    assert_eq!(rev(&s), 3, "saving is not a change to the document");
    rect(&mut s);
    rect(&mut s);
    assert_eq!(rev(&s), 5);
    let r = ex(&mut s, "file.revert", json!({}));
    assert_eq!(rev(&s), 6, "a reverted document is not back at a revision it had before");
    assert_eq!((r["id"].clone(), r["revision"].clone()), (json!(s.ds().unwrap().uid), json!(6)), "{r}");
    // So a change made for the document as it was before the revert is refused.
    let e = s.execute("shape.rectangle", &json!({"width": 5, "height": 5, "ifRevision": 5})).unwrap_err();
    assert!(e.to_string().contains("Revert to saved (user)"), "{e}");
    let reloaded = ex(&mut s, "file.open", json!({"path": path, "reload": true}));
    assert_eq!((reloaded["reloaded"].clone(), reloaded["revision"].clone()), (json!(true), json!(7)), "{reloaded}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_change_from_a_live_session_is_a_revision_and_a_confirmation_is_not() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let uid = s.ds().unwrap().uid;
    let before = rev(&s);
    // The server confirming your own edit hands back a document that says the same.
    let same = Arc::new((*s.ds().unwrap().doc).clone());
    s.apply_remote(uid, same);
    assert_eq!(rev(&s), before);
    let mut theirs = (*s.ds().unwrap().doc).clone();
    theirs.get_mut(a).unwrap().name = "Renamed by a guest".into();
    s.apply_remote(uid, Arc::new(theirs));
    assert_eq!(rev(&s), before + 1);
    let e = s.execute("node.set", &json!({"ids": [a], "props": {"x": 3}, "ifRevision": before})).unwrap_err();
    assert!(e.to_string().contains("Change from the live session"), "{e}");
}

#[test]
fn a_change_nothing_recorded_still_moves_the_revision_once() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let before = rev(&s);
    // Code that edits the document outside a command and a gesture.
    s.doc_mut().unwrap().get_mut(a).unwrap().name = "Edited directly".into();
    assert_eq!(rev(&s), before + 1);
    assert_eq!(rev(&s), before + 1, "asking again doesn't count it again");
    assert!(matches!(s.execute("node.set", &json!({"ids": [a], "props": {"x": 3}, "ifRevision": before})), Err(EngineError::Conflict(_))));
    // The next command's own change comes after it.
    ex(&mut s, "node.set", json!({"ids": [a], "props": {"x": 3}, "ifRevision": before + 1}));
    assert_eq!(rev(&s), before + 2);
}

#[test]
fn a_conditional_change_is_refused_when_the_document_moved_on() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let seen = rev(&s);
    // The agent read the document at `seen`; meanwhile the user renamed and moved the layer.
    ex(&mut s, "node.set", json!({"ids": [a], "props": {"name": "User's name"}}));
    s.execute_as(Some("Other"), "node.set", &json!({"ids": [a], "props": {"x": 70}})).unwrap();
    let doc = s.ds().unwrap().doc.clone();
    let e = s.execute_as(Some("Claude"), "node.set", &json!({"ids": [a], "props": {"x": 5}, "ifRevision": seen})).unwrap_err();
    let EngineError::Conflict(c) = &e else { panic!("expected a conflict, got {e:?}") };
    assert_eq!((c.expected, c.actual), (seen, seen + 2));
    let since: Vec<(u64, &str, &str)> = c.since.iter().map(|c| (c.revision, c.label.as_str(), c.author_name())).collect();
    assert_eq!(since, [(seen + 1, "Set properties", "user"), (seen + 2, "Set properties", "Other (agent)")]);
    let text = e.to_string();
    for part in
        ["conflict", "node.set", &format!("expected revision {seen}"), &format!("at revision {}", seen + 2), "Set properties (user)", "Other (agent)"]
    {
        assert!(text.contains(part), "`{part}` missing in: {text}");
    }
    assert!(Arc::ptr_eq(&doc, &s.ds().unwrap().doc), "a refused change changes nothing");
    assert_eq!(rev(&s), seen + 2);
    // With the revision it is at, the same change goes through.
    s.execute_as(Some("Claude"), "node.set", &json!({"ids": [a], "props": {"x": 5}, "ifRevision": seen + 2})).unwrap();
    assert_eq!(rev(&s), seen + 3);
}

#[test]
fn if_revision_is_every_commands_parameter() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let now = rev(&s);
    let stale = now - 1;
    // Commands with a parameter list, with free-form parameters, with none, a batch, undo, redo.
    for (cmd, params) in [
        ("shape.rectangle", json!({"width": 5, "height": 5})),
        ("node.set", json!({"ids": [a], "props": {"x": 9}})),
        ("select.all", json!({})),
        ("command.batch", json!({"commands": [{"command": "node.set", "params": {"ids": [a], "props": {"x": 9}}}]})),
        ("edit.undo", json!({})),
        ("edit.redo", json!({})),
        ("doc.list", json!({})),
    ] {
        let mut conditional = params.clone();
        conditional["ifRevision"] = json!(stale);
        let doc = s.ds().unwrap().doc.clone();
        assert!(matches!(s.execute(cmd, &conditional), Err(EngineError::Conflict(_))), "{cmd} at a stale revision");
        assert!(Arc::ptr_eq(&doc, &s.ds().unwrap().doc), "{cmd}");
        conditional["ifRevision"] = json!(rev(&s));
        s.execute(cmd, &conditional).unwrap_or_else(|e| panic!("{cmd} at the current revision: {e}"));
        // `null` is no condition.
        conditional["ifRevision"] = Value::Null;
        s.execute(cmd, &conditional).unwrap_or_else(|e| panic!("{cmd} with a null revision: {e}"));
    }
    // Not a revision number: refused, naming the parameter.
    for junk in [json!("3"), json!(-1), json!(1.5), json!([1]), json!({})] {
        let e = s.execute("select.all", &json!({"ifRevision": junk})).unwrap_err();
        assert_eq!(e.detail()["param"], "ifRevision", "{junk}: {e}");
        assert_eq!(e.detail()["code"], "badParams");
    }
    // The command log and "Repeat" hold the command without its condition.
    let last = s.last_command.clone().unwrap();
    assert!(last.1.get("ifRevision").is_none(), "{last:?}");
}

#[test]
fn steps_of_a_batch_are_checked_against_the_revision_the_batch_started_from() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let now = rev(&s);
    let step = |x: i32, at: u64| json!({"command": "node.set", "params": {"ids": [a], "props": {"x": x}, "ifRevision": at}});
    ex(&mut s, "command.batch", json!({"ifRevision": now, "commands": [step(1, now), step(2, now)]}));
    assert_eq!(rev(&s), now + 1);
    // A stale step fails the whole batch, and says which step.
    let e = s.execute("command.batch", &json!({"commands": [step(1, now + 1), step(2, now)]})).unwrap_err();
    let d = e.detail();
    assert_eq!((d["code"].clone(), d["step"].clone(), d["command"].clone()), (json!("conflict"), json!(1), json!("node.set")), "{d}");
    assert_eq!(rev(&s), now + 1);
}

#[test]
fn history_says_who_made_each_step() {
    let mut s = Session::new();
    rect(&mut s);
    s.execute_as(Some("Claude"), "shape.ellipse", &json!({"width": 10, "height": 10})).unwrap();
    s.execute_as(Some(""), "shape.ellipse", &json!({"width": 10, "height": 10})).unwrap();
    let batch = json!({"label": "Build card", "commands": [{"command": "shape.rectangle", "params": {"width": 5, "height": 5}}]});
    s.execute_as(Some("Claude"), "command.batch", &batch).unwrap();
    let doc = crate::inspect::document(&s, 0, false);
    assert_eq!(
        doc["history"]["steps"],
        json!([
            {"label": "Build card", "author": "Claude (agent)"},
            {"label": "Ellipse", "author": "AI agent"},
            {"label": "Ellipse", "author": "Claude (agent)"},
            {"label": "Rectangle", "author": "user"},
        ])
    );
    // The labels alone are where they always were.
    assert_eq!(doc["history"]["undo"], json!(["Build card", "Ellipse", "Ellipse", "Rectangle"]));
    // An undone step keeps its author on the way back.
    ex(&mut s, "edit.undo", json!({}));
    ex(&mut s, "edit.redo", json!({}));
    assert_eq!(s.undo_steps(1), [("Build card".to_string(), Some("Claude (agent)".to_string()))]);
}

#[test]
fn an_agents_undo_stays_in_its_own_work() {
    let mut s = Session::new();
    let a = rect(&mut s);
    s.execute_as(Some("Claude"), "node.set", &json!({"ids": [a], "props": {"x": 10}})).unwrap();
    // The user edits after the agent: that step is on top.
    ex(&mut s, "node.set", json!({"ids": [a], "props": {"name": "Mine"}}));
    let doc = s.ds().unwrap().doc.clone();
    let e = s.execute_as(Some("Claude"), "edit.undo", &json!({})).unwrap_err();
    assert!(matches!(&e, EngineError::OtherAuthor { label, author, .. } if label == "Set properties" && author == "user"), "{e:?}");
    let text = e.to_string();
    assert!(text.contains("\"Set properties\"") && text.contains("made by user") && text.contains("Leave it, or ask them"), "{text}");
    // `others` is for when the person asked, and the refusal says so (review R2-03).
    assert!(text.contains("pass `others: true` only if the person asked you to undo their step"), "{text}");
    let d = e.detail();
    assert_eq!((d["code"].clone(), d["label"].clone(), d["author"].clone()), (json!("otherAuthor"), json!("Set properties"), json!("user")), "{d}");
    assert!(Arc::ptr_eq(&doc, &s.ds().unwrap().doc), "the user's edit is still there");
    // Another agent's step is somebody else's too.
    assert!(matches!(s.execute_as(Some("Claude"), "edit.undo", &json!({"others": "yes"})), Err(EngineError::BadParams { .. })));
    ex(&mut s, "edit.undo", json!({}));
    assert!(
        matches!(s.execute_as(Some("Cursor"), "edit.undo", &json!({})), Err(EngineError::OtherAuthor { author, .. }) if author == "Claude (agent)")
    );
    // Its own step it undoes, and redoes; the redo of the user's step it leaves alone.
    assert_eq!(s.execute_as(Some("Claude"), "edit.undo", &json!({})).unwrap(), json!({"undone": true}));
    assert_eq!(s.execute_as(Some("Claude"), "edit.redo", &json!({})).unwrap(), json!({"redone": true}));
    assert!(matches!(s.execute_as(Some("Claude"), "edit.redo", &json!({})), Err(EngineError::OtherAuthor { cmd, .. }) if cmd == "edit.redo"));
    // Asked to, it takes anyone's step.
    assert_eq!(s.execute_as(Some("Claude"), "edit.redo", &json!({"others": true})).unwrap(), json!({"redone": true}));
    assert_eq!(s.execute_as(Some("Claude"), "edit.undo", &json!({"others": true})).unwrap(), json!({"undone": true}));
    // Nothing left to undo is not somebody else's step.
    let mut empty = Session::new();
    assert_eq!(empty.execute_as(Some("Claude"), "edit.undo", &json!({})).unwrap(), json!({"undone": false}));
}

#[test]
fn the_users_own_undo_takes_any_step() {
    let mut s = Session::new();
    let a = rect(&mut s);
    s.execute_as(Some("Claude"), "node.set", &json!({"ids": [a], "props": {"x": 10}})).unwrap();
    assert_eq!(ex(&mut s, "edit.undo", json!({})), json!({"undone": true}));
    assert_eq!(ex(&mut s, "edit.redo", json!({})), json!({"redone": true}));
    assert!(s.undo().unwrap());
    assert!(s.undo().unwrap());
    assert!(!s.undo().unwrap());
}

#[test]
fn what_an_agent_does_outside_a_command_is_its_step_too() {
    let mut s = Session::new();
    let a = rect(&mut s);
    s.acting_as(Some("Claude"), |s| {
        s.begin_interaction("Move").unwrap();
        s.preview(|doc, _| {
            doc.get_mut(a)?.transform = figcraft_geom::Affine::translate((9.0, 0.0));
            Ok(())
        })
        .unwrap();
        s.commit_interaction().unwrap();
    });
    assert_eq!(s.undo_steps(1), [("Move".to_string(), Some("Claude (agent)".to_string()))]);
    assert!(s.agent.is_none(), "the user is acting again afterwards");
}

#[test]
fn errors_say_what_they_are_about() {
    let mut s = Session::new();
    rect(&mut s);
    let detail = |s: &mut Session, cmd: &str, p: Value| s.execute(cmd, &p).unwrap_err().detail();
    let d = detail(&mut s, "no.such.command", json!({}));
    assert_eq!((d["code"].clone(), d["command"].clone()), (json!("unknownCommand"), json!("no.such.command")), "{d}");
    let d = detail(&mut s, "node.set", json!({"ids": [424_242], "props": {"x": 1}}));
    assert_eq!((d["code"].clone(), d["node"].clone()), (json!("noSuchNode"), json!(424_242)), "{d}");
    // Where a helper reads a parameter, the error names it.
    let d = detail(&mut s, "file.rename", json!({}));
    assert_eq!((d["code"].clone(), d["command"].clone(), d["param"].clone()), (json!("badParams"), json!("file.rename"), json!("name")), "{d}");
    let d = detail(&mut s, "proto.interaction.clear", json!({"ids": "all"}));
    assert_eq!(d["param"], "ids", "{d}");
    let d = detail(&mut s, "doc.switch", json!({"index": 2, "indexx": 3}));
    assert_eq!((d["code"].clone(), d["param"].clone()), (json!("badParams"), json!("indexx")), "{d}");
    let d = detail(&mut s, "file.revert", json!({}));
    assert_eq!((d["code"].clone(), d["command"].clone()), (json!("notAvailable"), json!("file.revert")), "{d}");
    // Every detail carries the text people read.
    let e = s.execute("node.set", &json!({"ids": [424_242], "props": {"x": 1}})).unwrap_err();
    assert_eq!(e.detail()["message"], e.to_string());
    let mut none = Session::empty();
    assert_eq!(none.execute("select.all", &json!({})).unwrap_err().detail()["code"], "noDocument");
    let e = s.document_uid(&json!(987_654)).unwrap_err();
    assert_eq!((e.detail()["code"].clone(), e.detail()["document"].clone()), (json!("noDocument"), json!("987654")));
    assert!(e.to_string().contains("no open document 987654"), "{e}");
}

#[test]
fn a_failed_batch_names_the_step_and_its_command_whatever_failed() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let ok = json!({"command": "node.set", "params": {"ids": [a], "props": {"x": 1}}});
    for (bad, code, in_text) in [
        // The two the step used to be missing from.
        (json!({"command": "node.set", "params": {"ids": [424_242], "props": {"x": 1}}}), "noSuchNode", "no such node 424242"),
        (json!({"command": "nope.nope", "params": {}}), "unknownCommand", "unknown command `nope.nope`"),
        (json!({"command": "edit.repeat", "params": {"ifRevision": 0}}), "conflict", "conflict"),
        (json!({"command": "node.set", "params": {"ids": [a], "props": {"nope": 1}}}), "badParams", "nope"),
        (json!({"command": "file.rename", "params": {}}), "badParams", "`name` must be a string"),
        (json!({"command": "node.set", "params": {"ids": [{"$ref": "missing"}], "props": {"x": 1}}}), "badParams", "names no step"),
    ] {
        let command = bad["command"].as_str().unwrap().to_string();
        let before = s.ds().unwrap().doc.clone();
        let e = s.execute("command.batch", &json!({"commands": [ok, ok, bad]})).unwrap_err();
        let (text, d) = (e.to_string(), e.detail());
        assert!(text.contains("step 2") && text.contains(&command) && text.contains(in_text), "{command}: {text}");
        assert_eq!((d["code"].clone(), d["step"].clone(), d["command"].clone()), (json!(code), json!(2), json!(command)), "{d}");
        assert_eq!(d["message"], text);
        assert!(Arc::ptr_eq(&before, &s.ds().unwrap().doc), "{command}: rolled back");
    }
    // The node and the parameter are still named beside the step.
    let e = s.execute("command.batch", &json!({"commands": [{"command": "node.set", "params": {"ids": [424_242], "props": {"x": 1}}}]})).unwrap_err();
    assert_eq!(e.detail()["node"], 424_242);
    assert_eq!(e.to_string(), "step 0 (node.set): no such node 424242");
    let e = s.execute("command.batch", &json!({"commands": [{"command": "file.rename", "params": {}}]})).unwrap_err();
    assert_eq!(e.detail()["param"], "name");
    // A step with something wrong in the batch itself.
    let e = s.execute("command.batch", &json!({"commands": [ok, {"command": "node.set", "args": {}}]})).unwrap_err();
    assert_eq!((e.detail()["step"].clone(), e.detail()["command"].clone()), (json!(1), json!("node.set")), "{e}");
    // With onError: continue each failed step says the same in its own outcome.
    let r = ex(&mut s, "command.batch", json!({"onError": "continue", "commands": [ok, {"command": "nope.nope"}]}));
    assert_eq!(r[1]["error"], "step 1 (nope.nope): unknown command `nope.nope`", "{r}");
}

#[test]
fn a_path_outside_the_automation_roots_has_its_own_code() {
    let dir = std::env::temp_dir().join(format!("figcraft-revision-roots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let roots = Arc::new(crate::file_access::Roots::new(&[dir.to_string_lossy().into_owned()], &[]).unwrap());
    let mut s = Session::new();
    let e = crate::file_access::confined(Some(&roots), || s.execute("file.open", &json!({"path": "/etc/hostname"})).unwrap_err());
    assert!(matches!(e, EngineError::PathRejected(_)), "{e:?}");
    assert_eq!(e.detail()["code"], "pathRejected");
    assert!(e.to_string().starts_with("automation path rejected"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_session_without_a_window_refuses_the_player_instead_of_answering_success() {
    let mut s = Session::new();
    ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 100, "height": 100}));
    s.windowless = true;
    for (cmd, p) in [
        ("proto.present", json!({})),
        ("proto.preview", json!({})),
        ("proto.closePreview", json!({})),
        ("proto.presentOptions", json!({"scale": "fit"})),
    ] {
        let e = s.execute(cmd, &p).unwrap_err();
        assert!(matches!(e, EngineError::Unsupported { .. }), "{cmd}: {e:?}");
        let d = e.detail();
        assert_eq!((d["code"].clone(), d["command"].clone()), (json!("unsupportedHere"), json!(cmd)), "{d}");
        assert!(e.to_string().contains("proto.simulate") && e.to_string().contains("proto.scenario.run"), "{e}");
        assert!(s.ui_requests.is_empty(), "{cmd} queued a request nobody reads");
    }
    // In a batch it fails the batch, so nothing before it stays applied by half.
    let before = s.ds().unwrap().doc.clone();
    let batch = json!({"commands": [{"command": "shape.rectangle", "params": {"width": 5, "height": 5}}, {"command": "proto.present"}]});
    assert_eq!(s.execute("command.batch", &batch).unwrap_err().detail()["code"], "unsupportedHere");
    assert!(Arc::ptr_eq(&before, &s.ds().unwrap().doc));
    // With a window the request is queued as before.
    s.windowless = false;
    ex(&mut s, "proto.present", json!({}));
    assert_eq!(s.ui_requests.last().map(|u| u.kind.as_str()), Some("present"));
}

#[test]
fn presenting_never_answers_a_deleted_flow_start() {
    let mut s = Session::new();
    let frame = |s: &mut Session, x: f64, name: &str| {
        id_of(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": 100, "height": 100, "name": name}))["id"]).unwrap()
    };
    let (a, b) = (frame(&mut s, 0.0, "A"), frame(&mut s, 200.0, "B"));
    ex(&mut s, "proto.flow.add", json!({"start": a, "name": "Checkout"}));
    ex(&mut s, "select.none", json!({}));
    assert_eq!(ex(&mut s, "proto.present", json!({}))["start"], json!(a));
    // The flow's starting frame is deleted: the flow still names it.
    ex(&mut s, "edit.delete", json!({"ids": [a]}));
    ex(&mut s, "select.none", json!({}));
    for cmd in ["proto.present", "proto.preview"] {
        let r = ex(&mut s, cmd, json!({}));
        assert_eq!(r["start"], json!(b), "{cmd} falls back to a frame that exists");
        assert!(s.doc().unwrap().contains(id_of(&r["start"]).unwrap()));
    }
    // With no frame left it fails and names the flow.
    ex(&mut s, "edit.delete", json!({"ids": [b]}));
    for cmd in ["proto.present", "proto.preview"] {
        let e = s.execute(cmd, &json!({})).unwrap_err().to_string();
        assert!(e.contains("no frame to present") && e.contains("\"Checkout\"") && e.contains(&a.to_string()), "{e}");
    }
    let e = s.execute("proto.present", &json!({"start": a})).unwrap_err();
    assert_eq!(e.detail()["param"], "start", "{e}");
}

#[test]
fn an_interaction_checks_its_actions_as_adding_an_action_does() {
    let mut s = Session::new();
    let id = |s: &mut Session, c: &str, p: Value| id_of(&ex(s, c, p)["id"]).unwrap();
    let a = id(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 100, "height": 100}));
    let b = id(&mut s, "frame.create", json!({"x": 200, "y": 0, "width": 100, "height": 100}));
    let btn = id(&mut s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 20, "parent": a}));
    ex(&mut s, "variable.createCollection", json!({"name": "State"}));
    ex(&mut s, "variable.create", json!({"collection": "State", "name": "count", "type": "number", "value": 0}));
    ex(&mut s, "proto.interaction.add", json!({"id": btn, "dest": b}));
    let bad_actions = [
        json!({"type": "setVariable", "variable": "gone", "value": 1}),
        json!({"type": "setVariable", "variable": "count", "expression": "\"text\""}),
        json!({"type": "conditional", "blocks": [{"expression": "count + 1", "actions": [{"type": "back"}]}]}),
        json!({"type": "navigate", "dest": 424_242}),
    ];
    for action in &bad_actions {
        // What `proto.action.add` says about it…
        let want = s.execute("proto.action.add", &json!({"id": btn, "index": 0, "action": action})).unwrap_err().to_string();
        let said = |e: EngineError, cmd: &str| e.to_string().replace(cmd, "proto.action.add");
        // …is what adding or changing an interaction with it says.
        let before = s.ds().unwrap().doc.clone();
        for p in [json!({"id": btn, "action": action}), json!({"id": btn, "actions": [{"type": "back"}, action]})] {
            let e = s.execute("proto.interaction.add", &p).unwrap_err();
            assert_eq!(said(e, "proto.interaction.add"), want, "{p}");
        }
        for p in [
            json!({"id": btn, "index": 0, "action": action}),
            json!({"id": btn, "index": 0, "actions": [action]}),
            json!({"id": btn, "index": 0, "interaction": {"trigger": {"type": "onClick"}, "actions": [action]}}),
        ] {
            let e = s.execute("proto.interaction.set", &p).unwrap_err();
            assert_eq!(said(e, "proto.interaction.set"), want, "{p}");
        }
        assert!(Arc::ptr_eq(&before, &s.ds().unwrap().doc), "{action}");
    }
    // Good ones still go in, and changing only the timing doesn't re-check what is there.
    ex(&mut s, "proto.interaction.add", json!({"id": btn, "action": {"type": "setVariable", "variable": "count", "expression": "count + 1"}}));
    ex(&mut s, "proto.interaction.set", json!({"id": btn, "index": 1, "durationMs": 120}));
}

#[test]
fn the_tab_in_front_is_known_while_a_request_runs_on_a_file_behind_it() {
    let mut s = Session::new();
    let first = s.ds().unwrap().uid;
    ex(&mut s, "file.new", json!({"name": "Second"}));
    let second = s.ds().unwrap().uid;
    // A request for the first file while the second is in front: doc.list still says which tab
    // the user sees in front.
    let forward = s.bring_forward(&json!(first)).unwrap();
    assert_eq!(s.front_uid(), Some(second));
    let list = ex(&mut s, "doc.list", json!({}));
    s.put_back(forward);
    assert_eq!((list[0]["active"].clone(), list[1]["active"].clone()), (json!(false), json!(true)), "{list}");
    assert_eq!(s.ds().unwrap().uid, second, "and the tab in front is back in front");
    // A request that switches to the very file it runs on brings it to the front for good.
    let forward = s.bring_forward(&json!(first)).unwrap();
    ex(&mut s, "doc.switch", json!({"index": 0}));
    assert_eq!(ex(&mut s, "doc.list", json!({}))[0]["active"], true);
    s.put_back(forward);
    assert_eq!(s.ds().unwrap().uid, first);
    // "Next tab" counts from the tab in front, not from the file the request runs on.
    let forward = s.bring_forward(&json!(second)).unwrap();
    ex(&mut s, "doc.nextTab", json!({}));
    s.put_back(forward);
    assert_eq!(s.ds().unwrap().uid, second);
    assert_eq!(s.front_uid(), Some(second), "nothing is left over from the request");
}

#[test]
fn authors_follow_a_live_sessions_own_history() {
    use crate::doc_state::LiveStep;
    let mut ds = crate::DocState::new(figcraft_doc::Document::new("Shared"), None);
    let agent = Some("Claude (agent)".to_string());
    ds.live_authors_sync(LiveStep::Edit(agent.as_deref()), 1, 0);
    ds.live_authors_sync(LiveStep::Edit(None), 2, 0);
    assert_eq!(ds.live_author(true), Some(&None), "the user's step is on top");
    ds.live_authors_sync(LiveStep::Undo, 1, 1);
    assert_eq!((ds.live_author(true), ds.live_author(false)), (Some(&agent), Some(&None)));
    ds.live_authors_sync(LiveStep::Redo, 2, 0);
    assert_eq!((ds.live_author(true), ds.live_author(false)), (Some(&None), None));
    // An undo that passes over a step that no longer changes anything takes the one below it.
    ds.live_authors_sync(LiveStep::Undo, 0, 1);
    assert_eq!((ds.live_author(true), ds.live_author(false)), (None, Some(&agent)));
    // A new edit drops the redo steps; a full history drops its oldest step.
    ds.live_authors_sync(LiveStep::Edit(None), 1, 0);
    ds.live_authors_sync(LiveStep::Edit(agent.as_deref()), 1, 0);
    assert_eq!((ds.live_author(true), ds.live_author(false)), (Some(&agent), None));
    // Steps the session added on its own join as the user's, as the oldest.
    ds.live_authors_sync(LiveStep::Other, 3, 0);
    assert_eq!(ds.live_undo_authors().cloned().collect::<Vec<_>>(), [agent, None, None]);
    ds.live_authors_sync(LiveStep::Other, 0, 0);
    assert_eq!(ds.live_author(true), None);
}

/// A command that fails after something inside it counted a change (a batch that was committed,
/// then refused by a check on the whole command) goes back to the revision it started from.
#[test]
fn a_failed_command_leaves_the_revision_and_the_change_log_as_they_were() {
    let mut s = Session::new();
    let a = rect(&mut s);
    let before = rev(&s);
    let mark = s.ds().unwrap().revision_mark();
    // As a batch or an undo inside a command does.
    s.ds_mut().unwrap().doc_mut().get_mut(a).unwrap().name = "Changed".into();
    s.ds_mut().unwrap().committed("Batch", None);
    assert_eq!(rev(&s), before + 1);
    assert!(mark.counted_since(s.ds().unwrap()));
    s.ds_mut().unwrap().restore_revision(mark);
    assert_eq!(s.ds().unwrap().recorded_revision(), before);
    assert!(s.ds().unwrap().changes_since(before - 1).iter().all(|c| c.label != "Batch"));
}
