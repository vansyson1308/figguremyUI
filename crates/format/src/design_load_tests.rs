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
