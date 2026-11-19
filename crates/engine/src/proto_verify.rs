//! Prototype verification commands: the flow graph check (`proto.check`), running triggers
//! against the simulator (`proto.simulate`), stored scenarios (`proto.scenario.*`) and the frames
//! where a flow may end (`proto.endState.*`), and the states each screen is declared to have
//! (`proto.state.*`).
//!
//! All of it is headless and deterministic: the same file gives the same answer, the file is
//! never changed by a check or a run (the simulator plays a copy), and what is reported is what
//! was measured (which screens are reachable, which step failed and why) rather than a verdict on
//! the design.
//!
//! What settles a measured finding is a person's to say, so the two records that can make one go
//! away have authors: an end state an AI agent declares is a proposal until a person accepts it,
//! and an agent changes and removes only the scenarios it wrote itself. A convention between
//! cooperating parties, not a security boundary (see [`crate::design`]).

use figcraft_doc::design::MAX_ENTRIES;
use serde_json::{Value, json};

use crate::check::Rule;
use crate::cmd::CommandSpec;
use crate::params::{bool_of, get, opt_id};
use crate::{Result, Session, bad, spec};

mod check;
mod end_state;
mod flow;
mod parse;
mod run;
pub use run::{KNOWN_DIFFERENCES, NOT_MODELLED};
mod scenario;
pub(crate) use scenario::{Walk, walks};
mod states;

#[cfg(test)]
mod tests;

pub(crate) use check::{PageFacts, Where, subject_where};

/// The step forms `proto.simulate` and `proto.scenario.*` take, for their parameter docs.
macro_rules! steps_doc {
    () => {
        "[{type: gesture, node (layer id), gesture? (click|hover|press|drag|mouseEnter|mouseLeave|mouseDown|mouseUp; default click)} | {type: key, key (Enter, Shift+K, Gamepad A)} | {type: wait (the pending after-delay interactions fire)} | {type: clickOutside (outside the top overlay)} | {type: setVariable, variable (id, Collection/name or a unique name), value}; each with expect? and note?; shorthand {click: <layer id>} (any gesture's name) and {key: \"Enter\"}]"
    };
}

/// The expectation form, for parameter docs.
macro_rules! expect_doc {
    () => {
        "{screen? (frame id), overlays? [ids, bottom first; [] for none], variables? {name: value}, variants? [{instance, variant (its name, one property value, or the variant's id)}], visible? [ids], hidden? [ids], text? [{node, equals? | contains?}], endState? bool}"
    };
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "proto.check",
            "Check prototype flow",
            "{page? (a page id; default: the current page)}  the page's prototype as a graph, without changing or playing anything → {page, prototyped (a flow start is declared, a screen plays an action that moves between screens, or another page leads here), starts: [{frame, name, source: flow|firstFrame|otherPage (an interaction on another page navigates here), flow}], screens: [{id, name, reachable, endState (declared and accepted by a person), endStateProposedBy? (the AI agent whose declaration waits to be accepted), deadEnd, overlay, component (a component or component set: never a start, dead end or unreachable), out: [{from, fromName, trigger, action, to?, toName?, key?, ms?}]}], states: [the states declared for the page's frames, as proto.state.list gives them], problems: [{rule, severity, nodes, subject? (what it is about when that isn't the layers), message, evidence, suggestion}], ok}. The page is read as the player has it: instances play their main components' interactions, wherever those live. Every interaction on a screen is an edge (all branches of a conditional and every variant a change-to leads to count as possible), so `reachable` means an interaction leads there, not that a viewer will get there. `screens` is complete whether or not there are problems. Rules: flow.brokenTarget, flow.deadEnd, flow.trappedOverlay, flow.unreachable, flow.noStart, scenario.fails, scenario.none, scenario.unvisited (design.check.rules describes each; the states.* rules run in design.check). `ok` says only that these rules found no blocker or major problem; it doesn't say the prototype is right",
            false,
            check::check
        ),
        spec!(
            "proto.endState.set",
            "Mark as end of flow",
            "{id (a top-level frame), reason? (why the flow may end here; required from an AI agent), on? (default true; false takes the declaration back)}  declares a frame where a flow may end on purpose, so flow.deadEnd doesn't report it. The user's declaration stands at once; on a frame an AI agent proposed, the user's call with `on: true` or with the frame alone accepts the proposal, while one that only gives a new `reason` leaves it proposed. An AI agent's is a proposal: the frame stays a dead end until a person accepts it (proto.endState.accept); an agent changes and takes back only its own proposals → {id, endState (declared and accepted), status: proposed|accepted|null, author, reason, acceptedBy? (the person who accepted an AI agent's proposal), note? (when proposed)}",
            true,
            end_state::set
        ),
        spec!(
            "proto.endState.accept",
            "Accept end of flow",
            "{id (a frame declared as an end state)}  a person stands behind an end state an AI agent proposed: from then on flow.deadEnd doesn't report the frame; refused to an AI agent; the person is recorded as acceptedBy → {id, endState, status, author, reason, acceptedBy?}",
            true,
            end_state::accept
        ),
        spec!(
            "proto.endState.list",
            "List ends of flows",
            "{}  → {endStates: [{node, name, reason, author, status: proposed|accepted, acceptedBy? (who accepted an AI agent's proposal), exists}]}",
            false,
            end_state::list
        ),
        spec!(
            "proto.state.set",
            "Declare screen state",
            "{frame (a top-level frame: the screen), kind: firstUse|empty|loading|error|returning|success|other, name? (required for other; a label otherwise: one declaration per frame, kind and name), shownBy? ({node} a top-level frame (another screen, an overlay) or a layer in the screen | {instance, variant} an instance in the screen and the variant it shows the state with (its name, one property value, or the variant's id) | null: not designed yet), reason? (why the screen has this state)}  declares a state the screen has because time passes (first use, nothing yet, waiting, failure, coming back) or its outcome, or changes the given keys of one declared already; what `shownBy` names must exist. The user's declaration stands; an AI agent's is a proposal until a person accepts it (proto.state.accept), and an agent changes and takes back only its own proposals. Proposals count in the states.* checks all the same → {state: {frame, frameName, kind, name?, shownBy?, shownByName?, reason?, author, status: proposed|accepted, acceptedBy?, exists}}",
            true,
            states::set
        ),
        spec!(
            "proto.state.remove",
            "Remove screen state",
            "{frame, kind, name?}  takes a declaration back (also one whose frame is gone); an AI agent takes back only its own proposals → {removed}",
            true,
            states::remove
        ),
        spec!(
            "proto.state.accept",
            "Accept screen state",
            "{frame, kind, name?}  a person stands behind a state an AI agent declared; refused to an AI agent; the person is recorded as acceptedBy → {state}",
            true,
            states::accept
        ),
        spec!(
            "proto.state.list",
            "List screen states",
            "{frame? (only this screen's)}  every declared state with what can be said about it → {states: [{frame, frameName, kind, name?, shownBy?, shownByName?, reason?, author, status, acceptedBy?, exists (the screen), designed (shownBy is given and what it names is there: a top-level frame, a layer in the screen, or an instance in the screen whose component set has the variant), notDesigned? (why not), reachable (by the flow or a scenario), reachedBy: [flow (a flow start reaches the frame that shows it through the page's interactions: the top-level frame shownBy names, else the screen; variables and bindings aren't followed, so for a layer or a variant only its screen is reached) | scenario], coveredBy: [scenario ids whose headless run shows it, at the start or after a step that passed]}], summary: {total, designed, reachable, proposed}}",
            false,
            states::list
        ),
        spec!(
            "proto.simulate",
            "Simulate prototype",
            concat!(
                "{start? (a top-level frame id; default as proto.present: the selected frame, else the first flow, else the first frame), steps ",
                steps_doc!(),
                ", expect? ",
                expect_doc!(),
                " (checked after the last step), stopOnFailure? (default true: stop at the first step that fails)}  plays the steps headless on a copy of the file, which never changes. The result is the simulator's: it follows the player's rules for what state comes next, and nobody watched the player do it → {passed, simulated: true, notModelled: [what the simulator leaves out], knownDifferences: [where the simulator and the app's player still differ], start, state: {screen, screenName, overlays: [{id, name}], history, variables (those the run changed), modes, variants: [{instance, component, name}], endState}, steps: [{index, action, note?, fired, effects, problem?, failures, state: {screen, screenName, overlays}}], failures (of the final expect), failedStep?, stepsTaken, screens (the screens it went through, each once, in the order first shown: the start, then the screen after each step that ran; overlays aren't screens), screenCount}. A step fails when it can't run (the layer isn't on what shows: the top overlay if one is open, else the screen), when a gesture or key sets nothing off, when an action can't run, or when its expect doesn't hold. A gesture reaches the first interaction with its trigger on the layer or a layer around it; hover and press last until the next step; wait fires every pending after-delay; scrolling, timing, devices and the player's own keys aren't simulated"
            ),
            false,
            simulate
        ),
        spec!(
            "proto.scenario.add",
            "Add scenario",
            concat!(
                "{name, start (a top-level frame id), steps ",
                steps_doc!(),
                ", expect? ",
                expect_doc!(),
                " (what must hold after the last step), purpose? (what passing shows)}  stores a walk through the prototype with what should be true along the way; every layer it names must exist in the file (layers that only exist once an instance was changed to another variant can't be named: name the instance and expect its variant); it is recorded as written by whoever adds it (the user, or the AI agent) → {id}"
            ),
            true,
            scenario::add
        ),
        spec!(
            "proto.scenario.set",
            "Edit scenario",
            "{id (a scenario id: S1), name?, start?, steps? (replaces them all; forms as proto.scenario.add), expect? ({} clears it), purpose? (\"\" clears it)}  only the keys given change; an AI agent changes only scenarios it wrote itself → {id, scenario}",
            true,
            scenario::set
        ),
        spec!(
            "proto.scenario.remove",
            "Remove scenario",
            "{id (a scenario id: S1)}  an AI agent removes only scenarios it wrote itself → {removed}",
            true,
            scenario::remove
        ),
        spec!(
            "proto.scenario.list",
            "List scenarios",
            "{}  → {scenarios: [{id, name, purpose?, author?, start, steps: [{action: {type…}, expect?, note?}], expect?}]}",
            false,
            scenario::list
        ),
        spec!(
            "proto.scenario.run",
            "Run scenarios",
            "{id? (a scenario id: S1; default: every stored scenario)}  runs them headless on a copy of the file, each stopping at its first failing step; simulated, as proto.simulate → {passed (no scenario failed), simulated: true, notModelled: [what the simulator leaves out], knownDifferences: [where it and the player still differ], total, failed, results: [{id, name, passed, failedStep? (index), failures (of that step, else of the final expect), steps (as proto.simulate), stepsTaken, screens, screenCount (as proto.simulate)}]}",
            false,
            scenario::run_stored
        ),
    ]
}

/// The flow and scenario rules `proto.check` runs, for `design.check` to list and run with its
/// own.
pub fn rules() -> Vec<Rule> {
    check::rules()
}

fn simulate(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.simulate";
    let steps =
        parse::steps(cmd, get(p, "steps").ok_or_else(|| bad(cmd, format!("`steps` is required (a list of up to {MAX_ENTRIES} steps)")))?, "steps")?;
    let expect = get(p, "expect").map(|v| parse::expect(cmd, v, "expect")).transpose()?.unwrap_or_default();
    let stop = match get(p, "stopOnFailure") {
        Some(v) => bool_of(v)
            .ok_or_else(|| bad(cmd, format!("`stopOnFailure` must be true or false (got {})", figcraft_format::clip(&v.to_string(), 40))))?,
        None => true,
    };
    let ds = s.ds()?;
    let start = match opt_id(cmd, p, "start")? {
        Some(id) => id,
        None => crate::cmd::proto::default_start(ds).ok_or_else(|| bad(cmd, "no frame to start at: the page has no top-level frame"))?,
    };
    let sim = crate::proto_sim::Sim::new(&ds.doc, start).map_err(|why| bad(cmd, format!("`start`: {why}")))?;
    let r = run::play(sim, &steps, &expect, stop);
    let mut out = json!({
        "passed": r.passed,
        "simulated": true,
        "notModelled": run::NOT_MODELLED,
        "knownDifferences": run::known_differences(),
        "start": start,
        "state": r.state,
        "steps": run::steps_json(&steps, &r),
        "failures": r.failures,
        "stepsTaken": r.steps.len(),
        "screens": r.screens(start),
        "screenCount": r.screens(start).len(),
    });
    if let (Some(o), Some(i)) = (out.as_object_mut(), r.failed_step) {
        o.insert("failedStep".into(), json!(i));
    }
    Ok(out)
}
