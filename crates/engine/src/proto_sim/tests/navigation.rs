//! Navigating, going back, sections, and which layer a gesture reaches.

use figcraft_doc::NodeId;
use serde_json::json;

use super::{button, click, ex, frame, idv, navigate, problem, rect, sim};
use crate::Session;
use crate::proto_sim::Sim;

#[test]
fn navigate_then_back_follows_the_history() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let go = button(&mut s, "Go", a, 20.0, navigate(b));
    let back = button(&mut s, "Back", b, 20.0, json!({"type": "back"}));
    let mut p = sim(&s, a);
    assert_eq!((p.screen(), p.history()), (a, &[][..]));
    let o = click(&mut p, go);
    assert!(o.fired && o.problem.is_none(), "{o:?}");
    assert!(o.effects.iter().any(|e| e.contains("navigate to `B`")), "{o:?}");
    assert_eq!((p.screen(), p.history()), (b, &[a][..]), "navigating remembers the screen left");
    let o = click(&mut p, back);
    assert!(o.fired, "{o:?}");
    assert_eq!((p.screen(), p.history()), (a, &[][..]), "back returns and forgets it");
    let st = p.state();
    assert_eq!((st.screen, st.screen_name.as_str()), (a, "A"));
}

#[test]
fn back_where_the_prototype_started_does_nothing_and_says_so() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let back = button(&mut s, "Back", a, 20.0, json!({"type": "back"}));
    let mut p = sim(&s, a);
    let o = click(&mut p, back);
    assert!(!o.fired, "{o:?}");
    assert!(problem(&o).contains("no screen to go back to"), "{o:?}");
    assert_eq!(p.screen(), a);
}

#[test]
fn a_start_that_is_not_a_top_level_frame_is_refused_by_name() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let inner = rect(&mut s, "Inner", a, 10.0, 10.0);
    let doc = s.doc().unwrap();
    let err = |start| Sim::new(doc, start).err().unwrap_or_default();
    assert!(err(inner).contains("`Inner`") && err(inner).contains("isn't a top-level frame"), "{}", err(inner));
    assert!(err(NodeId(987_654)).contains("no layer 987654"), "{}", err(NodeId(987_654)));
    assert!(Sim::new(doc, a).is_ok());
}

#[test]
fn navigating_to_the_screen_that_shows_does_nothing() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let again = button(&mut s, "Again", a, 20.0, navigate(a));
    let mut p = sim(&s, a);
    let o = click(&mut p, again);
    assert!(!o.fired && problem(&o).contains("already showing"), "{o:?}");
    assert!(p.history().is_empty(), "nothing is remembered for a navigation that didn't happen");
}

#[test]
fn a_layer_without_the_trigger_lets_the_gesture_through_to_the_layers_around_it() {
    let mut s = Session::new();
    let (a, b, c) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0));
    let card = idv(&ex(&mut s, "frame.create", json!({"x": 20, "y": 100, "width": 200, "height": 200, "parent": a, "name": "Card"})));
    ex(&mut s, "proto.interaction.add", json!({"id": card, "dest": b}));
    let label = rect(&mut s, "Label", card, 10.0, 10.0);
    let own = button(&mut s, "Own", card, 100.0, navigate(c));
    // The label has no click of its own: the card around it takes it.
    let mut p = sim(&s, a);
    let o = click(&mut p, label);
    assert!(o.fired && p.screen() == b, "{o:?}");
    // A layer with the trigger wins over the layers around it.
    let mut p = sim(&s, a);
    click(&mut p, own);
    assert_eq!(p.screen(), c);
    // The card has no hover: nothing reacts, here or further out.
    let mut p = sim(&s, a);
    let o = super::gesture(&mut p, label, figcraft_doc::design::Gesture::Hover);
    assert!(!o.fired && problem(&o).contains("nothing reacts to hover on `Label`"), "{o:?}");
}

#[test]
fn a_gesture_nothing_reacts_to_fails_with_the_layer_named() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let plain = rect(&mut s, "Plain", a, 20.0, 20.0);
    let mut p = sim(&s, a);
    let o = click(&mut p, plain);
    assert!(!o.fired && o.effects.is_empty(), "{o:?}");
    assert!(problem(&o).contains(&format!("nothing reacts to click on `Plain` ({plain})")), "{o:?}");
}

#[test]
fn layers_that_do_not_show_cannot_be_clicked() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let group = idv(&ex(&mut s, "frame.create", json!({"x": 20, "y": 100, "width": 200, "height": 200, "parent": a, "name": "Group"})));
    let hidden = button(&mut s, "Hidden", a, 20.0, navigate(b));
    let inside = button(&mut s, "Inside", group, 20.0, navigate(b));
    let elsewhere = button(&mut s, "Elsewhere", b, 20.0, navigate(a));
    ex(&mut s, "node.set", json!({"id": hidden, "props": {"visible": false}}));
    ex(&mut s, "node.set", json!({"id": group, "props": {"visible": false}}));
    let mut p = sim(&s, a);
    // (layer, what the problem says)
    for (layer, why) in [(hidden, "it is hidden"), (inside, "`Group`"), (elsewhere, "isn't on `A`")] {
        let o = click(&mut p, layer);
        assert!(!o.fired && problem(&o).contains("isn't on `A`") && problem(&o).contains(why), "{layer}: {o:?}");
        assert_eq!(p.screen(), a);
    }
    let o = click(&mut p, NodeId(987_654));
    assert!(!o.fired && problem(&o).contains("layer 987654 doesn't exist"), "{o:?}");
}

#[test]
fn navigating_to_a_section_shows_its_first_frame_then_the_one_shown_last() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0);
    let section = idv(&ex(&mut s, "node.create", json!({"type": "section", "x": 1000, "y": 0, "width": 900, "height": 500, "name": "Steps"})));
    let (one, two) = (frame(&mut s, "One", 1020.0), frame(&mut s, "Two", 1420.0));
    ex(&mut s, "node.reparent", json!({"ids": [one, two], "parent": section}));
    let to_section = button(&mut s, "Steps", home, 20.0, navigate(section));
    let next = button(&mut s, "Next", one, 20.0, navigate(two));
    let leave = button(&mut s, "Home", two, 20.0, navigate(home));
    let mut p = sim(&s, home);
    click(&mut p, to_section);
    assert_eq!(p.screen(), one, "a section not visited yet shows its first frame");
    click(&mut p, next);
    click(&mut p, leave);
    assert_eq!(p.screen(), home);
    click(&mut p, to_section);
    assert_eq!(p.screen(), two, "then the frame of it shown last");
    assert_eq!(p.history(), &[home, one, two, home][..]);
}

#[test]
fn scroll_to_and_open_link_are_reported_and_change_nothing() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let target = rect(&mut s, "Target", a, 20.0, 300.0);
    let far = rect(&mut s, "Far", b, 20.0, 300.0);
    let scroll = button(&mut s, "Scroll", a, 20.0, json!({"type": "scrollTo", "dest": target}));
    let link = button(&mut s, "Link", a, 80.0, json!({"type": "openUrl", "url": "https://example.com/help"}));
    let lost = button(&mut s, "Lost", a, 140.0, json!({"type": "scrollTo", "dest": far}));
    let script = button(&mut s, "Script", a, 200.0, json!({"type": "openUrl", "url": "javascript:alert(1)"}));
    let mut p = sim(&s, a);
    let before = p.state();
    let o = click(&mut p, scroll);
    assert!(o.fired && o.problem.is_none() && o.effects.iter().any(|e| e.contains("scroll to `Target`")), "{o:?}");
    let o = click(&mut p, link);
    assert!(o.fired && o.effects.iter().any(|e| e == "open link https://example.com/help"), "{o:?}");
    assert_eq!(p.state(), before, "neither changes the state");
    // A layer that isn't on what shows can't be scrolled to; an address the player won't open.
    let o = click(&mut p, lost);
    assert!(!o.fired && problem(&o).contains("scroll to `Far`") && problem(&o).contains("isn't on `A`"), "{o:?}");
    let o = click(&mut p, script);
    assert!(problem(&o).contains("isn't a web or mail address"), "{o:?}");
}

#[test]
fn the_file_the_simulator_was_made_from_never_changes() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    super::state_variables(&mut s);
    let go = button(&mut s, "Go", a, 20.0, json!({"type": "setVariable", "variable": "count", "expression": "count + 1"}));
    ex(&mut s, "proto.interaction.add", json!({"id": go, "trigger": {"type": "mouseEnter"}, "action": navigate(b)}));
    let doc = s.doc().unwrap().clone();
    let before = serde_json::to_value(&doc).unwrap();
    let mut p = Sim::new(&doc, a).unwrap();
    click(&mut p, go);
    super::gesture(&mut p, go, figcraft_doc::design::Gesture::MouseEnter);
    assert_eq!(p.screen(), b);
    assert_eq!(p.state().variables.get("State/count"), Some(&json!(1.0)));
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}
