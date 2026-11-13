//! Assumptions (what the design takes to be true without knowing it) and decisions (what was
//! chosen and why). A person edits everything here; what only a person may say (an assumption
//! was supported or refuted, a decision is accepted) is said here.

use egui::Ui;
use figcraft_doc::NodeId;
use figcraft_doc::design::{Assumption, AssumptionStatus, Decision, DecisionStatus};
use serde_json::{Value, json};

use super::{
    Cx, Form, FormEnd, FormKind, act, body, button, chip, choice, draft, entry, flow_row, form_end, go_to_entry, heading, layer_links, link, pad,
    part, quiet, refusal_for, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::widgets;

const STATUSES: [(AssumptionStatus, &str, &str); 3] = [
    (AssumptionStatus::Untested, "untested", tr_noop("Untested")),
    (AssumptionStatus::Supported, "supported", tr_noop("Supported")),
    (AssumptionStatus::Refuted, "refuted", tr_noop("Refuted")),
];

fn status_label(status: AssumptionStatus) -> &'static str {
    STATUSES.iter().find(|(s, ..)| *s == status).map_or("", |(_, _, label)| tr(label))
}

/// `links` with the selected layers added (those not linked yet), for a command's `links`.
fn with_selection(links: &[NodeId], selection: &[NodeId]) -> Option<Vec<NodeId>> {
    let mut next = links.to_vec();
    next.extend(selection.iter().filter(|s| !links.contains(s)));
    (next.len() > links.len()).then_some(next)
}

/// The layers an entry links to, with unlinking, and a button linking the selection. Returns the
/// new list of links when one of them was used.
fn links_row(app: &mut FigApp, ui: &mut Ui, cx: &Cx, links: &[NodeId]) -> Option<Vec<NodeId>> {
    let mut next = None;
    if !links.is_empty() {
        ui.add_space(2.0);
        quiet(ui, tr("Layers"));
        if let Some(gone) = layer_links(app, ui, cx, links, true) {
            next = Some(links.iter().copied().filter(|l| *l != gone).collect());
        }
    }
    if let Some(linked) = with_selection(links, &cx.selection) {
        ui.add_space(2.0);
        if button(ui, tr("Link the selected layer")) {
            next = Some(linked);
        }
    }
    next
}

pub(super) fn assumptions(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Assumptions"),
        tr(
            "What the design takes to be true without knowing it. Untested is the honest default; supported or refuted needs to say where that comes from.",
        ),
    );
    let mut form = app.ui.context.form.take();
    let editing = |form: &Option<Form>, id: Option<&str>| form.as_ref().is_some_and(|f| f.kind == FormKind::Assumption(id.map(str::to_string)));
    for a in &cx.design.assumptions {
        entry(app, ui, &a.id, |app, ui| {
            if editing(&form, Some(&a.id)) {
                assumption_form(app, ui, cx, &mut form);
            } else {
                assumption(app, ui, cx, a, &mut form);
            }
        });
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.assumptions.is_empty() {
            quiet(ui, tr("No assumptions are written down yet."));
            ui.add_space(4.0);
        }
        if editing(&form, None) {
            assumption_form(app, ui, cx, &mut form);
        } else if button(ui, tr("Add an assumption")) {
            form = Some(Form::new(FormKind::Assumption(None)));
        }
    });
    app.ui.context.form = form;
}

fn assumption(app: &mut FigApp, ui: &mut Ui, cx: &Cx, a: &Assumption, form: &mut Option<Form>) {
    flow_row(ui, |ui| {
        quiet(ui, &a.id);
        chip(ui, status_label(a.status));
    });
    body(ui, &a.text);
    part(ui, tr("Where that comes from"), &a.evidence);
    if let Some(links) = links_row(app, ui, cx, &a.links) {
        act(app, &a.id, "design.assumption.set", json!({"id": a.id, "links": links}));
    }
    ui.add_space(4.0);
    flow_row(ui, |ui| {
        if button(ui, tr("Edit")) {
            let at = STATUSES.iter().position(|(s, ..)| *s == a.status).unwrap_or(0);
            let mut f = Form::new(FormKind::Assumption(Some(a.id.clone()))).with_text(0, &a.text).with_text(1, &a.evidence);
            f.pick[0] = at;
            *form = Some(f);
        }
        if button(ui, tr("Remove")) {
            act(app, &a.id, "design.assumption.remove", json!({"id": a.id}));
        }
    });
    refusal_for(app, ui, &a.id);
}

fn assumption_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Assumption(id) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let [statement, evidence, ..] = &mut f.text;
    draft(ui, cx.id(("assumption", &id, "text")), tr("What is being assumed?"), statement, 2, focus);
    quiet(ui, tr("What is known about it?"));
    let labels: Vec<&str> = STATUSES.iter().map(|(_, _, label)| tr(label)).collect();
    if let Some(i) = choice(ui, &labels, f.pick[0]) {
        f.pick[0] = i;
    }
    ui.add_space(4.0);
    draft(ui, cx.id(("assumption", &id, "evidence")), tr("Where does that come from? (a study, analytics, a conversation)"), evidence, 2, false);
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let status = STATUSES.get(f.pick[0]).map_or("untested", |(_, id, _)| id);
            let mut params = json!({"text": f.text(0), "status": status, "evidence": f.text(1)});
            let cmd = match &id {
                Some(id) => {
                    params["id"] = json!(id);
                    "design.assumption.set"
                }
                None => "design.assumption.add",
            };
            submit(app, slot, cmd, params);
        }
    }
}

/// Run a form's command: the form closes when it is accepted and shows the refusal when not.
pub(super) fn submit(app: &mut FigApp, slot: &mut Option<Form>, cmd: &str, params: Value) -> Option<Value> {
    match app.try_run(cmd, params) {
        Ok(v) => {
            *slot = None;
            app.ui.context.error = None;
            Some(v)
        }
        Err(e) => {
            if let Some(f) = slot.as_mut() {
                f.error = Some(e);
            }
            None
        }
    }
}

pub(super) fn decisions(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Decisions"),
        tr("What was chosen, why, and what else was considered. An AI agent's decision is a proposal until a person accepts it."),
    );
    let mut form = app.ui.context.form.take();
    let editing = |form: &Option<Form>, id: Option<&str>| form.as_ref().is_some_and(|f| f.kind == FormKind::Decision(id.map(str::to_string)));
    for d in &cx.design.decisions {
        entry(app, ui, &d.id, |app, ui| {
            if editing(&form, Some(&d.id)) {
                decision_form(app, ui, cx, &mut form);
            } else {
                decision(app, ui, cx, d, &mut form);
            }
        });
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.decisions.is_empty() {
            quiet(ui, tr("No decisions are written down yet."));
            ui.add_space(4.0);
        }
        if editing(&form, None) {
            decision_form(app, ui, cx, &mut form);
        } else if button(ui, tr("Add a decision")) {
            form = Some(decision_draft(cx, None));
        }
    });
    app.ui.context.form = form;
}

/// A decision's form, filled from `d` (None: a new one): every assumption is offered to rest on.
fn decision_draft(cx: &Cx, d: Option<&Decision>) -> Form {
    let mut f = Form::new(FormKind::Decision(d.map(|d| d.id.clone())));
    if let Some(d) = d {
        f = f.with_text(0, &d.title).with_text(1, &d.rationale).with_text(2, &d.rejected);
    }
    f.checks = cx.design.assumptions.iter().map(|a| (a.id.clone(), d.is_some_and(|d| d.assumptions.contains(&a.id)))).collect();
    f
}

fn decision(app: &mut FigApp, ui: &mut Ui, cx: &Cx, d: &Decision, form: &mut Option<Form>) {
    let proposed = d.status == DecisionStatus::Proposed;
    flow_row(ui, |ui| {
        quiet(ui, &d.id);
        chip(ui, if proposed { tr("Proposed") } else { tr("Accepted") });
        if !d.author.is_empty() {
            quiet(ui, &trf("by {name}", &[("name", &d.author)]));
        }
    });
    strong(ui, &d.title);
    part(ui, tr("Why"), &d.rationale);
    part(ui, tr("What else was considered"), &d.rejected);
    if !d.assumptions.is_empty() {
        ui.add_space(2.0);
        flow_row(ui, |ui| {
            quiet(ui, tr("Rests on"));
            for a in &d.assumptions {
                if link(ui, a).clicked() {
                    go_to_entry(app, a);
                }
            }
        });
    }
    if let Some(links) = links_row(app, ui, cx, &d.links) {
        act(app, &d.id, "design.decision.set", json!({"id": d.id, "links": links}));
    }
    // A decision taken before the brief last changed may no longer follow from it.
    let stale = d.brief_revision < cx.design.brief_revision;
    if stale {
        ui.add_space(2.0);
        quiet(ui, tr("Made under an earlier brief: the brief has changed since."));
    }
    ui.add_space(4.0);
    flow_row(ui, |ui| {
        if proposed && widgets::button(ui, tr("Accept"), true, true).clicked() {
            act(app, &d.id, "design.decision.accept", json!({"id": d.id}));
        }
        if !proposed && stale && button(ui, tr("It still stands")) {
            act(app, &d.id, "design.decision.accept", json!({"id": d.id}));
        }
        if button(ui, tr("Edit")) {
            *form = Some(decision_draft(cx, Some(d)));
        }
        if button(ui, tr("Remove")) {
            act(app, &d.id, "design.decision.remove", json!({"id": d.id}));
        }
    });
    refusal_for(app, ui, &d.id);
}

fn decision_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Decision(id) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let [title, rationale, rejected, ..] = &mut f.text;
    draft(ui, cx.id(("decision", &id, "title")), tr("What was decided?"), title, 1, focus);
    draft(ui, cx.id(("decision", &id, "rationale")), tr("Why?"), rationale, 3, false);
    draft(ui, cx.id(("decision", &id, "rejected")), tr("What else was considered, and why did it lose?"), rejected, 2, false);
    if !f.checks.is_empty() {
        quiet(ui, tr("Which assumptions does it rest on?"));
        for (a, on) in &mut f.checks {
            if widgets::checkbox(ui, *on, a) {
                *on = !*on;
            }
            // The statement under its id, wrapped: a checkbox's own label is one line.
            if let Some(x) = cx.design.assumptions.iter().find(|x| x.id == *a) {
                quiet(ui, &x.text);
            }
        }
        ui.add_space(4.0);
    }
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let rests_on: Vec<&String> = f.checks.iter().filter(|(_, on)| *on).map(|(a, _)| a).collect();
            let mut params = json!({"title": f.text(0), "rationale": f.text(1), "rejected": f.text(2), "assumptions": rests_on});
            let cmd = match &id {
                Some(id) => {
                    params["id"] = json!(id);
                    "design.decision.set"
                }
                None => "design.decision.add",
            };
            submit(app, slot, cmd, params);
        }
    }
}
