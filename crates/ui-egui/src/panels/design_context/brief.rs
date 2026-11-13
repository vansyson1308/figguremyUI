//! The brief as plain questions. Every answer is optional and none is ever marked as missing: a
//! brief is something to come back to, not a form to complete. An answer is written to the file
//! when its field loses the keyboard (`design.brief.set` with that one field), so each answer is
//! one undo step.

use egui::Ui;
use figcraft_doc::design::Brief;
use serde_json::json;

use super::{Cx, act, pad, quiet, refusal_for, stage, strong};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trn};
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
        if let Some(text) = widgets::multiline_field(ui, cx.id(("brief", field)), answer(brief, field), "", 2) {
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
            if let Some(text) = widgets::multiline_field(ui, cx.id(("brief", field, i, items.len())), item, "", 1) {
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
