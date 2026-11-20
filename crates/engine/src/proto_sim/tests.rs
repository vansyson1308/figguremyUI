//! Behaviour of the headless prototype simulator. Each case builds a small file with commands,
//! plays it, and states the player's rule it pins (`ui-egui`'s `present/*`). The helpers that
//! build the files are shared with the tests of the commands on top (`proto_verify`).

use figcraft_doc::NodeId;
use figcraft_doc::design::{Gesture, StepAction};
use serde_json::{Value, json};

use super::{Sim, StepOutcome};
use crate::Session;
use crate::params::id_of;

mod components;
mod expectations;
mod navigation;
mod overlays;
mod pictures;
mod triggers;
mod variables;

pub(crate) fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

pub(crate) fn idv(v: &Value) -> NodeId {
    id_of(&v["id"]).unwrap()
}

/// A 300 × 400 top-level frame at `x`.
pub(crate) fn frame(s: &mut Session, name: &str, x: f64) -> NodeId {
    idv(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": 300, "height": 400, "name": name})))
}

/// A 100 × 40 rectangle in `parent` at `(x, y)` from its top-left.
pub(crate) fn rect(s: &mut Session, name: &str, parent: NodeId, x: f64, y: f64) -> NodeId {
    idv(&ex(s, "shape.rectangle", json!({"x": x, "y": y, "width": 100, "height": 40, "parent": parent, "props": {"name": name}})))
}

pub(crate) fn text(s: &mut Session, name: &str, parent: NodeId, characters: &str) -> NodeId {
    idv(&ex(s, "text.create", json!({"text": characters, "x": 10, "y": 300, "parent": parent, "name": name})))
}

/// An interaction on `layer`: `trigger` (a type name) runs `action`.
pub(crate) fn on(s: &mut Session, layer: NodeId, trigger: &str, action: Value) {
    ex(s, "proto.interaction.add", json!({"id": layer, "trigger": {"type": trigger}, "action": action}));
}

/// An "after delay" interaction on `layer`.
pub(crate) fn after_delay(s: &mut Session, layer: NodeId, ms: u32, action: Value) {
    ex(s, "proto.interaction.add", json!({"id": layer, "trigger": {"type": "afterDelay", "ms": ms}, "action": action}));
}

pub(crate) fn navigate(dest: NodeId) -> Value {
    json!({"type": "navigate", "dest": dest})
}

/// A rectangle named `name` on `parent` that does `action` on click.
pub(crate) fn button(s: &mut Session, name: &str, parent: NodeId, y: f64, action: Value) -> NodeId {
    let b = rect(s, name, parent, 20.0, y);
    on(s, b, "onClick", action);
    b
}

pub(crate) fn sim(s: &Session, start: NodeId) -> Sim {
    Sim::new(s.doc().unwrap(), start).unwrap()
}

pub(crate) fn gesture(sim: &mut Sim, node: NodeId, gesture: Gesture) -> StepOutcome {
    sim.step(&StepAction::Gesture { node, gesture })
}

pub(crate) fn click(sim: &mut Sim, node: NodeId) -> StepOutcome {
    gesture(sim, node, Gesture::Click)
}

/// The open overlays, bottom first.
pub(crate) fn open(sim: &Sim) -> Vec<NodeId> {
    sim.overlays().iter().map(|o| o.id).collect()
}

/// The step's problem, or an empty string.
pub(crate) fn problem(o: &StepOutcome) -> &str {
    o.problem.as_deref().unwrap_or_default()
}

/// A collection `State` with `count` (number, 0), `open` (boolean, false) and `label` (string,
/// "Hi").
pub(crate) fn state_variables(s: &mut Session) {
    ex(s, "variable.createCollection", json!({"name": "State"}));
    for (name, kind, value) in [("count", "number", json!(0)), ("open", "boolean", json!(false)), ("label", "string", json!("Hi"))] {
        ex(s, "variable.create", json!({"collection": "State", "name": name, "type": kind, "value": value}));
    }
}

/// Review R2-09: a layer's or a variable's name goes into the simulator's sentences as one line
/// of at most 60 characters, its control characters escaped and its backticks shown as quotes,
/// as the engine's other messages show document text.
#[test]
fn a_name_with_newlines_and_backticks_is_one_clipped_line_in_a_message() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let odd = format!("Line one\nline `two`\t{}", "x".repeat(80));
    let r = rect(&mut s, &odd, a, 20.0, 20.0);
    assert_eq!(s.doc().unwrap().node(r).unwrap().name, odd, "the layer keeps its name");
    let mut p = sim(&s, a);
    let why = problem(&click(&mut p, r)).to_string();
    let shown = format!("Line one\\nline 'two'\\t{}…", "x".repeat(40));
    assert_eq!(why, format!("nothing reacts to click on `{shown}` ({r})"));
    assert!(!why.contains('\n') && !why.contains('\t'), "one line: {why}");
    // A variable's name in an effect, the same way.
    ex(&mut s, "variable.createCollection", json!({"name": "C`ol\nlection"}));
    ex(&mut s, "variable.create", json!({"collection": "C`ol\nlection", "name": "n", "type": "number", "value": 0}));
    let var = s.doc().unwrap().variables.variables[0].id.clone();
    let b = button(&mut s, "Set", a, 100.0, json!({"type": "setVariable", "variable": var, "value": 1}));
    let mut p = sim(&s, a);
    let o = click(&mut p, b);
    assert!(o.effects.contains(&"set `C'ol\\nlection/n` to 1".to_string()), "{o:?}");
}
