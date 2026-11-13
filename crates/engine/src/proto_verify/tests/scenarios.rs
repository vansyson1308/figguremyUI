//! Stored scenarios and end states: kept in the file, undoable, run headless, and reported on by
//! the scenario rules.

use figcraft_doc::NodeId;
use figcraft_doc::design::MAX_ENTRIES;
use serde_json::{Value, json};

use super::{check, err, file, problems};
use crate::Session;
use crate::proto_sim::tests::{button, ex, frame, idv, navigate, state_variables};

/// A flow A → B → Done ("Next" on A, "Finish" on B; Done is declared an end state) and a stray
/// frame "Help" reached from A. Returns the session, the frames (a, b, done, help) and the
/// buttons (next, finish).
fn checkout() -> (Session, [NodeId; 4], [NodeId; 2]) {
    let mut s = Session::new();
    let frames = [frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "Done", 800.0), frame(&mut s, "Help", 1200.0)];
    let next = button(&mut s, "Next", frames[0], 20.0, navigate(frames[1]));
    let finish = button(&mut s, "Finish", frames[1], 20.0, navigate(frames[2]));
    button(&mut s, "Help", frames[0], 100.0, navigate(frames[3]));
    button(&mut s, "Back", frames[3], 20.0, json!({"type": "back"}));
    ex(&mut s, "proto.flow.add", json!({"start": frames[0]}));
    ex(&mut s, "proto.endState.set", json!({"id": frames[2], "reason": "the order is placed"}));
    (s, frames, [next, finish])
}

/// The scenario "Place an order": click Next, then Finish, ending on Done.
fn place_an_order(a: NodeId, b: NodeId, done: NodeId, next: NodeId, finish: NodeId) -> Value {
    json!({
        "name": "Place an order",
        "purpose": "the primary task can be finished from the start screen",
        "start": a,
        "steps": [{"click": next, "expect": {"screen": b}}, {"click": finish, "note": "the only button"}],
        "expect": {"screen": done, "endState": true},
    })
}

#[test]
fn a_scenario_is_stored_listed_run_edited_and_removed() {
    let (mut s, [a, b, done, help], [next, finish]) = checkout();
    assert_eq!(ex(&mut s, "proto.scenario.list", json!({})), json!({"scenarios": []}));
    assert_eq!(
        ex(&mut s, "proto.scenario.run", json!({})),
        json!({"passed": true, "total": 0, "failed": 0, "results": []}),
        "none stored: none failed"
    );
    let r = ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    assert_eq!(r, json!({"id": "S1"}));
    let listed = ex(&mut s, "proto.scenario.list", json!({}));
    assert_eq!(
        listed["scenarios"],
        json!([{
            "id": "S1",
            "name": "Place an order",
            "purpose": "the primary task can be finished from the start screen",
            "start": a,
            "steps": [
                {"action": {"type": "gesture", "node": next, "gesture": "click"}, "expect": {"screen": b}},
                {"action": {"type": "gesture", "node": finish, "gesture": "click"}, "note": "the only button"},
            ],
            "expect": {"screen": done, "endState": true},
        }])
    );
    let r = ex(&mut s, "proto.scenario.run", json!({}));
    assert_eq!((&r["passed"], &r["total"], &r["failed"]), (&json!(true), &json!(1), &json!(0)), "{r}");
    let one = &r["results"][0];
    assert_eq!(
        (&one["id"], &one["name"], &one["passed"], &one["failures"], &one["stepsTaken"]),
        (&json!("S1"), &json!("Place an order"), &json!(true), &json!([]), &json!(2))
    );
    assert!(one.get("failedStep").is_none());
    assert_eq!(one["steps"][1]["state"], json!({"screen": done, "screenName": "Done", "overlays": []}));
    // What is listed can be given back: only the keys given change.
    let r = ex(&mut s, "proto.scenario.set", json!({"id": "S1", "name": "  Order  ", "steps": listed["scenarios"][0]["steps"], "purpose": ""}));
    assert_eq!(
        (&r["scenario"]["name"], &r["scenario"]["start"], &r["scenario"]["expect"]),
        (&json!("Order"), &json!(a), &json!({"screen": done, "endState": true}))
    );
    assert!(r["scenario"].get("purpose").is_none(), "an empty purpose clears it");
    ex(&mut s, "proto.scenario.set", json!({"id": "S1", "expect": {}, "start": help, "steps": []}));
    let now = ex(&mut s, "proto.scenario.list", json!({}))["scenarios"][0].clone();
    assert_eq!(now, json!({"id": "S1", "name": "Order", "start": help, "steps": []}));
    // Ids are never given out twice.
    assert_eq!(ex(&mut s, "proto.scenario.add", json!({"name": "Second", "start": a, "steps": []})), json!({"id": "S2"}));
    assert_eq!(ex(&mut s, "proto.scenario.remove", json!({"id": "S2"})), json!({"removed": "S2"}));
    assert_eq!(ex(&mut s, "proto.scenario.add", json!({"name": "Third", "start": a, "steps": []})), json!({"id": "S3"}));
    let ids: Vec<Value> = ex(&mut s, "proto.scenario.list", json!({}))["scenarios"].as_array().unwrap().iter().map(|x| x["id"].clone()).collect();
    assert_eq!(ids, [json!("S1"), json!("S3")]);
    assert_eq!(ex(&mut s, "proto.scenario.run", json!({"id": "S3"}))["total"], json!(1));
}

#[test]
fn what_a_scenario_names_must_exist_and_the_error_says_where() {
    let (mut s, [a, b, done, _], [next, finish]) = checkout();
    let before = (file(&s), s.undo_depth());
    let with = |patch: Value| {
        let mut p = place_an_order(a, b, done, next, finish);
        p.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        p
    };
    // (parameters, what the error names)
    let cases = [
        (with(json!({"steps": [{"click": next}, {"click": 4711}]})), "`steps[1].node`: no layer 4711"),
        (with(json!({"steps": [{"click": next, "expect": {"visible": [next, 4712]}}]})), "`steps[0].expect.visible[1]`: no layer 4712"),
        (
            with(json!({"steps": [{"click": next, "expect": {"text": [{"node": 4713, "equals": "x"}]}}]})),
            "`steps[0].expect.text[0].node`: no layer 4713",
        ),
        (with(json!({"expect": {"screen": 4714}})), "`expect.screen`: no layer 4714"),
        (with(json!({"expect": {"overlays": [4715]}})), "`expect.overlays[0]`: no layer 4715"),
        (with(json!({"expect": {"variants": [{"instance": 4716, "variant": "On"}]}})), "`expect.variants[0].instance`: no layer 4716"),
        (with(json!({"start": 4717})), "`start`: no layer 4717"),
        (with(json!({"start": next})), "`start`: `Next`"),
        (with(json!({"name": "  "})), "`name` can't be empty"),
        (with(json!({"name": null})), "`name` is required"),
        (with(json!({"steps": null})), "`steps` is required"),
        (with(json!({"start": null})), "`start` must be a node id"),
        (with(json!({"steps": [{"type": "jump"}]})), "`steps[0].type`"),
        (with(json!({"purpose": "p".repeat(figcraft_doc::design::MAX_TEXT + 1)})), "`purpose`: longer than 4000 characters"),
    ];
    for (params, names) in cases {
        let e = err(&mut s, "proto.scenario.add", params.clone());
        assert!(e.contains(names), "{}: {e}", figcraft_format::clip(&params.to_string(), 160));
    }
    assert_eq!((file(&s), s.undo_depth()), before, "a refused scenario leaves the file as it was");
    // The same checks when one is edited, and an id that isn't stored is named with those that are.
    ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    let before = file(&s);
    assert!(err(&mut s, "proto.scenario.set", json!({"id": "S1", "steps": [{"click": 4711}]})).contains("`steps[0].node`: no layer 4711"));
    assert!(err(&mut s, "proto.scenario.set", json!({"id": "S1", "start": next})).contains("isn't a top-level frame"));
    assert!(err(&mut s, "proto.scenario.set", json!({"id": "S1", "name": ""})).contains("`name` can't be empty"));
    for cmd in ["proto.scenario.set", "proto.scenario.remove", "proto.scenario.run"] {
        let e = err(&mut s, cmd, json!({"id": "S9"}));
        assert!(e.contains("`id`: no scenario `S9` (stored: S1)"), "{cmd}: {e}");
    }
    assert!(err(&mut s, "proto.scenario.set", json!({})).contains("`id` must be a string"));
    assert_eq!(file(&s), before);
}

#[test]
fn every_edit_is_one_undo_step_that_restores_the_file_exactly() {
    let (mut s, [a, b, done, help], [next, finish]) = checkout();
    // (command, parameters), each on top of the one before.
    let edits = [
        ("proto.scenario.add", place_an_order(a, b, done, next, finish)),
        ("proto.scenario.set", json!({"id": "S1", "name": "Order", "expect": {"screen": done}})),
        ("proto.endState.set", json!({"id": help, "reason": "a help page ends the visit"})),
        ("proto.endState.set", json!({"id": help, "reason": "another reason"})),
        ("proto.endState.set", json!({"id": done, "on": false})),
        ("proto.scenario.remove", json!({"id": "S1"})),
    ];
    for (cmd, params) in edits {
        let before = file(&s);
        let depth = s.undo_depth().0;
        ex(&mut s, cmd, params.clone());
        let after = file(&s);
        assert_ne!(after, before, "{cmd} {params}");
        assert_eq!(s.undo_depth().0, depth + 1, "{cmd}: one step");
        assert!(s.undo().unwrap());
        assert_eq!(file(&s), before, "{cmd}: undo restores the file exactly");
        assert!(s.redo().unwrap());
        assert_eq!(file(&s), after, "{cmd}: redo applies it again");
    }
    // An edit that changes nothing is no step.
    let depth = s.undo_depth().0;
    ex(&mut s, "proto.endState.set", json!({"id": help}));
    ex(&mut s, "proto.scenario.add", json!({"name": "Look", "start": a, "steps": []}));
    ex(&mut s, "proto.scenario.set", json!({"id": "S2", "name": "Look"}));
    assert_eq!(s.undo_depth().0, depth + 1, "only the added scenario");
}

#[test]
fn scenarios_and_end_states_are_saved_with_the_file_and_still_pass() {
    let (mut s, [a, b, done, _], [next, finish]) = checkout();
    let plain = figcraft_format::to_json(s.doc().unwrap()).unwrap();
    assert!(plain.get("design").is_some(), "the end state is design context");
    ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (back, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(back.design, s.doc().unwrap().design);
    assert_eq!((back.design.scenarios.len(), back.design.end_states.len()), (1, 1));
    let mut reopened = Session::with_document(back);
    let r = ex(&mut reopened, "proto.scenario.run", json!({}));
    assert_eq!((&r["passed"], &r["total"]), (&json!(true), &json!(1)), "{r}");
    assert_eq!(ex(&mut reopened, "proto.endState.list", json!({}))["endStates"][0]["node"], json!(done));
    assert_eq!(
        ex(&mut reopened, "proto.scenario.add", json!({"name": "Next", "start": a, "steps": []})),
        json!({"id": "S2"}),
        "the id counter is saved too"
    );
}

#[test]
fn a_file_without_design_context_has_no_design_key_and_reading_adds_none() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let go = button(&mut s, "Go", a, 20.0, navigate(b));
    let before = file(&s);
    assert!(before.get("design").is_none());
    for (cmd, params) in [
        ("proto.check", json!({})),
        ("proto.simulate", json!({"start": a, "steps": [{"click": go}]})),
        ("proto.scenario.list", json!({})),
        ("proto.scenario.run", json!({})),
        ("proto.endState.list", json!({})),
    ] {
        ex(&mut s, cmd, params);
        assert_eq!(file(&s), before, "{cmd} only reads");
    }
    assert!(figcraft_format::to_json(s.doc().unwrap()).unwrap().get("design").is_none());
}

#[test]
fn a_scenario_that_no_longer_passes_is_found_with_its_first_failing_step() {
    let (mut s, [a, b, done, _], [next, finish]) = checkout();
    ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    assert!(problems(&check(&mut s), "scenario.fails").is_empty());
    // The prototype changes under the scenario: Finish goes back to A now.
    ex(&mut s, "proto.interaction.set", json!({"id": finish, "index": 0, "action": navigate(a)}));
    let r = ex(&mut s, "proto.scenario.run", json!({"id": "S1"}));
    assert_eq!(
        (&r["passed"], &r["failed"], &r["results"][0]["passed"], &r["results"][0]["stepsTaken"]),
        (&json!(false), &json!(1), &json!(false), &json!(2))
    );
    assert!(r["results"][0].get("failedStep").is_none(), "every step ran; the result isn't what was expected");
    let failures = r["results"][0]["failures"].as_array().unwrap();
    assert_eq!(failures[0], json!(format!("expected screen `Done` ({done}), but `A` ({a}) is showing")));
    assert!(failures[1].as_str().unwrap().contains("to be a declared end state"), "{failures:?}");
    let v = check(&mut s);
    let fails = problems(&v, "scenario.fails");
    assert_eq!(fails.len(), 1, "{v}");
    assert_eq!((&fails[0]["severity"], &fails[0]["nodes"], &v["ok"]), (&json!("major"), &json!([a]), &json!(false)));
    assert!(fails[0]["message"].as_str().unwrap().contains("scenario S1 \"Place an order\" doesn't pass"));
    assert!(fails[0]["evidence"].as_str().unwrap().starts_with(&format!("after the last step: expected screen `Done` ({done})")), "{fails:?}");
    // A step that fails is named with what it does, and its layer comes first.
    ex(&mut s, "proto.interaction.clear", json!({"ids": [next]}));
    let r = ex(&mut s, "proto.scenario.run", json!({}));
    assert_eq!((&r["results"][0]["failedStep"], &r["results"][0]["stepsTaken"]), (&json!(0), &json!(1)));
    let fails = problems(&check(&mut s), "scenario.fails");
    assert_eq!(fails[0]["nodes"], json!([next, a]));
    assert!(
        fails[0]["evidence"].as_str().unwrap().starts_with(&format!("steps[0] (click on `Next` ({next})): nothing reacts to click")),
        "{fails:?}"
    );
    // Its start deleted, it can't start, and it is reported wherever the check runs.
    ex(&mut s, "edit.delete", json!({"ids": [a]}));
    let v = check(&mut s);
    let fails = problems(&v, "scenario.fails");
    assert_eq!(fails[0]["nodes"], json!([]), "{v}");
    assert!(fails[0]["evidence"].as_str().unwrap().contains(&format!("before the first step: can't start: no layer {a}")), "{fails:?}");
}

#[test]
fn a_flow_without_scenarios_and_screens_no_scenario_shows_are_noted() {
    let (mut s, [a, b, done, help], [next, finish]) = checkout();
    let v = check(&mut s);
    let none = problems(&v, "scenario.none");
    assert_eq!(none.len(), 1, "{v}");
    assert_eq!((&none[0]["severity"], &none[0]["nodes"]), (&json!("note"), &json!([a])));
    assert!(problems(&v, "scenario.unvisited").is_empty(), "nothing is stored, so nothing is compared");
    // One scenario: the screens it never shows are listed once, by name.
    ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    let v = check(&mut s);
    assert!(problems(&v, "scenario.none").is_empty());
    let unvisited = problems(&v, "scenario.unvisited");
    assert_eq!(unvisited.len(), 1, "{v}");
    assert_eq!((&unvisited[0]["severity"], &unvisited[0]["nodes"]), (&json!("note"), &json!([help])));
    assert_eq!(unvisited[0]["evidence"], json!(format!("`Help` ({help})")));
    assert_eq!(v["ok"], json!(true), "notes are facts to weigh, not failures");
    // A second scenario that only starts there covers it.
    ex(&mut s, "proto.scenario.add", json!({"name": "Read the help", "start": help, "steps": []}));
    assert!(problems(&check(&mut s), "scenario.unvisited").is_empty());
}

#[test]
fn end_states_are_top_level_frames_declared_once() {
    let (mut s, [a, _, done, help], [next, _]) = checkout();
    assert_eq!(
        ex(&mut s, "proto.endState.list", json!({})),
        json!({"endStates": [{"node": done, "name": "Done", "reason": "the order is placed", "exists": true}]})
    );
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": help})), json!({"id": help, "endState": true, "reason": ""}));
    // Declaring again keeps the reason unless one is given.
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": done}))["reason"], json!("the order is placed"));
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": done, "reason": " paid "}))["reason"], json!("paid"));
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": help, "on": false})), json!({"id": help, "endState": false, "reason": null}));
    assert_eq!(s.doc().unwrap().design.end_states.len(), 1);
    // (parameters, what the error names)
    for (params, names) in [
        (json!({"id": next}), "`id`: `Next`"),
        (json!({"id": 987_654}), "`id`: no layer 987654"),
        (json!({"id": a, "on": false}), "isn't declared as an end state"),
        (json!({"id": a, "on": "perhaps"}), "`on` must be true or false"),
        (json!({}), "`id` must be a node id"),
    ] {
        let e = err(&mut s, "proto.endState.set", params.clone());
        assert!(e.contains(names), "{params}: {e}");
    }
    // A declared frame that was deleted stays listed as missing, and can be taken back.
    ex(&mut s, "edit.delete", json!({"ids": [done]}));
    assert_eq!(ex(&mut s, "proto.endState.list", json!({}))["endStates"][0]["exists"], json!(false));
    ex(&mut s, "proto.endState.set", json!({"id": done, "on": false}));
    assert!(s.doc().unwrap().design.end_states.is_empty());
}

#[test]
fn the_file_keeps_at_most_so_many_scenarios() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    for k in 0..MAX_ENTRIES {
        ex(&mut s, "proto.scenario.add", json!({"name": format!("N{k}"), "start": a, "steps": []}));
    }
    assert!(err(&mut s, "proto.scenario.add", json!({"name": "One more", "start": a, "steps": []})).contains("500 scenarios already"));
}

#[test]
fn runs_that_set_variables_and_change_variants_leave_the_file_alone() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let (set, [_, on]) = super::flow::toggle(&mut s, b);
    let instance = idv(&ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 200, "parent": a})));
    let more = button(&mut s, "More", a, 20.0, json!({"type": "setVariable", "variable": "count", "expression": "count + 1"}));
    ex(&mut s, "proto.flow.add", json!({"start": a}));
    let steps = json!([
        {"click": more},
        {"click": instance, "expect": {"variants": [{"instance": instance, "variant": "On"}], "variables": {"count": 1}, "screen": a}},
        {"click": instance, "expect": {"screen": b}},
    ]);
    ex(&mut s, "proto.scenario.add", json!({"name": "Switch on and go", "start": a, "steps": steps, "expect": {"variables": {"State/count": 1}}}));
    let before = (file(&s), s.undo_depth());
    let r = ex(&mut s, "proto.scenario.run", json!({}));
    assert_eq!((&r["passed"], &r["results"][0]["stepsTaken"]), (&json!(true), &json!(3)), "{r}");
    let r = ex(&mut s, "proto.simulate", json!({"start": a, "steps": steps}));
    assert_eq!(r["state"]["variants"], json!([{"instance": instance, "component": on, "name": "State=On"}]), "the run's copy changed");
    assert_eq!(r["state"]["variables"], json!({"State/count": 1.0}));
    let v = check(&mut s);
    assert!(problems(&v, "scenario.fails").is_empty(), "{v}");
    assert_eq!((file(&s), s.undo_depth()), before, "the file still has the instance off and count at 0");
}
