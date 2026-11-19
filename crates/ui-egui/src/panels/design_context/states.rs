//! The states of the screens: the places where time passes (first use, nothing yet, waiting,
//! failure, coming back) and how a screen ends, as declared for each screen, with what shows each
//! and whether the prototype gets there. Nothing is inferred: what isn't declared isn't listed,
//! and with nothing declared the panel says that.
//!
//! What is designed and reachable comes from `proto.state.list`, asked again only when the file
//! changed; every edit runs a `proto.state.*` command.

use egui::Ui;
use figcraft_doc::NodeId;
use figcraft_doc::design::{DeclaredState, ShownBy, StateKind};
use serde_json::{Value, json};

use super::records::submit;
use super::{
    Cx, Form, FormEnd, FormKind, Ran, act, authorship, body, button, chip, choice, draft, entry, flow_row, form_end, go_to, go_to_entry, heading,
    link, pad, quiet, refusal_for, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::widgets;

/// The kinds as the panel names them, in the engine's order.
const KINDS: [(StateKind, &str); 7] = [
    (StateKind::FirstUse, tr_noop("First use")),
    (StateKind::Empty, tr_noop("Nothing yet")),
    (StateKind::Loading, tr_noop("Waiting")),
    (StateKind::Error, tr_noop("Something went wrong")),
    (StateKind::Returning, tr_noop("Coming back")),
    (StateKind::Success, tr_noop("It worked")),
    (StateKind::Other, tr_noop("Another state")),
];

pub(super) fn kind_label(kind: StateKind) -> &'static str {
    KINDS.iter().find(|(k, _)| *k == kind).map_or("", |(_, label)| tr(label))
}

/// The form's key for a declaration: its kind's id and its name.
fn key_of(st: &DeclaredState) -> (String, String) {
    (st.kind.id().to_string(), st.name.clone())
}

/// What `proto.state.list` says now, asked again only when the file changed.
fn listed(app: &mut FigApp, cx: &Cx) -> Value {
    let kept = app.ui.context.results(cx.uid).and_then(|r| r.states.as_ref()).filter(|ran| !ran.stale(&cx.doc));
    if let Some(ran) = kept {
        return ran.result.clone();
    }
    // A read, run directly as `design.forNode` is (see `for_selected`).
    let Some(spec) = figcraft_engine::find_command("proto.state.list") else { return Value::Null };
    let Ok(result) = (spec.run)(&mut app.session, &json!({})) else { return Value::Null };
    if let Some(ran) = Ran::new(app, result.clone()) {
        app.ui.context.results_mut(cx.uid).states = Some(ran);
    }
    result
}

/// The facts the list gives about declaration `st`.
fn facts_of<'a>(listed: &'a Value, st: &DeclaredState) -> Option<&'a Value> {
    listed["states"]
        .as_array()?
        .iter()
        .find(|v| v["frame"] == json!(st.frame) && v["kind"] == json!(st.kind) && v["name"].as_str().unwrap_or_default() == st.name.as_str())
}

/// The screen a new declaration is for: the top-level frame of the selected layer.
fn selected_screen(cx: &Cx) -> Option<NodeId> {
    cx.selection.first().and_then(|id| cx.doc.top_level_frame(*id))
}

pub(super) fn show(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("States of the screens"),
        tr(
            "The states each screen has where time passes: the first time, nothing yet, waiting, something going wrong, coming back, and how it ends. Say which a screen has and what shows each; the checks compare that with the file.",
        ),
    );
    let listed = listed(app, cx);
    let mut form = app.ui.context.form.take();
    let editing = |form: &Option<Form>, frame: NodeId, key: Option<(String, String)>| {
        form.as_ref().is_some_and(|f| f.kind == FormKind::State(frame, key.clone()))
    };
    for st in &cx.design.states {
        let id = format!("state {} {} {}", st.frame, st.kind.id(), st.name);
        entry(app, ui, &id, |app, ui| {
            if editing(&form, st.frame, Some(key_of(st))) {
                state_form(app, ui, cx, &mut form);
            } else {
                state(app, ui, cx, st, facts_of(&listed, st), &mut form, &id);
            }
        });
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.states.is_empty() {
            quiet(ui, tr("No states are declared, so missing states aren't checked."));
            ui.add_space(4.0);
        }
        match selected_screen(cx) {
            Some(frame) if editing(&form, frame, None) => state_form(app, ui, cx, &mut form),
            Some(frame) => {
                strong(ui, &format!("“{}”", cx.name(frame).unwrap_or_default()));
                if button(ui, tr("Declare a state of this screen")) {
                    let mut f = Form::new(FormKind::State(frame, None));
                    // What is selected inside the screen, or another frame, is offered as what shows it.
                    f.checks = vec![("shown".into(), cx.selection.first().is_some_and(|s| *s != frame))];
                    form = Some(f);
                }
            }
            None => {
                quiet(ui, tr("Select a screen, or a layer in it, to declare one of its states."));
            }
        }
        refusal_for(app, ui, "states");
    });
    app.ui.context.form = form;
}

fn state(app: &mut FigApp, ui: &mut Ui, cx: &Cx, st: &DeclaredState, facts: Option<&Value>, form: &mut Option<Form>, id: &str) {
    flow_row(ui, |ui| {
        chip(ui, kind_label(st.kind));
        if !st.name.is_empty() {
            strong(ui, &st.name);
        }
        authorship(ui, &st.author, &st.accepted_by);
    });
    flow_row(ui, |ui| {
        quiet(ui, tr("The screen"));
        layer(app, ui, cx, st.frame);
    });
    match &st.shown_by {
        Some(by) => flow_row(ui, |ui| {
            quiet(ui, tr("Shown by"));
            layer(app, ui, cx, by.node());
            if let ShownBy::Variant { variant, .. } = by {
                quiet(ui, &trf("showing {variant}", &[("variant", variant)]));
            }
        }),
        None => {
            quiet(ui, tr("Nothing is said to show it yet."));
        }
    }
    if !st.reason.is_empty() {
        body(ui, &st.reason);
    }
    if let Some(f) = facts {
        ui.add_space(2.0);
        flow_row(ui, |ui| {
            chip(ui, if f["designed"] == json!(true) { tr("Designed") } else { tr("Not designed") });
            chip(ui, if f["reachable"] == json!(true) { tr("Reachable") } else { tr("Nothing gets there") });
        });
        if let Some(why) = f["notDesigned"].as_str() {
            quiet(ui, why);
        }
        let covered: Vec<&str> = f["coveredBy"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !covered.is_empty() {
            flow_row(ui, |ui| {
                quiet(ui, tr("A scenario shows it:"));
                for s in covered {
                    if link(ui, s).clicked() {
                        go_to_entry(app, s);
                    }
                }
            });
        } else if f["reachable"] == json!(true) {
            quiet(ui, tr("Its screen can be reached; no stored scenario shows the state itself."));
        }
    }
    ui.add_space(4.0);
    let params = |extra: Value| {
        let mut p = json!({"frame": st.frame, "kind": st.kind});
        if !st.name.is_empty() {
            p["name"] = json!(st.name);
        }
        if let (Some(o), Some(more)) = (p.as_object_mut(), extra.as_object()) {
            o.extend(more.clone());
        }
        p
    };
    flow_row(ui, |ui| {
        if !st.status.is_accepted() && widgets::button(ui, tr("Accept"), true, true).clicked() {
            act(app, id, "proto.state.accept", params(json!({})));
        }
        if button(ui, tr("Edit")) {
            let mut f = Form::new(FormKind::State(st.frame, Some(key_of(st)))).with_text(0, &st.name).with_text(1, &st.reason);
            if let Some(ShownBy::Variant { variant, .. }) = &st.shown_by {
                f = f.with_text(2, variant);
            }
            f.pick[0] = KINDS.iter().position(|(k, _)| *k == st.kind).unwrap_or(0);
            f.checks = vec![("shown".into(), false)];
            *form = Some(f);
        }
        if button(ui, tr("Remove")) {
            act(app, id, "proto.state.remove", params(json!({})));
        }
    });
    refusal_for(app, ui, id);
}

/// A layer as a link that selects it, or that it is gone.
fn layer(app: &mut FigApp, ui: &mut Ui, cx: &Cx, id: NodeId) {
    match cx.name(id) {
        Some(name) => {
            if link(ui, name).on_hover_text(tr("Select this layer and show it")).clicked() {
                go_to(app, id);
            }
        }
        None => {
            quiet(ui, &trf("missing layer ({id})", &[("id", &id)]));
        }
    }
}

fn state_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::State(frame, key) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let salt = (frame, key.clone());
    if key.is_none() {
        quiet(ui, tr("Which state?"));
        let labels: Vec<&str> = KINDS.iter().map(|(_, label)| tr(label)).collect();
        if let Some(i) = choice(ui, &labels, f.pick[0]) {
            f.pick[0] = i;
        }
        ui.add_space(4.0);
    }
    let [name, reason, variant, ..] = &mut f.text;
    if key.is_none() {
        draft(ui, cx.id(("state", &salt, "name")), tr("Its name (needed for another state)"), name, 1, focus);
    }
    draft(ui, cx.id(("state", &salt, "reason")), tr("Why does the screen have this state?"), reason, 2, focus && key.is_some());
    // What shows it: the selected layer, when it is in the screen or is a frame of its own.
    let selected = cx.selection.first().copied().filter(|s| cx.doc.top_level_frame(*s) == Some(*s) || cx.doc.is_ancestor(frame, *s));
    let instance = selected.filter(|s| cx.doc.node(*s).is_some_and(|n| matches!(n.kind, figcraft_doc::NodeKind::Instance { .. })));
    if let (Some(sel), Some((_, on))) = (selected, f.checks.first_mut()) {
        let label = trf("Shown by “{name}”", &[("name", &cx.name(sel).unwrap_or_default())]);
        if widgets::checkbox(ui, *on, &label) {
            *on = !*on;
        }
        if instance.is_some() && *on {
            draft(ui, cx.id(("state", &salt, "variant")), tr("The variant it shows (leave empty for the layer itself)"), variant, 1, false);
        }
    } else {
        quiet(ui, tr("To say what shows it, select that layer or frame before you save."));
    }
    let kind = match &key {
        Some((kind, _)) => kind.clone(),
        None => KINDS.get(f.pick[0]).map_or("error", |(k, _)| k.id()).to_string(),
    };
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let name = key.as_ref().map_or_else(|| f.text(0).trim().to_string(), |(_, n)| n.clone());
            let mut params = json!({"frame": frame, "kind": kind, "reason": f.text(1)});
            if !name.is_empty() {
                params["name"] = json!(name);
            }
            if let (Some(sel), Some((_, true))) = (selected, f.checks.first()) {
                let variant = f.text(2).trim();
                params["shownBy"] = match instance {
                    Some(inst) if !variant.is_empty() => json!({"instance": inst, "variant": variant}),
                    _ => json!({"node": sel}),
                };
            }
            submit(app, slot, "proto.state.set", params);
        }
    }
}

/// The panel's states for `ui.inspect`: how many are declared, and what the last list said.
pub(super) fn inspect(design: &figcraft_doc::design::DesignContext, listed: Option<&Value>) -> Value {
    let summary = listed.map(|l| l["summary"].clone());
    json!({
        "declared": design.states.len(),
        "proposed": design.states.iter().filter(|s| !s.status.is_accepted()).count(),
        "summary": summary,
    })
}
