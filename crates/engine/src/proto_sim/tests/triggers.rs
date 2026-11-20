//! Keys, timers and the pointer gestures other than a click.

use figcraft_doc::design::{Gesture, StepAction};
use serde_json::json;

use super::{after_delay, ex, frame, gesture, navigate, on, problem, rect, sim, state_variables};
use crate::Session;
use crate::proto_sim::MAX_DELAY_CHAIN;

fn key(k: &str) -> StepAction {
    StepAction::Key { key: k.into() }
}

#[test]
fn a_key_fires_the_trigger_for_exactly_that_key_or_combination() {
    let mut s = Session::new();
    let (a, b, c) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0));
    for (layer, k, dest) in [(a, "Enter", b), (a, "Shift+K", c), (b, "Gamepad A", a)] {
        ex(&mut s, "proto.interaction.add", json!({"id": layer, "trigger": {"type": "keyPress", "key": k}, "action": navigate(dest)}));
    }
    // (key pressed on A, screen after, derivation)
    for (pressed, after, why) in [
        ("Enter", b, "the key"),
        ("Return", b, "another name of the same key"),
        ("Shift+K", c, "the combination"),
        ("shift+k", c, "in any case"),
        ("K", a, "the key without its modifier is another trigger"),
        ("X", a, "a key nothing uses"),
    ] {
        let mut p = sim(&s, a);
        let o = p.step(&key(pressed));
        assert_eq!(p.screen(), after, "{pressed}: {why}: {o:?}");
        assert_eq!(o.fired, after != a, "{pressed}: {o:?}");
        if after == a {
            assert!(problem(&o).contains(&format!("nothing reacts to key `{pressed}` on `A`")), "{o:?}");
        }
    }
    // A controller button is a key by its name.
    let mut p = sim(&s, b);
    assert!(p.step(&key("Gamepad A")).fired);
    assert_eq!(p.screen(), a);
    let o = p.step(&key(""));
    assert!(!o.fired && problem(&o).contains("`key` is empty"), "{o:?}");
}

#[test]
fn waiting_fires_each_pending_delay_once() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    after_delay(&mut s, a, 500, json!({"type": "setVariable", "variable": "count", "expression": "count + 1"}));
    let mut p = sim(&s, a);
    let o = p.step(&StepAction::Wait);
    assert!(o.fired && o.problem.is_none(), "{o:?}");
    assert_eq!(p.state().variables.get("State/count"), Some(&json!(1.0)));
    // It fired for this arrival: more time changes nothing, and that is no mistake.
    let o = p.step(&StepAction::Wait);
    assert!(!o.fired && o.problem.is_none(), "{o:?}");
    assert_eq!(p.state().variables.get("State/count"), Some(&json!(1.0)));
}

#[test]
fn delays_fire_shortest_first_and_chain_onto_the_next_screen() {
    let mut s = Session::new();
    let (a, b, c, d) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0), frame(&mut s, "D", 1200.0));
    let late = rect(&mut s, "Late", a, 20.0, 20.0);
    after_delay(&mut s, late, 800, navigate(b));
    after_delay(&mut s, a, 200, navigate(c));
    after_delay(&mut s, c, 100, navigate(d));
    let mut p = sim(&s, a);
    let o = p.step(&StepAction::Wait);
    assert!(o.fired, "{o:?}");
    assert_eq!((p.screen(), p.history()), (d, &[a, c][..]), "200 ms beats 800 ms, then C's own timer runs");
}

#[test]
fn delays_that_chase_each_other_are_cut_off_as_a_loop() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    after_delay(&mut s, a, 100, navigate(b));
    after_delay(&mut s, b, 100, navigate(a));
    let mut p = sim(&s, a);
    let o = p.step(&StepAction::Wait);
    assert!(problem(&o).contains("after-delay loop"), "{o:?}");
    assert_eq!(p.history().len(), MAX_DELAY_CHAIN, "it stops after the cap");
}

#[test]
fn hover_and_press_last_until_the_next_step() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let tip = rect(&mut s, "Tip", a, 20.0, 20.0);
    on(&mut s, tip, "onHover", navigate(b));
    on(&mut s, tip, "onPress", json!({"type": "setVariable", "variable": "open", "value": true}));
    let mut p = sim(&s, a);
    let o = gesture(&mut p, tip, Gesture::Hover);
    assert!(o.fired && p.screen() == b, "while hovering: {o:?}");
    // The pointer moved on: the hover is over before anything else happens.
    let o = gesture(&mut p, tip, Gesture::Press);
    assert!(o.fired && o.effects.first().is_some_and(|e| e.contains("hover or press")), "{o:?}");
    assert_eq!((p.screen(), p.history()), (a, &[][..]), "back where it was, with nothing to go back to");
    assert_eq!(p.state().variables.get("State/open"), Some(&json!(true)), "while pressing");
    p.step(&StepAction::Wait);
    assert!(p.state().variables.is_empty(), "released: the variable is what the file says again");
}

#[test]
fn mouse_enter_and_drag_are_not_temporary() {
    let mut s = Session::new();
    let (a, b, c) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0));
    let zone = rect(&mut s, "Zone", a, 20.0, 20.0);
    let handle = rect(&mut s, "Handle", b, 20.0, 20.0);
    on(&mut s, zone, "mouseEnter", navigate(b));
    on(&mut s, handle, "onDrag", navigate(c));
    let mut p = sim(&s, a);
    assert!(gesture(&mut p, zone, Gesture::MouseEnter).fired);
    assert_eq!(p.screen(), b);
    // A click isn't a drag: the layer only has "on drag".
    let o = gesture(&mut p, handle, Gesture::Click);
    assert!(!o.fired && p.screen() == b, "nothing reverted the mouse enter: {o:?}");
    assert!(gesture(&mut p, handle, Gesture::Drag).fired);
    p.step(&StepAction::Wait);
    assert_eq!((p.screen(), p.history()), (c, &[a, b][..]));
}

#[test]
fn a_key_finds_the_first_layer_in_the_order_the_player_shows_them() {
    let mut s = Session::new();
    let (a, b, c) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0));
    let bar = rect(&mut s, "Bar", a, 0.0, 0.0);
    let content = rect(&mut s, "Content", a, 0.0, 100.0);
    for (layer, dest) in [(bar, b), (content, c)] {
        ex(&mut s, "proto.interaction.add", json!({"id": layer, "trigger": {"type": "keyPress", "key": "Enter"}, "action": navigate(dest)}));
    }
    let mut p = sim(&s, a);
    p.step(&key("Enter"));
    assert_eq!(p.screen(), b, "two layers take the key: the lower one is found first");
    // The player draws fixed layers above what scrolls under them, so the bar is the upper one now.
    ex(&mut s, "node.set", json!({"id": bar, "props": {"scrollPosition": "fixed"}}));
    let mut p = sim(&s, a);
    p.step(&key("Enter"));
    assert_eq!(p.screen(), c);
    assert_eq!(s.doc().unwrap().node(a).unwrap().children, [bar, content], "in the copy that plays; the file keeps its order");
}
