//! Alternatives: copying a screen without touching it, comparing by facts, choosing, removing.

use figcraft_doc::NodeKind;
use figcraft_doc::design::DecisionStatus;
use serde_json::json;

use super::{
    AGENT_NAME, NodeId, Session, Value, as_agent, doc_json, err, ex, ex_undoable, frame, label, nid, one_undo_step, rect, refused_unchanged, strings,
};

/// A 320×480 screen "Home" with a title, a body text and a button that goes to "Done".
struct Screen {
    home: NodeId,
    title: NodeId,
    body: NodeId,
    button: NodeId,
    done: NodeId,
}

fn screen(s: &mut Session) -> Screen {
    let home = frame(s, "Home", 0.0, 320.0, 480.0);
    let done = frame(s, "Done", 0.0, 320.0, 480.0);
    ex(s, "node.set", json!({"id": done, "props": {"y": 600}}));
    let title = label(s, home, "Hand over  a patient", [20.0, 20.0], json!({"name": "Title", "fontSize": 24}));
    let body = label(s, home, "Scan the wristband to start", [20.0, 80.0], json!({"name": "Body"}));
    let button = rect(s, home, [20.0, 400.0, 280.0, 48.0], json!({"name": "Start", "fill": "#0D99FF"}));
    ex(s, "proto.interaction.add", json!({"id": button, "dest": done}));
    Screen { home, title, body, button, done }
}

/// The layers of `root`'s subtree as they serialize.
fn subtree_json(s: &Session, root: NodeId) -> Vec<Value> {
    let doc = s.doc().unwrap();
    doc.subtree(root).into_iter().map(|i| serde_json::to_value(doc.node(i).unwrap()).unwrap()).collect()
}

/// The layer of `copy`'s subtree at the place `original` has in `source`'s.
fn twin(s: &Session, source: NodeId, copy: NodeId, original: NodeId) -> NodeId {
    let doc = s.doc().unwrap();
    let at = doc.subtree(source).iter().position(|i| *i == original).expect("the layer is in the source");
    doc.subtree(copy)[at]
}

/// Set `props` on the layer of `copy` that mirrors `original` of `source`.
fn set_twin(s: &mut Session, [source, copy, original]: [NodeId; 3], props: Value) {
    let id = twin(s, source, copy, original);
    ex(s, "node.set", json!({"id": id, "props": props}));
}

fn alternative(s: &mut Session, source: NodeId, name: &str) -> NodeId {
    crate::params::id_of(&ex(s, "design.alternative.create", json!({"source": source, "name": name, "constraint": "Half the words"}))["node"])
        .unwrap()
}

fn compared(s: &mut Session) -> Value {
    ex(s, "design.alternative.compare", json!({"set": "V1"}))
}

#[test]
fn creating_an_alternative_copies_the_source_beside_it_and_leaves_it_untouched() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let source_before = subtree_json(&s, sc.home);
    let before = doc_json(&s);
    let r = ex(
        &mut s,
        "design.alternative.create",
        json!({"source": sc.home, "name": "Compact", "constraint": "One thumb, no scrolling", "intent": "On the move", "question": "Can it be done one-handed?", "constraints": ["Same content", "Same brand colours"]}),
    );
    assert_eq!(r["set"], "V1");
    let copy = crate::params::id_of(&r["node"]).unwrap();
    assert_eq!(subtree_json(&s, sc.home), source_before, "the source must stay byte-identical");
    let doc = s.doc().unwrap();
    let (src, alt) = (doc.node(sc.home).unwrap(), doc.node(copy).unwrap());
    assert_eq!(alt.name, "Home / Compact");
    assert_eq!((alt.parent, alt.width, alt.height), (src.parent, 320.0, 480.0));
    let (sb, cb) = (doc.world_bounds(sc.home).unwrap(), doc.world_bounds(copy).unwrap());
    assert_eq!((cb.x0, cb.y0), (sb.x1 + 100.0, sb.y0), "to the right of the source, 100 px apart, level with it");
    assert_eq!(doc.subtree(copy).len(), doc.subtree(sc.home).len());
    // The copy's button leads where the source's does; the copy has its own layers.
    let button = twin(&s, sc.home, copy, sc.button);
    assert_ne!(button, sc.button);
    assert_eq!(
        serde_json::to_value(&doc.node(button).unwrap().interactions).unwrap(),
        serde_json::to_value(&doc.node(sc.button).unwrap().interactions).unwrap()
    );
    let set = &doc.design.alternatives[0];
    assert_eq!((set.id.as_str(), set.source, set.question.as_str()), ("V1", sc.home, "Can it be done one-handed?"));
    assert_eq!(set.constraints, ["Same content", "Same brand colours"]);
    assert_eq!(
        (set.members[0].node, set.members[0].name.as_str(), set.members[0].constraint.as_str(), set.members[0].intent.as_str()),
        (copy, "Compact", "One thumb, no scrolling", "On the move")
    );
    // One undo removes the copy and the record together.
    one_undo_step(&mut s, &before);
    assert!(s.undo().unwrap());
    assert!(!s.doc().unwrap().contains(copy) && s.doc().unwrap().design.alternatives.is_empty());
    assert!(s.redo().unwrap());

    // The next one joins the source's set and stands to the right of the last.
    let second = alternative(&mut s, sc.home, "Spacious");
    let doc = s.doc().unwrap();
    assert_eq!(doc.design.alternatives.len(), 1);
    assert_eq!(doc.design.alternatives[0].members.len(), 2);
    assert_eq!(doc.world_bounds(second).unwrap().x0, doc.world_bounds(copy).unwrap().x1 + 100.0);
    assert_eq!(doc.node(second).unwrap().name, "Home / Spacious");
    assert_eq!(subtree_json(&s, sc.home), source_before);
    // Another source starts another set; naming the set adds to that one.
    let third = alternative(&mut s, sc.done, "Quiet");
    assert_eq!(s.doc().unwrap().design.alternatives[1].id, "V2");
    let r = ex(
        &mut s,
        "design.alternative.create",
        json!({"source": sc.home, "name": "Third", "constraint": "c", "set": "v1", "question": "Can it be done one-handed?"}),
    );
    assert_eq!(r["set"], "V1");
    assert_eq!(s.doc().unwrap().design.alternatives[0].members.len(), 3);
    let _ = third;
}

#[test]
fn interactions_inside_the_copy_point_into_the_copy_and_instances_keep_their_overrides() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let chip = frame(&mut s, "Chip", 0.0, 80.0, 30.0);
    ex(&mut s, "node.set", json!({"id": chip, "props": {"y": 600}}));
    let chip_text = label(&mut s, chip, "Label", [5.0, 5.0], json!({"name": "Label"}));
    let main = nid(&ex(&mut s, "component.create", json!({"ids": [chip]})));
    let inst = nid(&ex(&mut s, "instance.create", json!({"component": main, "x": 20, "y": 20, "parent": home})));
    let inst_text = crate::components::tests::copy_of(&s, inst, chip_text);
    ex(&mut s, "node.set", json!({"id": inst_text, "props": {"characters": "Urgent", "fill": "#FF0000"}}));
    let panel = rect(&mut s, home, [20.0, 100.0, 100.0, 100.0], json!({"name": "Panel"}));
    let jump = rect(&mut s, home, [20.0, 300.0, 100.0, 40.0], json!({"name": "Jump"}));
    ex(&mut s, "proto.interaction.add", json!({"id": jump, "action": {"type": "scrollTo", "dest": panel}}));

    let copy = alternative(&mut s, home, "B");
    let doc = s.doc().unwrap();
    let (copy_inst, copy_text) = (twin(&s, home, copy, inst), twin(&s, home, copy, inst_text));
    assert!(
        matches!(doc.node(copy_inst).unwrap().kind, NodeKind::Instance { component, .. } if component == main),
        "still an instance of the same main"
    );
    let t = doc.node(copy_text).unwrap();
    assert_eq!(t.kind.text().unwrap().characters, "Urgent", "the text override is kept");
    assert_eq!(t.fills[0].solid_color().unwrap().to_hex(), "FF0000", "the fill override is kept");
    assert_eq!(t.overrides, doc.node(inst_text).unwrap().overrides);
    // The copy's scroll-to targets the copy's panel, not the source's.
    let copy_jump = doc.node(twin(&s, home, copy, jump)).unwrap();
    let dest = serde_json::to_value(&copy_jump.interactions[0].actions[0]).unwrap()["dest"].clone();
    assert_eq!(dest, json!(twin(&s, home, copy, panel)));
    crate::components::tests::healthy(&s);
}

#[test]
fn create_errors_name_the_parameter_and_change_nothing() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    alternative(&mut s, sc.home, "A");
    for (params, needle) in [
        (json!({"name": "x", "constraint": "c"}), "`source` is required".to_string()),
        (json!({"source": 9999, "name": "x", "constraint": "c"}), "`source`: no layer 9999".to_string()),
        (
            json!({"source": sc.title, "name": "x", "constraint": "c"}),
            format!("`source`: {} (`Title`) isn't a top-level frame or a component", sc.title),
        ),
        (json!({"source": sc.home, "constraint": "c"}), "`name` is required".to_string()),
        (json!({"source": sc.home, "name": " ", "constraint": "c"}), "`name` can't be empty".to_string()),
        (json!({"source": sc.home, "name": "x"}), "`constraint` is required".to_string()),
        (json!({"source": sc.home, "name": "x", "constraint": ""}), "`constraint` can't be empty".to_string()),
        (json!({"source": sc.home, "name": "x", "constraint": "c", "set": "V9"}), "`set`: no set of alternatives V9".to_string()),
        (json!({"source": sc.done, "name": "x", "constraint": "c", "set": "V1"}), format!("`set`: V1 holds alternatives of layer {}", sc.home)),
        (
            json!({"source": sc.home, "name": "x", "constraint": "c", "question": "Another question?"}),
            "`question`: set V1 exists already".to_string(),
        ),
        (
            json!({"source": sc.home, "name": "x", "constraint": "c", "constraints": ["New limit"]}),
            "`constraints`: set V1 exists already".to_string(),
        ),
        (json!({"source": sc.home, "name": "x", "constraint": "c", "constraints": "one"}), "`constraints` must be a list".to_string()),
    ] {
        let e = refused_unchanged(&mut s, "design.alternative.create", params.clone());
        assert!(e.contains(&needle), "{params}: {e}");
    }
}

#[test]
fn a_components_alternative_is_a_component_of_its_own() {
    let mut s = Session::new();
    let card = frame(&mut s, "Card", 0.0, 200.0, 100.0);
    label(&mut s, card, "Title", [10.0, 10.0], json!({}));
    let main = nid(&ex(&mut s, "component.create", json!({"ids": [card]})));
    let before = subtree_json(&s, main);
    let copy = alternative(&mut s, main, "Stacked");
    let doc = s.doc().unwrap();
    assert!(matches!(doc.node(copy).unwrap().kind, NodeKind::Component { .. }), "an instance couldn't be reworked");
    assert_eq!(doc.node(copy).unwrap().name, "Card / Stacked");
    assert_eq!(subtree_json(&s, main), before);
    crate::components::tests::healthy(&s);
    // A variant is only meaningful in its set.
    let other = frame(&mut s, "Card", 300.0, 200.0, 100.0);
    let second = nid(&ex(&mut s, "component.create", json!({"ids": [other]})));
    let fresh = frame(&mut s, "Tag", 600.0, 50.0, 20.0);
    let a = nid(&ex(&mut s, "component.create", json!({"ids": [fresh]})));
    let fresh = frame(&mut s, "Tag", 700.0, 50.0, 20.0);
    let b = nid(&ex(&mut s, "component.create", json!({"ids": [fresh]})));
    let set = ex(&mut s, "component.combineVariants", json!({"ids": [a, b]}));
    let e = refused_unchanged(&mut s, "design.alternative.create", json!({"source": a, "name": "x", "constraint": "c"}));
    assert!(e.contains("is a variant; make alternatives of its component set"), "{e}");
    let _ = (second, set);
}

#[test]
fn update_and_question_edit_the_record() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let copy = alternative(&mut s, sc.home, "A");
    let r = ex_undoable(
        &mut s,
        "design.alternative.update",
        json!({"set": "V1", "node": copy, "name": "Airy", "tradeoffs": "Scrolls on small phones", "intent": "First-time users"}),
    );
    assert_eq!(
        r["alternative"],
        json!({"node": copy, "name": "Airy", "constraint": "Half the words", "intent": "First-time users", "tradeoffs": "Scrolls on small phones"})
    );
    assert_eq!(s.doc().unwrap().node(copy).unwrap().name, "Home / A", "the layer keeps its name");
    let r =
        ex_undoable(&mut s, "design.alternative.question", json!({"set": "V1", "question": "Which reads fastest?", "constraints": ["Same words"]}));
    assert_eq!(r, json!({"set": "V1", "question": "Which reads fastest?", "constraints": ["Same words"]}));
    let r = ex(&mut s, "design.alternative.question", json!({"set": "V1", "constraints": []}));
    assert_eq!(r["question"], "Which reads fastest?", "only what is given changes");
    assert_eq!(r["constraints"], json!([]));
    for (cmd, params, needle) in [
        (
            "design.alternative.update",
            json!({"set": "V1", "node": sc.home, "name": "x"}),
            format!("`node`: layer {} isn't an alternative in set V1", sc.home),
        ),
        ("design.alternative.update", json!({"set": "V7", "node": copy}), "`set`: no set of alternatives V7".to_string()),
        ("design.alternative.update", json!({"set": "V1", "node": copy, "constraint": ""}), "`constraint` can't be empty".to_string()),
        ("design.alternative.update", json!({"set": "V1"}), "`node` is required".to_string()),
        ("design.alternative.question", json!({"question": "q"}), "`set` is required".to_string()),
        ("design.alternative.question", json!({"set": "V1", "constraints": [5]}), "`constraints`: entry 1 must be text".to_string()),
    ] {
        let e = refused_unchanged(&mut s, cmd, params.clone());
        assert!(e.contains(&needle), "{cmd} {params}: {e}");
    }
}

#[test]
fn compare_reports_facts_about_each_member() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let copy = alternative(&mut s, sc.home, "Same");
    let r = compared(&mut s);
    assert_eq!((r["set"].as_str(), r["question"].as_str(), r["members"].as_array().map(Vec::len)), (Some("V1"), Some(""), Some(2)));
    let source = &r["members"][0];
    assert_eq!(
        (source["node"].clone(), source["name"].as_str(), source["isSource"].as_bool(), source["exists"].as_bool()),
        (json!(sc.home), Some("Home"), Some(true), Some(true))
    );
    let m = &source["metrics"];
    assert_eq!((m["width"].as_f64(), m["height"].as_f64()), (Some(320.0), Some(480.0)));
    assert_eq!((m["layers"].as_u64(), m["depth"].as_u64(), m["textLayers"].as_u64()), (Some(3), Some(1), Some(2)));
    assert_eq!((m["words"].as_u64(), m["characters"].as_u64()), (Some(9), Some(47)));
    assert_eq!(
        (m["interactive"].as_u64(), m["instances"].as_u64(), m["components"].as_u64(), m["images"].as_u64()),
        (Some(1), Some(0), Some(0), Some(0))
    );
    let density = m["density"].as_f64().unwrap();
    assert!((0.08..=0.2).contains(&density), "the button alone covers 280×48 of 320×480: {density}");
    // What a viewer reads, top to bottom, one line each; what reacts to a click.
    assert_eq!(source["readingOrder"], json!(["Hand over a patient", "Scan the wristband to start"]));
    assert_eq!(source["actions"], json!(["Start"]));
    let member = &r["members"][1];
    assert_eq!(
        (member["node"].clone(), member["name"].as_str(), member["isSource"].as_bool(), member["constraint"].as_str()),
        (json!(copy), Some("Same"), Some(false), Some("Half the words"))
    );
    assert_eq!(member["metrics"], source["metrics"]);
    // An untouched copy is identical, and says so.
    let d = &r["differences"][0];
    assert_eq!((d["a"].clone(), d["b"].clone()), (json!(sc.home), json!(copy)));
    for key in ["structure", "order", "content", "actions", "size", "layout", "paint", "paintOnly"] {
        assert_eq!(d[key], false, "{key}: {d}");
    }
    assert_eq!(d["identical"], true);
    assert_eq!(strings(&r["warnings"]), ["`Same` is identical to the source (same layers, text, layout and paint): nothing was changed yet."]);
    // Facts only: nothing ranks or scores the members.
    let text = r.to_string();
    for word in ["score", "rank", "best", "winner", "recommend"] {
        assert!(!text.to_lowercase().contains(word), "compare mustn't judge: found `{word}`");
    }
    assert_eq!(s.undo_depth().1, 0);
}

#[test]
fn a_change_of_type_that_moves_layers_isnt_called_paint_only() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let big = alternative(&mut s, sc.home, "Big type");
    // A bigger title is a change of paint (fonts) that also changes the title's box: whether
    // anything else was rearranged with it can't be told, so it isn't flagged as a mere restyle.
    set_twin(&mut s, [sc.home, big, sc.title], json!({"fontSize": 40}));
    let r = compared(&mut s);
    let d = &r["differences"][0];
    assert_eq!(
        (d["paint"].as_bool(), d["layout"].as_bool(), d["paintOnly"].as_bool(), d["identical"].as_bool()),
        (Some(true), Some(true), Some(false), Some(false)),
        "{d}"
    );
    assert_eq!(strings(&r["warnings"]), Vec::<String>::new());
}

#[test]
fn a_recoloured_copy_is_flagged_as_a_restyle_and_a_reworked_one_is_not() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let recoloured = alternative(&mut s, sc.home, "Dark");
    let reworked = alternative(&mut s, sc.home, "Short");
    // Dark: new fills and a stroke; the same layers, words, actions and places.
    ex(&mut s, "node.set", json!({"id": recoloured, "props": {"fill": "#111111"}}));
    set_twin(&mut s, [sc.home, recoloured, sc.title], json!({"fill": "#FFFFFF"}));
    set_twin(&mut s, [sc.home, recoloured, sc.button], json!({"fill": "#FFD60A", "strokes": ["#000000"], "strokeWeight": 2}));
    // Short: the body text is gone and a second action is added.
    let body = twin(&s, sc.home, reworked, sc.body);
    ex(&mut s, "edit.delete", json!({"ids": [body]}));
    let skip = rect(&mut s, reworked, [20.0, 340.0, 280.0, 48.0], json!({"name": "Skip"}));
    ex(&mut s, "proto.interaction.add", json!({"id": skip, "dest": sc.done}));

    let r = compared(&mut s);
    let diff = |a: NodeId, b: NodeId| {
        r["differences"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["a"] == json!(a) && d["b"] == json!(b))
            .unwrap_or_else(|| panic!("no pair {a}/{b}"))
            .clone()
    };
    let dark = diff(sc.home, recoloured);
    assert_eq!((dark["paintOnly"].as_bool(), dark["paint"].as_bool(), dark["identical"].as_bool()), (Some(true), Some(true), Some(false)), "{dark}");
    for key in ["structure", "order", "content", "actions", "size"] {
        assert_eq!(dark[key], false, "{key}: {dark}");
    }
    let short = diff(sc.home, reworked);
    assert_eq!(
        (short["structure"].as_bool(), short["content"].as_bool(), short["order"].as_bool(), short["actions"].as_bool()),
        (Some(true), Some(true), Some(true), Some(true)),
        "{short}"
    );
    assert_eq!((short["paintOnly"].as_bool(), short["identical"].as_bool(), short["size"].as_bool()), (Some(false), Some(false), Some(false)));
    // The two members are compared with each other too.
    assert_eq!(diff(recoloured, reworked)["structure"], true);
    assert_eq!(r["differences"].as_array().unwrap().len(), 3);
    assert_eq!(
        strings(&r["warnings"]),
        ["`Dark` differs from the source only in paint (fills, strokes, effects, fonts): it is a restyle, not a structural alternative."]
    );
    assert_eq!(r["members"][2]["readingOrder"], json!(["Hand over a patient"]));
    assert_eq!(r["members"][2]["actions"], json!(["Skip", "Start"]));
    assert_eq!(r["members"][2]["metrics"]["interactive"], 2);
}

#[test]
fn compare_tells_order_content_size_and_layout_apart() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let swapped = alternative(&mut s, sc.home, "Body first");
    let reworded = alternative(&mut s, sc.home, "Reworded");
    let resized = alternative(&mut s, sc.home, "Tablet");
    let moved = alternative(&mut s, sc.home, "Button up");
    let hidden = alternative(&mut s, sc.home, "No body");
    // The same texts in another order.
    set_twin(&mut s, [sc.home, swapped, sc.title], json!({"y": 120}));
    // Other words; case and spacing alone don't count.
    set_twin(&mut s, [sc.home, reworded, sc.body], json!({"characters": "Scan to begin"}));
    set_twin(&mut s, [sc.home, reworded, sc.title], json!({"characters": "HAND OVER A   PATIENT"}));
    ex(&mut s, "node.set", json!({"id": resized, "props": {"width": 768}}));
    // Recoloured and rearranged: more than a restyle.
    set_twin(&mut s, [sc.home, moved, sc.button], json!({"y": 200, "fill": "#FF0000"}));
    // Hiding a layer changes what the viewer gets.
    set_twin(&mut s, [sc.home, hidden, sc.body], json!({"visible": false}));

    let r = compared(&mut s);
    let flags = |node: NodeId| -> Vec<String> {
        let d = r["differences"].as_array().unwrap().iter().find(|d| d["a"] == json!(sc.home) && d["b"] == json!(node)).unwrap().clone();
        ["structure", "order", "content", "actions", "size", "layout", "paint", "paintOnly", "identical"]
            .into_iter()
            .filter(|k| d[*k] == true)
            .map(str::to_string)
            .collect()
    };
    assert_eq!(flags(swapped), ["order", "layout"]);
    assert_eq!(
        flags(reworded),
        ["order", "content", "layout"],
        "the title only changed case and spacing; the body's words changed (and its box with them)"
    );
    assert_eq!(flags(resized), ["size", "layout"]);
    assert_eq!(flags(moved), ["layout", "paint"], "paint and geometry both changed: not paint only");
    assert_eq!(flags(hidden), ["structure", "order", "content"]);
    // Layers that can't be matched one to one aren't compared for layout or paint.
    let unmatched = r["differences"].as_array().unwrap().iter().find(|d| d["b"] == json!(hidden)).unwrap();
    assert_eq!((unmatched["layout"].clone(), unmatched["paint"].clone()), (Value::Null, Value::Null));
    assert_eq!(r["warnings"], json!([]));
    let swapped_row = r["members"].as_array().unwrap().iter().find(|m| m["node"] == json!(swapped)).unwrap();
    assert_eq!(swapped_row["readingOrder"], json!(["Scan the wristband to start", "Hand over a patient"]));
    // Long texts are cut to a line's worth, and at most 40 are listed.
    let long = alternative(&mut s, sc.home, "Wordy");
    set_twin(&mut s, [sc.home, long, sc.body], json!({"characters": "word ".repeat(60)}));
    for i in 0..45 {
        label(&mut s, long, &format!("Row {i}"), [20.0, 110.0 + f64::from(i) * 5.0], json!({}));
    }
    let r = compared(&mut s);
    let row = r["members"].as_array().unwrap().iter().find(|m| m["node"] == json!(long)).unwrap();
    let reading = strings(&row["readingOrder"]);
    assert_eq!(reading.len(), 40);
    assert_eq!(reading[1].chars().count(), 80);
    assert_eq!(row["metrics"]["textLayers"], 47);
    assert!(r["warnings"].as_array().unwrap().is_empty() && r["differences"].as_array().unwrap().len() == 6 + 15);
}

#[test]
fn compare_says_when_layers_are_gone() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let gone = alternative(&mut s, sc.home, "Gone");
    alternative(&mut s, sc.home, "Kept");
    ex(&mut s, "edit.delete", json!({"ids": [gone]}));
    let r = compared(&mut s);
    assert_eq!(
        (r["members"][1]["exists"].as_bool(), r["members"][1]["metrics"].clone(), r["members"][1]["readingOrder"].clone()),
        (Some(false), Value::Null, json!([]))
    );
    assert_eq!(r["differences"].as_array().unwrap().len(), 1, "only pairs whose layers exist: {r}");
    assert!(strings(&r["warnings"]).contains(&format!("`Gone` (layer {gone}) no longer exists")));
    ex(&mut s, "edit.delete", json!({"ids": [sc.home]}));
    let r = compared(&mut s);
    assert!(strings(&r["warnings"])[0].starts_with(&format!("the source (layer {}) no longer exists", sc.home)));
    assert_eq!(r["differences"], json!([]));
    assert!(err(&mut s, "design.alternative.compare", json!({"set": "V2"})).contains("`set`: no set of alternatives V2"));
    assert!(err(&mut s, "design.alternative.compare", json!({})).contains("`set` is required"));
}

#[test]
fn choosing_records_a_decision_and_deletes_nothing() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let a = alternative(&mut s, sc.home, "Compact");
    let b = alternative(&mut s, sc.home, "Airy");
    ex(&mut s, "design.alternative.question", json!({"set": "V1", "question": "Which is faster one-handed?"}));
    let layers = s.doc().unwrap().node_count();
    let r = ex_undoable(&mut s, "design.alternative.choose", json!({"set": "V1", "node": a, "rationale": "Everything is within thumb reach"}));
    assert_eq!(r, json!({"decision": "D1", "status": "accepted"}));
    let ctx = s.doc().unwrap().design.clone();
    assert_eq!((ctx.alternatives[0].chosen, ctx.alternatives[0].decision.as_str()), (Some(a), "D1"));
    let d = &ctx.decisions[0];
    assert_eq!((d.title.as_str(), d.rationale.as_str()), ("Chose \"Compact\": Which is faster one-handed?", "Everything is within thumb reach"));
    assert_eq!((d.links.clone(), d.author.as_str(), d.status), (vec![a], "user", DecisionStatus::Accepted));
    assert_eq!(s.doc().unwrap().node_count(), layers, "nothing is deleted");
    assert_eq!(compared(&mut s)["chosen"], json!(a));
    // An agent's choice is recorded as a proposal for a person to accept; the source can be chosen.
    let r = as_agent(&mut s, "design.alternative.choose", json!({"set": "V1", "node": sc.home, "rationale": "Neither improves on the original"}));
    assert_eq!(r, json!({"decision": "D2", "status": "proposed"}));
    let ctx = s.doc().unwrap().design.clone();
    assert_eq!((ctx.alternatives[0].chosen, ctx.alternatives[0].decision.as_str()), (Some(sc.home), "D2"));
    assert_eq!((ctx.decisions[1].title.as_str(), ctx.decisions[1].author.as_str()), ("Chose \"Home\": Which is faster one-handed?", AGENT_NAME));
    assert_eq!(ctx.decisions.len(), 2, "the earlier decision stays on record");
    for (params, needle) in [
        (json!({"set": "V1", "node": a}), "`rationale` is required".to_string()),
        (
            json!({"set": "V1", "node": sc.done, "rationale": "r"}),
            format!("`node`: layer {} is neither the source of set V1 nor one of its alternatives", sc.done),
        ),
        (json!({"set": "V1", "node": 99999, "rationale": "r"}), "`node`: no layer 99999".to_string()),
        (json!({"set": "V3", "node": a, "rationale": "r"}), "`set`: no set of alternatives V3".to_string()),
        (json!({"set": "V1", "rationale": "r"}), "`node` is required".to_string()),
    ] {
        let e = refused_unchanged(&mut s, "design.alternative.choose", params.clone());
        assert!(e.contains(&needle), "{params}: {e}");
    }
    let _ = b;
}

#[test]
fn removing_forgets_members_and_deletes_layers_only_when_asked() {
    let mut s = Session::new();
    let sc = screen(&mut s);
    let a = alternative(&mut s, sc.home, "A");
    let b = alternative(&mut s, sc.home, "B");
    let c = alternative(&mut s, sc.home, "C");
    ex(&mut s, "design.alternative.choose", json!({"set": "V1", "node": a, "rationale": "r"}));
    // Forget one: its layer stays; it was the chosen one, so nothing is chosen any more.
    let r = ex_undoable(&mut s, "design.alternative.remove", json!({"set": "V1", "node": a}));
    assert_eq!(r, json!({"removed": [a], "deleted": []}));
    let doc = s.doc().unwrap();
    assert!(doc.contains(a));
    assert_eq!((doc.design.alternatives[0].members.len(), doc.design.alternatives[0].chosen), (2, None));
    assert_eq!(doc.design.decisions.len(), 1, "the decision stays on record");
    // Forget one and delete its layer.
    let r = ex_undoable(&mut s, "design.alternative.remove", json!({"set": "V1", "node": b, "deleteLayer": true}));
    assert_eq!(r, json!({"removed": [b], "deleted": [b]}));
    assert!(!s.doc().unwrap().contains(b));
    // The source is never an alternative to remove.
    for (params, needle) in [
        (json!({"set": "V1", "node": sc.home, "deleteLayer": true}), format!("`node`: layer {} is the source of set V1", sc.home)),
        (json!({"set": "V1", "node": a}), format!("`node`: layer {a} isn't an alternative in set V1")),
        (json!({"set": "V4"}), "`set`: no set of alternatives V4".to_string()),
        (json!({"set": "V1", "deleteLayer": "perhaps"}), "`deleteLayer` must be true or false".to_string()),
        (json!({"node": c}), "`set` is required".to_string()),
    ] {
        let e = refused_unchanged(&mut s, "design.alternative.remove", params.clone());
        assert!(e.contains(&needle), "{params}: {e}");
    }
    // Forget the whole set and delete what was in it: the source stays.
    let r = ex_undoable(&mut s, "design.alternative.remove", json!({"set": "V1", "deleteLayer": true}));
    assert_eq!(r, json!({"removed": [c], "deleted": [c]}));
    let doc = s.doc().unwrap();
    assert!(doc.design.alternatives.is_empty() && !doc.contains(c) && doc.contains(sc.home) && doc.contains(a));
    // Without deleteLayer a whole set is forgotten and every layer stays.
    let d = alternative(&mut s, sc.home, "D");
    assert_eq!(s.doc().unwrap().design.alternatives[0].id, "V2", "ids aren't reused");
    assert_eq!(ex(&mut s, "design.alternative.remove", json!({"set": "V2"})), json!({"removed": [d], "deleted": []}));
    assert!(s.doc().unwrap().contains(d));
}
