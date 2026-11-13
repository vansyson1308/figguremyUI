//! Interactive components: "change to" switches an instance's variant, and the instance then
//! plays the interactions of the variant it shows.

use figcraft_doc::design::{Expect, VariantExpect};
use figcraft_doc::{NodeId, NodeKind};
use serde_json::json;

use super::{button, click, ex, frame, idv, problem, sim};
use crate::Session;

/// A component set "Toggle" (`State=Off`, `State=On`, each with a "Knob" layer) whose variants
/// change to each other on click, and a screen holding one instance of it. Returns the session,
/// the screen, the instance and the two variants (off, on).
fn toggle() -> (Session, NodeId, NodeId, [NodeId; 2]) {
    let mut s = Session::new();
    let mut variants = Vec::new();
    for (k, state) in ["Off", "On"].iter().enumerate() {
        let f =
            idv(&ex(&mut s, "frame.create", json!({"x": k as f64 * 150.0, "y": 600, "width": 100, "height": 40, "name": format!("State={state}")})));
        ex(&mut s, "shape.rectangle", json!({"x": 4, "y": 4, "width": 32, "height": 32, "parent": f, "props": {"name": "Knob"}}));
        variants.push(idv(&ex(&mut s, "component.create", json!({"ids": [f]}))));
    }
    let set = idv(&ex(&mut s, "component.combineVariants", json!({"ids": variants.clone()})));
    let (off, on) = (variants[0], variants[1]);
    ex(&mut s, "proto.interaction.add", json!({"id": off, "action": {"type": "changeTo", "dest": on}}));
    ex(&mut s, "proto.interaction.add", json!({"id": on, "action": {"type": "changeTo", "dest": off}}));
    let screen = frame(&mut s, "Screen", 0.0);
    let instance = idv(&ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 20, "parent": screen})));
    (s, screen, instance, [off, on])
}

fn shows(p: &crate::proto_sim::Sim, instance: NodeId) -> Option<NodeId> {
    match &p.document().node(instance)?.kind {
        NodeKind::Instance { component, .. } => Some(*component),
        _ => None,
    }
}

#[test]
fn a_click_changes_the_variant_and_the_next_click_plays_the_new_variant() {
    let (s, screen, instance, [off, on]) = toggle();
    let mut p = sim(&s, screen);
    assert_eq!(shows(&p, instance), Some(off));
    assert!(p.state().variants.is_empty(), "as in the file");
    let o = click(&mut p, instance);
    assert!(o.fired && o.problem.is_none(), "{o:?}");
    assert_eq!(shows(&p, instance), Some(on), "off → on");
    let st = p.state();
    assert_eq!(st.variants.len(), 1);
    assert_eq!((st.variants[0].instance, st.variants[0].component, st.variants[0].name.as_str()), (instance, on, "State=On"));
    // Now it plays On's interaction, which goes back to Off.
    let o = click(&mut p, instance);
    assert!(o.fired, "{o:?}");
    assert_eq!(shows(&p, instance), Some(off), "on → off");
    assert!(p.state().variants.is_empty(), "the same variant as in the file again");
    assert_eq!(p.screen(), screen);
}

#[test]
fn a_click_on_a_layer_inside_the_instance_reaches_the_instance() {
    let (s, screen, instance, [_, on]) = toggle();
    let knob = s.doc().unwrap().subtree(instance).into_iter().nth(1).unwrap();
    let mut p = sim(&s, screen);
    let o = click(&mut p, knob);
    assert!(o.fired, "the knob has no click of its own; the instance around it plays the variant's: {o:?}");
    assert_eq!(shows(&p, instance), Some(on));
}

#[test]
fn expectations_name_a_variant_by_value_pair_name_or_id() {
    let (s, screen, instance, [off, on]) = toggle();
    let mut p = sim(&s, screen);
    click(&mut p, instance);
    let expect = |variant: &str| Expect { variants: vec![VariantExpect { instance, variant: variant.into() }], ..Expect::default() };
    for name in ["On", "State=On", " State = On ", &on.to_string()] {
        assert_eq!(p.check(&expect(name)), Vec::<String>::new(), "{name}");
    }
    let failures = p.check(&expect("Off"));
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("to show variant `Off`") && failures[0].contains(&format!("it shows `State=On` ({on})")), "{failures:?}");
    assert!(p.check(&expect(&off.to_string()))[0].contains("State=On"));
    // Something that isn't an instance has no variant.
    let other = Expect { variants: vec![VariantExpect { instance: screen, variant: "On".into() }], ..Expect::default() };
    assert!(p.check(&other)[0].contains("isn't an instance"));
}

#[test]
fn change_to_something_that_is_not_a_component_is_a_problem() {
    let (mut s, screen, _, _) = toggle();
    let plain = frame(&mut s, "Plain", 400.0);
    let odd = button(&mut s, "Odd", screen, 200.0, json!({"type": "changeTo", "dest": plain}));
    let mut p = sim(&s, screen);
    let o = click(&mut p, odd);
    assert!(!o.fired && problem(&o).contains("`Plain`") && problem(&o).contains("isn't a component"), "{o:?}");
    assert!(p.state().variants.is_empty());
}
