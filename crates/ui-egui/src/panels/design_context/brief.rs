//! The brief as plain questions. Every answer is optional and none is ever marked as missing: a
//! brief is something to come back to, not a form to complete. An answer is written to the file
//! when its field loses the keyboard (`design.brief.set` with that one field), so each answer is
//! one undo step. Below the answers, what the design must show (`design.required.*`), each with
//! the layers that are it, which the checks look for.

use egui::Ui;
use figcraft_doc::NodeId;
use figcraft_doc::design::{Brief, RequiredElement};
use serde_json::json;

use super::records::submit;
use super::{
    Cx, Form, FormEnd, FormKind, act, authorship, body, button, draft, entry, flow_row, form_end, heading, layer_links, pad, quiet, refusal_for,
    stage, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf, trn};
use crate::widgets;

/// The brief's text answers: the command's field and the question the panel asks for it.
const QUESTIONS: [(&str, &str); 7] = [
    ("audience", tr_noop("Who is it for?")),
    ("sponsor", tr_noop("Who is asking for it, and what do they want from it?")),
    ("job", tr_noop("What do they need to get done?")),
    ("context", tr_noop("Where and how will they use it?")),
    ("content", tr_noop("What must it say or offer?")),
    ("format", tr_noop("What form does it take, and where do people meet it?")),
    ("tone", tr_noop("How should it sound?")),
];

/// The brief's list answers.
const LISTS: [(&str, &str); 2] = [("constraints", tr_noop("What can't change?")), ("success", tr_noop("How would we know it works?"))];

fn answer<'a>(brief: &'a Brief, field: &str) -> &'a str {
    match field {
        "audience" => &brief.audience,
        "sponsor" => &brief.sponsor,
        "job" => &brief.job,
        "context" => &brief.context,
        "content" => &brief.content,
        "format" => &brief.format,
        _ => &brief.tone,
    }
}

fn entries<'a>(brief: &'a Brief, field: &str) -> &'a [String] {
    if field == "constraints" { &brief.constraints } else { &brief.success }
}

pub(super) fn show(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    let brief = &cx.design.brief;
    ui.add_space(10.0);
    stage(app, ui, cx);
    ui.add_space(10.0);
    widgets::separator(ui);
    ui.add_space(10.0);
    pad(ui, |ui| quiet(ui, tr("Answer what you know. Everything here is optional, and any of it can change as the design does.")));
    for (field, question) in QUESTIONS {
        ui.add_space(10.0);
        pad(ui, |ui| strong(ui, tr(question)));
        ui.add_space(2.0);
        if let Some(text) = widgets::multiline_field_named(ui, cx.id(("brief", field)), answer(brief, field), "", 2, tr(question)) {
            act(app, "brief", "design.brief.set", json!({field: text}));
        }
    }
    for (field, question) in LISTS {
        ui.add_space(10.0);
        pad(ui, |ui| strong(ui, tr(question)));
        ui.add_space(2.0);
        let items = entries(brief, field);
        // Each entry is a field of its own; emptying one takes it off the list.
        let mut changed: Option<Vec<String>> = None;
        for (i, item) in items.iter().enumerate() {
            if let Some(text) = widgets::multiline_field_named(ui, cx.id(("brief", field, i, items.len())), item, "", 1, tr(question)) {
                let mut next = items.to_vec();
                match (text.trim().is_empty(), next.get_mut(i)) {
                    (true, _) => {
                        next.remove(i);
                    }
                    (false, Some(slot)) => *slot = text,
                    (false, None) => {}
                }
                changed = Some(next);
            }
            ui.add_space(2.0);
        }
        if let Some(text) = widgets::multiline_field(ui, cx.id(("brief", field, "new", items.len())), "", tr("Add one"), 1)
            && !text.trim().is_empty()
        {
            let mut next = items.to_vec();
            next.push(text);
            changed = Some(next);
        }
        if let Some(next) = changed {
            act(app, "brief", "design.brief.set", json!({field: next}));
        }
    }
    required(app, ui, cx);
    pad(ui, |ui| {
        refusal_for(app, ui, "brief");
        if cx.design.brief_revision > 0 {
            ui.add_space(10.0);
            quiet(
                ui,
                &trn(
                    "The brief has changed {n} times. Decisions remember which version they were made under.",
                    cx.design.brief_revision as usize,
                    &[],
                ),
            );
        }
    });
}

/// What the design must show: each element with the layers that are it and the screens it must
/// be on, and a form to add one.
fn required(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("What must it show?"),
        tr("Things the design has to show, such as consent, a price or a permission. Link the layers that are each one, so the checks can find it."),
    );
    let mut form = app.ui.context.form.take();
    let editing = |form: &Option<Form>, id: Option<&str>| form.as_ref().is_some_and(|f| f.kind == FormKind::Required(id.map(str::to_string)));
    for r in &cx.design.brief.required {
        entry(app, ui, &r.id, |app, ui| {
            if editing(&form, Some(&r.id)) {
                required_form(app, ui, cx, &mut form);
            } else {
                element(app, ui, cx, r, &mut form);
            }
        });
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.brief.required.is_empty() {
            quiet(ui, tr("Nothing is listed yet."));
            ui.add_space(4.0);
        }
        if editing(&form, None) {
            required_form(app, ui, cx, &mut form);
        } else if button(ui, tr("Add something it must show")) {
            let mut f = Form::new(FormKind::Required(None));
            f.checks = vec![("link".into(), !cx.selection.is_empty()), ("screens".into(), false)];
            form = Some(f);
        }
    });
    app.ui.context.form = form;
}

fn element(app: &mut FigApp, ui: &mut Ui, cx: &Cx, r: &RequiredElement, form: &mut Option<Form>) {
    flow_row(ui, |ui| {
        quiet(ui, &r.id);
        authorship(ui, &r.author, &r.accepted_by);
    });
    body(ui, &r.text);
    if !r.screens.is_empty() {
        ui.add_space(2.0);
        quiet(ui, tr("On these screens"));
        layer_links(app, ui, cx, &r.screens, false);
    }
    ui.add_space(2.0);
    if r.nodes.is_empty() {
        quiet(ui, tr("No layer is linked, so the checks can't find it yet."));
    } else {
        quiet(ui, tr("The layers that are it"));
        if let Some(gone) = layer_links(app, ui, cx, &r.nodes, true) {
            let next: Vec<NodeId> = r.nodes.iter().copied().filter(|n| *n != gone).collect();
            act(app, &r.id, "design.required.set", json!({"id": r.id, "nodes": next}));
        }
        // Hidden in the file, shown in a declared state: a fact, not a problem.
        for (id, node, state) in figcraft_engine::design::shown_in_states(&cx.doc, figcraft_engine::design::MAX_FACTS).0 {
            if id == r.id {
                quiet(ui, &trf("{layer} shows in {state}", &[("layer", &cx.name(node).unwrap_or_default()), ("state", &state)]));
            }
        }
    }
    let mut more = r.nodes.clone();
    more.extend(cx.selection.iter().filter(|s| !r.nodes.contains(s)));
    ui.add_space(4.0);
    flow_row(ui, |ui| {
        if !r.status.is_accepted() && widgets::button(ui, tr("Accept"), true, true).clicked() {
            act(app, &r.id, "design.required.accept", json!({"id": r.id}));
        }
        if more.len() > r.nodes.len() && button(ui, tr("Link the selected layer")) {
            act(app, &r.id, "design.required.set", json!({"id": r.id, "nodes": more}));
        }
        if button(ui, tr("Edit")) {
            *form = Some(Form::new(FormKind::Required(Some(r.id.clone()))).with_text(0, &r.text));
        }
        if button(ui, tr("Remove")) {
            act(app, &r.id, "design.required.remove", json!({"id": r.id}));
        }
    });
    refusal_for(app, ui, &r.id);
}

fn required_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Required(id) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let [what, ..] = &mut f.text;
    draft(ui, cx.id(("required", &id, "text")), tr("What must the design show, and where?"), what, 2, focus);
    // A new element can start linked to what is selected, and limited to the screens it is on.
    let screens: Vec<NodeId> = {
        let mut out: Vec<NodeId> = Vec::new();
        for top in cx.selection.iter().filter_map(|s| cx.doc.top_level_frame(*s)) {
            if !out.contains(&top) {
                out.push(top);
            }
        }
        out
    };
    if id.is_none() && !cx.selection.is_empty() {
        let labels = [tr("The selected layers are it"), tr("Only on their screens")];
        for ((_, on), label) in f.checks.iter_mut().zip(labels) {
            if widgets::checkbox(ui, *on, label) {
                *on = !*on;
            }
        }
    }
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let on = |i: usize| f.checks.get(i).is_some_and(|(_, on)| *on);
            let params = match &id {
                Some(id) => json!({"id": id, "text": f.text(0)}),
                None => {
                    let mut p = json!({"text": f.text(0)});
                    if on(0) {
                        p["nodes"] = json!(cx.selection);
                    }
                    if on(1) {
                        p["screens"] = json!(screens);
                    }
                    p
                }
            };
            let cmd = if id.is_some() { "design.required.set" } else { "design.required.add" };
            submit(app, slot, cmd, params);
        }
    }
}
