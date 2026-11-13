//! Alternatives: copies of a screen reworked side by side, each under a stated constraint. They
//! are compared by what can be counted, never ranked, and one is chosen by a person who says
//! why; the choice becomes a decision. Nothing is deleted from here: forgetting an alternative
//! keeps its layer.

use egui::Ui;
use figcraft_doc::design::{Alternative, AlternativeSet};
use figcraft_doc::{NodeId, NodeKind};
use serde_json::{Value, json};

use super::records::submit;
use super::{
    Cx, Form, FormEnd, FormKind, Ran, act, author_name, body, button, chip, draft, entry, flow_row, form_end, go_to, go_to_entry, heading, link, pad,
    part, primary, quiet, refusal_for, strong,
};
use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf};
use crate::theme::{Tokens, regular};
use crate::widgets;

/// The counts `design.alternative.compare` gives for each frame, as the table names them.
const METRICS: [(&str, &str); 12] = [
    ("width", tr_noop("Width")),
    ("height", tr_noop("Height")),
    ("layers", tr_noop("Layers")),
    ("depth", tr_noop("Nesting depth")),
    ("textLayers", tr_noop("Text layers")),
    ("words", tr_noop("Words")),
    ("characters", tr_noop("Characters")),
    ("interactive", tr_noop("Layers that react")),
    ("instances", tr_noop("Instances")),
    ("components", tr_noop("Components used")),
    ("images", tr_noop("Images")),
    ("density", tr_noop("Share of the frame covered")),
];

/// The ways two frames can differ, as `compare` reports them.
const ASPECTS: [(&str, &str); 7] = [
    ("structure", tr_noop("structure")),
    ("order", tr_noop("reading order")),
    ("content", tr_noop("text")),
    ("actions", tr_noop("what reacts")),
    ("size", tr_noop("size")),
    ("layout", tr_noop("layout")),
    ("paint", tr_noop("paint")),
];

pub(super) fn show(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    heading(
        ui,
        tr("Alternatives"),
        tr(
            "Copies of a screen reworked under a stated limit, kept beside the original. They are compared by facts, not ranked; choosing one records a decision.",
        ),
    );
    let mut form = app.ui.context.form.take();
    for set in &cx.design.alternatives {
        entry(app, ui, &set.id, |app, ui| alternative_set(app, ui, cx, set, &mut form));
    }
    ui.add_space(8.0);
    pad(ui, |ui| {
        if cx.design.alternatives.is_empty() {
            quiet(ui, tr("No alternatives are recorded in this file."));
            ui.add_space(4.0);
        }
        if matches!(form.as_ref().map(|f| &f.kind), Some(FormKind::Alternative(_))) {
            create_form(app, ui, cx, &mut form);
            return;
        }
        // A screen (a top-level frame) or a component can have alternatives; the engine says
        // which components can't.
        let can =
            |id: &NodeId| cx.doc.top_level_frame(*id) == Some(*id) || cx.doc.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. }));
        match cx.selection.first().copied().filter(|id| cx.selection.len() == 1 && can(id)) {
            Some(source) => {
                strong(ui, &format!("“{}”", cx.name(source).unwrap_or_default()));
                if button(ui, tr("Make an alternative of it")) {
                    form = Some(Form::new(FormKind::Alternative(source)));
                }
            }
            None => {
                quiet(ui, tr("Select a top-level frame or a component to make an alternative of it."));
            }
        }
    });
    app.ui.context.form = form;
}

/// A new alternative of the selected frame: a name and the limit it is made under are asked for;
/// the engine says what is missing.
fn create_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Alternative(source) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let name = cx.name(source).unwrap_or_default();
    strong(ui, &trf("An alternative of “{name}”", &[("name", &name)]));
    quiet(ui, tr("The frame is copied beside the original, which stays as it is."));
    ui.add_space(4.0);
    // A set made earlier for this frame keeps its question and limits.
    let first = !cx.design.alternatives.iter().any(|v| v.source == source);
    let [title, constraint, intent, question, constraints] = &mut f.text;
    draft(ui, cx.id("alternative name"), tr("What to call it"), title, 1, focus);
    draft(ui, cx.id("alternative constraint"), tr("The limit it is made under (“one thumb, no scrolling”)"), constraint, 2, false);
    draft(ui, cx.id("alternative intent"), tr("Who or what situation it serves best (optional)"), intent, 2, false);
    if first {
        draft(ui, cx.id("alternative question"), tr("What are the alternatives trying to find out? (optional)"), question, 2, false);
        draft(ui, cx.id("alternative constraints"), tr("Limits every alternative must respect, one a line (optional)"), constraints, 2, false);
    }
    match form_end(ui, f, tr("Make the alternative")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let mut params = json!({"source": source, "name": f.text(0), "constraint": f.text(1), "intent": f.text(2)});
            if first && !f.text(3).trim().is_empty() {
                params["question"] = json!(f.text(3));
            }
            if first && !f.lines(4).is_empty() {
                params["constraints"] = json!(f.lines(4));
            }
            submit(app, slot, "design.alternative.create", params);
        }
    }
}

/// A frame of a set as a link, or as missing.
fn frame_link(app: &mut FigApp, ui: &mut Ui, cx: &Cx, node: NodeId) {
    match cx.name(node) {
        Some(name) => {
            if link(ui, name).clicked() {
                go_to(app, node);
            }
        }
        None => {
            quiet(ui, &trf("missing layer ({id})", &[("id", &node)]));
        }
    }
}

fn alternative_set(app: &mut FigApp, ui: &mut Ui, cx: &Cx, set: &AlternativeSet, form: &mut Option<Form>) {
    quiet(ui, &set.id);
    if form.as_ref().is_some_and(|f| f.kind == FormKind::Set(set.id.clone())) {
        set_form(app, ui, cx, form);
    } else {
        if set.question.is_empty() {
            quiet(ui, tr("What the alternatives are trying to find out isn't written down."));
        } else {
            strong(ui, &set.question);
        }
        if !set.constraints.is_empty() {
            ui.add_space(2.0);
            quiet(ui, tr("Every alternative must respect"));
            for c in &set.constraints {
                body(ui, &format!("• {c}"));
            }
        }
    }
    ui.add_space(4.0);
    flow_row(ui, |ui| {
        quiet(ui, tr("Made from"));
        frame_link(app, ui, cx, set.source);
        if set.chosen == Some(set.source) {
            chip(ui, tr("Chosen"));
        }
        if set.proposal.as_ref().is_some_and(|p| p.node == set.source) {
            chip(ui, tr("Proposed"));
        }
    });
    for m in &set.members {
        ui.add_space(8.0);
        if form.as_ref().is_some_and(|f| f.kind == FormKind::Member(set.id.clone(), m.node)) {
            member_form(app, ui, cx, form);
        } else {
            member(app, ui, cx, set, m, form);
        }
    }
    ui.add_space(8.0);
    proposal(app, ui, cx, set);
    if form.as_ref().is_some_and(|f| matches!(&f.kind, FormKind::Choose(id, _) if *id == set.id)) {
        choose_form(app, ui, cx, set, form);
    }
    flow_row(ui, |ui| {
        if primary(ui, tr("Compare"))
            && let Some(result) = act(app, &set.id, "design.alternative.compare", json!({"set": set.id}))
        {
            app.ui.context.compare = Ran::new(app, result).map(|ran| (set.id.clone(), ran));
        }
        if set.chosen != Some(set.source) && button(ui, tr("Keep the original")) {
            *form = Some(Form::new(FormKind::Choose(set.id.clone(), set.source)));
        }
        if button(ui, tr("Edit the question")) {
            *form = Some(Form::new(FormKind::Set(set.id.clone())).with_text(0, &set.question).with_text(1, &set.constraints.join("\n")));
        }
        if widgets::button(ui, tr("Forget the set"), false, true).on_hover_text(tr("The layers stay in the file")).clicked() {
            act(app, &set.id, "design.alternative.remove", json!({"set": set.id}));
        }
    });
    if !set.decision.is_empty() {
        flow_row(ui, |ui| {
            quiet(ui, tr("The choice is recorded as"));
            if link(ui, &set.decision).clicked() {
                go_to_entry(app, &set.decision);
            }
        });
    }
    refusal_for(app, ui, &set.id);
    let shown = app.ui.context.compare.as_ref().filter(|(id, _)| *id == set.id).map(|(_, ran)| (ran.result.clone(), ran.stale(&cx.doc)));
    if let Some((result, stale)) = shown {
        ui.add_space(6.0);
        comparison(ui, cx, &result, stale);
    }
}

/// A choice an AI agent proposed for the set, waiting for a person: who proposes which frame and
/// why, and Accept (which accepts the proposed decision and so makes the choice). Choosing
/// another one by hand also settles it.
fn proposal(app: &mut FigApp, ui: &mut Ui, cx: &Cx, set: &AlternativeSet) {
    let Some(p) = &set.proposal else { return };
    let decision = cx.design.decisions.iter().find(|d| d.id == p.decision);
    let who = decision.map_or_else(|| tr("An AI agent").to_string(), |d| author_name(&d.author).to_string());
    let name = set.members.iter().find(|m| m.node == p.node).map(|m| m.name.clone()).or_else(|| {
        let n = cx.name(p.node)?;
        Some(if p.node == set.source { trf("{name} (the original)", &[("name", &n)]) } else { n.to_string() })
    });
    let name = name.unwrap_or_else(|| trf("missing layer ({id})", &[("id", &p.node)]));
    let rationale = decision.map_or("", |d| d.rationale.as_str());
    // (The member it proposes carries the "Proposed" mark; the sentence says the rest.)
    strong(ui, &trf("{agent} proposes “{member}”: {rationale}", &[("agent", &who), ("member", &name), ("rationale", &rationale)]));
    quiet(ui, tr("Waiting for you: nothing is chosen until you accept it or choose another."));
    ui.add_space(2.0);
    flow_row(ui, |ui| {
        if widgets::button(ui, tr("Accept"), true, true).clicked() {
            act(app, &set.id, "design.decision.accept", json!({"id": p.decision}));
        }
        if link(ui, &p.decision).on_hover_text(tr("The proposed decision")).clicked() {
            go_to_entry(app, &p.decision);
        }
    });
    ui.add_space(8.0);
}

fn member(app: &mut FigApp, ui: &mut Ui, cx: &Cx, set: &AlternativeSet, m: &Alternative, form: &mut Option<Form>) {
    flow_row(ui, |ui| {
        strong(ui, &m.name);
        if set.chosen == Some(m.node) {
            chip(ui, tr("Chosen"));
        }
        if set.proposal.as_ref().is_some_and(|p| p.node == m.node) {
            chip(ui, tr("Proposed"));
        }
    });
    flow_row(ui, |ui| {
        quiet(ui, tr("Layer"));
        frame_link(app, ui, cx, m.node);
    });
    part(ui, tr("Made under"), &m.constraint);
    part(ui, tr("Serves best"), &m.intent);
    part(ui, tr("Gives up"), &m.tradeoffs);
    ui.add_space(2.0);
    flow_row(ui, |ui| {
        if set.chosen != Some(m.node) && button(ui, tr("Choose")) {
            *form = Some(Form::new(FormKind::Choose(set.id.clone(), m.node)));
        }
        if button(ui, tr("Edit")) {
            let f = Form::new(FormKind::Member(set.id.clone(), m.node));
            *form = Some(f.with_text(0, &m.name).with_text(1, &m.constraint).with_text(2, &m.intent).with_text(3, &m.tradeoffs));
        }
        if widgets::button(ui, tr("Forget"), false, true).on_hover_text(tr("The layers stay in the file")).clicked() {
            act(app, &set.id, "design.alternative.remove", json!({"set": set.id, "node": m.node}));
        }
    });
}

fn member_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Member(set, node) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let [name, constraint, intent, tradeoffs, ..] = &mut f.text;
    draft(ui, cx.id(("member", node, "name")), tr("What to call it"), name, 1, focus);
    draft(ui, cx.id(("member", node, "constraint")), tr("The limit it is made under (“one thumb, no scrolling”)"), constraint, 2, false);
    draft(ui, cx.id(("member", node, "intent")), tr("Who or what situation it serves best (optional)"), intent, 2, false);
    draft(ui, cx.id(("member", node, "tradeoffs")), tr("What it gives up (optional)"), tradeoffs, 2, false);
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let params = json!({"set": set, "node": node, "name": f.text(0), "constraint": f.text(1), "intent": f.text(2), "tradeoffs": f.text(3)});
            submit(app, slot, "design.alternative.update", params);
        }
    }
}

fn set_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Set(set) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let [question, constraints, ..] = &mut f.text;
    draft(ui, cx.id(("set", &set, "question")), tr("What are the alternatives trying to find out?"), question, 2, focus);
    draft(ui, cx.id(("set", &set, "constraints")), tr("Limits every alternative must respect, one a line"), constraints, 3, false);
    match form_end(ui, f, tr("Save")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let params = json!({"set": set, "question": f.text(0), "constraints": f.lines(1)});
            submit(app, slot, "design.alternative.question", params);
        }
    }
}

fn choose_form(app: &mut FigApp, ui: &mut Ui, cx: &Cx, set: &AlternativeSet, slot: &mut Option<Form>) {
    let Some(f) = slot.as_mut() else { return };
    let FormKind::Choose(id, node) = f.kind.clone() else { return };
    let focus = std::mem::take(&mut f.focus);
    let name = set.members.iter().find(|m| m.node == node).map(|m| m.name.as_str()).or_else(|| cx.name(node)).unwrap_or_default();
    strong(ui, &trf("Choose “{name}”", &[("name", &name)]));
    quiet(ui, tr("Nothing is deleted. The choice is recorded as a decision with your reason."));
    let [why, ..] = &mut f.text;
    draft(ui, cx.id(("choose", &id, node)), tr("Why this one?"), why, 3, focus);
    match form_end(ui, f, tr("Record the choice")) {
        FormEnd::Open => {}
        FormEnd::Cancel => *slot = None,
        FormEnd::Submit => {
            let params = json!({"set": id, "node": node, "rationale": f.text(0)});
            submit(app, slot, "design.alternative.choose", params);
        }
    }
}

/// What `design.alternative.compare` answered: the counts side by side, where each pair differs,
/// and what the engine warns about. Facts only; no column is better than another.
fn comparison(ui: &mut Ui, cx: &Cx, result: &Value, stale: bool) {
    let t = Tokens::get(ui.ctx());
    if stale {
        quiet(ui, tr("The file has changed since this comparison: the numbers are from before."));
        ui.add_space(4.0);
    }
    let members = result["members"].as_array().map_or(&[][..], Vec::as_slice);
    let name = |m: &Value| -> String {
        let n = m["name"].as_str().unwrap_or_default();
        if m["isSource"].as_bool() == Some(true) { trf("{name} (the original)", &[("name", &n)]) } else { n.to_string() }
    };
    let by_node = |id: &Value| members.iter().find(|m| m["node"] == *id).map(&name).unwrap_or_default();
    // The table scrolls sideways when the frames don't fit the panel: nothing is cut off.
    egui::ScrollArea::horizontal().id_salt(cx.id(("compare", result["set"].as_str().unwrap_or_default()))).auto_shrink([false, true]).show(
        ui,
        |ui| {
            egui::Grid::new(cx.id("compare grid")).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                let cell = |ui: &mut Ui, text: &str, color| ui.label(egui::RichText::new(text).font(regular(11.0)).color(color));
                // Short heads keep two or three frames in view: the original is named above.
                cell(ui, "", t.text);
                for m in members {
                    let head = if m["isSource"].as_bool() == Some(true) {
                        tr("Original").to_string()
                    } else {
                        m["name"].as_str().unwrap_or_default().to_string()
                    };
                    cell(ui, &head, t.text);
                }
                ui.end_row();
                for (key, label) in METRICS {
                    cell(ui, tr(label), t.text_secondary);
                    for m in members {
                        let v = &m["metrics"][key];
                        let shown = match (key, v.as_f64()) {
                            (_, None) => "—".to_string(),
                            ("density", Some(d)) => format!("{:.0}%", d * 100.0),
                            (_, Some(n)) => widgets::fmt_num(n),
                        };
                        cell(ui, &shown, t.text);
                    }
                    ui.end_row();
                }
            });
            // Room for the bar that scrolls it sideways, under the last row.
            ui.add_space(12.0);
        },
    );
    ui.add_space(6.0);
    for m in members {
        let texts: Vec<&str> = m["readingOrder"].as_array().into_iter().flatten().filter_map(Value::as_str).take(6).collect();
        if !texts.is_empty() {
            part(ui, &trf("{name} reads, in order", &[("name", &name(m))]), &texts.join(" · "));
        }
    }
    let differences = result["differences"].as_array().map_or(&[][..], Vec::as_slice);
    if !differences.is_empty() {
        ui.add_space(6.0);
        quiet(ui, tr("Where they differ"));
    }
    for d in differences {
        let pair = trf("{a} and {b}", &[("a", &by_node(&d["a"])), ("b", &by_node(&d["b"]))]);
        let differs: Vec<&str> = ASPECTS.iter().filter(|(key, _)| d[*key].as_bool() == Some(true)).map(|(_, label)| tr(label)).collect();
        let what = if d["identical"].as_bool() == Some(true) {
            tr("identical").to_string()
        } else if d["paintOnly"].as_bool() == Some(true) {
            tr("paint only").to_string()
        } else {
            differs.join(", ")
        };
        body(ui, &format!("{pair}: {what}"));
    }
    for w in result["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        ui.add_space(2.0);
        body(ui, w);
    }
}
