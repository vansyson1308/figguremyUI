//! Findings: what the checks measure in the file, and the findings stored in it. The three kinds
//! are shown apart and settled differently: a check was measured by a rule and only running the
//! rule again settles it; a judgement is somebody's opinion; feedback is what a person said.
//! Nothing is counted into a score.

use egui::{CornerRadius, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::NodeId;
use figcraft_doc::design::{Finding, FindingKind, FindingStatus, Severity};
use serde_json::{Value, json};

use super::records::submit;
use super::{
    Cx, Form, FormEnd, FormKind, Ran, SEVERITIES, act, body, button, chip, choice, draft, entry, flow_row, form_end, heading, layer_links, pad, part,
    primary, quiet, refusal_for, severity_label, stage, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::theme::Tokens;
use crate::widgets;

/// What a finding's action asks for in words before it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// A fix is claimed: what was changed.
    Address,
    /// A person confirms it is met: what they looked at.
    Verify,
    /// It stays as it is on purpose: why.
    Accept,
}

const KINDS: [(FindingKind, &str, &str, &str); 3] = [
    (FindingKind::Check, "check", tr_noop("Check|finding"), tr_noop("measured from the file by a rule; running the rule again settles it")),
    (
        FindingKind::Judgement,
        "judgement",
        tr_noop("Judgement"),
        tr_noop("the opinion of a reviewer or an AI agent; a person says when it is settled"),
    ),
    (FindingKind::Feedback, "feedback", tr_noop("Feedback"), tr_noop("what a person said about the design; that person's word settles it")),
];

const STATUSES: [(FindingStatus, &str); 4] = [
    (FindingStatus::Open, tr_noop("Open|finding")),
    (FindingStatus::Addressed, tr_noop("Addressed, not confirmed yet")),
    (FindingStatus::Verified, tr_noop("Confirmed")),
    (FindingStatus::Accepted, tr_noop("Accepted as it is")),
];

const SEVERITY_ORDER: [(Severity, &str); 4] =
    [(Severity::Blocker, "blocker"), (Severity::Major, "major"), (Severity::Minor, "minor"), (Severity::Note, "note")];

fn kind_label(kind: FindingKind) -> &'static str {
    KINDS.iter().find(|(k, ..)| *k == kind).map_or("", |(_, _, label, _)| tr(label))
}

/// The mark of a kind, drawn before its name so the three can be told apart at a glance without
/// relying on colour: a filled square (measured), a ring (an opinion), a speech mark (said).
fn kind_mark(ui: &mut Ui, kind: FindingKind) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 18.0), Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    match kind {
        FindingKind::Check => {
            p.rect_filled(egui::Rect::from_center_size(c, vec2(9.0, 9.0)), CornerRadius::same(1), t.text);
        }
        FindingKind::Judgement => {
            p.circle_stroke(c, 4.5, Stroke::new(1.5, t.text));
        }
        FindingKind::Feedback => {
            let bubble = egui::Rect::from_center_size(pos2(c.x, c.y - 1.0), vec2(11.0, 8.0));
            p.rect_stroke(bubble, CornerRadius::same(3), Stroke::new(1.5, t.text), StrokeKind::Inside);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pos2(bubble.min.x + 2.0, bubble.max.y - 0.5),
                    pos2(bubble.min.x + 6.0, bubble.max.y - 0.5),
                    pos2(bubble.min.x + 2.0, bubble.max.y + 3.5),
                ],
                t.text,
                Stroke::NONE,
            ));
        }
    }
}

/// A kind's mark and name.
fn kind_tag(ui: &mut Ui, kind: FindingKind) {
    kind_mark(ui, kind);
    chip(ui, kind_label(kind));
}

/// What a rule found, as the commands report it (`design.check`'s findings, `proto.check`'s
/// problems): how much it matters, the rule, what it says, what it measured, what would help and
/// the layers it is about.
pub(super) fn found(app: &mut FigApp, ui: &mut Ui, cx: &Cx, f: &Value) {
    let text = |key: &str| f[key].as_str().unwrap_or_default();
    flow_row(ui, |ui| {
        kind_tag(ui, FindingKind::Check);
        chip(ui, severity_label(text("severity")));
        quiet(ui, text("rule"));
    });
    body(ui, text("message"));
    part(ui, tr("What was measured"), text("evidence"));
    part(ui, tr("What would help"), text("suggestion"));
    let nodes: Vec<NodeId> = f["nodes"].as_array().into_iter().flatten().filter_map(figcraft_engine::params::id_of).collect();
    if !nodes.is_empty() {
        ui.add_space(2.0);
        quiet(ui, tr("Layers"));
        layer_links(app, ui, cx, &nodes, false);
    }
}

pub(super) fn show(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(ui, tr("Findings"), tr("Three kinds, kept apart:"));
    pad(ui, |ui| {
        for (kind, _, label, about) in KINDS {
            // The mark, then the sentence wrapping beside it.
            ui.horizontal_top(|ui| {
                kind_mark(ui, kind);
                quiet(ui, &format!("{}: {}", tr(label), tr(about)));
            });
        }
    });
    checks(app, ui, cx);
    stored(app, ui, cx);
}

/// Running the checks: what they find now, which rules didn't run, and storing the results.
fn checks(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Checks"),
        tr("Measure the file as it is now. Which rules run depends on how far along the design is; the rest are listed as not run."),
    );
    stage(app, ui, cx);
    ui.add_space(8.0);
    pad(ui, |ui| {
        flow_row(ui, |ui| {
            if primary(ui, tr("Run the checks"))
                && let Some(result) = act(app, "checks", "design.check", json!({}))
            {
                app.ui.context.checks = Ran::new(app, result);
            }
            if button(ui, tr("Store what they find"))
                && let Some(result) = act(app, "checks", "design.check.record", json!({}))
            {
                app.ui.context.recorded = Ran::new(app, result);
            }
            if cx.design.findings.iter().any(|f| f.kind == FindingKind::Check) && button(ui, tr("Re-check the stored checks")) {
                act(app, "checks", "design.finding.recheck", json!({}));
            }
        });
        refusal_for(app, ui, "checks");
        if let Some(ran) = app.ui.context.recorded.as_ref().filter(|ran| !ran.stale(&cx.doc)) {
            let n = |key: &str| ran.result[key].as_array().map_or(0, Vec::len);
            quiet(ui, &trf("Stored {added} new; {existing} were stored already.", &[("added", &n("added")), ("existing", &n("existing"))]));
        }
    });
    let Some((result, stale)) = app.ui.context.checks.as_ref().map(|ran| (ran.result.clone(), ran.stale(&cx.doc))) else { return };
    let list = result["findings"].as_array().map_or(&[][..], Vec::as_slice);
    ui.add_space(6.0);
    pad(ui, |ui| {
        if stale {
            quiet(ui, tr("The file has changed since the checks ran: what follows is from before."));
            ui.add_space(4.0);
        }
        let rules = result["rules"].as_array().map_or(0, Vec::len);
        body(ui, &trf("Rules run: {rules}. Reported: {found}.", &[("rules", &rules), ("found", &list.len())]));
        // The rules that didn't run are said, never left out silently.
        let skipped: Vec<&str> = result["skipped"].as_array().into_iter().flatten().filter_map(|s| s["id"].as_str()).collect();
        if !skipped.is_empty() {
            ui.add_space(2.0);
            quiet(ui, &trf("Not run at this stage ({n}): {rules}", &[("n", &skipped.len()), ("rules", &skipped.join(", "))]));
        }
    });
    for (i, f) in list.iter().enumerate() {
        entry(app, ui, &format!("check result {i}"), |app, ui| found(app, ui, cx, f));
    }
}

/// The findings stored in the file, by where they stand, then by how much they matter.
fn stored(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(ui, tr("Stored findings"), "");
    let mut form = app.ui.context.form.take();
    let all = &cx.design.findings;
    if all.is_empty() {
        pad(ui, |ui| quiet(ui, tr("No findings are stored in this file.")));
    }
    for (status, status_text) in STATUSES {
        for (severity, severity_id) in SEVERITY_ORDER {
            let group: Vec<&Finding> = all.iter().filter(|f| f.status == status && f.severity == severity).collect();
            if group.is_empty() {
                continue;
            }
            ui.add_space(8.0);
            pad(ui, |ui| strong(ui, &format!("{} · {}", tr(status_text), severity_label(severity_id))));
            for f in group {
                entry(app, ui, &f.id, |app, ui| finding(app, ui, cx, f, &mut form));
            }
        }
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if form.as_ref().is_some_and(|f| f.kind == FormKind::Finding) {
            add_form(app, ui, cx, &mut form);
        } else if button(ui, tr("Add a judgement or feedback")) {
            let mut f = Form::new(FormKind::Finding);
            f.pick[1] = 1;
            f.checks = vec![(String::new(), !cx.selection.is_empty())];
            form = Some(f);
        }
    });
    app.ui.context.form = form;
}

fn finding(app: &mut FigApp, ui: &mut Ui, cx: &Cx, f: &Finding, form: &mut Option<Form>) {
    let check = f.kind == FindingKind::Check;
    flow_row(ui, |ui| {
        kind_tag(ui, f.kind);
        quiet(ui, &f.id);
        quiet(ui, &f.rule);
    });
    body(ui, &f.message);
    part(ui, if check { tr("What was measured") } else { tr("What was observed") }, &f.evidence);
    part(ui, tr("What would help"), &f.suggestion);
    part(ui, tr("Raised by"), &f.source);
    match f.status {
        FindingStatus::Accepted => part(ui, tr("Why it stays as it is"), &f.resolution),
        _ => part(ui, tr("What was changed"), &f.resolution),
    }
    if !f.verified_by.is_empty() {
        part(ui, &trf("Confirmed by {name}, who looked at", &[("name", &f.verified_by)]), &f.verification);
    }
    if let Some(r) = &f.recheck {
        part(ui, if r.passed { tr("Last re-check: no longer reported") } else { tr("Last re-check: still reported") }, &r.detail);
    }
    if !f.nodes.is_empty() {
        ui.add_space(2.0);
        quiet(ui, tr("Layers"));
        layer_links(app, ui, cx, &f.nodes, false);
    }
    ui.add_space(4.0);
    if let Some(FormKind::FindingAct(id, _)) = form.as_ref().map(|x| &x.kind)
        && *id == f.id
    {
        act_form(app, ui, cx, form);
        return;
    }
    let ask = |what: Act| Some(Form::new(FormKind::FindingAct(f.id.clone(), what)));
    flow_row(ui, |ui| {
        if check && button(ui, tr("Re-check")) {
            act(app, &f.id, "design.finding.recheck", json!({"id": f.id}));
        }
        if f.status == FindingStatus::Open && button(ui, tr("Mark as addressed")) {
            *form = ask(Act::Address);
        }
        if !check && f.status != FindingStatus::Verified && button(ui, tr("Confirm it is met")) {
            *form = ask(Act::Verify);
        }
        if f.status != FindingStatus::Accepted && button(ui, tr("Accept as it is")) {
            *form = ask(Act::Accept);
        }
        if f.status != FindingStatus::Open && button(ui, tr("Reopen")) {
            act(app, &f.id, "design.finding.reopen", json!({"id": f.id}));
        }
        if button(ui, tr("Remove")) {
            act(app, &f.id, "design.finding.remove", json!({"id": f.id}));
        }
    });
    refusal_for(app, ui, &f.id);
}

/// The one text an action on a finding needs.
fn act_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::FindingAct(id, what) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let (label, cmd, key, done) = match what {
        Act::Address => (tr("What was changed?"), "design.finding.address", "resolution", tr("Mark as addressed")),
        Act::Verify => (tr("What did you look at to confirm it is met?"), "design.finding.verify", "evidence", tr("Confirm it is met")),
        Act::Accept => (tr("Why does it stay as it is?"), "design.finding.accept", "reason", tr("Accept as it is")),
    };
    let [text, ..] = &mut f.text;
    draft(ui, cx.id(("finding act", &id, key)), label, text, 2, focus);
    match form_end(ui, f, done) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let params = json!({"id": id, key: f.text(0)});
            submit(app, slot, cmd, params);
        }
    }
}

/// A judgement or feedback written by hand.
fn add_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let focus = std::mem::take(&mut f.focus);
    quiet(ui, tr("What kind of finding is it?"));
    let kinds: Vec<&str> = KINDS.iter().skip(1).map(|(_, _, label, _)| tr(label)).collect();
    if let Some(i) = choice(ui, &kinds, f.pick[0]) {
        f.pick[0] = i;
    }
    ui.add_space(4.0);
    quiet(ui, tr("How much does it matter?"));
    let severities: Vec<&str> = SEVERITIES.iter().map(|(_, label)| tr(label)).collect();
    if let Some(i) = choice(ui, &severities, f.pick[1]) {
        f.pick[1] = i;
    }
    ui.add_space(4.0);
    let feedback = f.pick[0] == 1;
    let [label, message, evidence, suggestion, source] = &mut f.text;
    draft(ui, cx.id("finding label"), tr("A short label for it"), label, 1, focus);
    draft(ui, cx.id("finding message"), tr("What is the problem?"), message, 2, false);
    let observed = if feedback { tr("What did the person say, in their words?") } else { tr("What was observed?") };
    draft(ui, cx.id("finding evidence"), observed, evidence, 2, false);
    draft(ui, cx.id("finding suggestion"), tr("What would help? (optional)"), suggestion, 1, false);
    let who = if feedback { tr("Who said it?") } else { tr("Whose judgement is it? (left empty: yours)") };
    draft(ui, cx.id("finding source"), who, source, 1, false);
    if !cx.selection.is_empty()
        && let Some((_, on)) = f.checks.first_mut()
        && widgets::checkbox(ui, *on, tr("It is about the selected layers"))
    {
        *on = !*on;
    }
    match form_end(ui, f, tr("Add")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let kind = KINDS.get(f.pick[0] + 1).map_or("judgement", |(_, id, ..)| id);
            let severity = SEVERITIES.get(f.pick[1]).map_or("major", |(id, _)| id);
            let about_selection = f.checks.first().is_some_and(|(_, on)| *on);
            let nodes: &[NodeId] = if about_selection { &cx.selection } else { &[] };
            let mut params = json!({"kind": kind, "severity": severity, "rule": f.text(0), "message": f.text(1), "evidence": f.text(2), "suggestion": f.text(3), "nodes": nodes});
            // Left empty, the engine names who is asking (the user).
            if !f.text(4).trim().is_empty() {
                params["source"] = json!(f.text(4));
            }
            submit(app, slot, "design.finding.add", params);
        }
    }
}
