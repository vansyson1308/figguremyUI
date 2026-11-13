//! `proto.check`: the flow graph's facts, and each flow rule with a file that triggers it and
//! one that just doesn't.

use std::sync::Arc;

use figcraft_doc::proto::{Action, ConditionalBlock, Expr, ExprOp, Interaction, Trigger};
use figcraft_doc::{Document, NodeId, VariableValue};
use serde_json::{Value, json};

use super::{check, found, problems, screen};
use crate::Session;
use crate::proto_sim::tests::{button, ex, frame, idv, navigate, rect, state_variables};

fn flow(s: &mut Session, start: NodeId) {
    ex(s, "proto.flow.add", json!({"start": start}));
}

/// A flow starting at A, whose "Go" button navigates to B.
fn a_to_b() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let go = button(&mut s, "Go", a, 20.0, navigate(b));
    flow(&mut s, a);
    (s, a, b, go)
}

#[test]
fn the_graph_is_reported_whole_when_nothing_is_wrong() {
    let (mut s, a, b, go) = a_to_b();
    let back = button(&mut s, "Back", b, 20.0, json!({"type": "back"}));
    ex(&mut s, "proto.interaction.add", json!({"id": b, "trigger": {"type": "keyPress", "key": "Escape"}, "action": navigate(a)}));
    let v = check(&mut s);
    assert_eq!(v["starts"], json!([{"frame": a, "name": "A", "source": "flow", "flow": "Flow 1"}]));
    assert_eq!(v["page"], json!(s.page().unwrap()));
    let (sa, sb) = (screen(&v, a), screen(&v, b));
    assert_eq!(
        (&sa["name"], &sa["reachable"], &sa["endState"], &sa["deadEnd"], &sa["overlay"]),
        (&json!("A"), &json!(true), &json!(false), &json!(false), &json!(false))
    );
    assert_eq!(sa["out"], json!([{"from": go, "fromName": "Go", "trigger": "onClick", "action": "navigate", "to": b, "toName": "B"}]));
    assert_eq!(
        sb["out"],
        json!([
            {"from": b, "fromName": "B", "trigger": "keyPress", "key": "Escape", "action": "navigate", "to": a, "toName": "A"},
            {"from": back, "fromName": "Back", "trigger": "onClick", "action": "back"},
        ])
    );
    assert_eq!((&sb["reachable"], &sb["deadEnd"]), (&json!(true), &json!(false)), "back leaves B: it isn't where a flow starts");
    // Only the note that nothing checks the flow yet: `ok` is about blockers and majors.
    let rules: Vec<&str> = v["problems"].as_array().unwrap().iter().map(|p| p["rule"].as_str().unwrap()).collect();
    assert_eq!(rules, ["scenario.none"]);
    assert_eq!(v["ok"], json!(true));
}

#[test]
fn a_reachable_screen_with_no_way_out_is_a_dead_end_unless_declared() {
    let (mut s, a, b, _) = a_to_b();
    let v = check(&mut s);
    let dead = problems(&v, "flow.deadEnd");
    assert_eq!(dead.len(), 1, "{v}");
    assert_eq!((&dead[0]["severity"], &dead[0]["nodes"]), (&json!("major"), &json!([b])));
    assert!(dead[0]["message"].as_str().unwrap().contains(&format!("`B` ({b}) is a dead end")), "{dead:?}");
    assert!(dead[0]["suggestion"].as_str().unwrap().contains("proto.endState.set"));
    assert_eq!((&screen(&v, b)["deadEnd"], &v["ok"]), (&json!(true), &json!(false)));
    assert_eq!(screen(&v, a)["deadEnd"], json!(false));
    // Declared as a place the flow may end: a fact about the screen, and no finding.
    ex(&mut s, "proto.endState.set", json!({"id": b, "reason": "the order is placed"}));
    let v = check(&mut s);
    assert!(problems(&v, "flow.deadEnd").is_empty(), "{v}");
    assert_eq!((&screen(&v, b)["endState"], &screen(&v, b)["deadEnd"], &v["ok"]), (&json!(true), &json!(false), &json!(true)));
    // Taken back, a way out does it too: a link counts as leaving.
    ex(&mut s, "proto.endState.set", json!({"id": b, "on": false}));
    button(&mut s, "Help", b, 20.0, json!({"type": "openUrl", "url": "https://example.com"}));
    assert!(problems(&check(&mut s), "flow.deadEnd").is_empty());
}

#[test]
fn back_is_no_way_out_of_the_screen_a_flow_starts_on() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    button(&mut s, "Back", a, 20.0, json!({"type": "back"}));
    button(&mut s, "Back", b, 20.0, json!({"type": "back"}));
    flow(&mut s, a);
    let v = check(&mut s);
    let dead = problems(&v, "flow.deadEnd");
    assert_eq!(dead.len(), 1, "{v}");
    assert_eq!(dead[0]["nodes"], json!([a]));
    assert!(dead[0]["evidence"].as_str().unwrap().contains("a flow starts here"), "{dead:?}");
    // B has the same and only "back", but nothing reaches it: unreachable, not a dead end.
    assert_eq!(screen(&v, b)["deadEnd"], json!(false));
}

#[test]
fn an_overlay_that_cannot_be_closed_traps_the_viewer() {
    let overlay = |close_on_outside: bool, closer: Option<Value>| {
        let mut s = Session::new();
        let (a, menu) = (frame(&mut s, "A", 0.0), frame(&mut s, "Menu", 400.0));
        let opener = button(&mut s, "Open", a, 20.0, json!({"type": "openOverlay", "dest": menu, "closeOnOutside": close_on_outside}));
        if let Some(action) = closer {
            button(&mut s, "Item", menu, 20.0, action);
        }
        flow(&mut s, a);
        (check(&mut s), a, menu, opener)
    };
    let (v, _, menu, opener) = overlay(false, None);
    let trapped = problems(&v, "flow.trappedOverlay");
    assert_eq!(trapped.len(), 1, "{v}");
    assert_eq!((&trapped[0]["severity"], &trapped[0]["nodes"]), (&json!("major"), &json!([menu, opener])));
    assert!(trapped[0]["evidence"].as_str().unwrap().contains(&format!("`Open` ({opener}) opens it with close-on-outside-click off")));
    // The overlay is a frame of the page that is shown as an overlay, not a screen nobody reaches.
    assert_eq!((&screen(&v, menu)["overlay"], &screen(&v, menu)["reachable"]), (&json!(true), &json!(false)));
    assert!(problems(&v, "flow.unreachable").is_empty(), "{v}");
    // Near misses: a click outside closes it, or something in it does.
    assert!(problems(&overlay(true, None).0, "flow.trappedOverlay").is_empty());
    for closer in [json!({"type": "closeOverlay"}), json!({"type": "back"})] {
        assert!(problems(&overlay(false, Some(closer.clone())).0, "flow.trappedOverlay").is_empty(), "{closer}");
    }
    let (v, a, ..) = overlay(false, Some(json!({"type": "openUrl", "url": "https://example.com"})));
    assert_eq!(problems(&v, "flow.trappedOverlay").len(), 1, "a link opens a page; the overlay is still there");
    assert_eq!(screen(&v, a)["deadEnd"], json!(false), "opening an overlay counts as a way on from A");
}

#[test]
fn a_target_deleted_after_it_was_linked_is_a_broken_target() {
    let (mut s, _, b, go) = a_to_b();
    assert!(problems(&check(&mut s), "flow.brokenTarget").is_empty());
    ex(&mut s, "edit.delete", json!({"ids": [b]}));
    let v = check(&mut s);
    let broken = problems(&v, "flow.brokenTarget");
    assert_eq!(broken.len(), 1, "{v}");
    assert_eq!((&broken[0]["severity"], &broken[0]["nodes"], &v["ok"]), (&json!("blocker"), &json!([go]), &json!(false)));
    let evidence = broken[0]["evidence"].as_str().unwrap();
    assert_eq!(evidence, format!("`Go` ({go}), On click → Navigate to: layer {b} doesn't exist"));
    // The edge is still a fact of the graph, without a target.
    let v = check(&mut s);
    let a = v["starts"][0]["frame"].clone();
    let out = v["screens"].as_array().unwrap().iter().find(|sc| sc["id"] == a).unwrap()["out"].clone();
    assert_eq!(out, json!([{"from": go, "fromName": "Go", "trigger": "onClick", "action": "navigate"}]));
}

fn on_click(doc: &mut Document, node: NodeId, actions: Vec<Action>) {
    let it =
        Interaction { trigger: Trigger::OnClick, actions, transition: Default::default(), duration_ms: 0, easing: Default::default(), delay_ms: 0 };
    doc.get_mut(node).unwrap().interactions.push(it);
}

/// What `flow.brokenTarget` says about `actions` on a layer of screen A.
fn broken(doc: &Document, layer: NodeId, actions: Vec<Action>) -> Vec<String> {
    let mut doc = doc.clone();
    on_click(&mut doc, layer, actions);
    let scope = [doc.top_level_frame(layer).unwrap()];
    found(&doc, "flow.brokenTarget", &scope).into_iter().map(|f| f.evidence).collect()
}

#[test]
fn every_kind_of_target_that_cannot_work_is_reported_and_its_near_miss_is_not() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let layer = rect(&mut s, "Layer", a, 20.0, 20.0);
    let loose = idv(&ex(&mut s, "shape.rectangle", json!({"x": 2000, "y": 0, "width": 10, "height": 10, "props": {"name": "Loose"}})));
    let empty = idv(&ex(&mut s, "node.create", json!({"type": "section", "x": 3000, "y": 0, "width": 500, "height": 500, "name": "Empty"})));
    let full = idv(&ex(&mut s, "node.create", json!({"type": "section", "x": 4000, "y": 0, "width": 500, "height": 500, "name": "Full"})));
    let inside = frame(&mut s, "Inside", 4050.0);
    ex(&mut s, "node.reparent", json!({"ids": [inside], "parent": full}));
    let doc = s.doc().unwrap().clone();
    let vars = &doc.variables;
    let (count, label) = (vars.get("count").unwrap().id.clone(), vars.get("label").unwrap().id.clone());
    let collection = vars.collections[0].clone();
    let mode = collection.modes[0].id.clone();
    let gone = NodeId(987_654);
    let nav = |dest| Action::Navigate { dest, preserve_scroll: false };
    let set = |variable: &str, value: Value, expr| Action::SetVariable { variable: variable.into(), value, expr };
    let plus_one = |id: &str| Expr::op(ExprOp::Add, vec![Expr::var(id), Expr::Value(VariableValue::Number(1.0))]);
    let when = |id: &str| Action::Conditional {
        blocks: vec![ConditionalBlock {
            condition: Some(Expr::op(ExprOp::Greater, vec![Expr::var(id), Expr::Value(VariableValue::Number(1.0))])),
            actions: vec![nav(b)],
        }],
    };
    // (action, what the evidence says; `None`: it works)
    let cases: Vec<(Action, Option<&str>)> = vec![
        (nav(b), None),
        (nav(gone), Some("Navigate to: layer 987654 doesn't exist")),
        (nav(loose), Some("isn't a top-level frame or inside one")),
        (nav(layer), None),
        (nav(empty), Some("has no frame to show")),
        (nav(full), None),
        (Action::ScrollTo { dest: gone, offset: [0.0, 0.0] }, Some("Scroll to: layer 987654 doesn't exist")),
        (Action::ScrollTo { dest: layer, offset: [0.0, 0.0] }, None),
        (Action::SwapOverlay { dest: gone }, Some("Swap overlay: layer 987654 doesn't exist")),
        (Action::SwapOverlay { dest: b }, None),
        (Action::ChangeTo { dest: gone }, Some("Change to: layer 987654 doesn't exist")),
        (Action::ChangeTo { dest: b }, Some("isn't a component")),
        (set("nope", json!(1), None), Some("there is no variable `nope`")),
        (set(&count, json!("many"), None), Some("\"many\" isn't a number value")),
        (set(&count, json!("12"), None), None),
        (set(&count, json!(null), Some(plus_one(&count))), None),
        (set(&count, json!(null), Some(plus_one("nope"))), Some("its expression reads variable `nope`")),
        (set(&count, json!(null), Some(Expr::var(&label))), Some("its expression doesn't give a number value")),
        (set(&label, json!(null), Some(plus_one(&count))), None),
        (Action::SetVariableMode { collection: collection.id.clone(), mode: mode.clone() }, None),
        (Action::SetVariableMode { collection: "nope".into(), mode: mode.clone() }, Some("there is no collection `nope`")),
        (Action::SetVariableMode { collection: collection.id.clone(), mode: "nope".into() }, Some("has no mode `nope`")),
        (when(&count), None),
        (when("nope"), Some("a condition reads variable `nope`")),
        (Action::Back, None),
        (Action::CloseOverlay, None),
    ];
    for (action, want) in cases {
        let got = broken(&doc, layer, vec![action.clone()]);
        match want {
            None => assert!(got.is_empty(), "{action:?} works: {got:?}"),
            Some(why) => {
                assert_eq!(got.len(), 1, "{action:?}: {got:?}");
                assert!(got[0].starts_with(&format!("`Layer` ({layer}), On click → ")) && got[0].contains(why), "{action:?}: {got:?}");
            }
        }
    }
    // An action nested in a conditional is looked at too.
    let nested = Action::Conditional { blocks: vec![ConditionalBlock { condition: None, actions: vec![nav(gone)] }] };
    assert_eq!(broken(&doc, layer, vec![nested]).len(), 1);
}

/// A component set "Toggle" (Off, On): Off changes to On on click, and On navigates to `dest`.
/// Returns the set and the variants.
pub(super) fn toggle(s: &mut Session, dest: NodeId) -> (NodeId, [NodeId; 2]) {
    let mut variants = Vec::new();
    for (k, state) in ["Off", "On"].iter().enumerate() {
        let f = idv(&ex(s, "frame.create", json!({"x": k as f64 * 150.0, "y": 900, "width": 100, "height": 40, "name": format!("State={state}")})));
        variants.push(idv(&ex(s, "component.create", json!({"ids": [f]}))));
    }
    let set = idv(&ex(s, "component.combineVariants", json!({"ids": variants.clone()})));
    ex(s, "proto.interaction.add", json!({"id": variants[0], "action": {"type": "changeTo", "dest": variants[1]}}));
    ex(s, "proto.interaction.add", json!({"id": variants[1], "dest": dest}));
    (set, [variants[0], variants[1]])
}

#[test]
fn change_to_must_name_a_variant_the_instance_could_show() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let (set, [off, on]) = toggle(&mut s, b);
    let instance = idv(&ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 20, "parent": a})));
    let plain = rect(&mut s, "Plain", a, 20.0, 200.0);
    let lone = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 1200, "width": 50, "height": 50, "name": "Lone"})));
    let lone = idv(&ex(&mut s, "component.create", json!({"ids": [lone]})));
    flow(&mut s, a);
    // As built: the variants change to each other, and the instance plays them.
    let v = check(&mut s);
    assert!(problems(&v, "flow.brokenTarget").is_empty(), "{v}");
    let doc = s.doc().unwrap().clone();
    assert!(broken(&doc, instance, vec![Action::ChangeTo { dest: on }]).is_empty(), "on the instance itself");
    assert!(broken(&doc, off, vec![Action::ChangeTo { dest: on }]).is_empty(), "on the variant, which instances play");
    let why = broken(&doc, plain, vec![Action::ChangeTo { dest: on }]);
    assert!(why.len() == 1 && why[0].contains("no instance of"), "nothing around it shows the set: {why:?}");
    let why = broken(&doc, instance, vec![Action::ChangeTo { dest: lone }]);
    assert!(why.len() == 1 && why[0].contains("isn't a variant of a component set"), "{why:?}");
}

#[test]
fn what_an_instance_can_be_changed_to_counts_as_what_the_screen_can_do() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let (set, [_, on]) = toggle(&mut s, b);
    ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 20, "parent": a}));
    flow(&mut s, a);
    let v = check(&mut s);
    // The instance shows Off, whose click changes it to On; On's click navigates to B.
    assert_eq!(screen(&v, b)["reachable"], json!(true), "{v}");
    assert_eq!(screen(&v, a)["deadEnd"], json!(false));
    let out = screen(&v, a)["out"].as_array().unwrap().clone();
    assert!(out.iter().any(|e| e["action"] == "changeTo" && e["to"] == json!(on)), "{out:?}");
    assert!(out.iter().any(|e| e["action"] == "navigate" && e["from"] == json!(on) && e["to"] == json!(b)), "{out:?}");
    // The components themselves aren't screens anybody should reach.
    assert!(problems(&v, "flow.unreachable").is_empty(), "{v}");
}

#[test]
fn every_branch_of_a_conditional_counts_and_overlays_lead_on() {
    let mut s = Session::new();
    let (a, b, c, menu, d) =
        (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "C", 800.0), frame(&mut s, "Menu", 1200.0), frame(&mut s, "D", 1600.0));
    state_variables(&mut s);
    let fork = rect(&mut s, "Fork", a, 20.0, 20.0);
    let branches = json!({"type": "conditional", "blocks": [{"expression": "count > 3", "actions": [navigate(b)]}, {"actions": [navigate(c)]}]});
    ex(&mut s, "proto.interaction.add", json!({"id": fork, "action": branches}));
    button(&mut s, "Open", b, 20.0, json!({"type": "openOverlay", "dest": menu}));
    button(&mut s, "On", menu, 20.0, navigate(d));
    flow(&mut s, a);
    let v = check(&mut s);
    for (id, name) in [(b, "B"), (c, "C"), (d, "D")] {
        assert_eq!(screen(&v, id)["reachable"], json!(true), "{name}: {v}");
    }
    assert_eq!((&screen(&v, menu)["reachable"], &screen(&v, menu)["overlay"]), (&json!(false), &json!(true)));
}

#[test]
fn frames_no_flow_reaches_are_listed_unless_they_are_not_meant_as_screens() {
    let (mut s, a, b, _) = a_to_b();
    ex(&mut s, "proto.endState.set", json!({"id": b}));
    let quiet = frame(&mut s, "Quiet", 800.0);
    let busy = frame(&mut s, "Busy", 1200.0);
    button(&mut s, "Home", busy, 20.0, navigate(a));
    let variant = frame(&mut s, "Variant", 1600.0);
    let source = frame(&mut s, "Source", 2000.0);
    let part = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 1200, "width": 50, "height": 50, "name": "Part"})));
    let part = idv(&ex(&mut s, "component.create", json!({"ids": [part]})));
    let set: figcraft_doc::design::AlternativeSet =
        serde_json::from_value(json!({"id": "V1", "source": source, "members": [{"node": variant}]})).unwrap();
    Arc::make_mut(&mut s.doc_mut().unwrap().design).alternatives.push(set);
    let v = check(&mut s);
    let unreachable = problems(&v, "flow.unreachable");
    // (frame, severity): one with interactions of its own was meant to be part of a flow.
    let got: Vec<(Value, Value)> = unreachable.iter().map(|p| (p["nodes"].clone(), p["severity"].clone())).collect();
    assert_eq!(got, [(json!([busy]), json!("minor")), (json!([quiet]), json!("note"))], "{v}");
    for skipped in [variant, source, part] {
        assert_eq!(screen(&v, skipped)["reachable"], json!(false), "a fact all the same");
    }
    assert_eq!(v["ok"], json!(true), "minor findings and notes don't make it not ok");
    // A page nobody prototyped on has nothing to reach.
    let mut s = Session::new();
    frame(&mut s, "A", 0.0);
    frame(&mut s, "B", 400.0);
    assert_eq!(check(&mut s)["problems"], json!([]));
}

#[test]
fn interactions_without_a_flow_start_fall_back_to_the_first_frame() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    button(&mut s, "Go", a, 20.0, navigate(b));
    button(&mut s, "Back", b, 20.0, json!({"type": "back"}));
    let v = check(&mut s);
    assert_eq!(v["starts"], json!([{"frame": a, "name": "A", "source": "firstFrame", "flow": null}]));
    let none = problems(&v, "flow.noStart");
    assert_eq!(none.len(), 1, "{v}");
    assert_eq!((&none[0]["severity"], &none[0]["nodes"]), (&json!("minor"), &json!([a])));
    assert!(none[0]["evidence"].as_str().unwrap().contains(&format!("`A` ({a})")));
    assert_eq!(screen(&v, b)["reachable"], json!(true), "reached from the frame the player falls back to");
    assert!(problems(&v, "scenario.none").is_empty(), "no flow, so no scenario is missed");
    flow(&mut s, a);
    assert!(problems(&check(&mut s), "flow.noStart").is_empty());
}

#[test]
fn the_check_is_read_only_ordered_and_takes_a_page() {
    let (mut s, a, b, _) = a_to_b();
    ex(&mut s, "edit.delete", json!({"ids": [b]}));
    frame(&mut s, "Lost", 800.0);
    let first = s.page().unwrap();
    let other = idv(&ex(&mut s, "page.add", json!({})));
    ex(&mut s, "page.select", json!({"id": first}));
    let before = (serde_json::to_value(s.doc().unwrap()).unwrap(), s.undo_depth());
    let v = check(&mut s);
    assert_eq!(check(&mut s), v, "the same file gives the same answer");
    assert_eq!((serde_json::to_value(s.doc().unwrap()).unwrap(), s.undo_depth()), before, "nothing changed, nothing to undo");
    // The worst first, then by rule.
    let order: Vec<(&str, &str)> =
        v["problems"].as_array().unwrap().iter().map(|p| (p["severity"].as_str().unwrap(), p["rule"].as_str().unwrap())).collect();
    assert_eq!(order, [("blocker", "flow.brokenTarget"), ("major", "flow.deadEnd"), ("note", "flow.unreachable"), ("note", "scenario.none")], "{v}");
    assert_eq!(v["problems"][1]["nodes"], json!([a]), "its only action leads nowhere now");
    // Another page by id; something that isn't a page is refused by name.
    let v = ex(&mut s, "proto.check", json!({"page": other}));
    assert_eq!(
        (&v["page"], &v["starts"], &v["screens"], &v["problems"], &v["ok"]),
        (&json!(other), &json!([]), &json!([]), &json!([]), &json!(true))
    );
    let err = s.execute("proto.check", &json!({"page": a})).unwrap_err().to_string();
    assert!(err.contains("`page`") && err.contains("`A`") && err.contains("isn't a page"), "{err}");
    assert!(s.execute("proto.check", &json!({"page": 987_654})).unwrap_err().to_string().contains("no layer 987654"));
}

#[test]
fn the_rules_are_listed_for_design_check_and_follow_its_scope() {
    let (s, a, b, _) = a_to_b();
    let doc = s.doc().unwrap();
    let ids: Vec<&str> = crate::proto_verify::rules().iter().map(|r| r.id).collect();
    assert_eq!(
        ids,
        [
            "flow.brokenTarget",
            "flow.deadEnd",
            "flow.trappedOverlay",
            "flow.unreachable",
            "flow.noStart",
            "scenario.fails",
            "scenario.none",
            "scenario.unvisited"
        ]
    );
    assert!(crate::proto_verify::rules().iter().all(|r| r.about.len() > 40), "each says what it measures");
    // B is a dead end: found for B, for the page and for no scope at all, not for A alone.
    let page = doc.page_of(a).unwrap();
    for (scope, finds) in [(vec![b], true), (vec![a, b], true), (vec![page], true), (vec![], true), (vec![a], false)] {
        let got = found(doc, "flow.deadEnd", &scope);
        assert_eq!(got.len(), usize::from(finds), "{scope:?}");
        assert!(got.iter().all(|f| f.rule == "flow.deadEnd" && f.nodes == [b]));
    }
}
