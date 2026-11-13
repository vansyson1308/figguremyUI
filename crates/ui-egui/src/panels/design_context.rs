//! The Design context panel, shown in the left sidebar in place of Pages and Layers: what the
//! file records about why it is the way it is and how to tell whether it works. The brief as
//! plain questions, the assumptions it rests on, the decisions taken, the scenarios that walk the
//! prototype and what the flow check finds, the findings from checks and reviews, and the
//! alternatives explored for a screen.
//!
//! Everything shown is read from the document each frame, so what an AI agent records through
//! the control channel appears as it is written and undo and redo show at once. Every edit runs
//! a `design.*` or `proto.*` command; what the engine refuses is shown, in its words, next to
//! what was asked. Nothing here grades the file: no score, no percentage, no mark for a brief
//! with empty answers.

use std::collections::BTreeSet;
use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Id, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::design::{DecisionStatus, DesignContext, FindingStatus, Stage};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::FigApp;
use crate::i18n::{tr, tr_noop, trf, trn};
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, PAD};

mod alternatives;
mod brief;
mod findings;
mod flow;
mod records;

/// The narrowest the left sidebar is drawn while it shows the design context: sentences need
/// more room than layer names.
pub const MIN_WIDTH: f32 = 320.0;

/// The parts of the design context, one shown at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Brief,
    Assumptions,
    Decisions,
    Flow,
    Findings,
    Alternatives,
}

impl Section {
    pub const ALL: [Section; 6] = [Section::Brief, Section::Assumptions, Section::Decisions, Section::Flow, Section::Findings, Section::Alternatives];

    /// Its name in commands (`view.designContext {section}`) and `ui.inspect`.
    pub fn id(self) -> &'static str {
        match self {
            Section::Brief => "brief",
            Section::Assumptions => "assumptions",
            Section::Decisions => "decisions",
            Section::Flow => "flow",
            Section::Findings => "findings",
            Section::Alternatives => "alternatives",
        }
    }

    pub fn parse(s: &str) -> Option<Section> {
        Self::ALL.into_iter().find(|x| x.id().eq_ignore_ascii_case(s.trim()))
    }

    fn label(self) -> &'static str {
        match self {
            Section::Brief => tr("Brief"),
            Section::Assumptions => tr("Assumptions"),
            Section::Decisions => tr("Decisions"),
            Section::Flow => tr("Scenarios and flow"),
            Section::Findings => tr("Findings"),
            Section::Alternatives => tr("Alternatives"),
        }
    }

    /// How many entries the section holds (the brief isn't counted: it has no "done").
    fn count(self, c: &DesignContext) -> Option<usize> {
        match self {
            Section::Brief => None,
            Section::Assumptions => Some(c.assumptions.len()),
            Section::Decisions => Some(c.decisions.len()),
            Section::Flow => Some(c.scenarios.len()),
            Section::Findings => Some(c.findings.len()),
            Section::Alternatives => Some(c.alternatives.len()),
        }
    }
}

/// What a read-only command answered, with the document it answered about: when the file has
/// changed since, the answer is shown as being about an earlier state.
pub struct Ran {
    pub result: Value,
    doc: Arc<Document>,
}

impl Ran {
    fn new(app: &FigApp, result: Value) -> Option<Ran> {
        Some(Ran { result, doc: app.session.ds().ok()?.doc.clone() })
    }

    /// The file is no longer the one this was run on.
    pub fn stale(&self, doc: &Arc<Document>) -> bool {
        !Arc::ptr_eq(&self.doc, doc)
    }
}

/// What an inline form is for. The texts it holds are in [`Form::text`], by the slots named in
/// each variant.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FormKind {
    /// A new assumption, or the one with this id: 0 statement, 1 evidence; pick 0 status.
    Assumption(Option<String>),
    /// A new decision, or the one with this id: 0 title, 1 rationale, 2 what was rejected;
    /// `checks`: the assumptions it rests on.
    Decision(Option<String>),
    /// A finding added by hand: 0 label, 1 message, 2 evidence, 3 suggestion, 4 source; pick 0
    /// kind, pick 1 severity; `checks[0]`: it is about the selected layers.
    Finding,
    /// One text a finding's action needs (what changed, what was looked at, why it stays): 0.
    FindingAct(String, findings::Act),
    /// A new alternative of this frame: 0 name, 1 constraint, 2 intent, 3 the set's question, 4
    /// the set's constraints (one a line).
    Alternative(NodeId),
    /// A member of a set: 0 name, 1 constraint, 2 intent, 3 trade-offs.
    Member(String, NodeId),
    /// A set's question (0) and constraints (1, one a line).
    Set(String),
    /// Choosing a member of a set: 0 rationale.
    Choose(String, NodeId),
}

/// A form open in the panel: one at a time, drawn where it belongs.
pub(crate) struct Form {
    pub kind: FormKind,
    pub text: [String; 5],
    pub pick: [usize; 2],
    pub checks: Vec<(String, bool)>,
    /// What the engine answered to the last submit, when it refused.
    pub error: Option<String>,
    /// Put the keyboard in the first field on the next frame.
    focus: bool,
}

impl Form {
    pub(crate) fn new(kind: FormKind) -> Form {
        Form { kind, text: Default::default(), pick: [0, 0], checks: Vec::new(), error: None, focus: true }
    }

    pub(crate) fn with_text(mut self, slot: usize, value: &str) -> Form {
        if let Some(t) = self.text.get_mut(slot) {
            *t = value.to_string();
        }
        self
    }

    pub(crate) fn text(&self, slot: usize) -> &str {
        self.text.get(slot).map_or("", String::as_str)
    }

    /// The non-empty lines of a text slot (a list written one entry a line).
    pub(crate) fn lines(&self, slot: usize) -> Vec<String> {
        self.text(slot).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
    }
}

/// How a form's frame ended.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormEnd {
    Open,
    Submit,
    Cancel,
}

/// The panel's state.
#[derive(Default)]
pub struct ContextUi {
    pub open: bool,
    pub section: Section,
    pub(crate) form: Option<Form>,
    /// What the engine refused outside a form, and the entry it was about (`A1`, `brief`…).
    pub(crate) error: Option<(String, String)>,
    /// The last `proto.scenario.run`, `proto.check` and `design.check` shown.
    pub(crate) scenario_run: Option<Ran>,
    pub(crate) flow: Option<Ran>,
    pub(crate) checks: Option<Ran>,
    /// What the last `design.check.record` stored.
    pub(crate) recorded: Option<Ran>,
    /// The last `design.alternative.compare` shown, and its set.
    pub(crate) compare: Option<(String, Ran)>,
    /// Scenarios whose steps are unfolded.
    pub(crate) unfolded: BTreeSet<String>,
    /// The entry to point out (a link to it was followed), and whether to scroll to it still.
    pub(crate) highlight: Option<String>,
    pub(crate) scroll_to: bool,
    /// `design.forNode` of the selected layer, kept while neither changes.
    for_node: Option<(NodeId, Ran)>,
    /// Where the panel was last drawn (window points), and the part of it that scrolls.
    pub(crate) rect: Option<Rect>,
    pub(crate) body: Option<Rect>,
}

/// What a frame of the panel reads: the document as it is now (not borrowed from the app, so
/// commands can run while it is being drawn).
pub(crate) struct Cx {
    pub doc: Arc<Document>,
    pub design: Arc<DesignContext>,
    /// The file's id among the open ones: field ids carry it, so a draft never moves to another file.
    pub uid: u64,
    pub selection: Vec<NodeId>,
}

impl Cx {
    pub(crate) fn name(&self, id: NodeId) -> Option<&str> {
        self.doc.node(id).map(|n| n.name.as_str())
    }

    pub(crate) fn id(&self, salt: impl egui::AsIdSalt) -> Id {
        field_id(self.uid, salt)
    }
}

/// The id of a field of the panel in the file with this `uid`.
pub(crate) fn field_id(uid: u64, salt: impl egui::AsIdSalt) -> Id {
    Id::unique(("design_context", uid)).with(salt)
}

/// Open the panel, on `section` when one is named (View ▸ Panels ▸ Design context, the rail).
pub fn open(app: &mut FigApp, section: Option<Section>) {
    app.ui.context.open = true;
    if let Some(s) = section {
        app.ui.context.section = s;
    }
    app.ui.versions = Default::default();
    app.ui.open_left_panel();
}

pub fn close(app: &mut FigApp) {
    app.ui.context.open = false;
    app.ui.context.form = None;
}

/// A UI request from `view.designContext {section?, on?}`.
pub fn request(app: &mut FigApp, data: &Value) {
    if data.get("on").and_then(Value::as_bool) == Some(false) {
        close(app);
    } else {
        open(app, data.get("section").and_then(Value::as_str).and_then(Section::parse));
    }
}

/// Run a command for the panel: the result, or the engine's error kept to show next to
/// `target` (an entry id, or a part of the panel).
pub(crate) fn act(app: &mut FigApp, target: &str, cmd: &str, params: Value) -> Option<Value> {
    match app.try_run(cmd, params) {
        Ok(v) => {
            app.ui.context.error = None;
            Some(v)
        }
        Err(e) => {
            app.ui.context.error = Some((target.to_string(), e));
            None
        }
    }
}

/// Select `id` and bring it into view (on its own page when it is on another).
pub(crate) fn go_to(app: &mut FigApp, id: NodeId) {
    app.run("select.set", json!({"ids": [id]}));
    app.run("view.zoomSelection", json!({}));
}

/// Follow a link to entry `id` of the context (`A1`, `D2`, `S1`, `F3`, `V1`).
pub(crate) fn go_to_entry(app: &mut FigApp, id: &str) {
    let section = match id.chars().next() {
        Some('A') => Section::Assumptions,
        Some('D') => Section::Decisions,
        Some('S') => Section::Flow,
        Some('F') => Section::Findings,
        Some('V') => Section::Alternatives,
        _ => return,
    };
    let ui = &mut app.ui.context;
    ui.section = section;
    ui.highlight = Some(id.to_string());
    ui.scroll_to = true;
}

// ---------------------------------------------------------------------------------------------
// Small pieces the sections share. Text always wraps; colours are the theme's text colours,
// which reach 4.5:1 on the panel and on fields in both themes.

/// `add` inside the panel's side padding.
pub(crate) fn pad<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(PAD as i8, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn wrapped(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).wrap())
}

/// Body text.
pub(crate) fn body(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    wrapped(ui, text, regular(11.0), t.text)
}

/// A name or a title.
pub(crate) fn strong(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    wrapped(ui, text, semibold(11.0), t.text)
}

/// Secondary text: explanations, what something is.
pub(crate) fn quiet(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    wrapped(ui, text, regular(11.0), t.text_secondary)
}

/// A labelled piece of an entry ("Evidence", then the text), left out when it is empty.
pub(crate) fn part(ui: &mut Ui, label: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    ui.add_space(2.0);
    quiet(ui, label);
    body(ui, text);
}

/// Text that goes somewhere when clicked.
pub(crate) fn link(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.add(egui::Label::new(egui::RichText::new(text).font(regular(11.0)).color(t.accent_text).underline()).wrap().sense(Sense::click()));
    crate::a11y::link(&resp, text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(resp.rect.expand(2.0), CornerRadius::same(3), Stroke::new(1.5, t.focus), StrokeKind::Outside);
    }
    resp
}

/// A small label in an outline: a status, a severity, an id. It states, it doesn't grade.
pub(crate) fn chip(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(text.to_string(), regular(10.0), t.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 12.0, 18.0), Sense::hover());
    crate::a11y::status(&resp, text, "");
    ui.painter().rect_filled(rect, CornerRadius::same(9), t.field);
    ui.painter().rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, t.border_strong), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp
}

/// A text button. A label too long for the panel (a long translation) ends in an ellipsis and
/// shows whole on hover, so the button never runs past the panel's edge.
pub(crate) fn button(ui: &mut Ui, label: &str) -> bool {
    text_button(ui, label, false).clicked()
}

/// [`button`], filled with the accent colour: the thing a section is there to do.
pub(crate) fn primary(ui: &mut Ui, label: &str) -> bool {
    text_button(ui, label, true).clicked()
}

fn text_button(ui: &mut Ui, label: &str, primary: bool) -> Response {
    let room = (ui.max_rect().width() - 24.0).max(40.0);
    let shown = widgets::elide(ui, label, room);
    let resp = widgets::button(ui, &shown, primary, true);
    if shown == label { resp } else { resp.on_hover_text(label) }
}

/// A row of things that wraps onto more lines when the panel is narrow.
pub(crate) fn flow_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        add(ui)
    })
    .inner
}

/// A field of a form, with what it asks above it. `rows` 1 is one line that still wraps.
pub(crate) fn draft(ui: &mut Ui, id: Id, label: &str, text: &mut String, rows: usize, focus: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    quiet(ui, label);
    let w = ui.available_width();
    let r = egui::Frame::NONE
        .fill(t.field)
        .corner_radius(CornerRadius::same(5))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(text).id(id).frame(egui::Frame::NONE).font(regular(11.0)).desired_rows(rows).desired_width(w - 12.0))
        })
        .inner;
    crate::a11y::text(&r, label, text);
    if r.has_focus() {
        ui.painter().rect_stroke(r.rect.expand(6.0), CornerRadius::same(5), Stroke::new(1.0, t.focus), StrokeKind::Inside);
    }
    if focus {
        r.request_focus();
    }
    ui.add_space(4.0);
    r
}

/// A choice among a few named options, each on its own pill so long names wrap to the next line
/// instead of being cut. Returns the option clicked.
pub(crate) fn choice(ui: &mut Ui, labels: &[&str], active: usize) -> Option<usize> {
    let mut out = None;
    flow_row(ui, |ui| {
        for (i, label) in labels.iter().enumerate() {
            if pill(ui, label, i == active).clicked() {
                out = Some(i);
            }
        }
    });
    out
}

/// One option of a [`choice`], or a tab.
fn pill(ui: &mut Ui, label: &str, active: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = if active { semibold(11.0) } else { regular(11.0) };
    let color = if active { t.text } else { t.text_secondary };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, color);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 24.0), Sense::click());
    crate::a11y::toggle(&resp, label, active);
    let fill = if active {
        t.selected_row
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    if !active {
        ui.painter().rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.0, t.border), StrokeKind::Inside);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.5, t.focus), StrokeKind::Outside);
    }
    ui.painter().galley(pos2(rect.min.x + 8.0, rect.center().y - galley.size().y / 2.0), galley, color);
    resp
}

/// The Save and Cancel of a form, with what the engine said to the last Save above them.
pub(crate) fn form_end(ui: &mut Ui, form: &Form, submit: &str) -> FormEnd {
    if let Some(e) = &form.error {
        refusal(ui, e);
    }
    let mut end = FormEnd::Open;
    flow_row(ui, |ui| {
        if widgets::button(ui, submit, true, true).clicked() {
            end = FormEnd::Submit;
        }
        if widgets::button(ui, tr("Cancel"), false, true).clicked() {
            end = FormEnd::Cancel;
        }
    });
    ui.add_space(6.0);
    end
}

/// What the engine refused, in its words: beside a bar in the danger colour, in body text.
pub(crate) fn refusal(ui: &mut Ui, message: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    let r = egui::Frame::NONE.inner_margin(egui::Margin { left: 10, right: 0, top: 2, bottom: 2 }).show(ui, |ui| body(ui, message)).response;
    ui.painter().rect_filled(Rect::from_min_max(r.rect.min, pos2(r.rect.min.x + 3.0, r.rect.max.y)), CornerRadius::same(1), t.danger);
    ui.add_space(4.0);
}

/// The engine's refusal about `target`, when the last action on it was refused.
pub(crate) fn refusal_for(app: &FigApp, ui: &mut Ui, target: &str) {
    if let Some((about, message)) = &app.ui.context.error
        && about == target
    {
        refusal(ui, message);
    }
}

/// The layers an entry links to, each a link that selects the layer and brings it into view. A
/// link whose layer is gone stays listed as missing. With `unlink`, each has a button that
/// reports the layer to take off the list.
pub(crate) fn layer_links(app: &mut FigApp, ui: &mut Ui, cx: &Cx, layers: &[NodeId], unlink: bool) -> Option<NodeId> {
    let mut gone = None;
    flow_row(ui, |ui| {
        for id in layers {
            match cx.name(*id) {
                Some(name) => {
                    if link(ui, name).on_hover_text(tr("Select this layer and show it")).clicked() {
                        go_to(app, *id);
                    }
                }
                None => {
                    quiet(ui, &trf("missing layer ({id})", &[("id", id)]));
                }
            }
            if unlink && icons::button(ui, Icon::Close, 16.0, false, tr("Remove this link")).clicked() {
                gone = Some(*id);
            }
        }
    });
    gone
}

/// Where an entry begins: a rule, a little room, and the tint of an entry a link pointed to. Returns the
/// place to paint the tint once the entry's height is known.
pub(crate) fn entry<R>(app: &mut FigApp, ui: &mut Ui, id: &str, add: impl FnOnce(&mut FigApp, &mut Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    let pointed = app.ui.context.highlight.as_deref() == Some(id);
    let mark = ui.painter().add(egui::Shape::Noop);
    ui.add_space(8.0);
    let r = pad(ui, |ui| ui.vertical(|ui| add(app, ui)));
    ui.add_space(8.0);
    let rect = Rect::from_min_max(pos2(ui.max_rect().min.x, r.response.rect.min.y - 8.0), pos2(ui.max_rect().max.x, r.response.rect.max.y + 8.0));
    if pointed {
        ui.painter().set(mark, egui::Shape::rect_filled(rect, CornerRadius::ZERO, t.selected_row_child));
        if std::mem::take(&mut app.ui.context.scroll_to) {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
    }
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.border));
    r.inner
}

/// The heading of a list inside a section, with what it holds said once, quietly.
pub(crate) fn heading(ui: &mut Ui, title: &str, about: &str) {
    ui.add_space(10.0);
    pad(ui, |ui| {
        let t = Tokens::get(ui.ctx());
        wrapped(ui, title, semibold(12.0), t.text);
        if !about.is_empty() {
            quiet(ui, about);
        }
    });
    ui.add_space(4.0);
}

// ---------------------------------------------------------------------------------------------

fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Exploring => tr("Exploring"),
        Stage::Converging => tr("Converging"),
        Stage::Ready => tr("Ready"),
    }
}

/// What a stage changes, in one line: which checks run.
fn stage_about(stage: Stage) -> &'static str {
    match stage {
        Stage::Exploring => {
            tr("Trying directions. Checks report only what would break the file: actions that lead nowhere and links to layers that are gone.")
        }
        Stage::Converging => tr(
            "A direction is chosen. Checks also measure contrast, clipped text and content, the size of things to tap, and the flow and its scenarios.",
        ),
        Stage::Ready => tr(
            "About to be handed over. Checks also read the reasoning: the brief, assumptions nobody tested, decisions made under an earlier brief, and screens no scenario shows.",
        ),
    }
}

/// How far along the design is, with what that changes in one line: which checks run. Shown
/// where it matters, with the brief and above the checks.
pub(crate) fn stage(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    pad(ui, |ui| {
        strong(ui, tr("How far along is the design?"));
        ui.add_space(2.0);
        let labels: Vec<&str> = Stage::ALL.iter().map(|s| stage_label(*s)).collect();
        let at = Stage::ALL.iter().position(|s| *s == cx.design.stage).unwrap_or(0);
        if let Some(stage) = choice(ui, &labels, at).and_then(|i| Stage::ALL.get(i)) {
            act(app, "stage", "design.stage.set", json!({"stage": stage.id()}));
        }
        ui.add_space(2.0);
        quiet(ui, stage_about(cx.design.stage));
        refusal_for(app, ui, "stage");
    });
}

/// Draw the panel.
pub fn panel(app: &mut FigApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width();
    app.ui.context.rect = Some(ui.max_rect());
    // Header: title and close.
    let (head, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
    ui.painter().text(pos2(head.min.x + 16.0, head.center().y), Align2::LEFT_CENTER, tr("Design context"), semibold(13.0), t.text);
    let mut bar =
        ui.new_child(egui::UiBuilder::new().max_rect(head.shrink2(vec2(12.0, 12.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    if icons::button(&mut bar, Icon::Close, 24.0, false, tr("Close design context")).clicked() {
        close(app);
        app.ui.left_tab = crate::LeftTab::File;
        return;
    }
    ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, t.border));
    let Ok(ds) = app.session.ds() else {
        widgets::note(ui, tr("Open a file to see its design context."), t.text_secondary);
        return;
    };
    let cx = Cx { doc: ds.doc.clone(), design: ds.doc.design.clone(), uid: ds.uid, selection: ds.selection.clone() };

    // The sections.
    ui.add_space(10.0);
    pad(ui, |ui| {
        flow_row(ui, |ui| {
            for s in Section::ALL {
                let label = match s.count(&cx.design) {
                    Some(n) if n > 0 => format!("{} {n}", s.label()),
                    _ => s.label().to_string(),
                };
                if pill(ui, &label, s == app.ui.context.section).clicked() {
                    app.ui.context.section = s;
                    app.ui.context.form = None;
                }
            }
        });
    });
    waiting_line(app, ui, &cx);
    ui.add_space(8.0);
    widgets::separator(ui);
    let body = egui::ScrollArea::vertical().id_salt(("design_context", app.ui.context.section.id())).auto_shrink([false, false]).show(ui, |ui| {
        selected_layer(app, ui, &cx);
        match app.ui.context.section {
            Section::Brief => brief::show(app, ui, &cx),
            Section::Assumptions => records::assumptions(app, ui, &cx),
            Section::Decisions => records::decisions(app, ui, &cx),
            Section::Flow => flow::show(app, ui, &cx),
            Section::Findings => findings::show(app, ui, &cx),
            Section::Alternatives => alternatives::show(app, ui, &cx),
        }
        ui.add_space(24.0);
    });
    app.ui.context.body = Some(body.inner_rect);
}

/// What AI agents proposed that waits for a person, said once above the sections, each kind a
/// link to where it is accepted. Nothing when nothing waits.
fn waiting_line(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    let (decisions, ends) = waiting(&cx.design);
    let choices = cx.design.alternatives.iter().filter(|v| v.proposal.is_some()).count();
    if decisions + ends == 0 {
        return;
    }
    ui.add_space(6.0);
    pad(ui, |ui| {
        flow_row(ui, |ui| {
            quiet(ui, &trn("{n} proposals wait for you:", decisions + ends, &[]));
            let mut go = |ui: &mut Ui, text: String, section: Section| {
                if link(ui, &text).clicked() {
                    app.ui.context.section = section;
                    app.ui.context.form = None;
                }
            };
            // A proposed choice among alternatives is its proposed decision: counted once.
            let other = decisions.saturating_sub(choices);
            if other > 0 {
                go(ui, trn("{n} decisions", other, &[]), Section::Decisions);
            }
            if choices > 0 {
                go(ui, trn("{n} choices among alternatives", choices, &[]), Section::Alternatives);
            }
            if ends > 0 {
                go(ui, trn("{n} ends of the flow", ends, &[]), Section::Flow);
            }
        });
    });
}

/// `design.forNode` of the first selected layer, asked again only when the file or the selection
/// changed.
fn for_selected(app: &mut FigApp, cx: &Cx) -> Option<(NodeId, Value)> {
    let node = *cx.selection.first()?;
    let kept = app.ui.context.for_node.as_ref().filter(|(n, ran)| *n == node && !ran.stale(&cx.doc));
    if let Some((_, ran)) = kept {
        return Some((node, ran.result.clone()));
    }
    // A read, so not through `Session::execute`: that ends a live text preview (a font or style
    // hovered in a menu), whose change of the file would ask this again, and so on: a flicker.
    let spec = figcraft_engine::find_command("design.forNode")?;
    let result = (spec.run)(&mut app.session, &json!({"id": node})).ok()?;
    app.ui.context.for_node = Ran::new(app, result.clone()).map(|ran| (node, ran));
    Some((node, result))
}

/// With a layer selected: what in the design context refers to it, each a link to the entry.
fn selected_layer(app: &mut FigApp, ui: &mut Ui, cx: &Cx) {
    let Some((node, refs)) = for_selected(app, cx) else {
        app.ui.context.for_node = None;
        return;
    };
    let Some(name) = cx.name(node) else { return };
    let ids = |key: &str| -> Vec<String> { refs[key].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect() };
    let entries: Vec<String> = ["assumptions", "decisions", "scenarios", "findings", "alternatives"].into_iter().flat_map(ids).collect();
    let end = refs["endState"].as_bool() == Some(true);
    ui.add_space(8.0);
    pad(ui, |ui| {
        quiet(ui, &trf("Selected: {name}", &[("name", &name)]));
        if entries.is_empty() && !end {
            quiet(ui, tr("Nothing in the design context refers to this layer."));
            return;
        }
        flow_row(ui, |ui| {
            quiet(ui, tr("Referred to by"));
            for id in &entries {
                if link(ui, id).clicked() {
                    go_to_entry(app, id);
                }
            }
            if end && link(ui, tr("an end of the flow")).clicked() {
                app.ui.context.section = Section::Flow;
            }
        });
    });
    ui.add_space(8.0);
    widgets::separator(ui);
}

/// The panel's state for `ui.inspect`: what the person sees.
pub fn inspect(app: &FigApp) -> Value {
    let ui = &app.ui.context;
    let ds = app.session.ds().ok();
    let design = ds.map(|d| d.doc.design.clone());
    let stale = |ran: &Ran| ds.is_none_or(|d| ran.stale(&d.doc));
    let counts = design.as_ref().map(|c| {
        let (decisions, ends) = waiting(c);
        json!({
            "assumptions": c.assumptions.len(),
            "decisions": c.decisions.len(),
            "proposedDecisions": decisions,
            "scenarios": c.scenarios.len(),
            "endStates": c.end_states.len(),
            "proposedEndStates": ends,
            "findings": c.findings.len(),
            "openFindings": c.findings.iter().filter(|f| f.status == FindingStatus::Open).count(),
            "alternatives": c.alternatives.len(),
            // Sets with a choice an AI agent proposed (its decision is among `proposedDecisions`).
            "proposedChoices": c.alternatives.iter().filter(|v| v.proposal.is_some()).count(),
            // What waits for a person to accept: the proposed decisions and end states.
            "waitingForAPerson": decisions + ends,
        })
    });
    let scenario_run = ui.scenario_run.as_ref().map(|ran| {
        let results: Vec<Value> = ran.result["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| json!({"id": r["id"], "passed": r["passed"], "failedStep": r["failedStep"], "failures": r["failures"]}))
            .collect();
        json!({"passed": ran.result["passed"], "total": ran.result["total"], "failed": ran.result["failed"], "results": results, "simulated": ran.result["simulated"], "notModelled": ran.result["notModelled"], "stale": stale(ran)})
    });
    let problems = |list: &Value| -> Vec<Value> {
        list.as_array()
            .into_iter()
            .flatten()
            .map(|f| json!({"rule": f["rule"], "severity": f["severity"], "nodes": f["nodes"], "message": f["message"]}))
            .collect()
    };
    let flow = ui.flow.as_ref().map(|ran| {
        let screens = ran.result["screens"].as_array().map_or(&[][..], Vec::as_slice);
        // The frames an AI agent proposed as ends of the flow, waiting for a person.
        let proposed: Vec<&Value> = screens.iter().filter(|s| s["endStateProposedBy"].is_string()).map(|s| &s["id"]).collect();
        json!({"problems": problems(&ran.result["problems"]), "screens": screens.len(), "prototyped": ran.result["prototyped"], "proposedEndStates": proposed, "stale": stale(ran)})
    });
    let checks = ui.checks.as_ref().map(|ran| {
        let skipped: Vec<&Value> = ran.result["skipped"].as_array().into_iter().flatten().map(|s| &s["id"]).collect();
        json!({"findings": problems(&ran.result["findings"]), "summary": ran.result["summary"], "skipped": skipped, "stage": ran.result["stage"], "stale": stale(ran)})
    });
    let compare = ui.compare.as_ref().map(|(set, ran)| {
        json!({"set": set, "members": ran.result["members"].as_array().map_or(0, Vec::len), "warnings": ran.result["warnings"], "stale": stale(ran)})
    });
    json!({
        "open": ui.open,
        "rect": ui.rect.filter(|_| ui.open).map(|r| [r.min.x, r.min.y, r.max.x, r.max.y]),
        "section": ui.section.id(),
        "stage": design.as_ref().map(|c| c.stage),
        "briefRevision": design.as_ref().map(|c| c.brief_revision),
        "counts": counts,
        "form": ui.form.as_ref().map(|f| form_name(&f.kind)),
        "refused": ui.error.as_ref().map(|(about, message)| json!({"about": about, "message": message})).or_else(|| ui.form.as_ref().and_then(|f| f.error.as_ref()).map(|m| json!({"about": "form", "message": m}))),
        "highlight": ui.highlight,
        "selected": ui.for_node.as_ref().map(|(node, ran)| json!({"node": node, "refs": ran.result})),
        "scenarioRun": scenario_run,
        "flowCheck": flow,
        "checks": checks,
        "recorded": ui.recorded.as_ref().map(|ran| &ran.result),
        "compare": compare,
    })
}

fn form_name(kind: &FormKind) -> &'static str {
    match kind {
        FormKind::Assumption(_) => "assumption",
        FormKind::Decision(_) => "decision",
        FormKind::Finding => "finding",
        FormKind::FindingAct(..) => "findingAction",
        FormKind::Alternative(_) => "alternative",
        FormKind::Member(..) => "alternativeMember",
        FormKind::Set(_) => "alternativeSet",
        FormKind::Choose(..) => "choose",
    }
}

/// The statuses and severities as the panel names them (the engine's ids stay English).
pub(crate) const SEVERITIES: [(&str, &str); 4] =
    [("blocker", tr_noop("Blocker")), ("major", tr_noop("Major")), ("minor", tr_noop("Minor")), ("note", tr_noop("Note"))];

/// A severity id (`major`) as the panel shows it.
pub(crate) fn severity_label(id: &str) -> &str {
    SEVERITIES.iter().find(|(k, _)| *k == id).map_or(id, |(_, label)| tr(label))
}

/// Who an entry is by, as the panel names them: the person using the app is "you" (`user`, or
/// nobody named in a file from before entries had authors); an AI agent by its signed name.
pub(crate) fn author_name(author: &str) -> &str {
    if author.is_empty() || author == "user" { tr("you") } else { author }
}

/// The proposals of AI agents that wait for a person: decisions (a choice among alternatives an
/// agent put forward is its proposed decision) and end states.
pub(crate) fn waiting(c: &DesignContext) -> (usize, usize) {
    let decisions = c.decisions.iter().filter(|d| d.status == DecisionStatus::Proposed).count();
    let ends = c.end_states.iter().filter(|e| !e.status.is_accepted()).count();
    (decisions, ends)
}

/// "3 steps", "1 layer"… helpers the sections share.
pub(crate) fn steps_text(n: usize) -> String {
    trn("{n} steps", n, &[])
}
