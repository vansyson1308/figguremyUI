//! One press-and-release on a layer that reacts to more than the click: "while pressing" shows
//! while the button is down, reverts when it comes up, and the click then runs; a mouse up and a
//! click on one layer both run, the mouse up first. Driven through egui headless (the harness is
//! `present_tests`'), as a person's pointer would.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2};
use figcraft_doc::{NodeId, NodeKind};
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::present_tests::{Ui as Harness, id};

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn frame(s: &mut Session, name: &str, x: f64) -> NodeId {
    id(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": 300, "height": 400, "name": name})))
}

fn on(s: &mut Session, layer: NodeId, trigger: &str, action: Value) {
    ex(s, "proto.interaction.add", json!({"id": layer, "trigger": {"type": trigger}, "action": action, "transition": {"type": "instant"}}));
}

fn navigate(dest: NodeId) -> Value {
    json!({"type": "navigate", "dest": dest})
}

fn presenter(u: &Harness) -> Value {
    u.app.inspect()["presenter"].clone()
}

fn screen(u: &Harness) -> String {
    presenter(u)["name"].as_str().unwrap_or_default().to_string()
}

/// The centre of the presenter's hotspot on `layer`.
fn hotspot(u: &Harness, layer: NodeId) -> Pos2 {
    let hot = presenter(u)["hotspots"].as_array().cloned().unwrap_or_default();
    let h = hot.iter().find(|h| h["id"] == json!(layer)).unwrap_or_else(|| panic!("no hotspot on {layer}: {hot:?}"));
    let r: Vec<f32> = h["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    pos2((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
}

fn button(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

/// Move over `at` and hold the button down.
fn press(u: &mut Harness, at: Pos2) {
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![button(at, true)]);
    u.frames(2, Vec::new);
}

fn lift(u: &mut Harness, at: Pos2) {
    u.frame(vec![button(at, false)]);
    u.frames(2, Vec::new);
}

/// The variant instance `inst` shows in the player's copy of the file.
fn variant(u: &Harness, inst: NodeId) -> Option<NodeId> {
    let p = u.app.ui.present.as_ref()?;
    match &p.design.as_ref()?.node(inst)?.kind {
        NodeKind::Instance { component, .. } => Some(*component),
        _ => None,
    }
}

/// A number variable's value on the screen showing, in the player's copy of the file.
fn number(u: &Harness, name: &str) -> Option<f64> {
    let p = u.app.ui.present.as_ref()?;
    let doc = p.design.as_ref()?;
    let v = figcraft_engine::proto_sim::find_variable(doc, name).ok()?;
    match doc.variables.resolve(&v.id, &doc.modes_for(p.current))? {
        figcraft_doc::VariableValue::Number(n) => Some(n),
        _ => None,
    }
}

/// Screens A (presented), B and C; "Hot" on A goes to B while pressed and to C on click.
fn press_and_click() -> (Harness, NodeId, NodeId) {
    let mut s = Session::new();
    let [a, b, c] = [("A", 0.0), ("B", 400.0), ("C", 800.0)].map(|(n, x)| frame(&mut s, n, x));
    let hot = id(&ex(&mut s, "shape.rectangle", json!({"x": 50, "y": 50, "width": 100, "height": 60, "parent": a, "props": {"name": "Hot"}})));
    on(&mut s, hot, "onPress", navigate(b));
    on(&mut s, hot, "onClick", navigate(c));
    let mut u = Harness::new(s);
    u.run("proto.present", json!({"start": a}));
    (u, a, hot)
}

#[test]
fn a_press_and_release_shows_the_press_then_runs_the_click() {
    let (mut u, a, hot) = press_and_click();
    let at = hotspot(&u, hot);
    press(&mut u, at);
    assert_eq!(screen(&u), "B", "while pressing: shown while the button is down");
    lift(&mut u, at);
    assert_eq!(screen(&u), "C", "the press reverts on release and the same release is the click");
    assert_eq!(presenter(&u)["history"], json!([a]), "back from C is A: the pressed screen left no trace");
}

#[test]
fn a_press_dragged_off_the_layer_reverts_without_a_click() {
    let (mut u, _, hot) = press_and_click();
    let at = hotspot(&u, hot);
    press(&mut u, at);
    let away = pos2(at.x + 200.0, at.y + 200.0);
    u.frame(vec![Event::PointerMoved(away)]);
    lift(&mut u, away);
    assert_eq!(screen(&u), "A", "the press ends and nothing was clicked");
}

#[test]
fn an_instance_shows_its_pressed_variant_then_plays_its_own_click() {
    let mut s = Session::new();
    let mut variants = Vec::new();
    for (k, state) in ["Default", "Pressed"].iter().enumerate() {
        let f =
            id(&ex(&mut s, "frame.create", json!({"x": k as f64 * 150.0, "y": 600, "width": 100, "height": 40, "name": format!("State={state}")})));
        variants.push(id(&ex(&mut s, "component.create", json!({"ids": [f]}))));
    }
    let set = id(&ex(&mut s, "component.combineVariants", json!({"ids": variants.clone()})));
    let (default, pressed) = (variants[0], variants[1]);
    on(&mut s, default, "onPress", json!({"type": "changeTo", "dest": pressed}));
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let inst = id(&ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 20, "parent": a})));
    on(&mut s, inst, "onClick", navigate(b));
    let mut u = Harness::new(s);
    u.run("proto.present", json!({"start": a}));
    assert_eq!(variant(&u, inst), Some(default));
    let at = hotspot(&u, inst);
    press(&mut u, at);
    assert_eq!((variant(&u, inst), screen(&u).as_str()), (Some(pressed), "A"), "the variant's while-pressing shows while the button is down");
    lift(&mut u, at);
    assert_eq!(screen(&u), "B", "the instance's own click runs on release");
    assert_eq!(variant(&u, inst), Some(default), "and the instance is back on the variant it showed");
    assert_eq!(u.app.session.doc().unwrap().node(inst).map(|n| n.interactions.len()), Some(1), "the file itself is untouched");
}

#[test]
fn a_mouse_up_and_a_click_on_one_layer_both_run_the_mouse_up_first() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    ex(&mut s, "variable.createCollection", json!({"name": "State"}));
    ex(&mut s, "variable.create", json!({"collection": "State", "name": "count", "type": "number", "value": 1}));
    let hot = id(&ex(&mut s, "shape.rectangle", json!({"x": 50, "y": 50, "width": 100, "height": 60, "parent": a, "props": {"name": "Hot"}})));
    // Listed click first, so the order they run in isn't the order they were added in.
    let click = json!([{"type": "setVariable", "variable": "count", "expression": "count * 10"}, navigate(b)]);
    ex(&mut s, "proto.interaction.add", json!({"id": hot, "trigger": {"type": "onClick"}, "actions": click}));
    on(&mut s, hot, "mouseUp", json!({"type": "setVariable", "variable": "count", "expression": "count + 1"}));
    let mut u = Harness::new(s);
    u.run("proto.present", json!({"start": a}));
    let at = hotspot(&u, hot);
    press(&mut u, at);
    assert_eq!((number(&u, "count"), screen(&u).as_str()), (Some(1.0), "A"), "nothing before the button comes up");
    lift(&mut u, at);
    assert_eq!(screen(&u), "B", "the click ran");
    assert_eq!(number(&u, "count"), Some(20.0), "(1 + 1) × 10: the mouse up ran, and before the click");
}

#[test]
fn a_press_with_no_click_still_only_reverts() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let hot = id(&ex(&mut s, "shape.rectangle", json!({"x": 50, "y": 50, "width": 100, "height": 60, "parent": a, "props": {"name": "Hot"}})));
    on(&mut s, hot, "onPress", navigate(b));
    let mut u = Harness::new(s);
    u.run("proto.present", json!({"start": a}));
    let at = hotspot(&u, hot);
    press(&mut u, at);
    assert_eq!(screen(&u), "B");
    lift(&mut u, at);
    assert_eq!(screen(&u), "A");
    assert_eq!(presenter(&u)["hotspotFlash"], json!(false), "the press reacted: the hotspots don't flash as after a click on nothing");
}
