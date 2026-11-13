//! What an expectation checks, and how a failure reads.

use std::sync::Arc;

use figcraft_doc::NodeId;
use figcraft_doc::design::{EndState, Expect, TextExpect};
use serde_json::json;

use super::{button, click, ex, frame, navigate, rect, sim, text};
use crate::Session;
use crate::proto_sim::Sim;

#[test]
fn screen_overlays_and_end_state_say_what_was_expected_and_what_is() {
    let mut s = Session::new();
    let (a, b, menu) = (frame(&mut s, "A", 0.0), frame(&mut s, "Done", 400.0), frame(&mut s, "Menu", 800.0));
    let go = button(&mut s, "Go", a, 20.0, navigate(b));
    let opener = button(&mut s, "Open", a, 80.0, json!({"type": "openOverlay", "dest": menu}));
    let mut doc = s.doc().unwrap().clone();
    Arc::make_mut(&mut doc.design).end_states.push(EndState { node: b, reason: "the order is placed".into() });
    let mut p = Sim::new(&doc, a).unwrap();
    assert!(p.check(&Expect::default()).is_empty(), "nothing asked, nothing fails");
    let at = |screen, overlays: &[NodeId], end| Expect {
        screen: Some(screen),
        overlays: Some(overlays.to_vec()),
        end_state: Some(end),
        ..Expect::default()
    };
    assert!(p.check(&at(a, &[], false)).is_empty());
    assert!(!p.state().end_state);
    let failures = p.check(&at(b, &[menu], true));
    assert_eq!(failures.len(), 3, "{failures:?}");
    assert!(failures[0].contains(&format!("expected screen `Done` ({b}), but `A` ({a}) is showing")), "{failures:?}");
    assert!(failures[1].contains(&format!("expected `Menu` ({menu}) open, but no overlay is open")), "{failures:?}");
    assert!(failures[2].contains("expected `A`") && failures[2].contains("to be a declared end state, but it isn't one"), "{failures:?}");
    click(&mut p, opener);
    assert!(p.check(&at(a, &[menu], false)).is_empty());
    assert!(p.check(&at(a, &[], false))[0].contains(&format!("expected no overlay open, but `Menu` ({menu}) is open")));
    p.step(&figcraft_doc::design::StepAction::ClickOutside);
    click(&mut p, go);
    assert!(p.check(&at(b, &[], true)).is_empty());
    assert!(p.state().end_state);
    assert!(p.check(&at(b, &[], false))[0].contains("not to be an end state, but it is declared as one"));
}

#[test]
fn visible_means_showing_on_the_screen_or_an_open_overlay() {
    let mut s = Session::new();
    let (a, b, menu) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "Menu", 800.0));
    let here = rect(&mut s, "Here", a, 20.0, 200.0);
    let there = rect(&mut s, "There", b, 20.0, 200.0);
    let item = rect(&mut s, "Item", menu, 20.0, 20.0);
    let off = rect(&mut s, "Off", a, 150.0, 200.0);
    ex(&mut s, "node.set", json!({"id": off, "props": {"visible": false}}));
    let opener = button(&mut s, "Open", a, 20.0, json!({"type": "openOverlay", "dest": menu}));
    let mut p = sim(&s, a);
    let gone = NodeId(987_654);
    assert!(p.check(&Expect { visible: vec![here, a], hidden: vec![there, item, off, b], ..Expect::default() }).is_empty());
    // (layer expected to show, why it doesn't)
    let failures = p.check(&Expect { visible: vec![there, off, gone], ..Expect::default() });
    for (failure, why) in failures.iter().zip(["it isn't on `A`", "it is hidden", "it doesn't exist"]) {
        assert!(failure.contains("to be showing, but") && failure.contains(why), "{failure}");
    }
    assert_eq!(failures.len(), 3);
    // A layer that is gone can't be confirmed hidden either: the scenario is out of date.
    assert!(p.check(&Expect { hidden: vec![gone], ..Expect::default() })[0].contains("doesn't exist"));
    // Under an open overlay the screen still shows, and so does what is in the overlay.
    click(&mut p, opener);
    assert!(p.check(&Expect { visible: vec![here, item], hidden: vec![there], ..Expect::default() }).is_empty());
}

#[test]
fn text_expectations_need_a_text_layer() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let title = text(&mut s, "Title", a, "Order placed");
    let shape = rect(&mut s, "Shape", a, 20.0, 20.0);
    let p = sim(&s, a);
    let reads = |node, equals: Option<&str>, contains: Option<&str>| Expect {
        text: vec![TextExpect { node, equals: equals.map(str::to_string), contains: contains.map(str::to_string) }],
        ..Expect::default()
    };
    assert!(p.check(&reads(title, Some("Order placed"), Some("placed"))).is_empty());
    assert!(p.check(&reads(title, None, Some("Order"))).is_empty());
    assert!(p.check(&reads(title, None, Some("order")))[0].contains("to contain \"order\", but it reads \"Order placed\""));
    assert!(p.check(&reads(shape, Some("x"), None))[0].contains("isn't a text layer"));
    assert!(p.check(&reads(NodeId(987_654), Some("x"), None))[0].contains("doesn't exist"));
    assert!(p.check(&reads(title, None, None))[0].contains("neither `equals` nor `contains`"));
}
