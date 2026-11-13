//! `proto.simulate`: steps played headless, each reported, with nothing changed in the file.

use figcraft_doc::NodeId;
use figcraft_doc::design::MAX_ENTRIES;
use serde_json::{Value, json};

use super::{err, file};
use crate::Session;
use crate::proto_sim::tests::{button, ex, frame, navigate, rect, state_variables};

/// A → B → Done: "Next" on A adds one to `count` and navigates to B, "Finish" on B navigates to
/// Done. Returns the session, the frames and the two buttons.
fn checkout() -> (Session, [NodeId; 3], [NodeId; 2]) {
    let mut s = Session::new();
    let frames = [frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "Done", 800.0)];
    state_variables(&mut s);
    let next = rect(&mut s, "Next", frames[0], 20.0, 20.0);
    let actions = json!([{"type": "setVariable", "variable": "count", "expression": "count + 1"}, navigate(frames[1])]);
    ex(&mut s, "proto.interaction.add", json!({"id": next, "actions": actions}));
    let finish = button(&mut s, "Finish", frames[1], 20.0, navigate(frames[2]));
    ex(&mut s, "proto.flow.add", json!({"start": frames[0]}));
    (s, frames, [next, finish])
}

#[test]
fn steps_run_in_order_and_each_reports_what_happened() {
    let (mut s, [a, b, done], [next, finish]) = checkout();
    let before = (file(&s), s.undo_depth());
    let r = ex(
        &mut s,
        "proto.simulate",
        json!({
            "start": a,
            "steps": [
                {"type": "gesture", "node": next, "expect": {"screen": b, "variables": {"count": 1}}, "note": "the primary action"},
                {"click": finish.to_string()},
            ],
            "expect": {"screen": done, "overlays": [], "variables": {"State/count": 1}},
        }),
    );
    assert_eq!((&r["passed"], &r["stepsTaken"], &r["failures"], &r["start"]), (&json!(true), &json!(2), &json!([]), &json!(a)), "{r}");
    // What passed is a simulated run, and the result says what the simulator leaves out.
    assert_eq!(r["simulated"], json!(true));
    assert_eq!(r["notModelled"].as_array().map(Vec::len), Some(crate::proto_verify::NOT_MODELLED.len()), "{r}");
    // And where it still does something else than the app's player (review R2-08).
    let known: Vec<&str> = crate::proto_verify::KNOWN_DIFFERENCES.iter().map(|(_, says)| *says).collect();
    assert_eq!(r["knownDifferences"], json!(known), "{r}");
    assert!(r["notModelled"][0].as_str().unwrap().starts_with("animation and timing"), "{r}");
    assert!(r.get("failedStep").is_none());
    let steps = r["steps"].as_array().unwrap();
    assert_eq!(steps[0]["index"], json!(0));
    assert_eq!(steps[0]["action"], json!({"type": "gesture", "node": next, "gesture": "click"}));
    assert_eq!((&steps[0]["fired"], &steps[0]["failures"], &steps[0]["note"]), (&json!(true), &json!([]), &json!("the primary action")));
    assert_eq!(steps[0]["effects"], json!(["set `State/count` to 1", format!("navigate to `B` ({b})")]));
    assert_eq!(steps[0]["state"], json!({"screen": b, "screenName": "B", "overlays": []}));
    assert!(steps[0].get("problem").is_none() && steps[1].get("note").is_none());
    assert_eq!(steps[1]["action"], json!({"type": "gesture", "node": finish, "gesture": "click"}), "the shorthand, with the id as text");
    assert_eq!(
        r["state"],
        json!({"screen": done, "screenName": "Done", "overlays": [], "history": [a, b], "variables": {"State/count": 1.0}, "modes": {}, "variants": [], "endState": false})
    );
    assert_eq!((file(&s), s.undo_depth()), before, "the run played a copy: the file and its history are as they were");
}

#[test]
fn a_failing_step_stops_the_run_unless_asked_to_go_on() {
    let (mut s, [a, b, done], [next, finish]) = checkout();
    let steps = json!([
        {"click": next, "expect": {"screen": done}},
        {"click": finish},
    ]);
    let r = ex(&mut s, "proto.simulate", json!({"start": a, "steps": steps, "expect": {"screen": a}}));
    assert_eq!((&r["passed"], &r["failedStep"], &r["stepsTaken"]), (&json!(false), &json!(0), &json!(1)), "{r}");
    assert_eq!(r["steps"].as_array().unwrap().len(), 1, "the steps after the failure didn't run");
    let failure = r["steps"][0]["failures"][0].as_str().unwrap();
    assert_eq!(failure, format!("expected screen `Done` ({done}), but `B` ({b}) is showing"));
    assert_eq!(r["failures"], json!([]), "the final expectation isn't checked on a run that stopped short");
    assert_eq!(r["state"]["screen"], json!(b), "the state is where it stopped");
    // Going on: every step runs, and the final expectation is checked where the run ended.
    let r = ex(&mut s, "proto.simulate", json!({"start": a, "steps": steps, "expect": {"screen": a}, "stopOnFailure": false}));
    assert_eq!((&r["passed"], &r["failedStep"], &r["stepsTaken"]), (&json!(false), &json!(0), &json!(2)), "{r}");
    assert_eq!(r["steps"][1]["failures"], json!([]));
    assert_eq!(r["failures"], json!([format!("expected screen `A` ({a}), but `Done` ({done}) is showing")]));
}

#[test]
fn a_gesture_or_key_that_sets_nothing_off_fails_and_a_quiet_wait_does_not() {
    let (mut s, [a, ..], [next, _]) = checkout();
    let plain = rect(&mut s, "Plain", a, 20.0, 200.0);
    // (step, passes, what the problem says)
    for (step, passes, why) in [
        (json!({"type": "wait"}), true, ""),
        (json!({"type": "clickOutside"}), true, ""),
        (json!({"click": plain}), false, "nothing reacts to click on `Plain`"),
        (json!({"type": "key", "key": "Enter"}), false, "nothing reacts to key `Enter`"),
        (json!({"key": "Shift+K"}), false, "nothing reacts to key `Shift+K`"),
        (json!({"type": "gesture", "node": next, "gesture": "hover"}), false, "nothing reacts to hover on `Next`"),
        (json!({"click": 987_654}), false, "layer 987654 doesn't exist"),
        (json!({"type": "setVariable", "variable": "count", "value": "many"}), false, "isn't a number value"),
        (json!({"type": "setVariable", "variable": "State/label", "value": "typed"}), true, ""),
    ] {
        let r = ex(&mut s, "proto.simulate", json!({"start": a, "steps": [step]}));
        assert_eq!(r["passed"], json!(passes), "{step}: {r}");
        let problem = r["steps"][0]["problem"].as_str().unwrap_or_default();
        assert!(problem.contains(why), "{step}: {r}");
        assert_eq!(problem.is_empty(), passes, "{step}: {r}");
    }
}

#[test]
fn without_a_start_it_begins_where_presenting_would() {
    let (mut s, [a, b, _], _) = checkout();
    let start = |s: &mut Session| ex(s, "proto.simulate", json!({"steps": []}))["start"].clone();
    ex(&mut s, "select.set", json!({"ids": [b]}));
    assert_eq!(start(&mut s), json!(b), "the selected frame");
    ex(&mut s, "select.none", json!({}));
    assert_eq!(start(&mut s), json!(a), "else the first flow");
    ex(&mut s, "proto.flow.remove", json!({"start": a}));
    ex(&mut s, "proto.flow.add", json!({"start": b}));
    assert_eq!(start(&mut s), json!(b));
    ex(&mut s, "proto.flow.remove", json!({"start": b}));
    assert_eq!(start(&mut s), json!(a), "else the page's first frame");
    // No steps: the start state, which passes.
    let r = ex(&mut s, "proto.simulate", json!({"steps": [], "expect": {"screen": a}}));
    assert_eq!((&r["passed"], &r["stepsTaken"], &r["state"]["screen"]), (&json!(true), &json!(0), &json!(a)));
    let mut empty = Session::new();
    assert!(err(&mut empty, "proto.simulate", json!({"steps": []})).contains("no frame to start at"));
}

#[test]
fn mistakes_in_the_parameters_are_named_where_they_are() {
    let (mut s, [a, ..], [next, _]) = checkout();
    let long = "x".repeat(figcraft_doc::design::MAX_TEXT + 1);
    let many: Vec<Value> = (0..=MAX_ENTRIES).map(|_| json!({"type": "wait"})).collect();
    let too_many = format!("`steps`: {} entries, at most {MAX_ENTRIES}", MAX_ENTRIES + 1);
    // (parameters, what the error names)
    let cases = [
        (json!({}), "`steps` is required"),
        (json!({"steps": {"click": next}}), "`steps` must be a list"),
        (json!({"steps": many}), too_many.as_str()),
        (json!({"steps": [{"click": next}, 7]}), "`steps[1]` must be an object"),
        (json!({"steps": [{"click": next}, {}]}), "`steps[1]` needs a `type`"),
        (json!({"steps": [{"type": "tap", "node": next}]}), "`steps[0].type`: \"tap\" isn't a step type"),
        (json!({"steps": [{"type": "gesture"}]}), "`steps[0].node` is required"),
        (json!({"steps": [{"type": "gesture", "node": "the button"}]}), "`steps[0].node` must be a layer id (got \"the button\")"),
        (json!({"steps": [{"type": "gesture", "node": next, "gesture": "swipe"}]}), "`steps[0].gesture`: \"swipe\" isn't a gesture"),
        (json!({"steps": [{"click": next, "expects": {}}]}), "`steps[0]`: unknown key `expects`"),
        (json!({"steps": [{"click": next, "type": "wait"}]}), "`steps[0]` says what to do more than once"),
        (json!({"steps": [{"type": "key"}]}), "`steps[0].key` is required"),
        (json!({"steps": [{"type": "key", "key": long}]}), "`steps[0].key`: longer than 4000 characters"),
        (json!({"steps": [{"type": "setVariable", "variable": "count"}]}), "`steps[0].value` is required"),
        (json!({"steps": [{"click": next, "expect": {"screens": a}}]}), "`steps[0].expect`: unknown key `screens`"),
        (json!({"steps": [{"click": next, "expect": {"visible": a}}]}), "`steps[0].expect.visible` must be a list"),
        (json!({"steps": [{"click": next, "expect": {"visible": [a, "x"]}}]}), "`steps[0].expect.visible[1]` must be a layer id"),
        (json!({"steps": [], "expect": {"text": [{"node": a}]}}), "`expect.text[0]` needs `equals` or `contains`"),
        (json!({"steps": [], "expect": {"variants": [{"instance": a}]}}), "`expect.variants[0].variant` is required"),
        (json!({"steps": [], "expect": {"endState": "perhaps"}}), "`expect.endState` must be true or false"),
        (json!({"steps": [], "expect": {"variables": [1]}}), "`expect.variables` must be an object"),
        (json!({"steps": [], "stopOnFailure": "perhaps"}), "`stopOnFailure` must be true or false"),
        (json!({"steps": [], "start": next}), "`start`: `Next`"),
        (json!({"steps": [], "start": 987_654}), "`start`: no layer 987654"),
        (json!({"steps": [], "start": "the first one"}), "`start` must be a node id"),
        (json!({"steps": [], "speed": 2}), "unknown parameter `speed`"),
    ];
    for (params, names) in cases {
        let e = err(&mut s, "proto.simulate", params.clone());
        assert!(e.contains(names), "{}: {e}", figcraft_format::clip(&params.to_string(), 120));
    }
}

#[test]
fn the_stored_form_of_a_step_is_taken_too() {
    let (mut s, [a, b, _], [next, _]) = checkout();
    let stored =
        json!({"action": {"type": "gesture", "node": next, "gesture": "click"}, "expect": {"screen": b}, "note": "as proto.scenario.list gives it"});
    let r = ex(&mut s, "proto.simulate", json!({"start": a, "steps": [stored]}));
    assert_eq!((&r["passed"], &r["steps"][0]["state"]["screen"]), (&json!(true), &json!(b)), "{r}");
}
