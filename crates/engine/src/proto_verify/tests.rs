//! Behaviour of the prototype verification commands: the flow check and its rules, simulated
//! runs, stored scenarios and end states. Files are built with commands; where a command refuses
//! to store what a rule must find, the document is edited directly.

use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::check::Found;
use crate::proto_sim::tests::ex;
use crate::{EngineError, Session};

mod flow;
mod scenarios;
mod simulate;

/// `proto.check` on the current page.
fn check(s: &mut Session) -> Value {
    ex(s, "proto.check", json!({}))
}

/// The problems of rule `rule` in a `proto.check` answer, in the order given.
fn problems(v: &Value, rule: &str) -> Vec<Value> {
    v["problems"].as_array().unwrap().iter().filter(|p| p["rule"] == rule).cloned().collect()
}

/// The entry of screen `id` in a `proto.check` answer.
fn screen(v: &Value, id: NodeId) -> Value {
    v["screens"].as_array().unwrap().iter().find(|sc| sc["id"] == json!(id)).unwrap_or_else(|| panic!("no screen {id} in {v}")).clone()
}

/// What rule `rule` finds for `scope`, as `design.check` runs it.
fn found(doc: &Document, rule: &str, scope: &[NodeId]) -> Vec<Found> {
    let rules = crate::proto_verify::rules();
    let r = rules.iter().find(|r| r.id == rule).unwrap_or_else(|| panic!("no rule {rule}"));
    (r.run)(doc, scope)
}

/// The file as it would be saved, to compare before and after.
fn file(s: &Session) -> Value {
    serde_json::to_value(s.doc().unwrap()).unwrap()
}

/// The error `cmd` gives for `params`.
fn err(s: &mut Session, cmd: &str, params: Value) -> String {
    match s.execute(cmd, &params) {
        Err(e) => e.to_string(),
        Ok(v) => panic!("{cmd} {params} ran: {v}"),
    }
}

/// Hostile parameters: nothing panics, and nothing but a stored scenario or end state changes
/// the file.
#[test]
fn hostile_parameters_are_refused_or_run_without_a_crash() {
    let deep = (0..200).fold(json!({"click": 1}), |inner, _| json!({"action": inner, "expect": {"screen": 1}}));
    let wide: Vec<Value> = (0..5000).map(|k| json!({"click": k, "expect": {"visible": [k, -k], "variables": {"": k}}})).collect();
    let junk = [
        json!({"steps": deep}),
        json!({"steps": [deep], "expect": deep, "start": deep}),
        json!({"steps": wide, "name": "wide", "start": 1}),
        json!({"steps": [{"click": 18_446_744_073_709_551_615u64}, {"click": -1}, {"click": 1.5}, {"click": "0:9999999999999999999999"}], "name": "ids", "start": 2}),
        json!({"steps": [{"type": "key", "key": "+++"}, {"type": "key", "key": "Shift+"}, {"key": "\u{0}"}, {"key": "Gamepad \u{202e}A"}], "name": "keys", "start": 2}),
        json!({"steps": [{"type": "setVariable", "variable": "\u{0}", "value": {"a": [1e308, null]}}, {"type": "setVariable", "variable": "", "value": 1e308}], "name": "vars", "start": 2}),
        json!({"steps": [], "expect": {"variables": {"x": [[[[]]]]}, "variants": [{"instance": 2, "variant": 1e308}], "text": [{"node": 2, "contains": ""}], "overlays": [], "endState": 0}, "name": "expect", "start": 2}),
        json!({"id": {"x": 1}, "on": [], "reason": 7, "page": "nope", "stopOnFailure": 1e308}),
        json!({"id": "S1\u{0}", "name": ["x"], "purpose": {"x": 1}, "start": "2", "steps": "all"}),
    ];
    for spec in crate::proto_verify::specs() {
        for j in &junk {
            let mut taken = j.clone();
            if let (Some(names), Some(o)) = (crate::cmd::param_names(spec.id), taken.as_object_mut()) {
                o.retain(|k, _| matches!(k.as_str(), "ids" | "id") || names.contains(&k.as_str()));
            }
            let mut s = Session::new();
            let a = ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 100, "height": 100}));
            ex(&mut s, "proto.interaction.add", json!({"id": a["id"], "trigger": {"type": "afterDelay", "ms": 1}, "action": {"type": "back"}}));
            let _ = s.execute("proto.scenario.add", &json!({"name": "kept", "start": a["id"], "steps": [{"type": "wait"}]}));
            if let Err(EngineError::Internal { cmd, msg }) = s.execute(spec.id, &taken) {
                panic!("{cmd} panicked on {}: {msg}", figcraft_format::clip(&taken.to_string(), 200));
            }
        }
    }
}
