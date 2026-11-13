//! Scenarios and flow: the walks through the prototype stored in the file, run on the headless
//! simulator with what each step did; what the flow check finds on the page, as facts; and the
//! frames declared as places where a flow may end.
//!
//! Scenario steps are written by AI agents and from the command line (`proto.scenario.add`); this
//! panel reads, runs and removes them.

use egui::Ui;
use figcraft_doc::NodeId;
use figcraft_doc::design::{Gesture, Scenario, StepAction};
use serde_json::{Value, json};

use super::findings::found;
use super::{
    Cx, Ran, act, author_name, body, button, chip, entry, flow_row, go_to, heading, link, pad, part, primary, quiet, refusal_for, steps_text, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::widgets;

/// What each gesture of a step reads as; `{layer}` is the layer's name.
const GESTURES: [(Gesture, &str); 8] = [
    (Gesture::Click, tr_noop("Click {layer}")),
    (Gesture::Hover, tr_noop("Hover over {layer}")),
    (Gesture::Press, tr_noop("Press and hold {layer}")),
    (Gesture::Drag, tr_noop("Drag {layer}")),
    (Gesture::MouseEnter, tr_noop("Move the pointer onto {layer}")),
    (Gesture::MouseLeave, tr_noop("Move the pointer off {layer}")),
    (Gesture::MouseDown, tr_noop("Press the button on {layer}")),
    (Gesture::MouseUp, tr_noop("Let go of the button on {layer}")),
];

/// A layer as a sentence names it: its name in quotes, or that it is gone.
fn layer(cx: &Cx, id: NodeId) -> String {
    match cx.name(id) {
        Some(name) => format!("“{name}”"),
        None => trf("missing layer ({id})", &[("id", &id)]),
    }
}

/// What a step does, in words.
fn step_text(cx: &Cx, action: &StepAction) -> String {
    match action {
        StepAction::Gesture { node, gesture } => {
            let key = GESTURES.iter().find(|(g, _)| g == gesture).map_or("{layer}", |(_, text)| text);
            trf(key, &[("layer", &layer(cx, *node))])
        }
        StepAction::Key { key } => trf("Press the key {key}", &[("key", key)]),
        StepAction::Wait => tr("Wait for the delays to pass").to_string(),
        StepAction::ClickOutside => tr("Click outside the overlay").to_string(),
        StepAction::SetVariable { variable, value } => {
            trf("Set {variable} to {value} (stands in for input the prototype can't take)", &[("variable", variable), ("value", value)])
        }
    }
}

pub(super) fn show(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    scenarios(app, ui, cx);
    flow_check(app, ui, cx);
    end_states(app, ui, cx);
}

/// Run `proto.scenario.run` (one scenario, or all) and keep what it answered.
fn run(app: &mut FigApp, id: Option<&str>) {
    let params = id.map_or_else(|| json!({}), |id| json!({"id": id}));
    if let Some(result) = act(app, id.unwrap_or("scenarios"), "proto.scenario.run", params) {
        app.ui.context.scenario_run = Ran::new(app, result);
    }
}

fn scenarios(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Scenarios"),
        tr(
            "Walks through the prototype with what should be true along the way. Running one plays it without a window: timing, scrolling and drags aren't played.",
        ),
    );
    let list = &cx.design.scenarios;
    pad(ui, |ui| {
        quiet(ui, tr("AI agents and the command line write scenarios (proto.scenario.add). Here you can read, run and remove them."));
        ui.add_space(4.0);
        if list.is_empty() {
            quiet(ui, tr("No scenarios are stored in this file."));
        } else if primary(ui, tr("Run all")) {
            run(app, None);
        }
        refusal_for(app, ui, "scenarios");
        if let Some(ran) = &app.ui.context.scenario_run {
            if ran.stale(&cx.doc) {
                ui.add_space(4.0);
                quiet(ui, tr("The file has changed since the scenarios last ran: the results below are from before."));
            }
            // A pass is the simulator's state rules, not a session somebody watched.
            if let Some((line, whole)) = simulated(&ran.result) {
                ui.add_space(4.0);
                quiet(ui, &line).on_hover_text(whole);
            }
        }
    });
    for s in list {
        entry(app, ui, &s.id, |app, ui| scenario(app, ui, cx, s));
    }
}

fn scenario(app: &mut FigApp, ui: &mut Ui, cx: &Cx, s: &Scenario) {
    flow_row(ui, |ui| {
        quiet(ui, &s.id);
        quiet(ui, &steps_text(s.steps.len()));
        quiet(ui, &trf("by {name}", &[("name", &author_name(&s.author))]));
    });
    strong(ui, &s.name);
    part(ui, tr("What passing shows"), &s.purpose);
    ui.add_space(2.0);
    flow_row(ui, |ui| {
        quiet(ui, tr("Starts on"));
        match cx.name(s.start) {
            Some(name) => {
                if link(ui, name).clicked() {
                    go_to(app, s.start);
                }
            }
            None => {
                quiet(ui, &trf("missing layer ({id})", &[("id", &s.start)]));
            }
        }
    });
    // What the last run said about this scenario.
    let result =
        app.ui.context.scenario_run.as_ref().and_then(|ran| ran.result["results"].as_array()?.iter().find(|r| r["id"] == json!(s.id)).cloned());
    if let Some(r) = &result {
        ui.add_space(4.0);
        outcome(ui, r, s.steps.len());
    }
    let open = app.ui.context.unfolded.contains(&s.id);
    ui.add_space(4.0);
    flow_row(ui, |ui| {
        if button(ui, tr("Run")) {
            run(app, Some(&s.id));
        }
        if !s.steps.is_empty() && button(ui, if open { tr("Hide the steps") } else { tr("Show the steps") }) {
            if open {
                app.ui.context.unfolded.remove(&s.id);
            } else {
                app.ui.context.unfolded.insert(s.id.clone());
            }
        }
        if button(ui, tr("Remove")) {
            act(app, &s.id, "proto.scenario.remove", json!({"id": s.id}));
        }
    });
    refusal_for(app, ui, &s.id);
    if open {
        ui.add_space(4.0);
        for (i, step) in s.steps.iter().enumerate() {
            let ran = result.as_ref().and_then(|r| r["steps"].as_array()?.get(i));
            body(ui, &format!("{}. {}", i + 1, step_text(cx, &step.action)));
            if !step.note.is_empty() {
                quiet(ui, &step.note);
            }
            if let Some(ran) = ran {
                step_result(ui, ran);
            }
            ui.add_space(2.0);
        }
    }
}

/// "Simulated: <what the simulator leaves out>", for a result the simulator gave (`simulated`),
/// each item without the engine's explanation in brackets (which the line shows on hover): the
/// line is there so a pass is not read as a session somebody watched.
pub(super) fn simulated(result: &Value) -> Option<(String, String)> {
    if result["simulated"].as_bool() != Some(true) {
        return None;
    }
    let left_out: Vec<&str> = result["notModelled"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let short: Vec<&str> = left_out.iter().map(|s| s.split(" (").next().unwrap_or(s).trim()).collect();
    Some((trf("Simulated (not modelled: {items})", &[("items", &short.join("; "))]), left_out.join("\n")))
}

/// Whether the scenario passed in the last run, and where and why it didn't.
fn outcome(ui: &mut Ui, r: &Value, steps: usize) {
    let failures: Vec<&str> = r["failures"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if r["passed"].as_bool() == Some(true) {
        flow_row(ui, |ui| {
            chip(ui, tr("Passed"));
            quiet(ui, tr("every step ran and every expectation held"));
        });
        return;
    }
    let at = match r["failedStep"].as_u64() {
        Some(i) => trf("Failed at step {step} of {steps}", &[("step", &(i + 1)), ("steps", &steps)]),
        None if r["stepsTaken"].as_u64() == Some(0) && steps > 0 => tr("Could not start").to_string(),
        None => tr("Failed after the last step").to_string(),
    };
    flow_row(ui, |ui| chip(ui, tr("Failed")));
    strong(ui, &at);
    for f in failures {
        body(ui, f);
    }
}

/// What one step did in the last run: what it set off, and what didn't hold.
fn step_result(ui: &mut Ui, ran: &Value) {
    let texts = |key: &str| -> Vec<&str> { ran[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect() };
    for effect in texts("effects") {
        quiet(ui, &format!("→ {effect}"));
    }
    if let Some(problem) = ran["problem"].as_str() {
        body(ui, problem);
    }
    for failure in texts("failures") {
        body(ui, failure);
    }
}

fn flow_check(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Flow"),
        tr(
            "The page's prototype as a map: which screens the interactions can lead to, and where an action leads nowhere. Every interaction counts as possible.",
        ),
    );
    pad(ui, |ui| {
        if primary(ui, tr("Check the flow"))
            && let Some(result) = act(app, "flow", "proto.check", json!({}))
        {
            app.ui.context.flow = Ran::new(app, result);
        }
        refusal_for(app, ui, "flow");
    });
    let Some((result, stale)) = app.ui.context.flow.as_ref().map(|ran| (ran.result.clone(), ran.stale(&cx.doc))) else { return };
    ui.add_space(6.0);
    pad(ui, |ui| {
        if stale {
            quiet(ui, tr("The file has changed since the flow was checked: what follows is from before."));
            ui.add_space(4.0);
        }
        let screens = result["screens"].as_array().map_or(&[][..], Vec::as_slice);
        let reached = screens.iter().filter(|s| s["reachable"].as_bool() == Some(true)).count();
        body(
            ui,
            &trf("{reached} of {total} screens can be reached from where the prototype starts.", &[("reached", &reached), ("total", &screens.len())]),
        );
        let starts: Vec<(NodeId, String)> = result["starts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| Some((figcraft_engine::params::id_of(&s["frame"])?, s["name"].as_str().unwrap_or_default().to_string())))
            .collect();
        if !starts.is_empty() {
            flow_row(ui, |ui| {
                quiet(ui, tr("It starts on"));
                for (id, name) in &starts {
                    if link(ui, name).clicked() {
                        go_to(app, *id);
                    }
                }
            });
        }
        if result["prototyped"].as_bool() == Some(false) {
            quiet(
                ui,
                tr(
                    "Nothing on this page leads from one screen to another yet, so the rules about starts, dead ends and unreachable screens don't apply.",
                ),
            );
        }
        // Frames an AI agent declared as ends of the flow: still dead ends until a person says so.
        for s in screens {
            let (Some(agent), Some(id)) = (s["endStateProposedBy"].as_str(), figcraft_engine::params::id_of(&s["id"])) else { continue };
            let name = s["name"].as_str().unwrap_or_default();
            ui.add_space(4.0);
            body(ui, &trf("“{screen}” is proposed as an end of the flow by {agent}, waiting for you.", &[("screen", &name), ("agent", &agent)]));
            // Accepted since the check ran: the button goes, the line stays until the next check.
            let pending = cx.design.end_states.iter().any(|e| e.node == id && !e.status.is_accepted());
            if pending && button(ui, tr("Accept it as an end")) {
                act(app, "flow", "proto.endState.accept", json!({"id": id}));
            }
        }
    });
    let problems = result["problems"].as_array().map_or(&[][..], Vec::as_slice);
    if problems.is_empty() {
        pad(ui, |ui| quiet(ui, tr("The flow rules found nothing to report.")));
    }
    for (i, p) in problems.iter().enumerate() {
        entry(app, ui, &format!("flow problem {i}"), |app, ui| found(app, ui, cx, p));
    }
}

fn end_states(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Ends of the flow"),
        tr(
            "Screens where a walk may end on purpose (a confirmation, a sent message). A screen with no way out that isn't listed here is reported as a dead end. One an AI agent declares stays a dead end until you accept it.",
        ),
    );
    for e in &cx.design.end_states {
        let proposed = !e.status.is_accepted();
        ui.add_space(8.0);
        pad(ui, |ui| {
            flow_row(ui, |ui| {
                match cx.name(e.node) {
                    Some(name) => {
                        if link(ui, name).clicked() {
                            go_to(app, e.node);
                        }
                    }
                    None => {
                        quiet(ui, &trf("missing layer ({id})", &[("id", &e.node)]));
                    }
                }
                chip(ui, if proposed { tr("Proposed") } else { tr("Accepted") });
                quiet(ui, &trf("by {name}", &[("name", &author_name(&e.author))]));
            });
            if proposed {
                quiet(
                    ui,
                    tr(
                        "Waiting for you: until you accept it, it is no end of the flow, and the flow check may still report the screen as a dead end.",
                    ),
                );
                ui.add_space(2.0);
                if widgets::button(ui, tr("Accept"), true, true).clicked() {
                    act(app, "ends", "proto.endState.accept", json!({"id": e.node}));
                }
                ui.add_space(2.0);
            }
            quiet(ui, tr("Why the flow may end here"));
        });
        // The reason is written when its field loses the keyboard. A proposal's too: a person's
        // `proto.endState.set` that only gives a new reason leaves it a proposal (only Accept,
        // above, accepts it), so rewording what the agent wrote accepts nothing unasked.
        if cx.name(e.node).is_some()
            && let Some(reason) = widgets::multiline_field(ui, cx.id(("end", e.node)), &e.reason, "", 1)
        {
            act(app, "ends", "proto.endState.set", json!({"id": e.node, "reason": reason}));
        }
        pad(ui, |ui| {
            ui.add_space(4.0);
            if button(ui, tr("No longer an end")) {
                act(app, "ends", "proto.endState.set", json!({"id": e.node, "on": false}));
            }
        });
        ui.add_space(8.0);
        widgets::separator(ui);
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.end_states.is_empty() {
            quiet(ui, tr("No screen is declared as an end of the flow."));
            ui.add_space(4.0);
        }
        // The selected top-level frame can be declared.
        let frame = cx.selection.first().copied().filter(|id| cx.selection.len() == 1 && cx.doc.top_level_frame(*id) == Some(*id));
        match frame {
            Some(id) if cx.design.end_states.iter().any(|e| e.node == id) => {}
            Some(id) => {
                strong(ui, &format!("“{}”", cx.name(id).unwrap_or_default()));
                if button(ui, tr("Declare it as an end of the flow")) {
                    act(app, "ends", "proto.endState.set", json!({"id": id}));
                }
            }
            None => {
                quiet(ui, tr("Select a top-level frame to declare it as an end of the flow."));
            }
        }
        refusal_for(app, ui, "ends");
    });
}
