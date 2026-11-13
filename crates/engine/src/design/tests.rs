//! Behaviour tests of the design context commands: each command's result, its errors (which
//! name the parameter), one undo step that restores the file exactly, and the file format.

mod alternatives;
mod checks;
mod context;
mod findings;

use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::Session;

/// The AI agent the agent-run commands of these tests are for.
const AGENT: &str = "Claude";
/// How that agent is named in what it records.
const AGENT_NAME: &str = "Claude (agent)";

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

/// The error of a command the user runs that must be refused.
fn err(s: &mut Session, cmd: &str, p: Value) -> String {
    match s.execute(cmd, &p) {
        Ok(v) => panic!("{cmd} {p} should be refused, gave {v}"),
        Err(e) => e.to_string(),
    }
}

fn as_agent(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute_as(Some(AGENT), cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p} (as an agent): {e}"))
}

/// The error of a command an agent runs that must be refused.
fn agent_err(s: &mut Session, cmd: &str, p: Value) -> String {
    match s.execute_as(Some(AGENT), cmd, &p) {
        Ok(v) => panic!("{cmd} {p} should be refused to an agent, gave {v}"),
        Err(e) => e.to_string(),
    }
}

fn nid(v: &Value) -> NodeId {
    crate::params::id_of(&v["id"]).unwrap_or_else(|| panic!("no id in {v}"))
}

/// A top-level frame (white, clipping) at `x` on the page.
fn frame(s: &mut Session, name: &str, x: f64, w: f64, h: f64) -> NodeId {
    nid(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": w, "height": h, "name": name})))
}

fn rect(s: &mut Session, parent: NodeId, [x, y, w, h]: [f64; 4], props: Value) -> NodeId {
    nid(&ex(s, "shape.rectangle", json!({"x": x, "y": y, "width": w, "height": h, "parent": parent, "props": props})))
}

/// A text layer that grows with its text.
fn label(s: &mut Session, parent: NodeId, chars: &str, [x, y]: [f64; 2], props: Value) -> NodeId {
    figcraft_text::scan_system_fonts();
    nid(&ex(s, "text.create", json!({"text": chars, "x": x, "y": y, "parent": parent, "props": props})))
}

/// The whole document as it serializes: what undo has to restore exactly.
fn doc_json(s: &Session) -> Value {
    serde_json::to_value(s.doc().unwrap()).unwrap()
}

/// The last command was one undo step: undo gives back `before` exactly, redo the result.
fn one_undo_step(s: &mut Session, before: &Value) {
    let after = doc_json(s);
    assert_ne!(&after, before, "the command changed nothing");
    assert!(s.undo().unwrap(), "nothing to undo");
    assert_eq!(&doc_json(s), before, "undo doesn't restore the file exactly");
    assert!(s.redo().unwrap(), "nothing to redo");
    assert_eq!(doc_json(s), after, "redo doesn't re-apply the command");
}

/// Run `cmd`, check it was exactly one undo step, and give its result.
fn ex_undoable(s: &mut Session, cmd: &str, p: Value) -> Value {
    let before = doc_json(s);
    let out = ex(s, cmd, p);
    one_undo_step(s, &before);
    out
}

/// A refused command leaves the file and its undo history as they were.
fn refused_unchanged(s: &mut Session, cmd: &str, p: Value) -> String {
    let (before, depth) = (doc_json(s), s.undo_depth());
    let e = err(s, cmd, p);
    assert_eq!(doc_json(s), before, "{cmd} was refused but changed the file");
    assert_eq!(s.undo_depth(), depth, "{cmd} was refused but left an undo step");
    e
}

/// The document after saving it in the native format and opening that.
fn reopened(s: &Session) -> Document {
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (doc, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    doc
}

/// The strings of a JSON array.
fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap_or_else(|| panic!("not a list: {v}")).iter().map(|x| x.as_str().unwrap_or_default().to_string()).collect()
}

/// Every `design.*` command with whether it is an undo step: reading and checking never are.
const COMMANDS: [(&str, bool); 28] = [
    ("design.get", false),
    ("design.forNode", false),
    ("design.brief.set", true),
    ("design.stage.set", true),
    ("design.assumption.add", true),
    ("design.assumption.set", true),
    ("design.assumption.remove", true),
    ("design.decision.add", true),
    ("design.decision.set", true),
    ("design.decision.remove", true),
    ("design.decision.accept", true),
    ("design.check", false),
    ("design.check.rules", false),
    ("design.check.record", true),
    ("design.finding.add", true),
    ("design.finding.list", false),
    ("design.finding.address", true),
    ("design.finding.recheck", true),
    ("design.finding.verify", true),
    ("design.finding.accept", true),
    ("design.finding.reopen", true),
    ("design.finding.remove", true),
    ("design.alternative.create", true),
    ("design.alternative.update", true),
    ("design.alternative.question", true),
    ("design.alternative.compare", false),
    ("design.alternative.choose", true),
    ("design.alternative.remove", true),
];

#[test]
fn every_design_command_is_registered_and_documents_its_parameters() {
    let registered: Vec<&str> = crate::all_commands().iter().map(|c| c.id).filter(|id| id.starts_with("design.")).collect();
    assert_eq!(registered, COMMANDS.map(|(id, _)| id), "the design.* commands, in order");
    for (id, undoable) in COMMANDS {
        let spec = crate::find_command(id).unwrap();
        assert_eq!(spec.history, undoable, "{id}");
        assert!(!spec.label.is_empty() && crate::cmd::param_names(id).is_some(), "{id} has no label or parameter list");
    }
    // A misspelt parameter is refused by name rather than ignored.
    let mut s = Session::new();
    assert!(err(&mut s, "design.finding.add", json!({"kind": "judgement", "rule": "x", "mesage": "typo"})).contains("`mesage`"));
    assert_eq!(
        crate::cmd::param_names("design.alternative.create"),
        Some(&["source", "name", "constraint", "intent", "set", "question", "constraints"][..])
    );
    assert_eq!(crate::cmd::param_names("design.check"), Some(&["scope", "rules"][..]));
}

#[test]
fn read_only_commands_leave_no_undo_step() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    ex(&mut s, "design.brief.set", json!({"audience": "Nurses"}));
    ex(&mut s, "design.alternative.create", json!({"source": home, "name": "B", "constraint": "c"}));
    let (before, depth) = (doc_json(&s), s.undo_depth());
    for (cmd, params) in [
        ("design.get", json!({})),
        ("design.forNode", json!({"id": home})),
        ("design.check", json!({"rules": ["contrast.text", "target.small", "context.briefIncomplete"]})),
        ("design.check.rules", json!({})),
        ("design.finding.list", json!({})),
        ("design.alternative.compare", json!({"set": "V1"})),
    ] {
        ex(&mut s, cmd, params);
    }
    assert_eq!(doc_json(&s), before);
    assert_eq!(s.undo_depth(), depth);
}
