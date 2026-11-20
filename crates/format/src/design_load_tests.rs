//! A design context that can't be read, or passes its limits, doesn't stop a file from opening:
//! the layers load, what was left out or cut is listed with the repairs, and saving the file
//! doesn't write the unreadable value back.

use figcraft_doc::{Document, NodeKind};
use serde_json::{Value, json};

use super::*;

/// A saved document's JSON (one frame, "Home") with `design` replaced by `design`.
fn file_with(design: Value) -> String {
    let mut d = Document::new("context");
    let page = d.first_page().unwrap();
    d.create(page, NodeKind::Frame, "Home", 0.0, 0.0, 100.0, 100.0).unwrap();
    let mut v = to_json(&d).unwrap();
    assert!(v.get("design").is_none(), "a file without a design context has no `design` key");
    v["design"] = design;
    v.to_string()
}

fn has_home(d: &Document) -> bool {
    d.nodes().any(|n| n.name == "Home" && matches!(n.kind, NodeKind::Frame))
}

#[test]
fn a_file_whose_design_context_cant_be_read_opens_with_its_layers_and_lists_the_issue() {
    let cases = [
        (
            json!({"brief": {"job": "Hand over"}, "scenarios": [{"id": "S1", "name": "x"}], "counters": {"S": 1}}),
            "`scenarios[0]`: missing field `start`",
        ),
        (json!({"brief": {"job": "Hand over"}, "stage": "shipping"}), "`stage`: unknown variant `shipping`"),
        (
            json!({"brief": {"job": "Hand over"}, "scenarios": [{"id": "S1", "name": "n", "start": 4, "steps": [{"action": {"type": "swipe"}}]}], "counters": {"S": 1}}),
            "`scenarios[0]`: unknown variant `swipe`",
        ),
    ];
    for (design, why) in cases {
        let text = file_with(design);
        let (doc, issues) = from_json(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert!(has_home(&doc), "the layers are intact");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].starts_with("part of the design context couldn't be read and was left out (") && issues[0].contains(why), "{issues:?}");
        assert_eq!(doc.design.brief.job, "Hand over", "what could be read is kept");
        assert!(doc.design.scenarios.is_empty());
        // Through the archive too, and what couldn't be read isn't written back.
        let mut archive = zip_files([("document.json", text.as_bytes())]).unwrap();
        let (again, issues) = load_bytes(&archive).unwrap();
        assert!(has_home(&again) && issues.len() == 1, "{issues:?}");
        archive = save_bytes(&again, None).unwrap();
        let (saved, issues) = load_bytes(&archive).unwrap();
        assert!(issues.is_empty(), "the saved file is clean: {issues:?}");
        assert_eq!(saved.design, again.design);
        let written = to_json(&again).unwrap()["design"].to_string();
        assert!(!written.contains("swipe") && !written.contains("shipping") && !written.contains("S1"), "{written}");
    }
    // Not a context at all: left out whole.
    let (doc, issues) = from_json(&file_with(json!("everything"))).unwrap();
    assert!(has_home(&doc) && doc.design.is_empty());
    assert!(issues.len() == 1 && issues[0].starts_with("the design context couldn't be read and was left out: "), "{issues:?}");
    assert!(to_json(&doc).unwrap().get("design").is_none());
}

#[test]
fn a_design_context_past_its_limits_is_cut_on_load_and_reported() {
    let max = figcraft_doc::design::MAX_ENTRIES;
    let decisions: Vec<Value> = (1..=max + 1).map(|k| json!({"id": format!("D{k}"), "title": "t", "rationale": "r"})).collect();
    let (doc, issues) = from_json(&file_with(json!({"decisions": decisions, "counters": {"D": max + 1}}))).unwrap();
    assert!(has_home(&doc));
    assert_eq!(doc.design.decisions.len(), max);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains(&format!("`decisions` from {} to {max} entries", max + 1)), "{issues:?}");
}

#[test]
fn a_readable_design_context_is_no_issue() {
    let design = json!({"brief": {"audience": "Nurses"}, "endStates": [{"node": 3}], "scenarios": [{"id": "S1", "name": "n", "start": 3, "steps": []}], "counters": {"S": 1}});
    let (doc, issues) = from_json(&file_with(design.clone())).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(to_json(&doc).unwrap()["design"], design, "written back as it was read");
}

/// A saved document's JSON with `design` set to the JSON text `design` as written (a `Value` can't
/// hold a lone surrogate, a number past `f64` or a key twice).
fn file_with_text(design: &str) -> String {
    file_with(Value::Null).replacen("\"design\":null", &format!("\"design\":{design}"), 1)
}

/// How a file whose JSON a reader refuses opened before the design context was read straight from
/// its text (M12.45): refused as corrupt, with serde_json's own message for that text, the fault
/// and its place in `document.json`.
fn refused_as_it_always_was(text: &str) {
    let before = serde_json::from_str::<Value>(text).expect_err("the case is JSON a reader refuses").to_string();
    match from_json(text) {
        Err(FormatError::Corrupt(why)) => assert_eq!(why, before, "{text}"),
        Err(e) => panic!("{text}: refused, but not as corrupt: {e}"),
        Ok((doc, issues)) => panic!("{text}: opened (context {:?}, issues {issues:?})", doc.design.brief),
    }
}

#[test]
fn a_design_context_that_isnt_valid_json_refuses_the_file_as_it_always_did() {
    // A lone surrogate in a field the context reads, in one it doesn't know, in a key; a number
    // past f64's range in a field it reads and in one it doesn't. Opening one of these used to
    // refuse the file; it must never open with the context dropped (saving would delete it).
    for design in [
        r#"{"brief":{"audience":"\ud800"}}"#,
        r#"{"brief":{"audience":"a\udc00b"}}"#,
        r#"{"brief":{"audience":"Nurses"},"later":"\ud800x"}"#,
        r#"{"brief":{"audience":"Nurses","\udfff":1}}"#,
        r#"{"brief":{"audience":"Nurses"},"later":1e400}"#,
        r#"{"brief":{"audience":"Nurses"},"later":[{"deep":-2.5E+999}]}"#,
        r#"{"counters":{"S":1e400}}"#,
    ] {
        refused_as_it_always_was(&file_with_text(design));
    }
    // A surrogate pair and a large finite number read as ever.
    let (doc, issues) = from_json(&file_with_text(r#"{"brief":{"audience":"\ud83d\ude00"},"later":1e300}"#)).unwrap();
    assert!(issues.is_empty() && doc.design.brief.audience == "\u{1F600}", "{issues:?}");
}

#[test]
fn a_design_context_nested_past_the_readers_limit_refuses_the_file_as_it_always_did() {
    // serde_json refuses nesting 128 deep, counted from the document; reading the context from its
    // own text would count from the context, and skipping a field it doesn't know counts nothing.
    for depth in 120..=130 {
        let nest = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        for design in [
            format!(r#"{{"brief":{{"audience":"N"}},"later":{nest}}}"#),
            format!(
                r#"{{"scenarios":[{{"id":"S1","name":"n","start":4,"steps":[{{"action":{{"type":"setVariable","variable":"v","value":{nest}}}}}]}}]}}"#
            ),
        ] {
            let text = file_with_text(&design);
            if serde_json::from_str::<Value>(&text).is_err() {
                refused_as_it_always_was(&text);
            } else {
                assert!(from_json(&text).is_ok(), "{depth}: {design}");
            }
        }
    }
}

#[test]
fn a_design_context_with_a_key_twice_opens_as_it_always_did() {
    // Before M12.45 the context was read through a `Value`, which keeps the last of a key given
    // twice; reading it straight must give the same context and the same report, nested objects
    // included: the state's `shownBy` (its variant is told apart by its fields), a brief, a step's
    // action, an expectation's variables, a variable's value set by a step, a counter.
    for design in [
        r#"{"states":[{"frame":4,"kind":"error","shownBy":{"node":1,"instance":2,"variant":"x","node":3}}]}"#,
        r#"{"states":[{"frame":4,"kind":"error","shownBy":{"instance":2,"variant":"x","instance":5}}]}"#,
        r#"{"states":[{"frame":4,"kind":"error","shownBy":{"node":1,"node":"bad"}}]}"#,
        r#"{"brief":{"audience":"A","audience":"B"}}"#,
        r#"{"stage":"converging","brief":{"audience":"A"},"stage":"ready"}"#,
        r#"{"scenarios":[{"id":"S1","name":"n","start":4,"steps":[{"action":{"type":"wait","type":"key","key":"Enter"}}]}]}"#,
        r#"{"scenarios":[{"id":"S1","name":"n","start":4,"steps":[{"action":{"type":"wait"},"expect":{"variables":{"n":1,"n":2}}}]}]}"#,
        r#"{"scenarios":[{"id":"S1","name":"n","start":4,"steps":[{"action":{"type":"setVariable","variable":"v","value":{"a":1,"a":{"b":2,"b":3}}}}]}]}"#,
        r#"{"counters":{"S":1,"S":4}}"#,
    ] {
        let text = file_with_text(design);
        let (straight, straight_issues) = from_json(&text).unwrap_or_else(|e| panic!("{design}: {e}"));
        // Through a `Value`, as the file was read before: the last of each key.
        let mut through: figcraft_doc::Document = serde_json::from_value(serde_json::from_str(&text).unwrap()).unwrap();
        let through_issues = figcraft_doc::design::take_load_issues(&mut through.design);
        assert_eq!(to_json(&straight).unwrap()["design"], to_json(&through).unwrap()["design"], "{design}");
        assert_eq!(straight.design, through.design, "{design}");
        let straight_issues: Vec<&String> = straight_issues.iter().filter(|i| i.contains("design context")).collect();
        assert_eq!(straight_issues, through_issues.iter().collect::<Vec<_>>(), "{design}");
    }
    // The state reads as the layer it names last, as it did.
    let (doc, _) =
        from_json(&file_with_text(r#"{"states":[{"frame":4,"kind":"error","shownBy":{"node":1,"instance":2,"variant":"x","node":3}}]}"#)).unwrap();
    assert_eq!(
        doc.design.states.first().and_then(|s| s.shown_by.clone()),
        Some(figcraft_doc::design::ShownBy::Node { node: figcraft_doc::NodeId(3) })
    );
}
