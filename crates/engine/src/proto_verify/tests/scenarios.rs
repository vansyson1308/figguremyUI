//! Stored scenarios and end states: kept in the file, undoable, run headless, and reported on by
//! the scenario rules.

use figcraft_doc::NodeId;
use figcraft_doc::design::MAX_ENTRIES;
use serde_json::{Value, json};

use super::{check, err, file, problems, screen};
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
    let none = ex(&mut s, "proto.scenario.run", json!({}));
    assert_eq!(
        (&none["passed"], &none["total"], &none["failed"], &none["results"]),
        (&json!(true), &json!(0), &json!(0), &json!([])),
        "none stored: none failed"
    );
    // A run is the simulator's, and says what the simulator leaves out.
    assert_eq!(none["simulated"], json!(true));
    let left_out = none["notModelled"].as_array().unwrap();
    for part in ["animation and timing", "scroll position", "drag direction", "device frame and responsive resizing", "pointer hit-testing"] {
        assert!(left_out.iter().any(|x| x.as_str().unwrap().contains(part)), "{part}: {left_out:?}");
    }
    let r = ex(&mut s, "proto.scenario.add", place_an_order(a, b, done, next, finish));
    assert_eq!(r, json!({"id": "S1"}));
    let listed = ex(&mut s, "proto.scenario.list", json!({}));
    assert_eq!(
        listed["scenarios"],
        json!([{
            "id": "S1",
            "name": "Place an order",
            "purpose": "the primary task can be finished from the start screen",
            "author": "user",
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
    assert_eq!(now, json!({"id": "S1", "name": "Order", "author": "user", "start": help, "steps": []}));
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
        json!({"endStates": [{"node": done, "name": "Done", "reason": "the order is placed", "author": "user", "status": "accepted", "exists": true}]})
    );
    assert_eq!(
        ex(&mut s, "proto.endState.set", json!({"id": help})),
        json!({"id": help, "endState": true, "status": "accepted", "author": "user", "reason": ""})
    );
    // Declaring again keeps the reason unless one is given.
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": done}))["reason"], json!("the order is placed"));
    assert_eq!(ex(&mut s, "proto.endState.set", json!({"id": done, "reason": " paid "}))["reason"], json!("paid"));
    assert_eq!(
        ex(&mut s, "proto.endState.set", json!({"id": help, "on": false})),
        json!({"id": help, "endState": false, "status": null, "reason": null})
    );
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

/// The AI agent the agent-run commands of these tests are for, and its name in records.
const AGENT: &str = "Claude";
const AGENT_NAME: &str = "Claude (agent)";

fn as_agent(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute_as(Some(AGENT), cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p} (as an agent): {e}"))
}

/// The error of a command an agent runs that must be refused, alone and as the only step of a
/// batch the agent runs; the file is left as it was either way.
fn agent_refused(s: &mut Session, cmd: &str, p: Value) -> String {
    let before = (file(s), s.undo_depth());
    let batch = json!({"commands": [{"command": cmd, "params": p.clone()}]});
    let in_batch = s.execute_as(Some(AGENT), "command.batch", &batch);
    assert!(in_batch.is_err(), "{cmd} {p} should be refused to an agent inside a batch too, gave {in_batch:?}");
    let e = match s.execute_as(Some(AGENT), cmd, &p) {
        Ok(v) => panic!("{cmd} {p} should be refused to an agent, gave {v}"),
        Err(e) => e.to_string(),
    };
    assert_eq!((file(s), s.undo_depth()), before, "{cmd} was refused but changed the file");
    e
}

/// A flow A → B ("Go" on A) on which B has no way out. Returns the session and (a, b, go).
fn a_dead_end() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let go = button(&mut s, "Go", a, 20.0, navigate(b));
    ex(&mut s, "proto.flow.add", json!({"start": a}));
    (s, a, b, go)
}

#[test]
fn an_end_state_an_agent_declares_is_a_proposal_that_exempts_nothing_until_a_person_accepts_it() {
    let (mut s, a, b, go) = a_dead_end();
    ex(&mut s, "design.check.record", json!({"rules": ["flow.deadEnd"]}));
    // Without a reason an agent declares nothing.
    for params in [json!({"id": b}), json!({"id": b, "reason": "  "})] {
        let e = agent_refused(&mut s, "proto.endState.set", params);
        assert!(e.contains("`reason` is required when an AI agent declares an end state") && e.contains("proto.endState.accept"), "{e}");
    }
    // With one, the frame is proposed as an end state: recorded with its author, and still a
    // dead end for the check, the recheck and a run.
    let before = file(&s);
    let r = as_agent(&mut s, "proto.endState.set", json!({"id": b, "reason": "The order is placed here"}));
    assert_eq!(
        (&r["endState"], &r["status"], &r["author"], &r["reason"]),
        (&json!(false), &json!("proposed"), &json!(AGENT_NAME), &json!("The order is placed here"))
    );
    assert!(r["note"].as_str().unwrap().contains("until a person accepts it with proto.endState.accept"), "{r}");
    assert_eq!(
        serde_json::to_value(&s.doc().unwrap().design.end_states).unwrap(),
        json!([{"node": b, "reason": "The order is placed here", "author": AGENT_NAME, "status": "proposed"}])
    );
    // One undo step that restores the file exactly.
    let after = file(&s);
    assert!(s.undo().unwrap());
    assert_eq!(file(&s), before);
    assert!(s.redo().unwrap());
    assert_eq!(file(&s), after);
    let v = check(&mut s);
    let dead = problems(&v, "flow.deadEnd");
    assert_eq!(dead.len(), 1, "{v}");
    assert!(
        dead[0]["evidence"]
            .as_str()
            .unwrap()
            .ends_with("; declared as an end state by Claude (agent), waiting for a person to accept (proto.endState.accept)"),
        "{dead:?}"
    );
    assert!(dead[0]["suggestion"].as_str().unwrap().contains("proto.endState.accept"));
    let sb = screen(&v, b);
    assert_eq!(
        (&sb["endState"], &sb["deadEnd"], &sb["endStateProposedBy"], &v["ok"]),
        (&json!(false), &json!(true), &json!(AGENT_NAME), &json!(false))
    );
    let recheck = ex(&mut s, "design.finding.recheck", json!({"id": "F1"}));
    assert_eq!((recheck["results"][0]["passed"].as_bool(), recheck["results"][0]["status"].as_str()), (Some(false), Some("open")), "{recheck}");
    let run = ex(&mut s, "proto.simulate", json!({"start": a, "steps": [{"click": go}], "expect": {"endState": true}}));
    assert_eq!((&run["passed"], &run["state"]["endState"]), (&json!(false), &json!(false)), "a proposal isn't an end state yet: {run}");
    assert_eq!(
        ex(&mut s, "proto.endState.list", json!({}))["endStates"],
        json!([{"node": b, "name": "B", "reason": "The order is placed here", "author": AGENT_NAME, "status": "proposed", "exists": true}])
    );
}

#[test]
fn a_person_accepts_an_agents_end_state_and_from_then_on_it_is_theirs() {
    let (mut s, a, b, go) = a_dead_end();
    ex(&mut s, "design.check.record", json!({"rules": ["flow.deadEnd"]}));
    as_agent(&mut s, "proto.endState.set", json!({"id": b, "reason": "The order is placed here"}));
    // The agent can't accept it; it may reword its own proposal, or take it back.
    let e = agent_refused(&mut s, "proto.endState.accept", json!({"id": b}));
    assert!(e.contains("a person accepts an end state") && e.contains("Claude (agent) can't"), "{e}");
    as_agent(&mut s, "proto.endState.set", json!({"id": b, "reason": "The order is placed"}));
    assert_eq!(s.doc().unwrap().design.end_states[0].reason, "The order is placed");
    // A person accepts: one undo step, and the dead end is settled by the rule.
    let before = file(&s);
    let r = ex(&mut s, "proto.endState.accept", json!({"id": b}));
    assert_eq!((&r["endState"], &r["status"], &r["author"]), (&json!(true), &json!("accepted"), &json!(AGENT_NAME)));
    let after = file(&s);
    assert!(s.undo().unwrap());
    assert_eq!(file(&s), before);
    assert!(s.redo().unwrap());
    assert_eq!(file(&s), after);
    let v = check(&mut s);
    assert!(problems(&v, "flow.deadEnd").is_empty() && v["ok"] == json!(true), "{v}");
    assert_eq!((&screen(&v, b)["endState"], screen(&v, b).get("endStateProposedBy")), (&json!(true), None));
    assert_eq!(ex(&mut s, "design.finding.recheck", json!({"id": "F1"}))["results"][0]["status"], "verified");
    let run = ex(&mut s, "proto.simulate", json!({"start": a, "steps": [{"click": go}], "expect": {"endState": true}}));
    assert_eq!(run["passed"], json!(true), "{run}");
    // Accepted, it is the person's: the agent neither rewords it nor takes it back.
    for params in [json!({"id": b, "reason": "Anything"}), json!({"id": b, "on": false})] {
        let e = agent_refused(&mut s, "proto.endState.set", params);
        assert!(e.contains("is an end state declared by Claude (agent) (accepted)") && e.contains("only its own proposals"), "{e}");
    }
    // Accepting again changes nothing; what isn't declared can't be accepted.
    let depth = s.undo_depth();
    ex(&mut s, "proto.endState.accept", json!({"id": b}));
    assert_eq!(s.undo_depth(), depth);
    assert!(err(&mut s, "proto.endState.accept", json!({"id": go})).contains("isn't declared as an end state"));
    assert!(err(&mut s, "proto.endState.accept", json!({})).contains("`id`"));
    // An agent takes back a proposal of its own that nobody accepted.
    as_agent(&mut s, "proto.endState.set", json!({"id": a, "reason": "A visitor may stop here"}));
    let r = as_agent(&mut s, "proto.endState.set", json!({"id": a, "on": false}));
    assert_eq!((&r["endState"], &r["status"]), (&json!(false), &json!(null)));
    assert_eq!(s.doc().unwrap().design.end_states.len(), 1);
}

#[test]
fn an_agent_leaves_a_persons_end_states_alone_and_a_person_settles_an_agents() {
    let (mut s, a, b, _) = a_dead_end();
    ex(&mut s, "proto.endState.set", json!({"id": b, "reason": "Paid"}));
    for params in [json!({"id": b, "reason": "Reworded"}), json!({"id": b, "on": false})] {
        let e = agent_refused(&mut s, "proto.endState.set", params);
        assert!(e.contains("is an end state declared by user (accepted)") && e.contains("Ask them to change it"), "{e}");
    }
    assert_eq!(s.doc().unwrap().design.end_states[0].reason, "Paid");
    // An end state from a file written before authors were recorded is the user's too.
    std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().design).end_states[0].author.clear();
    let e = agent_refused(&mut s, "proto.endState.set", json!({"id": b, "on": false}));
    assert!(e.contains("declared by user (accepted)"), "{e}");
    // The user declaring a frame an agent proposed stands behind it; the user may also drop it.
    as_agent(&mut s, "proto.endState.set", json!({"id": a, "reason": "A visitor may stop at the start"}));
    let r = ex(&mut s, "proto.endState.set", json!({"id": a}));
    assert_eq!((&r["status"], &r["author"], &r["endState"]), (&json!("accepted"), &json!(AGENT_NAME), &json!(true)));
    ex(&mut s, "proto.endState.set", json!({"id": a, "on": false}));
    // Saved and reopened, authors and statuses are as they were.
    as_agent(&mut s, "proto.endState.set", json!({"id": a, "reason": "Proposed again"}));
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (back, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(back.design.end_states, s.doc().unwrap().design.end_states);
    assert_eq!(back.design.end_states.len(), 2);
}

#[test]
fn an_agent_changes_and_removes_only_the_scenarios_it_wrote() {
    let (mut s, a, b, go) = a_dead_end();
    // The user's scenario expects the wrong screen: it fails, and the failure is recorded.
    ex(&mut s, "proto.scenario.add", json!({"name": "Go on", "start": a, "steps": [{"click": go, "expect": {"screen": a}}]}));
    assert_eq!(s.doc().unwrap().design.scenarios[0].author, "user");
    ex(&mut s, "design.check.record", json!({"rules": ["scenario.fails"]}));
    // An agent can't make the failure go away by rewriting or removing the person's scenario.
    for (cmd, params) in [
        ("proto.scenario.set", json!({"id": "S1", "steps": [{"click": go, "expect": {"screen": b}}]})),
        ("proto.scenario.set", json!({"id": "S1", "name": "Renamed"})),
        ("proto.scenario.remove", json!({"id": "S1"})),
    ] {
        let e = agent_refused(&mut s, cmd, params);
        assert!(e.contains("`id`: S1 was written by user") && e.contains("only its own scenarios") && e.contains("proto.scenario.add"), "{cmd}: {e}");
    }
    let recheck = ex(&mut s, "design.finding.recheck", json!({"id": "F1"}));
    assert_eq!((recheck["results"][0]["passed"].as_bool(), recheck["results"][0]["status"].as_str()), (Some(false), Some("open")), "{recheck}");
    // It adds its own, recorded as its own, and edits and removes that one.
    let before = file(&s);
    assert_eq!(as_agent(&mut s, "proto.scenario.add", json!({"name": "Mine", "start": a, "steps": [{"click": go}]})), json!({"id": "S2"}));
    assert_eq!(s.doc().unwrap().design.scenarios[1].author, AGENT_NAME);
    let after = file(&s);
    assert!(s.undo().unwrap());
    assert_eq!(file(&s), before);
    assert!(s.redo().unwrap());
    assert_eq!(file(&s), after);
    assert_eq!(ex(&mut s, "proto.scenario.list", json!({}))["scenarios"][1]["author"], AGENT_NAME);
    as_agent(&mut s, "proto.scenario.set", json!({"id": "S2", "name": "Mine, renamed"}));
    // Another agent's scenario isn't its own; a scenario without an author (an older file) is the user's.
    s.execute_as(Some("Other"), "proto.scenario.add", &json!({"name": "Theirs", "start": a, "steps": []})).unwrap();
    assert!(agent_refused(&mut s, "proto.scenario.remove", json!({"id": "S3"})).contains("S3 was written by Other (agent)"));
    std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().design).scenarios[0].author.clear();
    assert!(agent_refused(&mut s, "proto.scenario.remove", json!({"id": "S1"})).contains("S1 was written by user"));
    // Saved and reopened, the authors are kept; the user edits and removes any of them, and an
    // edit doesn't change whose scenario it is.
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (back, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(back.design.scenarios, s.doc().unwrap().design.scenarios);
    ex(&mut s, "proto.scenario.set", json!({"id": "S2", "name": "Reviewed"}));
    assert_eq!(s.doc().unwrap().design.scenarios[1].author, AGENT_NAME);
    as_agent(&mut s, "proto.scenario.remove", json!({"id": "S2"}));
    ex(&mut s, "proto.scenario.remove", json!({"id": "S3"}));
    ex(&mut s, "proto.scenario.remove", json!({"id": "S1"}));
    assert!(s.doc().unwrap().design.scenarios.is_empty());
}

#[test]
fn a_file_written_before_authors_and_subjects_opens_and_works_as_before() {
    let (mut s, a, b, go) = a_dead_end();
    // The document as the version before wrote it: an end state with a node and a reason, a
    // scenario without an author, a finding without a subject.
    let mut old = figcraft_format::to_json(s.doc().unwrap()).unwrap();
    assert!(old.get("design").is_none());
    old["design"] = json!({
        "scenarios": [{"id": "S1", "name": "Go on", "start": a, "steps": [{"action": {"type": "gesture", "node": go, "gesture": "click"}, "expect": {"screen": b}}], "expect": {"endState": true}}],
        "endStates": [{"node": b, "reason": "The order is placed"}],
        "findings": [{"id": "F1", "kind": "check", "rule": "scenario.none", "severity": "note", "nodes": [a], "message": "the page has a flow but no stored scenario, so nothing checks that the flow works", "source": "check", "status": "open"}],
        "counters": {"F": 1, "S": 1},
    });
    let (doc, issues) = figcraft_format::from_json(&old.to_string()).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(figcraft_format::to_json(&doc).unwrap(), old, "and is written back as it was");
    s = Session::with_document(doc);
    // The end state counts (accepted, the user's), the scenario passes and is the user's.
    let v = check(&mut s);
    assert!(problems(&v, "flow.deadEnd").is_empty() && screen(&v, b)["endState"] == json!(true), "{v}");
    assert_eq!(ex(&mut s, "proto.scenario.run", json!({}))["passed"], json!(true));
    assert_eq!(ex(&mut s, "proto.endState.list", json!({}))["endStates"][0]["status"], "accepted");
    assert!(agent_refused(&mut s, "proto.scenario.remove", json!({"id": "S1"})).contains("written by user"));
    assert!(agent_refused(&mut s, "proto.endState.set", json!({"id": b, "on": false})).contains("declared by user"));
    // The old finding is settled by the rule as any other.
    assert_eq!(ex(&mut s, "design.finding.recheck", json!({"id": "F1"}))["results"][0]["status"], "verified");
}
