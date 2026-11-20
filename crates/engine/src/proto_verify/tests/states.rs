//! Declared states (`proto.state.*`): stored, listed with what is designed and reachable, owned
//! by who declared them, and compared with the file by the `states.*` rules.

use figcraft_doc::NodeId;
use figcraft_doc::design::{ProposalStatus, Severity, StateKind};
use serde_json::{Value, json};

use super::{check, err, file, found};
use crate::Session;
use crate::proto_sim::tests::{button, ex, frame, idv, navigate, rect};

const AGENT: &str = "Claude";
const AGENT_NAME: &str = "Claude (agent)";

/// A flow Home → List, where "Fail" on Home opens the overlay "Oops" and List has a hidden
/// "Empty note" and an instance "Spinner" of a set `State=Idle` / `State=Busy`; a frame "Later"
/// nothing leads to. Returns the session and [home, list, oops, later, empty note, spinner].
fn screens() -> (Session, [NodeId; 6]) {
    let mut s = Session::new();
    let [home, list, oops, later] =
        [frame(&mut s, "Home", 0.0), frame(&mut s, "List", 400.0), frame(&mut s, "Oops", 800.0), frame(&mut s, "Later", 1200.0)];
    button(&mut s, "Open", home, 20.0, navigate(list));
    button(&mut s, "Fail", home, 80.0, json!({"type": "openOverlay", "dest": oops}));
    button(&mut s, "Close", oops, 20.0, json!({"type": "closeOverlay"}));
    button(&mut s, "Back", list, 20.0, json!({"type": "back"}));
    let note = rect(&mut s, "Empty note", list, 20.0, 100.0);
    ex(&mut s, "node.set", json!({"id": note, "visible": false}));
    let mut variants = Vec::new();
    for (k, state) in ["Idle", "Busy"].iter().enumerate() {
        let f =
            idv(&ex(&mut s, "frame.create", json!({"x": k as f64 * 150.0, "y": 900, "width": 60, "height": 40, "name": format!("State={state}")})));
        variants.push(idv(&ex(&mut s, "component.create", json!({"ids": [f]}))));
    }
    let set = idv(&ex(&mut s, "component.combineVariants", json!({"ids": variants})));
    let spinner = idv(&ex(&mut s, "instance.create", json!({"component": set, "x": 20, "y": 200, "parent": list})));
    ex(&mut s, "node.set", json!({"id": spinner, "name": "Spinner"}));
    ex(&mut s, "proto.flow.add", json!({"start": home}));
    (s, [home, list, oops, later, note, spinner])
}

fn as_agent(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute_as(Some(AGENT), cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p} (as an agent): {e}"))
}

fn agent_err(s: &mut Session, cmd: &str, p: Value) -> String {
    let before = (file(s), s.undo_depth());
    let e = match s.execute_as(Some(AGENT), cmd, &p) {
        Ok(v) => panic!("{cmd} {p} should be refused to an agent, gave {v}"),
        Err(e) => e.to_string(),
    };
    assert_eq!((file(s), s.undo_depth()), before, "{cmd} was refused but changed the file");
    e
}

/// The entry of `proto.state.list` for (frame, kind).
fn listed(s: &mut Session, frame: NodeId, kind: &str) -> Value {
    let all = ex(s, "proto.state.list", json!({}));
    all["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["frame"] == json!(frame) && v["kind"] == kind)
        .cloned()
        .unwrap_or_else(|| panic!("no {kind} of {frame} in {all}"))
}

#[test]
fn a_state_is_declared_changed_and_removed_one_undo_step_each() {
    let (mut s, [home, list, _, _, note, _]) = screens();
    assert_eq!(
        ex(&mut s, "proto.state.list", json!({})),
        json!({"states": [], "summary": {"total": 0, "designed": 0, "reachable": 0, "proposed": 0}})
    );
    let before = (file(&s), s.undo_depth());
    let r =
        ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "Empty", "shownBy": {"node": note}, "reason": " a new member has no items "}));
    assert_eq!(
        r["state"],
        json!({"frame": list, "frameName": "List", "kind": "empty", "author": "user", "status": "accepted", "exists": true, "shownBy": {"node": note}, "shownByName": "Empty note", "reason": "a new member has no items"})
    );
    assert_eq!(s.undo_depth().0, before.1.0 + 1, "one undo step");
    // The same again is no edit; a new reason is, and keeps what wasn't given.
    let depth = s.undo_depth();
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "reason": "a new member has no items"}));
    assert_eq!(s.undo_depth(), depth, "declaring what is declared is no undo step");
    let r = ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "reason": "nothing saved yet"}));
    assert_eq!((&r["state"]["reason"], &r["state"]["shownBy"]), (&json!("nothing saved yet"), &json!({"node": note})));
    // `null` takes what shows it away; a name makes another declaration of the same kind.
    let r = ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": null}));
    assert!(r["state"].get("shownBy").is_none(), "{r}");
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "other", "name": "Offline"}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "name": "No network"}));
    let all = ex(&mut s, "proto.state.list", json!({}));
    assert_eq!(all["summary"]["total"], 3);
    assert_eq!(ex(&mut s, "proto.state.list", json!({"frame": home}))["states"].as_array().unwrap().len(), 1, "one screen's");
    // The name compares without case.
    assert_eq!(
        ex(&mut s, "proto.state.remove", json!({"frame": list, "kind": "other", "name": " offline "})),
        json!({"removed": format!("state other \"Offline\" of {list}")})
    );
    assert!(err(&mut s, "proto.state.remove", json!({"frame": list, "kind": "other", "name": "Offline"})).contains("no state other"));
    // Undo takes each step back; the file without declarations is as it was.
    while s.undo_depth().0 > before.1.0 {
        assert!(s.undo().unwrap());
    }
    assert_eq!(file(&s), before.0);
    assert!(file(&s)["design"].get("states").is_none(), "nothing declared: no `states` in the file");
}

#[test]
fn what_a_declaration_names_must_be_there_and_the_error_says_what() {
    let (mut s, [home, list, oops, _, note, spinner]) = screens();
    let before = (file(&s), s.undo_depth());
    let cases = [
        (json!({"frame": list}), "`kind` is required"),
        (json!({"frame": list, "kind": "broken"}), "`kind` must be one of firstUse|empty|loading|error|returning|success|other"),
        (json!({"frame": list, "kind": "other"}), "`name` is required for kind `other`"),
        (json!({"frame": note, "kind": "empty"}), "isn't a top-level frame"),
        (json!({"frame": 4711, "kind": "empty"}), "`frame`: no layer 4711"),
        (json!({"frame": home, "kind": "empty", "shownBy": {"node": note}}), "is neither a top-level frame nor a layer in"),
        (json!({"frame": list, "kind": "empty", "shownBy": {"node": 4712}}), "no longer exists"),
        (
            json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner, "variant": "State=Spinning"}}),
            "has no variant `State=Spinning` (its variants: `State=Idle`, `State=Busy`)",
        ),
        (json!({"frame": list, "kind": "loading", "shownBy": {"instance": note, "variant": "Busy"}}), "isn't an instance"),
        (json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner}}), "`shownBy.variant` is required"),
        (json!({"frame": list, "kind": "loading", "shownBy": {"node": note, "instance": spinner, "variant": "Busy"}}), "not both"),
        (json!({"frame": list, "kind": "loading", "shownBy": {"layer": note}}), "unknown key `layer`"),
        (json!({"frame": list, "kind": "loading", "shownBy": "Busy"}), "`shownBy` must be"),
        (json!({"frame": list, "kind": "loading", "colour": 1}), "`colour`"),
    ];
    for (params, says) in cases {
        let e = err(&mut s, "proto.state.set", params.clone());
        assert!(e.contains(says), "{params}: {e}");
    }
    assert_eq!((file(&s), s.undo_depth()), before, "a refused declaration leaves the file as it was");
    // Another frame (an overlay) shows a state of the screen; a variant by a property value.
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": oops}}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner, "variant": "Busy"}}));
    assert_eq!(listed(&mut s, list, "loading")["designed"], true);
}

#[test]
fn an_agents_declaration_is_a_proposal_it_alone_changes_until_a_person_accepts_it() {
    let (mut s, [home, list, _, _, note, _]) = screens();
    let r = as_agent(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    assert_eq!((&r["state"]["author"], &r["state"]["status"]), (&json!(AGENT_NAME), &json!("proposed")));
    as_agent(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "reason": "mine to change"}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "firstUse"}));
    // A person's is theirs.
    for cmd in ["proto.state.set", "proto.state.remove"] {
        let e = agent_err(&mut s, cmd, json!({"frame": home, "kind": "firstUse"}));
        assert!(e.contains("declared by user") && e.contains("only its own proposals"), "{e}");
    }
    let e = agent_err(&mut s, "proto.state.accept", json!({"frame": list, "kind": "empty"}));
    assert!(e.contains("a person accepts a declared state"), "{e}");
    // A person accepts it: then it is no longer the agent's to change or take back.
    let r = ex(&mut s, "proto.state.accept", json!({"frame": list, "kind": "empty"}));
    assert_eq!((&r["state"]["status"], &r["state"]["acceptedBy"], &r["state"]["author"]), (&json!("accepted"), &json!("user"), &json!(AGENT_NAME)));
    let depth = s.undo_depth();
    ex(&mut s, "proto.state.accept", json!({"frame": list, "kind": "empty"}));
    assert_eq!(s.undo_depth(), depth, "accepting what is accepted is no edit");
    let e = agent_err(&mut s, "proto.state.remove", json!({"frame": list, "kind": "empty"}));
    assert!(e.contains("stands accepted"), "{e}");
    let st = &s.doc().unwrap().design.states;
    assert_eq!((st[0].status, st[0].accepted_by.as_str()), (ProposalStatus::Accepted, "user"));
    // The agent takes back its own proposal.
    as_agent(&mut s, "proto.state.set", json!({"frame": list, "kind": "error"}));
    as_agent(&mut s, "proto.state.remove", json!({"frame": list, "kind": "error"}));
    assert_eq!(s.doc().unwrap().design.states.len(), 2);
}

#[test]
fn designed_reachable_and_covered_say_what_the_file_the_flow_and_the_scenarios_show() {
    let (mut s, [home, list, oops, later, note, spinner]) = screens();
    let open = s.doc().unwrap().node(home).unwrap().children[0];
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": oops}}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner, "variant": "Idle"}}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "returning", "shownBy": {"node": later}}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "success"}));
    // The overlay is reached through the flow. Of the hidden note only its screen is: the graph
    // doesn't follow what would show the note, so it isn't counted as reachable. Nothing leads
    // to "Later".
    let error = listed(&mut s, home, "error");
    assert_eq!(
        (&error["designed"], &error["reachable"], &error["reachedBy"], &error["coveredBy"]),
        (&json!(true), &json!(true), &json!(["flow"]), &json!([]))
    );
    let empty = listed(&mut s, list, "empty");
    assert_eq!((&empty["reachable"], &empty["screenReachable"], &empty["reachedBy"]), (&json!(false), &json!(true), &json!(["screen"])), "{empty}");
    let later_state = listed(&mut s, home, "returning");
    assert_eq!((&later_state["designed"], &later_state["reachable"], &later_state["reachedBy"]), (&json!(true), &json!(false), &json!([])));
    let success = listed(&mut s, list, "success");
    assert_eq!((&success["designed"], &success["notDesigned"]), (&json!(false), &json!("nothing is said to show it (no `shownBy`)")));
    // A scenario that opens List shows the instance in its variant, not the hidden note, and one
    // that opens the overlay shows the error.
    ex(&mut s, "proto.scenario.add", json!({"name": "Look", "start": home, "steps": [{"click": open, "expect": {"screen": list}}]}));
    let fail = s.doc().unwrap().node(home).unwrap().children[1];
    ex(&mut s, "proto.scenario.add", json!({"name": "Fail", "start": home, "steps": [{"click": fail}]}));
    assert_eq!(listed(&mut s, list, "loading")["coveredBy"], json!(["S1"]));
    assert_eq!(listed(&mut s, list, "empty")["coveredBy"], json!([]), "hidden on the way: not shown");
    let error = listed(&mut s, home, "error");
    assert_eq!((&error["coveredBy"], &error["reachedBy"]), (&json!(["S2"]), &json!(["flow", "scenario"])));
    let loading = listed(&mut s, list, "loading");
    assert_eq!((&loading["reachable"], &loading["reachedBy"]), (&json!(true), &json!(["screen", "scenario"])));
    // A step that fails shows nothing, though the simulator shows the overlay after it: the walk
    // isn't the one the scenario meant (the step opens Oops but expects List).
    ex(&mut s, "proto.scenario.set", json!({"id": "S2", "steps": [{"click": fail, "expect": {"screen": list}}]}));
    assert_eq!(listed(&mut s, home, "error")["coveredBy"], json!([]));
    // And neither does what follows a failing step.
    ex(&mut s, "proto.scenario.set", json!({"id": "S2", "steps": [{"click": open, "expect": {"screen": oops}}, {"click": fail}]}));
    assert_eq!(listed(&mut s, home, "error")["coveredBy"], json!([]));
    // A scenario that shows "Later" makes it reachable though no interaction leads there.
    ex(&mut s, "proto.scenario.add", json!({"name": "Back later", "start": later, "steps": []}));
    let later_state = listed(&mut s, home, "returning");
    assert_eq!(
        (&later_state["reachable"], &later_state["reachedBy"], &later_state["coveredBy"]),
        (&json!(true), &json!(["scenario"]), &json!(["S3"]))
    );
    // proto.check gives the page's declarations with the same facts.
    let v = check(&mut s);
    assert_eq!(v["states"].as_array().unwrap().len(), 5, "{v}");
    assert!(v["states"].as_array().unwrap().iter().any(|st| st["kind"] == "loading" && st["coveredBy"] == json!(["S1"])));
    // Deleting what showed a state makes it not designed, and says why.
    ex(&mut s, "edit.delete", json!({"ids": [later]}));
    assert!(listed(&mut s, home, "returning")["notDesigned"].as_str().unwrap().contains("no longer exists"));
}

#[test]
fn states_none_is_one_note_that_says_nothing_was_declared() {
    let (mut s, [home, list, ..]) = screens();
    let f = found(s.doc().unwrap(), "states.none", &[]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!((f[0].severity, f[0].subject.as_str()), (Severity::Note, "states"));
    assert!(f[0].message.starts_with("No states are declared") && f[0].message.contains("not a finding about the design"), "{}", f[0].message);
    // Near misses: one declaration, or no screens at all.
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty"}));
    assert!(found(s.doc().unwrap(), "states.none", &[]).is_empty());
    let empty = Session::new();
    assert!(found(empty.doc().unwrap(), "states.none", &[]).is_empty(), "no frames: nothing to declare");
    let _ = home;
}

#[test]
fn not_designed_is_a_gap_when_nothing_shows_it_and_major_when_its_link_broke() {
    let (mut s, [home, list, oops, _, note, spinner]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "success"}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner, "variant": "Busy"}}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": oops}}));
    let f = found(s.doc().unwrap(), "states.notDesigned", &[]);
    assert_eq!(f.len(), 1, "only the one with nothing to show it: {f:?}");
    assert_eq!((f[0].severity, f[0].nodes.clone(), f[0].subject.clone()), (Severity::Minor, vec![list], format!("state:{list}:success:")));
    assert!(f[0].evidence.contains("no `shownBy`"), "{}", f[0].evidence);
    // The layer goes, and the variant: what the file said it has, it no longer has.
    ex(&mut s, "edit.delete", json!({"ids": [note]}));
    let set = s
        .doc()
        .unwrap()
        .node(
            s.doc()
                .unwrap()
                .node(spinner)
                .map(|n| match &n.kind {
                    figcraft_doc::NodeKind::Instance { component, .. } => *component,
                    _ => panic!("not an instance"),
                })
                .unwrap(),
        )
        .unwrap()
        .parent
        .unwrap();
    let busy = s.doc().unwrap().node(set).unwrap().children[1];
    ex(&mut s, "node.set", json!({"id": busy, "name": "State=Working"}));
    let f = found(s.doc().unwrap(), "states.notDesigned", &[]);
    let majors: Vec<&str> = f.iter().filter(|x| x.severity == Severity::Major).map(|x| x.evidence.as_str()).collect();
    assert_eq!(majors.len(), 2, "{f:?}");
    assert!(majors.iter().any(|e| e.contains("no longer exists")) && majors.iter().any(|e| e.contains("has no variant `Busy`")), "{majors:?}");
    // Scoped to the overlay's screen: only what names a layer in it, or none.
    assert!(found(s.doc().unwrap(), "states.notDesigned", &[home]).is_empty());
}

#[test]
fn unreachable_is_a_designed_state_neither_the_flow_nor_a_scenario_gets_to() {
    let (mut s, [home, _, oops, later, ..]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": oops}}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "returning", "shownBy": {"node": later}}));
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "firstUse"}));
    let f = found(s.doc().unwrap(), "states.unreachable", &[]);
    assert_eq!(f.len(), 1, "the reached overlay and the undesigned state aren't reported: {f:?}");
    assert_eq!((f[0].severity, f[0].nodes.clone()), (Severity::Minor, vec![home, later]));
    assert!(f[0].evidence.contains("`Later`") && f[0].evidence.contains("no stored scenario shows it"), "{}", f[0].evidence);
    // Near miss: a scenario shows it.
    ex(&mut s, "proto.scenario.add", json!({"name": "Back after a week", "start": later, "steps": []}));
    assert!(found(s.doc().unwrap(), "states.unreachable", &[]).is_empty());
}

#[test]
fn a_layer_whose_screen_is_reached_is_a_note_until_a_scenario_shows_it() {
    let (mut s, [home, list, _, _, note, _]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    let f = found(s.doc().unwrap(), "states.unreachable", &[]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Note, "only a scenario can show a layer is shown: a note, not a defect");
    assert!(f[0].message.contains("`Empty note`") && f[0].suggestion.contains("scenario step"), "{f:?}");
    // A scenario that shows it: nothing to say.
    let open = s.doc().unwrap().node(home).unwrap().children[0];
    ex(&mut s, "node.set", json!({"id": note, "visible": true}));
    ex(&mut s, "proto.scenario.add", json!({"name": "Nothing yet", "start": home, "steps": [{"click": open, "expect": {"visible": [note]}}]}));
    assert!(found(s.doc().unwrap(), "states.unreachable", &[]).is_empty());
}

#[test]
fn a_declared_screen_that_moved_inside_another_frame_is_reported_and_can_be_changed_or_removed() {
    let (mut s, [home, list, _, later, note, _]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": later, "kind": "returning", "reason": "a week later"}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    // "Later" goes inside Home: it is no longer a screen.
    ex(&mut s, "node.reparent", json!({"ids": [later], "parent": home}));
    let f = found(s.doc().unwrap(), "states.notDesigned", &[]);
    let moved: Vec<_> = f.iter().filter(|x| x.subject == format!("state:{later}:returning:")).collect();
    assert_eq!(moved.len(), 1, "{f:?}");
    assert_eq!(moved[0].severity, Severity::Major);
    assert!(
        moved[0].evidence.contains("`Later`") && moved[0].evidence.contains("is no longer a screen: it is inside `Home`"),
        "{}",
        moved[0].evidence
    );
    assert!(moved[0].suggestion.contains("proto.state.remove") && moved[0].suggestion.contains("`Home`"), "{}", moved[0].suggestion);
    assert!(found(s.doc().unwrap(), "states.unreachable", &[]).iter().all(|x| !x.subject.contains(&later.to_string())), "no unreachable for it");
    let l = listed(&mut s, later, "returning");
    assert_eq!((&l["designed"], &l["reachable"]), (&json!(false), &json!(false)));
    // Its reason can still be changed, a new declaration there can't be made, and it can go.
    ex(&mut s, "proto.state.set", json!({"frame": later, "kind": "returning", "reason": "two weeks later"}));
    assert!(err(&mut s, "proto.state.set", json!({"frame": later, "kind": "error"})).contains("isn't a top-level frame"));
    ex(&mut s, "proto.state.remove", json!({"frame": later, "kind": "returning"}));
    // A gone screen: the same, and removable.
    ex(&mut s, "edit.delete", json!({"ids": [list]}));
    let f = found(s.doc().unwrap(), "states.notDesigned", &[]);
    assert!(f.iter().any(|x| x.evidence.contains(&format!("the screen (layer {list}) no longer exists"))), "{f:?}");
    ex(&mut s, "proto.state.remove", json!({"frame": list, "kind": "empty"}));
    assert!(s.doc().unwrap().design.states.is_empty());
}

#[test]
fn a_persons_edit_of_an_agents_declaration_is_recorded_and_ends_the_agents_say_over_it() {
    let (mut s, [_, list, _, _, note, _]) = screens();
    as_agent(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty"}));
    let r = ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "shownBy": {"node": note}}));
    assert_eq!((&r["state"]["editedBy"], &r["state"]["status"]), (&json!("user"), &json!("proposed")), "{r}");
    let e = agent_err(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty", "reason": "mine"}));
    assert!(e.contains("edited by user"), "{e}");
    agent_err(&mut s, "proto.state.remove", json!({"frame": list, "kind": "empty"}));
    // Accepting is still the person's.
    ex(&mut s, "proto.state.accept", json!({"frame": list, "kind": "empty"}));
}

#[test]
fn error_empty_asks_a_screen_with_states_but_neither_whether_it_can_fail() {
    let (mut s, [home, list, ..]) = screens();
    ex(&mut s, "design.brief.set", json!({"context": "On a phone, on the bus, where the signal comes and goes"}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading"}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "success"}));
    let f = found(s.doc().unwrap(), "states.errorEmpty", &[]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!((f[0].severity, f[0].nodes.clone(), f[0].subject.clone()), (Severity::Note, vec![list], format!("stateKinds:{list}")));
    assert!(f[0].message.starts_with("Can `List`") && f[0].message.contains('?'), "a question, not a defect: {}", f[0].message);
    assert!(f[0].evidence.contains("declared: loading, success") && f[0].evidence.contains("signal comes and goes"), "{}", f[0].evidence);
    // Near misses: an empty state; a screen with no declarations isn't asked.
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "empty"}));
    assert!(found(s.doc().unwrap(), "states.errorEmpty", &[]).is_empty());
    let _ = home;
}

#[test]
fn the_states_rules_run_from_their_stage_and_say_so_when_they_dont() {
    let (mut s, [home, _, _, later, ..]) = screens();
    let skipped = |v: &Value, id: &str| v["skipped"].as_array().unwrap().iter().any(|x| x["id"] == id);
    let v = ex(&mut s, "design.check", json!({}));
    for id in ["states.none", "states.notDesigned", "states.unreachable", "states.errorEmpty"] {
        assert!(skipped(&v, id), "exploring: {id} doesn't run");
    }
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let v = ex(&mut s, "design.check", json!({}));
    assert!(!skipped(&v, "states.notDesigned"), "{v}");
    assert!(skipped(&v, "states.none") && skipped(&v, "states.unreachable") && skipped(&v, "states.errorEmpty"));
    ex(&mut s, "design.stage.set", json!({"stage": "ready"}));
    let v = ex(&mut s, "design.check", json!({}));
    assert_eq!(v["summary"]["byRule"]["states.none"], 1, "ready: said once that nothing is declared");
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "returning", "shownBy": {"node": later}}));
    let v = ex(&mut s, "design.check", json!({}));
    assert!(v["summary"]["byRule"].get("states.none").is_none(), "{v}");
    assert_eq!((&v["summary"]["byRule"]["states.unreachable"], &v["summary"]["byRule"]["states.errorEmpty"]), (&json!(1), &json!(1)), "{v}");
    let rules = ex(&mut s, "design.check.rules", json!({}));
    for id in ["states.none", "states.notDesigned", "states.unreachable", "states.errorEmpty"] {
        assert!(rules.as_array().unwrap().iter().any(|r| r["id"] == id), "{id} is listed");
    }
    // Recorded, fixed and re-checked: verified.
    ex(&mut s, "design.check.record", json!({"rules": ["states.unreachable"]}));
    ex(&mut s, "proto.scenario.add", json!({"name": "Back", "start": later, "steps": []}));
    let r = ex(&mut s, "design.finding.recheck", json!({}));
    assert!(r["results"].as_array().unwrap().iter().any(|x| x["passed"] == true), "{r}");
}

#[test]
fn declared_states_survive_save_and_reopen() {
    let (mut s, [home, list, oops, _, _, spinner]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": oops}, "reason": "the network fails"}));
    as_agent(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading", "shownBy": {"instance": spinner, "variant": "State=Busy"}}));
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (doc, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(doc.design.states, s.doc().unwrap().design.states);
    assert_eq!((doc.design.states[1].kind, doc.design.states[1].status), (StateKind::Loading, ProposalStatus::Proposed));
}

#[test]
fn a_run_reports_the_screens_it_went_through() {
    let (mut s, [home, list, oops, ..]) = screens();
    let (open, fail) = {
        let d = s.doc().unwrap();
        (d.node(home).unwrap().children[0], d.node(home).unwrap().children[1])
    };
    let back = s.doc().unwrap().node(list).unwrap().children[0];
    let r = ex(&mut s, "proto.simulate", json!({"start": home, "steps": [{"click": open}, {"click": back}, {"click": fail}, {"click": open}]}));
    // Home, List, Home again (counted once), then the overlay (not a screen) and back to List.
    assert_eq!((&r["stepsTaken"], &r["screens"], &r["screenCount"]), (&json!(4), &json!([home, list]), &json!(2)), "{r}");
    ex(&mut s, "proto.scenario.add", json!({"name": "Open", "start": home, "steps": [{"click": open}]}));
    let r = ex(&mut s, "proto.scenario.run", json!({}));
    assert_eq!((&r["results"][0]["screens"], &r["results"][0]["screenCount"]), (&json!([home, list]), &json!(2)));
    let _ = oops;
}

/// P3-4: a frame that is no longer a top-level screen isn't asked whether it can fail:
/// `states.notDesigned` says it is no longer a screen.
#[test]
fn error_empty_doesnt_ask_about_a_frame_that_is_no_longer_a_screen() {
    let (mut s, [home, list, _, later, ..]) = screens();
    ex(&mut s, "proto.state.set", json!({"frame": later, "kind": "returning"}));
    ex(&mut s, "proto.state.set", json!({"frame": list, "kind": "loading"}));
    let asked = |s: &Session| found(s.doc().unwrap(), "states.errorEmpty", &[]).into_iter().map(|f| f.subject).collect::<Vec<_>>();
    assert_eq!(asked(&s), [format!("stateKinds:{list}"), format!("stateKinds:{later}")]);
    ex(&mut s, "node.reparent", json!({"ids": [later], "parent": home}));
    assert_eq!(asked(&s), [format!("stateKinds:{list}")], "Later is inside Home now");
    assert!(found(s.doc().unwrap(), "states.notDesigned", &[]).iter().any(|f| f.evidence.contains("is no longer a screen")));
}
