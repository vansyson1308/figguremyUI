//! The Design context panel, driven headless through egui as a person drives it (clicks on what
//! the screen reader tree names, typed text, Tab) and as an AI agent does (commands through the
//! control channel): everything the file records can be read and edited, edits go through the
//! engine's commands and undo with them, what the engine refuses shows where it was asked, and
//! the layout holds at the panel's narrowest and in a narrow window.

use egui::accesskit::{Node, Role};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2, pos2, vec2};
use figcraft_doc::NodeId;
use figcraft_doc::design::{AssumptionStatus, DecisionStatus, FindingKind, FindingStatus};
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::design_context::{MIN_WIDTH, Section, field_id};
use crate::theme::Tokens;
use crate::{FigApp, NoServices};

/// The app in a window of a given size.
struct Win {
    ctx: egui::Context,
    app: FigApp,
    time: f64,
    size: Vec2,
}

impl Win {
    fn new(s: Session) -> Win {
        Win::sized(s, vec2(1440.0, 900.0))
    }

    fn sized(s: Session, size: Vec2) -> Win {
        let mut w = Win { ctx: egui::Context::default(), app: FigApp::new(s, Box::new(NoServices)), time: 0.0, size };
        w.frames(3);
        w
    }

    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), time: Some(self.time), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
        out
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(Vec::new());
        }
    }

    /// A command as the user's own (a menu, a shortcut).
    fn run(&mut self, cmd: &str, params: Value) -> Value {
        let r = self.app.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
        self.app.handle_ui_requests();
        self.frames(2);
        r
    }

    /// A command as an AI agent sends it through the control channel.
    fn agent(&mut self, cmd: &str, params: Value) -> Result<Value, String> {
        let ctx = self.ctx.clone();
        let r = crate::control::handle(&mut self.app, &ctx, "engine.execute", &json!({"command": cmd, "params": params, "agent": "Claude"}));
        self.frames(2);
        r
    }

    fn open(&mut self, section: Section) {
        self.run("view.designContext", json!({"section": section.id()}));
        assert!(self.app.ui.context.open);
    }

    /// The panel's state as `ui.inspect` reports it.
    fn shown(&self) -> Value {
        self.app.inspect()["designContext"].clone()
    }

    fn design(&self) -> std::sync::Arc<figcraft_doc::design::DesignContext> {
        self.app.session.doc().unwrap().design.clone()
    }

    /// The nodes of the tree a screen reader gets.
    fn nodes(&mut self) -> Vec<Node> {
        self.ctx.enable_accesskit();
        self.frame(Vec::new());
        self.frame(Vec::new()).platform_output.accesskit_update.expect("an AccessKit update").nodes.into_iter().map(|(_, n)| n).collect()
    }

    /// The controls (not plain text) named `label`, top to bottom and left to right.
    fn controls(&mut self, label: &str) -> Vec<Rect> {
        let mut found: Vec<Rect> = self
            .nodes()
            .iter()
            .filter(|n| matches!(n.role(), Role::Button | Role::Link | Role::Tab | Role::CheckBox | Role::TextInput | Role::MultilineTextInput))
            .filter(|n| n.label().is_some_and(|l| l.trim() == label))
            .filter_map(|n| n.bounds())
            .map(|b| Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)))
            .collect();
        found.sort_by(|a, b| a.min.y.total_cmp(&b.min.y).then(a.min.x.total_cmp(&b.min.x)));
        found
    }

    /// Every text on screen: what a screen reader would read (names of controls, values of
    /// fields) and every text painted, whole (a wrapped paragraph is one text).
    fn texts(&mut self) -> Vec<String> {
        let mut all: Vec<String> = self.nodes().iter().filter_map(|n| n.label().or(n.value()).map(str::to_string)).collect();
        all.extend(painted(self).into_iter().map(|(text, ..)| text));
        all
    }

    /// The texts painted in the panel from its top to its end, in lower case and joined: what
    /// the person reads there, scrolling through it.
    fn panel_reads(&mut self) -> String {
        let panel = self.panel();
        let mut texts: Vec<String> = Vec::new();
        self.wheel(20_000.0);
        for _ in 0..12 {
            for (text, rect, ..) in painted(self) {
                if panel.contains(rect.min) && !texts.contains(&text) {
                    texts.push(text);
                }
            }
            self.wheel(-400.0);
        }
        self.wheel(20_000.0);
        texts.join("\n").to_lowercase()
    }

    fn says(&mut self, text: &str) -> bool {
        self.texts().iter().any(|t| t.contains(text))
    }

    /// Turn the wheel over the panel's scrolling part and let the scroll settle.
    fn wheel(&mut self, dy: f32) {
        let over = self.app.ui.context.body.map_or(pos2(200.0, 500.0), |b| b.center());
        self.frame(vec![
            Event::PointerMoved(over),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Wheel,
                delta: vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.frames(30);
    }

    fn pointer(&mut self, at: Pos2, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(at),
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
        ]);
    }

    fn click(&mut self, at: Pos2) {
        self.pointer(at, true);
        self.pointer(at, false);
        self.frames(2);
    }

    /// Click the `nth` control named `label`: of those in the open panel when it has any (the
    /// panel is scrolled until it shows), else of the window's (the rail).
    fn press(&mut self, label: &str, nth: usize) {
        for _ in 0..60 {
            let all = self.controls(label);
            let (panel, body) = match (self.app.ui.context.open, self.app.ui.context.rect, self.app.ui.context.body) {
                (true, Some(panel), Some(body)) => (panel, body),
                _ => (Rect::NOTHING, Rect::NOTHING),
            };
            // By column: what is scrolled out of sight is still in the panel.
            let inside: Vec<Rect> = all.iter().copied().filter(|r| panel.x_range().contains(r.center().x)).collect();
            if inside.is_empty() {
                let Some(r) = all.get(nth) else { panic!("no control {label:?} #{nth} on screen: {:?}", self.texts()) };
                self.click(r.center());
                return;
            }
            let Some(r) = inside.get(nth).copied() else { panic!("no control {label:?} #{nth} in the panel: {:?}", self.texts()) };
            if body.shrink(6.0).contains(r.center()) {
                self.click(r.center());
                return;
            }
            self.wheel(if r.center().y <= body.center().y { 120.0 } else { -120.0 });
        }
        panic!("{label:?} #{nth} never scrolled into view");
    }

    fn button(&mut self, label: &str) {
        self.press(label, 0);
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_string())]);
        self.frame(Vec::new());
    }

    fn key(&mut self, key: Key) {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        self.frame(vec![k(true), k(false)]);
        self.frames(2);
    }

    /// Put the keyboard in the form field asking `label`, and type `text` there.
    fn fill(&mut self, label: &str, text: &str) {
        self.press(label, 0);
        self.type_text(text);
    }

    /// Where the plain text `text` is.
    fn label_rect(&mut self, text: &str) -> Rect {
        let found = painted(self).into_iter().find(|(t, ..)| t == text);
        found.unwrap_or_else(|| panic!("no text {text:?}")).1
    }

    fn uid(&self) -> u64 {
        self.app.session.ds().unwrap().uid
    }

    /// Where the left sidebar is drawn.
    fn panel(&self) -> Rect {
        self.app.ui.context.rect.expect("the panel was drawn")
    }
}

fn idv(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

/// Screens "Home" and "Done" with a "Continue" button on Home that goes to Done.
fn two_screens() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let home = idv(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 300, "height": 400, "name": "Home"})));
    let done = idv(&ex(&mut s, "frame.create", json!({"x": 400, "y": 0, "width": 300, "height": 400, "name": "Done"})));
    let go =
        idv(&ex(&mut s, "shape.rectangle", json!({"x": 20, "y": 20, "width": 120, "height": 48, "parent": home, "props": {"name": "Continue"}})));
    ex(&mut s, "proto.interaction.add", json!({"id": go, "trigger": {"type": "onClick"}, "action": {"type": "navigate", "dest": done}}));
    ex(&mut s, "select.set", json!({"ids": []}));
    (s, home, done, go)
}

#[test]
fn the_panel_opens_from_the_command_the_menu_and_the_rail() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    assert_eq!(w.shown()["open"], json!(false));
    w.run("view.designContext", json!({}));
    assert_eq!((w.shown()["open"].clone(), w.shown()["section"].clone()), (json!(true), json!("brief")));
    assert!(w.says("Who is it for?"), "the brief's questions show");
    w.run("view.designContext", json!({"section": "findings"}));
    assert_eq!(w.shown()["section"], json!("findings"));
    w.run("view.designContext", json!({"on": false}));
    assert!(!w.app.ui.context.open);
    // The command is in the main menu (View ▸ Panels) and runs like any menu item.
    let item = figcraft_engine::catalog::MENU.iter().find(|m| m.id == "view.designContext").expect("in the menu catalog");
    assert_eq!((item.menu, item.label), ("View/Panels", "Design context"));
    crate::menus::invoke(&mut w.app, "view.designContext");
    w.frames(2);
    assert!(w.app.ui.context.open);
    // One click on the rail closes it and opens it again; the Layers come back in between.
    w.button("Context");
    assert!(!w.app.ui.context.open);
    assert!(!w.says("Three kinds, kept apart:"));
    w.button("Context");
    assert!(w.app.ui.context.open && w.says("Three kinds, kept apart:"), "back on the section it showed");
    // An agent's command opens it too (it is a panel, not a dialog).
    w.run("view.designContext", json!({"on": false}));
    w.agent("view.designContext", json!({"section": "decisions"})).unwrap();
    assert_eq!((w.shown()["open"].clone(), w.shown()["section"].clone()), (json!(true), json!("decisions")));
    // Version history takes the sidebar back.
    w.run("file.versionHistory", json!({}));
    assert!(!w.app.ui.context.open && w.app.ui.versions.open);
}

#[test]
fn a_brief_answer_is_one_undo_step_and_tab_moves_between_the_questions() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Brief);
    let uid = w.uid();
    let field = |name: &str| field_id(uid, ("brief", name)).with("te");
    let (audience, sponsor) = (field("audience"), field("sponsor"));
    // A click into the field under the first question.
    let question = w.label_rect("Who is it for?");
    w.click(pos2(question.min.x + 40.0, question.max.y + 18.0));
    assert_eq!(w.ctx.memory(|m| m.focused()), Some(audience), "the click put the keyboard in the answer's field");
    for word in ["Nurses ", "on a ", "night shift"] {
        w.type_text(word);
    }
    assert!(w.design().brief.audience.is_empty(), "nothing is written while typing");
    // Tab commits the answer and moves to the next question.
    w.key(Key::Tab);
    assert_eq!(w.design().brief.audience, "Nurses on a night shift");
    assert_eq!(w.ctx.memory(|m| m.focused()), Some(sponsor), "Tab went to the next question's field");
    w.type_text("The ward manager");
    w.key(Key::Tab);
    assert_eq!(w.design().brief.sponsor, "The ward manager");
    assert_eq!(w.design().brief_revision, 2);
    // One undo step for each answer, whatever was typed to get there.
    w.run("edit.undo", json!({}));
    assert_eq!((w.design().brief.sponsor.as_str(), w.design().brief.audience.as_str()), ("", "Nurses on a night shift"));
    w.run("edit.undo", json!({}));
    assert!(w.design().brief.is_empty());
    assert!(!w.says("Nurses on a night shift"), "the field follows the undo");
    w.run("edit.redo", json!({}));
    assert!(w.says("Nurses on a night shift"), "and the redo");
}

#[test]
fn an_empty_brief_is_never_marked_as_missing_anything() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Brief);
    let texts = w.panel_reads();
    assert!(texts.contains("who is it for?"), "{texts}");
    for nag in ["incomplete", "missing", "required", "%", "0 of", "0/", "score", "health", "warning"] {
        assert!(!texts.contains(nag), "an empty brief shows {nag:?}:\n{texts}");
    }
    // The tabs count entries; the brief has no count, and a list answer adds none.
    w.run("design.brief.set", json!({"success": ["Half the calls to the ward stop"], "constraints": ["The hospital's colours"]}));
    assert!(w.controls("Brief").len() == 1 && w.says("Half the calls to the ward stop"));
}

#[test]
fn what_an_agent_records_shows_at_once_and_a_person_accepts_it() {
    let (s, home, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Decisions);
    assert!(w.says("No decisions are written down yet."));
    let d = w
        .agent("design.decision.add", json!({"title": "One primary action per screen", "rationale": "The brief names one job.", "links": [home]}))
        .unwrap();
    assert_eq!(d["status"], json!("proposed"));
    assert!(w.says("One primary action per screen") && w.says("Proposed") && w.says("by Claude"), "{:?}", w.texts());
    assert_eq!(w.shown()["counts"]["proposedDecisions"], json!(1));
    // The agent can't accept its own proposal; the person can, with one click.
    assert!(w.agent("design.decision.accept", json!({"id": "D1"})).is_err());
    w.button("Accept");
    assert_eq!(w.design().decisions[0].status, DecisionStatus::Accepted);
    assert!(w.says("Accepted") && w.controls("Accept").is_empty());
    // A change to the brief afterwards marks it as made under an earlier brief, until it is looked at again.
    w.agent("design.brief.set", json!({"audience": "Night nurses"})).unwrap();
    assert!(w.says("Made under an earlier brief"));
    w.button("It still stands");
    assert!(!w.says("Made under an earlier brief"));
    assert_eq!(w.design().decisions[0].brief_revision, w.design().brief_revision);
    // The linked layer is a link: it selects the layer.
    w.button("Home");
    assert_eq!(w.app.session.selection(), vec![home]);
    // Undo takes back the acceptance like any edit, and the panel follows.
    w.run("edit.undo", json!({}));
    w.run("edit.undo", json!({}));
    w.run("edit.undo", json!({}));
    assert_eq!(w.design().decisions[0].status, DecisionStatus::Proposed);
    assert!(w.says("Proposed"));
}

#[test]
fn an_assumptions_status_needs_evidence_as_the_engine_says() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Assumptions);
    w.button("Add an assumption");
    assert_eq!(w.shown()["form"], json!("assumption"));
    // The statement field has the keyboard.
    w.type_text("Nurses carry a phone on rounds");
    w.button("Supported");
    w.button("Save");
    let refused = w.shown()["refused"]["message"].as_str().unwrap_or_default().to_string();
    assert!(refused.contains("`evidence` is required"), "the engine's own words: {refused}");
    assert!(w.says("`evidence` is required"), "shown in the form");
    assert!(w.design().assumptions.is_empty(), "and nothing was stored");
    w.fill("Where does that come from? (a study, analytics, a conversation)", "Ward visit, 3 March: 9 of 11 did");
    w.button("Save");
    let a = &w.design().assumptions[0];
    assert_eq!(
        (a.status, a.evidence.as_str(), a.text.as_str()),
        (AssumptionStatus::Supported, "Ward visit, 3 March: 9 of 11 did", "Nurses carry a phone on rounds")
    );
    assert_eq!(w.shown()["form"], Value::Null);
    assert!(w.says("Supported") && w.says("Ward visit, 3 March: 9 of 11 did"));
    // Edit puts it back to untested; Remove takes it away; each is one undo step.
    w.button("Edit");
    w.button("Untested");
    w.button("Save");
    assert_eq!(w.design().assumptions[0].status, AssumptionStatus::Untested);
    w.button("Remove");
    assert!(w.design().assumptions.is_empty());
    w.run("edit.undo", json!({}));
    assert_eq!(w.design().assumptions.len(), 1);
}

#[test]
fn links_go_both_ways_and_a_missing_layer_stays_listed() {
    let (mut s, home, done, go) = two_screens();
    ex(&mut s, "design.assumption.add", json!({"text": "People read the button first", "links": [go]}));
    let mut w = Win::new(s);
    w.open(Section::Brief);
    // With the layer selected, the panel says what refers to it.
    w.run("select.set", json!({"ids": [go]}));
    assert!(w.says("Selected: Continue") && w.says("Referred to by"));
    assert_eq!(w.shown()["selected"]["refs"]["assumptions"], json!(["A1"]));
    w.button("A1");
    assert_eq!((w.shown()["section"].clone(), w.shown()["highlight"].clone()), (json!("assumptions"), json!("A1")));
    // From the entry back to the layer.
    w.run("select.set", json!({"ids": [done]}));
    assert!(w.says("Nothing in the design context refers to this layer."));
    w.button("Continue");
    assert_eq!(w.app.session.selection(), vec![go]);
    // Link another layer to it, and unlink one.
    w.run("select.set", json!({"ids": [home]}));
    w.button("Link the selected layer");
    assert_eq!(w.design().assumptions[0].links, vec![go, home]);
    // Each remove button is named after what it takes away (QC2-11).
    assert!(w.controls("Remove this link").is_empty());
    w.button("Remove the link to Home");
    assert_eq!(w.design().assumptions[0].links, vec![go]);
    // The layer is deleted: the link stays, shown as missing, and no longer a link.
    w.run("edit.delete", json!({"ids": [go]}));
    w.run("select.set", json!({"ids": [home]}));
    let missing = format!("missing layer ({go})");
    assert!(w.says(&missing), "{:?}", w.texts());
    assert!(w.controls("Continue").is_empty());
    // Linking another layer now is refused by the engine (it checks the whole list): the
    // refusal shows at the entry, and taking the missing link off makes room.
    w.button("Link the selected layer");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains(&format!("no layer {go}")));
    assert!(w.says(&format!("no layer {go}")));
    w.button(&format!("Remove the link to missing layer ({go})"));
    assert!(w.design().assumptions[0].links.is_empty());
    w.button("Link the selected layer");
    assert_eq!(w.design().assumptions[0].links, vec![home]);
}

#[test]
fn scenarios_run_and_show_the_step_that_failed_and_the_flow_check_lists_facts() {
    let (mut s, home, done, go) = two_screens();
    ex(&mut s, "proto.flow.add", json!({"start": home}));
    ex(
        &mut s,
        "proto.scenario.add",
        json!({"name": "Continue reaches Done", "purpose": "The one action works", "start": home, "steps": [{"click": go, "expect": {"screen": done}}]}),
    );
    ex(&mut s, "proto.scenario.add", json!({"name": "Continue stays home", "start": home, "steps": [{"click": go, "expect": {"screen": home}}]}));
    let mut w = Win::new(s);
    w.open(Section::Flow);
    assert!(w.says("Continue reaches Done") && w.says("The one action works") && w.says("1 step"));
    assert!(w.says("AI agents and the command line write scenarios"), "said once, quietly: steps aren't edited here");
    assert_eq!(w.shown()["scenarioRun"], Value::Null);
    w.button("Run all");
    let run = w.shown()["scenarioRun"].clone();
    assert_eq!((run["passed"].clone(), run["total"].clone(), run["failed"].clone()), (json!(false), json!(2), json!(1)));
    assert_eq!((run["results"][0]["passed"].clone(), run["results"][1]["failedStep"].clone()), (json!(true), json!(0)));
    let why = run["results"][1]["failures"][0].as_str().unwrap().to_string();
    assert!(w.says("Passed") && w.says("Failed at step 1 of 1") && w.says(&why), "the failing step's own words show: {why}");
    // The steps unfold with what each did.
    w.press("Show the steps", 1);
    assert!(w.says("Click “Continue”") && w.says("navigate to `Done`"), "{:?}", w.texts());
    // An edit makes the results old, and says so.
    w.run("node.set", json!({"id": go, "props": {"name": "Next"}}));
    assert_eq!(w.shown()["scenarioRun"]["stale"], json!(true));
    assert!(w.says("The file has changed since the scenarios last ran"));
    // The flow check: Done has no way out and isn't declared as an end.
    w.button("Check the flow");
    let problems = w.shown()["flowCheck"]["problems"].clone();
    assert!(problems.as_array().unwrap().iter().any(|p| p["rule"] == "flow.deadEnd" && p["nodes"] == json!([done])), "{problems}");
    assert!(w.says("flow.deadEnd") && w.says("2 of 2 screens can be reached"));
    // Its layer is a link: the problem can be gone to.
    w.run("select.set", json!({"ids": []}));
    w.press("Done", 0);
    assert_eq!(w.app.session.selection(), vec![done]);
    // Declaring the selected frame as an end takes the dead end away at the next check.
    w.button("Declare it as an end of the flow");
    assert_eq!(w.design().end_states.iter().map(|e| e.node).collect::<Vec<_>>(), vec![done]);
    w.button("Check the flow");
    let problems = w.shown()["flowCheck"]["problems"].clone();
    assert!(!problems.as_array().unwrap().iter().any(|p| p["rule"] == "flow.deadEnd"), "{problems}");
    w.button("No longer an end");
    assert!(w.design().end_states.is_empty());
    // Removing a scenario.
    w.press("Remove", 0);
    assert_eq!(w.design().scenarios.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["S2"]);
}

#[test]
fn checks_run_are_stored_and_settled_by_running_them_again() {
    let (mut s, home, ..) = two_screens();
    let tiny = idv(&ex(&mut s, "shape.rectangle", json!({"x": 200, "y": 20, "width": 10, "height": 10, "parent": home, "props": {"name": "Tiny"}})));
    ex(&mut s, "proto.interaction.add", json!({"id": tiny, "trigger": {"type": "onClick"}, "action": {"type": "back"}}));
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    ex(&mut s, "select.set", json!({"ids": []}));
    let mut w = Win::new(s);
    w.open(Section::Findings);
    for kind in ["Check: measured from the file", "Judgement: the opinion", "Feedback: what a person said"] {
        assert!(w.says(kind), "the legend names {kind:?}");
    }
    w.button("Run the checks");
    let run = w.shown()["checks"].clone();
    assert!(run["findings"].as_array().unwrap().iter().any(|f| f["rule"] == "target.small" && f["nodes"] == json!([tiny])), "{run}");
    assert!(run["skipped"].as_array().unwrap().contains(&json!("context.briefIncomplete")), "{run}");
    assert!(w.says("Not run at this stage") && w.says("context.briefIncomplete"), "the rules that didn't run are named");
    assert!(w.design().findings.is_empty(), "running stores nothing");
    w.button("Store what they find");
    let stored = w.design().findings.iter().filter(|f| f.rule == "target.small").count();
    assert_eq!(stored, 1);
    let id = w.design().findings.iter().find(|f| f.rule == "target.small").unwrap().id.clone();
    let status = |w: &Win| w.design().findings.iter().find(|f| f.id == id).unwrap().status;
    // Saying it is fixed doesn't make it so: the re-check puts it back to open.
    // Open findings show by severity, then in the file's order: the finding's place among them.
    let mut open: Vec<(figcraft_doc::design::Severity, usize, String)> =
        w.design().findings.iter().enumerate().filter(|(_, f)| f.status == FindingStatus::Open).map(|(i, f)| (f.severity, i, f.id.clone())).collect();
    open.sort();
    let at = open.iter().position(|(_, _, f)| *f == id).unwrap();
    w.press("Mark as addressed", at);
    assert_eq!(w.shown()["form"], json!("findingAction"));
    w.type_text("Made the target bigger");
    w.press("Mark as addressed", at);
    assert_eq!(status(&w), FindingStatus::Addressed, "{} {:?}", w.shown()["refused"], w.design().findings);
    w.button("Re-check the stored checks");
    assert_eq!(status(&w), FindingStatus::Open, "{} {:?}", w.shown()["refused"], w.design().findings);
    assert!(w.says("Last re-check: still reported"));
    // Fixed for real: the re-check confirms it.
    w.run("node.set", json!({"id": tiny, "props": {"width": 48, "height": 48}}));
    w.button("Re-check the stored checks");
    assert_eq!(status(&w), FindingStatus::Verified);
    assert!(w.says("Last re-check: no longer reported") && w.says("Confirmed"));
    // No score anywhere.
    let texts = w.panel_reads();
    assert!(texts.contains("confirmed"), "{texts}");
    for word in ["score", "health", "%", "grade", "rating"] {
        assert!(!texts.contains(word), "{word:?} in the findings:\n{texts}");
    }
}

#[test]
fn a_judgement_is_added_by_hand_and_settled_by_a_person() {
    let (s, _, _, go) = two_screens();
    let mut w = Win::new(s);
    w.run("select.set", json!({"ids": [go]}));
    w.open(Section::Findings);
    w.button("Add a judgement or feedback");
    w.type_text("hierarchy");
    w.button("Add");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`message`"), "{}", w.shown()["refused"]);
    w.fill("What is the problem?", "The button competes with the heading");
    w.button("Add");
    let f = w.design().findings[0].clone();
    assert_eq!((f.kind, f.rule.as_str(), f.source.as_str(), f.nodes.clone()), (FindingKind::Judgement, "hierarchy", "user", vec![go]));
    assert!(w.says("Judgement") && w.says("The button competes with the heading") && w.says("Open · Major"));
    // A check's action isn't offered on a judgement; a person confirms it with what they looked at.
    assert!(w.controls("Re-check").is_empty());
    w.button("Confirm it is met");
    w.button("Confirm it is met");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`evidence`"));
    w.fill("What did you look at to confirm it is met?", "Compared the two side by side");
    w.button("Confirm it is met");
    assert_eq!((w.design().findings[0].status, w.design().findings[0].verified_by.as_str()), (FindingStatus::Verified, "user"));
    w.button("Reopen");
    assert_eq!(w.design().findings[0].status, FindingStatus::Open);
    w.button("Accept as it is");
    w.type_text("The heading is the brand's");
    w.button("Accept as it is");
    assert_eq!((w.design().findings[0].status, w.design().findings[0].resolution.as_str()), (FindingStatus::Accepted, "The heading is the brand's"));
    assert!(w.says("Why it stays as it is"));
    // Feedback needs the person's words: the engine says so in the form.
    w.button("Add a judgement or feedback");
    w.button("Feedback");
    w.fill("A short label for it", "wording");
    w.fill("What is the problem?", "“Continue” doesn't say where to");
    w.button("Add");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`evidence` is required for feedback"));
    w.fill("What did the person say, in their words?", "I didn't know what would happen");
    w.fill("Who said it?", "Hoa, ward 4");
    w.button("Add");
    let fb = w.design().findings[1].clone();
    assert_eq!((fb.kind, fb.source.as_str()), (FindingKind::Feedback, "Hoa, ward 4"));
    w.press("Remove", 0);
    assert_eq!(w.design().findings.len(), 1);
}

#[test]
fn an_alternative_is_made_compared_by_facts_and_chosen_with_a_reason() {
    let (s, home, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Alternatives);
    assert!(w.says("Select a top-level frame or a component to make an alternative of it."));
    w.run("select.set", json!({"ids": [home]}));
    assert!(w.says("“Home”"));
    w.button("Make an alternative of it");
    w.button("Make the alternative");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`name`"), "a name is asked for by the engine: {}", w.shown()["refused"]);
    w.fill("What to call it", "One thumb");
    w.button("Make the alternative");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`constraint`"), "and the limit it is made under");
    w.fill("The limit it is made under (“one thumb, no scrolling”)", "Reachable with one thumb, no scrolling");
    w.fill("What are the alternatives trying to find out? (optional)", "Does the action need to be at the bottom?");
    w.button("Make the alternative");
    let set = w.design().alternatives[0].clone();
    assert_eq!((set.source, set.members.len(), set.question.as_str()), (home, 1, "Does the action need to be at the bottom?"));
    let copy = set.members[0].node;
    assert_eq!(w.app.session.doc().unwrap().node(copy).unwrap().name, "Home / One thumb");
    assert!(w.says("One thumb") && w.says("Reachable with one thumb, no scrolling"));
    // Compare: a table of counts and what the engine warns about, no ranking.
    w.button("Compare");
    let compare = w.shown()["compare"].clone();
    assert_eq!((compare["set"].clone(), compare["members"].clone()), (json!("V1"), json!(2)));
    assert!(compare["warnings"][0].as_str().unwrap().contains("identical to the source"), "{compare}");
    assert!(w.says("Layers that react") && w.says("Original") && w.says("Home (the original)") && w.says("identical to the source"));
    let texts = w.panel_reads();
    assert!(texts.contains("layers that react"), "{texts}");
    // (The panel's own words say they are "not ranked", and name whom each "serves best".)
    for word in ["winner", "score", "better", "worse", "recommended", "#1"] {
        assert!(!texts.contains(word), "{word:?} in a comparison:\n{texts}");
    }
    // Choose it, with a reason: a decision is recorded and linked.
    w.button("Choose");
    w.button("Record the choice");
    assert!(w.shown()["refused"]["message"].as_str().unwrap().contains("`rationale`"));
    w.fill("Why this one?", "The thumb reaches it");
    w.button("Record the choice");
    let set = w.design().alternatives[0].clone();
    assert_eq!((set.chosen, set.decision.as_str()), (Some(copy), "D1"));
    assert_eq!((w.design().decisions[0].status, w.design().decisions[0].links.clone()), (DecisionStatus::Accepted, vec![copy]));
    assert!(w.says("Chosen"));
    // Forgetting the alternative keeps its layer.
    w.button("Forget");
    assert!(w.design().alternatives[0].members.is_empty());
    assert!(w.app.session.doc().unwrap().contains(copy));
}

/// The texts painted this frame with where: (text, rect, the galley was cut short, what it is
/// clipped to).
fn painted(w: &mut Win) -> Vec<(String, Rect, bool, Rect)> {
    fn walk(shape: &egui::Shape, clip: Rect, out: &mut Vec<(String, Rect, bool, Rect)>) {
        match shape {
            egui::Shape::Text(t) => {
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                if clip.intersects(rect) && !t.galley.text().trim().is_empty() {
                    out.push((t.galley.text().to_string(), rect, t.galley.elided, clip));
                }
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    w.frame(Vec::new());
    let frame = w.frame(Vec::new());
    let mut out = Vec::new();
    for c in &frame.shapes {
        walk(&c.shape, c.clip_rect, &mut out);
    }
    out
}

const LONG_VI: &str = "Điều dưỡng trực đêm tại khoa hồi sức cần xem nhanh y lệnh thuốc của từng bệnh nhân mà không phải quay lại trạm điều dưỡng, kể cả khi đang đeo găng tay và chỉ dùng được một tay.";

/// A file with long Vietnamese text in every kind of entry.
fn long_file() -> Session {
    let (mut s, home, done, go) = two_screens();
    ex(&mut s, "design.brief.set", json!({"audience": LONG_VI, "job": LONG_VI, "constraints": [LONG_VI], "success": [LONG_VI, LONG_VI]}));
    ex(&mut s, "design.assumption.add", json!({"text": LONG_VI, "links": [go, home]}));
    ex(&mut s, "design.decision.add", json!({"title": LONG_VI, "rationale": LONG_VI, "rejected": LONG_VI, "assumptions": ["A1"], "links": [done]}));
    ex(
        &mut s,
        "design.finding.add",
        json!({"kind": "feedback", "rule": "từ ngữ", "message": LONG_VI, "evidence": LONG_VI, "source": "Chị Hoa, khoa 4", "nodes": [go]}),
    );
    ex(
        &mut s,
        "proto.scenario.add",
        json!({"name": LONG_VI, "purpose": LONG_VI, "start": home, "steps": [{"click": go, "expect": {"screen": home}, "note": LONG_VI}]}),
    );
    ex(
        &mut s,
        "design.alternative.create",
        json!({"source": home, "name": "Một ngón cái", "constraint": LONG_VI, "intent": LONG_VI, "question": LONG_VI}),
    );
    // A long name on the screen and on the layer the panel's buttons and links name.
    ex(&mut s, "node.set", json!({"id": home, "props": {"name": "Màn hình y lệnh thuốc trong ca trực đêm của khoa hồi sức tích cực"}}));
    ex(&mut s, "node.set", json!({"id": go, "props": {"name": "Nút xác nhận đã phát thuốc cho bệnh nhân ở giường này"}}));
    ex(&mut s, "select.set", json!({"ids": [home]}));
    s
}

/// Every text of the panel is inside it, whole, and long text takes more than one line. The
/// panel is scrolled through to its end.
fn assert_fits(w: &mut Win, what: &str) {
    let panel = w.panel();
    let mut wrapped = false;
    for _ in 0..12 {
        for (text, rect, cut, clip) in painted(w) {
            // Only what is drawn in the panel: clipped to it, with its left edge inside it (a
            // frame's name on the canvas may begin under the panel).
            if !clip.intersects(panel) || clip.min.x >= panel.max.x - 1.0 || rect.min.x < panel.min.x || rect.min.x > panel.max.x {
                continue;
            }
            // The comparison table scrolls sideways in a frame of its own, narrower than the
            // panel: there a column may be out of sight, never painted over the canvas.
            let in_table = clip.width() < panel.width() - 8.0;
            let shown = rect.intersect(clip);
            assert!(
                shown.max.x <= panel.max.x + 0.5,
                "{what}: {text:?} shows to x {} past the panel edge {}; rect {rect:?} clip {clip:?} panel {panel:?}",
                shown.max.x,
                panel.max.x
            );
            assert!(in_table || rect.max.x <= panel.max.x + 0.5, "{what}: {text:?} runs to x {} past the panel's edge {}", rect.max.x, panel.max.x);
            assert!(!cut, "{what}: {text:?} is cut short");
            wrapped |= text.contains("Điều dưỡng") && rect.height() > 30.0;
        }
        w.wheel(-400.0);
    }
    assert!(wrapped, "{what}: the long text wraps onto more lines");
}

#[test]
fn long_text_wraps_inside_the_panel_at_every_width() {
    // (window size, what)
    for (size, what) in [(vec2(1440.0, 900.0), "a wide window"), (vec2(900.0, 700.0), "a small window"), (vec2(420.0, 800.0), "a narrow window")] {
        let mut w = Win::sized(long_file(), size);
        for section in Section::ALL {
            w.open(section);
            if section == Section::Flow {
                w.button("Run all");
                w.button("Show the steps");
            }
            if section == Section::Alternatives {
                w.button("Compare");
            }
            let panel = w.panel();
            assert!(panel.width() >= (size.x - 56.0 - 48.0).min(MIN_WIDTH) - 4.0, "{what}: the panel is {} wide", panel.width());
            assert_fits(&mut w, &format!("{what}, {}", section.id()));
        }
        // And in the dark theme.
        w.run("view.themeDark", json!({}));
        assert!(Tokens::get(&w.ctx).dark);
        w.open(Section::Decisions);
        assert_fits(&mut w, &format!("{what}, dark"));
    }
}

/// WCAG relative luminance of an opaque colour.
fn luminance(c: egui::Color32) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn the_panels_text_colours_reach_aa_on_every_surface_they_sit_on_in_both_themes() {
    for t in [Tokens::light(), Tokens::dark()] {
        // Body, secondary and link text; on the panel, a field, a pointed-out entry, the active
        // tab and a hovered one.
        for (name, text) in [("text", t.text), ("text_secondary", t.text_secondary), ("accent_text", t.accent_text)] {
            for (on, surface) in [
                ("panel", t.panel),
                ("field", t.field),
                ("selected_row_child", t.selected_row_child),
                ("selected_row", t.selected_row),
                ("hover", t.hover),
            ] {
                let ratio = contrast(text, surface);
                assert!(ratio >= 4.5, "{} theme: {name} on {on} is {ratio:.2}:1", if t.dark { "dark" } else { "light" });
            }
        }
        // The bar beside a refusal and the focus ring are not text: 3:1 against the panel.
        for (name, mark) in [("danger", t.danger), ("focus", t.focus)] {
            assert!(contrast(mark, t.panel) >= 3.0, "{name} on the panel is {:.2}:1", contrast(mark, t.panel));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// What AI agents propose waits for a person, and the panel says whose each entry is.

#[test]
fn an_end_state_an_agent_proposes_waits_for_a_person_and_its_reason_stays_the_agents() {
    let (s, home, done, _) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Flow);
    w.agent("proto.endState.set", json!({"id": done, "reason": "The handover is recorded here"})).unwrap();
    assert_eq!(w.design().end_states[0].status, figcraft_doc::design::EndStateStatus::Proposed);
    assert!(w.says("Proposed") && w.says("by Claude (agent)") && w.says("The handover is recorded here"), "{:?}", w.texts());
    assert!(w.says("Waiting for you: until you accept it, it is no end of the flow, and the flow check may still report the screen as a dead end."));
    let counts = w.shown()["counts"].clone();
    assert_eq!((counts["proposedEndStates"].clone(), counts["waitingForAPerson"].clone()), (json!(1), json!(1)), "{counts}");
    // Said once above the sections, with where to go.
    assert!(w.says("1 proposal waits for you:") && w.says("1 end of the flow"), "{:?}", w.texts());
    // Its reason is a field the person may reword: that leaves it a proposal (only Accept
    // accepts it), still the agent's.
    let panel = w.panel();
    let in_panel = |n: &Node| n.bounds().is_some_and(|b| panel.contains(pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)));
    let fields: Vec<Rect> = w
        .nodes()
        .iter()
        .filter(|n| matches!(n.role(), Role::MultilineTextInput | Role::TextInput) && in_panel(n))
        .filter_map(|n| n.bounds())
        .map(|b| Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)))
        .collect();
    assert_eq!(fields.len(), 1, "one field in the panel: the proposal's reason");
    w.click(fields[0].center());
    w.key(Key::End);
    w.type_text(" (checked with the ward)");
    w.key(Key::Escape);
    let end = w.design().end_states[0].clone();
    assert_eq!(end.reason, "The handover is recorded here (checked with the ward)");
    assert_eq!((end.status, end.author.as_str()), (figcraft_doc::design::EndStateStatus::Proposed, "Claude (agent)"), "rewording accepts nothing");
    assert!(w.says("Proposed") && w.controls("Accept").len() == 1, "{:?}", w.texts());
    assert_eq!(w.shown()["counts"]["proposedEndStates"], json!(1));
    // The flow check says so for the frame, as the engine does.
    w.button("Check the flow");
    assert_eq!(w.shown()["flowCheck"]["proposedEndStates"], json!([done]));
    assert!(w.says("“Done” is proposed as an end of the flow by Claude (agent), waiting for you."), "{:?}", w.texts());
    assert!(w.shown()["flowCheck"]["problems"].as_array().unwrap().iter().any(|p| p["rule"] == "flow.deadEnd"), "still a dead end");
    // The person accepts it from the flow check.
    w.button("Accept it as an end");
    assert_eq!(w.design().end_states[0].status, figcraft_doc::design::EndStateStatus::Accepted);
    assert_eq!(w.design().end_states[0].author, "Claude (agent)", "the author stays the agent");
    assert!(!w.says("1 proposal waits for you:") && w.controls("Accept it as an end").is_empty());
    assert_eq!(w.shown()["counts"]["waitingForAPerson"], json!(0));
    // And the panel's own Accept, for a second proposal.
    w.agent("proto.endState.set", json!({"id": home, "reason": "Nothing to do"})).unwrap();
    w.button("Accept");
    assert!(w.design().end_states.iter().all(|e| e.status.is_accepted()));
}

#[test]
fn scenarios_say_who_wrote_them_and_a_run_says_it_was_simulated() {
    let (s, home, done, go) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Flow);
    w.agent("proto.scenario.add", json!({"name": "Continue reaches Done", "start": home, "steps": [{"click": go, "expect": {"screen": done}}]}))
        .unwrap();
    w.run("proto.scenario.add", json!({"name": "Mine", "start": home, "steps": [{"click": go}]}));
    assert!(w.says("by Claude (agent)") && w.says("by you"), "{:?}", w.texts());
    w.button("Run all");
    let run = w.shown()["scenarioRun"].clone();
    assert_eq!(run["simulated"], json!(true));
    assert_eq!(run["notModelled"].as_array().map(Vec::len), Some(figcraft_engine::proto_verify::NOT_MODELLED.len()));
    let line = "Simulated (not modelled: animation and timing; scroll position; drag direction; the device frame and responsive resizing; pointer hit-testing; a gesture step is one trigger; the player's own keys; widgets)";
    assert!(w.says(line), "one quiet line, so a pass isn't read as a watched session: {:?}", w.texts());
}

#[test]
fn a_choice_an_agent_proposes_among_alternatives_is_accepted_by_a_person() {
    let (s, home, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Alternatives);
    let made = w.run("design.alternative.create", json!({"source": home, "name": "One thumb", "constraint": "Reachable with one thumb"}));
    let copy = NodeId(made["node"].as_u64().unwrap());
    w.agent("design.alternative.choose", json!({"set": "V1", "node": copy, "rationale": "The action sits under the thumb"})).unwrap();
    let set = w.design().alternatives[0].clone();
    assert_eq!((set.chosen, set.proposal.as_ref().map(|p| p.node)), (None, Some(copy)), "nothing chosen yet");
    assert!(w.says("Claude (agent) proposes “One thumb”: The action sits under the thumb"), "{:?}", w.texts());
    assert!(w.says("Waiting for you: nothing is chosen until you accept it or choose another."));
    let counts = w.shown()["counts"].clone();
    assert_eq!((counts["proposedChoices"].clone(), counts["waitingForAPerson"].clone()), (json!(1), json!(1)), "{counts}");
    assert!(w.says("1 choice among alternatives"), "counted once, not also as a decision: {:?}", w.texts());
    w.button("Accept");
    let set = w.design().alternatives[0].clone();
    assert_eq!((set.chosen, set.proposal.is_none()), (Some(copy), true));
    assert_eq!(w.design().decisions[0].status, DecisionStatus::Accepted);
    assert!(w.says("Chosen") && !w.says("1 proposal waits for you:"));
}

#[test]
fn a_decision_says_who_changed_it_and_a_finding_who_stored_it() {
    let (s, _, _, go) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Decisions);
    w.run("design.decision.add", json!({"title": "One action", "rationale": "The brief names one job."}));
    w.agent("design.decision.set", json!({"id": "D1", "title": "One primary action"})).unwrap();
    assert!(w.says("by you") && w.says("changed by Claude (agent)"), "{:?}", w.texts());
    w.open(Section::Findings);
    w.agent(
        "design.finding.add",
        json!({"kind": "feedback", "rule": "wording", "message": "“Continue” is vague", "evidence": "She asked what happens next", "source": "Ana, ward 4", "nodes": [go]}),
    )
    .unwrap();
    assert!(w.says("Ana, ward 4") && w.says("Relayed by Claude (agent)"), "{:?}", w.texts());
    assert_eq!(w.design().findings[0].recorded_by, "Claude (agent)");
}

/// QC-01: what the panel shows is the file's own. Results of scenario runs, the flow check and
/// the checks run in one file never show in another one (a tab switch, a new file), and coming
/// back shows the first file's own again.
#[test]
fn each_file_shows_its_own_results_after_a_tab_switch() {
    let (mut s, home, done, go) = two_screens();
    ex(&mut s, "proto.flow.add", json!({"start": home}));
    ex(&mut s, "proto.scenario.add", json!({"name": "Continue reaches Done", "start": home, "steps": [{"click": go, "expect": {"screen": done}}]}));
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let mut w = Win::new(s);
    let first = w.uid();
    w.open(Section::Flow);
    w.button("Run all");
    w.button("Check the flow");
    assert_eq!(w.shown()["scenarioRun"]["passed"], json!(true));
    assert!(w.says("Passed") && w.says("screens can be reached"), "{:?}", w.texts());
    w.open(Section::Findings);
    w.button("Run the checks");
    assert!(w.shown()["checks"].is_object());
    // A second file with a scenario of the same id, never run.
    w.run("file.new", json!({}));
    assert_ne!(w.uid(), first);
    let other = idv(&w.run("frame.create", json!({"x": 0, "y": 0, "width": 300, "height": 400, "name": "Start"})));
    let next = idv(&w.run("frame.create", json!({"x": 400, "y": 0, "width": 300, "height": 400, "name": "Next"})));
    let tap = idv(&w.run("shape.rectangle", json!({"x": 20, "y": 20, "width": 120, "height": 48, "parent": other, "props": {"name": "Tap"}})));
    w.run("proto.interaction.add", json!({"id": tap, "trigger": {"type": "onClick"}, "action": {"type": "navigate", "dest": next}}));
    w.run("proto.scenario.add", json!({"name": "Tap reaches Next", "start": other, "steps": [{"click": tap}]}));
    w.open(Section::Findings);
    let shown = w.shown();
    for key in ["scenarioRun", "flowCheck", "checks", "recorded", "compare"] {
        assert_eq!(shown[key], Value::Null, "{key} of the first file shows in the second: {shown}");
    }
    assert!(!w.says("Rules run:"), "{:?}", w.texts());
    w.open(Section::Flow);
    assert!(w.says("Tap reaches Next"));
    assert!(!w.says("Passed") && !w.says("screens can be reached"), "nothing was run in this file: {:?}", w.texts());
    // Run in the second file, then go back: the first file's own results, and not the second's.
    w.button("Run all");
    assert_eq!(w.shown()["scenarioRun"]["total"], json!(1));
    w.run("doc.switch", json!({"index": 0}));
    assert_eq!(w.uid(), first);
    w.open(Section::Flow);
    let run = w.shown()["scenarioRun"].clone();
    assert_eq!((run["results"][0]["id"].clone(), run["passed"].clone(), run["stale"].clone()), (json!("S1"), json!(true), json!(false)), "{run}");
    assert!(w.says("Continue reaches Done") && !w.says("Tap reaches Next"));
    assert!(w.shown()["flowCheck"].is_object() && w.shown()["checks"].is_object());
    // A closed file's results go with it.
    w.run("doc.switch", json!({"index": 1}));
    let second = w.uid();
    assert!(w.app.ui.context.results(second).is_some());
    w.run("file.close", json!({"force": true}));
    w.open(Section::Flow);
    assert!(w.app.ui.context.results(second).is_none());
}

/// QC-01: a form being filled in stays with its file.
#[test]
fn a_form_stays_with_its_file() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Assumptions);
    w.button("Add an assumption");
    assert_eq!(w.shown()["form"], json!("assumption"));
    w.run("file.new", json!({}));
    w.open(Section::Assumptions);
    assert_eq!(w.shown()["form"], Value::Null, "the other file's form doesn't show here");
    w.run("doc.switch", json!({"index": 0}));
    w.open(Section::Assumptions);
    assert_eq!(w.shown()["form"], json!("assumption"), "it comes back with its file");
}

/// QC-08: storing what the checks found is the panel's own edit; it leaves the results it
/// stored current. Any other edit makes them old.
#[test]
fn storing_the_findings_leaves_the_checks_current() {
    let (mut s, home, ..) = two_screens();
    let tiny = idv(&ex(&mut s, "shape.rectangle", json!({"x": 200, "y": 20, "width": 10, "height": 10, "parent": home, "props": {"name": "Tiny"}})));
    ex(&mut s, "proto.interaction.add", json!({"id": tiny, "trigger": {"type": "onClick"}, "action": {"type": "back"}}));
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let mut w = Win::new(s);
    w.open(Section::Findings);
    w.button("Run the checks");
    w.button("Store what they find");
    assert!(!w.design().findings.is_empty());
    assert_eq!(w.shown()["checks"]["stale"], json!(false), "{}", w.shown()["checks"]);
    assert!(!w.says("The file has changed since the checks ran"), "{:?}", w.texts());
    w.run("node.set", json!({"id": tiny, "props": {"name": "Small"}}));
    assert_eq!(w.shown()["checks"]["stale"], json!(true));
}

/// QC-04: Tab through the panel never leaves the keyboard on a control scrolled out of sight:
/// the panel scrolls the focused control into view, down and back up again.
#[test]
fn tab_through_the_panel_keeps_the_focused_control_in_view() {
    let (s, ..) = two_screens();
    let mut w = Win::sized(s, vec2(1100.0, 560.0));
    w.open(Section::Brief);
    // The keyboard goes into the panel's first field.
    let question = w.label_rect("Who is it for?");
    w.click(pos2(question.min.x + 40.0, question.max.y + 18.0));
    let focused = |w: &Win| {
        let id = w.ctx.memory(|m| m.focused()).expect("a control has the keyboard");
        w.ctx.read_response(id).expect("it was drawn").rect
    };
    let body = w.app.ui.context.body.expect("the panel scrolls");
    let mut moved = 0;
    for step in 0..40 {
        let before = w.ctx.memory(|m| m.focused());
        w.key(Key::Tab);
        // The scroll is animated: let it finish.
        w.frames(30);
        let r = focused(&w);
        if w.ctx.memory(|m| m.focused()) != before {
            moved += 1;
        }
        if !w.panel().x_range().contains(r.center().x) {
            continue;
        }
        // Past the panel's last control the keyboard wraps to its header (above the part that scrolls).
        if moved > 1 && r.max.y <= body.min.y {
            break;
        }
        assert!(body.expand(1.0).contains_rect(r), "Tab {step}: the focused control {r:?} is outside the visible part {body:?}");
    }
    assert!(moved > 10, "Tab moved through the panel's controls ({moved})");
    // Back up with Shift+Tab: the same holds.
    for step in 0..40 {
        let k = |pressed| Event::Key { key: Key::Tab, physical_key: None, pressed, repeat: false, modifiers: Modifiers::SHIFT };
        w.frame(vec![k(true), k(false)]);
        w.frames(30);
        let r = focused(&w);
        // The header's controls (title bar, sections) sit above the part that scrolls.
        if w.panel().x_range().contains(r.center().x) && r.min.y >= body.min.y - 1.0 {
            assert!(body.expand(1.0).contains_rect(r), "Shift+Tab {step}: {r:?} outside {body:?}");
        }
    }
}

/// QC-16: `ui.inspect` says the keyboard is in the Design context panel, on which control and in
/// which entry.
#[test]
fn ui_inspect_reports_the_focus_in_the_panel() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Decisions);
    w.run("design.decision.add", json!({"title": "One action", "rationale": "The brief names one job."}));
    // The keyboard on the decision's Edit button.
    let edit = w.controls("Edit")[0];
    let id = w.ctx.viewport(|v| v.prev_pass.widgets.get_layer(egui::LayerId::background()).find(|r| r.rect == edit).map(|r| r.id)).unwrap();
    w.ctx.memory_mut(|m| m.request_focus(id));
    w.app.ui.nav.keyboard = true;
    w.frames(3);
    let ctx = w.ctx.clone();
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    let focus = &v["focus"];
    assert_eq!((&focus["region"], &focus["control"], &focus["entry"]), (&json!("designContext"), &json!("Edit"), &json!("D1")), "{focus}");
    assert_eq!(v["designContext"]["focusEntry"], json!("D1"));
    // Tab, as a person presses it, moves on: the control with the keyboard is named again.
    w.key(Key::Tab);
    w.frames(2);
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    assert_eq!((&v["focus"]["region"], &v["focus"]["control"]), (&json!("designContext"), &json!("Remove")), "{}", v["focus"]);
    // Also while the window isn't the system's focused one (an agent drives it in the background,
    // as under a virtual display): egui still knows which control has the keyboard.
    for _ in 0..2 {
        w.time += 1.0 / 60.0;
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, w.size)), time: Some(w.time), focused: false, ..Default::default() };
        let app = &mut w.app;
        let mut out = w.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
    }
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    assert_eq!(v["focus"]["control"], json!("Remove"), "{}", v["focus"]);
    // Closed, the left panel is the layers again.
    w.run("view.designContext", json!({"on": false}));
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    assert_ne!(v["focus"]["region"], json!("designContext"));
}

/// QC-06: a rail label too long for the rail (the Vietnamese "Tài nguyên") wraps onto a second
/// line, whole, instead of being cut; a short one stays on one line.
#[test]
fn a_rail_label_too_long_for_one_line_wraps_whole() {
    let mut w = Win::new(Session::new());
    let max = crate::layers::RAIL_WIDTH - 6.0;
    let mut out: Vec<(Vec<String>, Vec<f32>, f32)> = Vec::new();
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, w.size)), ..Default::default() };
    let mut output = w.ctx.run_ui(input, |ui| {
        for label in ["Tài nguyên", "File", "Bối cảnh"] {
            let lines = crate::layers::rail_label_lines(ui, label, max);
            let widths =
                lines.iter().map(|l| ui.painter().layout_no_wrap(l.clone(), crate::theme::regular(10.0), egui::Color32::WHITE).size().x).collect();
            let whole = ui.painter().layout_no_wrap(label.to_string(), crate::theme::regular(10.0), egui::Color32::WHITE).size().x;
            out.push((lines, widths, whole));
        }
    });
    output.textures_delta.clear();
    let (lines, widths, whole) = &out[0];
    assert!(*whole > max, "the case QC-06 saw: one line doesn't hold it ({whole} > {max})");
    assert_eq!(lines, &vec!["Tài".to_string(), "nguyên".to_string()], "two lines, nothing cut");
    assert!(widths.iter().all(|x| *x <= max), "{widths:?}");
    assert_eq!(out[1].0, vec!["File".to_string()]);
    for (lines, widths, _) in &out {
        assert!(lines.len() <= 2 && widths.iter().all(|x| *x <= max) && lines.iter().all(|l| !l.contains('…')), "{lines:?} {widths:?}");
    }
    w.frames(1);
}

/// QC-15: an accepted proposal says who proposed it and who accepted it, so it never reads as
/// if the agent accepted it.
#[test]
fn an_accepted_proposal_says_who_proposed_and_who_accepted_it() {
    let (s, _, done, _) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Decisions);
    w.agent("design.decision.add", json!({"title": "One action", "rationale": "The brief names one job."})).unwrap();
    assert!(w.says("proposed by Claude (agent)") && !w.says("accepted by"), "{:?}", w.texts());
    w.button("Accept");
    assert_eq!(w.design().decisions[0].accepted_by, "user");
    assert!(w.says("proposed by Claude (agent)") && w.says("accepted by you"), "{:?}", w.texts());
    // The person's own decision is theirs: "by you", no "proposed by", nobody else accepting it.
    w.run("design.decision.add", json!({"title": "Mine", "rationale": "Mine"}));
    assert!(w.says("by you") && !w.says("proposed by you") && !w.says("accepted by Claude"), "{:?}", w.texts());
    assert_eq!(w.design().decisions[1].accepted_by, "");
    // An end of the flow the agent declared, accepted by the person.
    w.open(Section::Flow);
    w.agent("proto.endState.set", json!({"id": done, "reason": "The handover is recorded here"})).unwrap();
    w.button("Accept");
    assert_eq!(w.design().end_states[0].accepted_by, "user");
    assert!(w.says("proposed by Claude (agent)") && w.says("accepted by you"), "{:?}", w.texts());
}

/// The states of the screens, as an agent declares them and as a person adds one with clicks
/// and keys: what shows each, whether it is designed and reached, and who proposed it.
#[test]
fn declared_states_show_what_shows_them_and_a_person_declares_one_by_clicks() {
    let (s, home, done, go) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Flow);
    assert!(w.says("No states are declared, so missing states aren't checked."), "{:?}", w.texts());
    w.agent("proto.state.set", json!({"frame": home, "kind": "error", "shownBy": {"node": go}, "reason": "The code is wrong"})).unwrap();
    assert!(w.says("Something went wrong") && w.says("proposed by Claude (agent)") && w.says("The code is wrong"), "{:?}", w.texts());
    // What is designed and reached is asked for, not worked out again on each change.
    assert!(w.says("Not checked yet: check the states again to see whether it is designed and reached."), "{:?}", w.texts());
    w.button("Check the states again");
    assert!(w.says("Designed") && w.says("Only its screen is reached"), "{:?}", w.texts());
    assert_eq!(w.shown()["states"]["declared"], json!(1));
    assert_eq!(w.shown()["states"]["summary"]["designed"], json!(1), "{}", w.shown()["states"]);
    // What shows it is a link that selects the layer.
    w.press("Continue", 0);
    assert_eq!(w.app.session.ds().unwrap().selection, vec![go]);
    // The person accepts the agent's declaration.
    w.button("Accept");
    assert!(w.design().states[0].status.is_accepted() && w.design().states[0].accepted_by == "user");
    // A screen selected: declare its success state with clicks and keys.
    w.run("select.set", json!({"ids": [done]}));
    w.button("Declare a state of this screen");
    assert_eq!(w.shown()["form"], json!("state"));
    w.button("It worked");
    w.fill("Why does the screen have this state?", "The parcel is ready to collect");
    w.button("Save");
    let st = w.design().states.iter().find(|st| st.frame == done).cloned().expect("declared");
    assert_eq!(
        (st.kind, st.reason.as_str(), st.author.as_str(), st.shown_by.is_none()),
        (figcraft_doc::design::StateKind::Success, "The parcel is ready to collect", "user", true)
    );
    assert!(w.says("Nothing is said to show it yet."), "{:?}", w.texts());
    w.button("Check the states again");
    assert!(w.says("Not designed"), "{:?}", w.texts());
    // Undo takes it back, as every command.
    w.run("edit.undo", json!({}));
    assert_eq!(w.design().states.len(), 1);
    // The agent may not take back what the person accepted.
    let e = w.agent("proto.state.remove", json!({"frame": home, "kind": "error"})).unwrap_err();
    assert!(e.contains("stands accepted"), "{e}");
    w.button("Remove");
    assert!(w.design().states.is_empty());
}

/// The brief lists what the design must show, with the layers that are it; a person adds one
/// from the selection, an agent's waits to be accepted.
#[test]
fn required_elements_are_listed_with_their_layers_and_added_from_the_brief() {
    let (s, home, _, go) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Brief);
    assert!(w.says("What must it show?") && w.says("Nothing is listed yet."), "{:?}", w.texts());
    w.run("select.set", json!({"ids": [go]}));
    w.button("Add something it must show");
    assert_eq!(w.shown()["form"], json!("required"));
    w.fill("What must the design show, and where?", "The price before paying");
    w.button("Only on their screens");
    w.button("Save");
    let r = w.design().brief.required[0].clone();
    assert_eq!((r.id.as_str(), r.text.as_str(), r.nodes.clone(), r.screens.clone()), ("R1", "The price before paying", vec![go], vec![home]));
    assert!(w.says("The layers that are it") && w.says("On these screens"), "{:?}", w.texts());
    assert_eq!(w.shown()["required"], json!({"total": 1, "unlinked": 0, "proposed": 0}));
    w.agent("design.required.add", json!({"text": "Consent to be contacted"})).unwrap();
    assert!(w.says("proposed by Claude (agent)") && w.says("No layer is linked, so the checks can't find it yet."), "{:?}", w.texts());
    assert_eq!(w.shown()["required"]["unlinked"], json!(1));
    w.button("Accept");
    assert!(w.design().brief.required[1].status.is_accepted());
    // The selection (still the button) links to the second one.
    w.button("Link the selected layer");
    assert_eq!(w.design().brief.required[1].nodes, vec![go]);
    // The selected layer says what refers to it.
    assert_eq!(w.shown()["selected"]["refs"]["required"], json!(["R1", "R2"]));
}

/// The keyboard in a declared state's controls is reported by `ui.inspect` with that entry.
#[test]
fn ui_inspect_names_the_declared_state_the_keyboard_is_in() {
    let (s, home, _, _) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Flow);
    w.run("proto.state.set", json!({"frame": home, "kind": "loading"}));
    let edit = w.controls("Edit")[0];
    let id = w.ctx.viewport(|v| v.prev_pass.widgets.get_layer(egui::LayerId::background()).find(|r| r.rect == edit).map(|r| r.id)).unwrap();
    w.ctx.memory_mut(|m| m.request_focus(id));
    w.app.ui.nav.keyboard = true;
    w.frames(3);
    let ctx = w.ctx.clone();
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    assert_eq!((&v["focus"]["region"], &v["focus"]["control"]), (&json!("designContext"), &json!("Edit")), "{}", v["focus"]);
    assert_eq!(v["designContext"]["focusEntry"], json!(format!("state {home} loading ")));
}

/// Item 3 of the review: the declare form keeps its screen while the person selects the overlay
/// that shows the state, and saves it as `shownBy`.
#[test]
fn the_declare_form_keeps_its_screen_while_another_frame_is_selected_to_show_the_state() {
    let (mut s, home, _, _) = two_screens();
    let sheet = idv(&ex(&mut s, "frame.create", json!({"x": 800, "y": 0, "width": 300, "height": 200, "name": "Error sheet"})));
    let mut w = Win::new(s);
    w.open(Section::Flow);
    w.run("select.set", json!({"ids": [home]}));
    w.button("Declare a state of this screen");
    w.button("Something went wrong");
    // Selecting the overlay keeps the form for Home open: the selection only says what shows it.
    w.run("select.set", json!({"ids": [sheet]}));
    assert_eq!(w.shown()["form"], json!("state"));
    w.button("Shown by “Error sheet”");
    w.button("Save");
    let st = w.design().states[0].clone();
    assert_eq!(
        (st.frame, st.kind, st.shown_by),
        (home, figcraft_doc::design::StateKind::Error, Some(figcraft_doc::design::ShownBy::Node { node: sheet }))
    );
}

/// Item 7 of the review: the states' facts replay the scenarios only when asked, and are marked as
/// being about an earlier state once the file changes.
#[test]
fn the_states_facts_are_asked_for_and_marked_stale_not_replayed_on_every_change() {
    let (s, home, done, go) = two_screens();
    let mut w = Win::new(s);
    w.run("proto.state.set", json!({"frame": home, "kind": "success", "shownBy": {"node": done}}));
    w.open(Section::Flow);
    assert!(w.says("Reachable"), "asked the first time the section shows the file: {:?}", w.texts());
    let asked = w.app.ui.context.results(w.uid()).and_then(|r| r.states.as_ref()).map(|r| r.result.clone());
    w.run("design.brief.set", json!({"audience": "Couriers"}));
    w.frames(5);
    assert!(w.says("The file has changed since the states were checked: what follows is from before."), "{:?}", w.texts());
    let kept = w.app.ui.context.results(w.uid()).and_then(|r| r.states.as_ref()).map(|r| r.result.clone());
    assert_eq!(kept, asked, "not asked again by the change");
    // Running the scenarios asks again.
    w.run("proto.scenario.add", json!({"name": "Go", "start": home, "steps": [{"click": go}]}));
    w.button("Run all");
    assert!(!w.says("The file has changed since the states were checked: what follows is from before."));
    assert!(w.says("A scenario shows it:"), "{:?}", w.texts());
}

/// QC2-08: a stored check says which rule measured it, not "Raised by: check".
#[test]
fn a_stored_check_says_which_rule_measured_it() {
    let (mut s, home, ..) = two_screens();
    ex(&mut s, "proto.flow.add", json!({"start": home}));
    ex(&mut s, "design.stage.set", json!({"stage": "ready"}));
    ex(&mut s, "design.check.record", json!({"rules": ["scenario.none"]}));
    let mut w = Win::new(s);
    w.open(Section::Findings);
    assert!(w.says("Measured by the rule") && w.says("scenario.none"), "{:?}", w.texts());
    assert!(!w.texts().iter().any(|t| t == "Raised by"), "{:?}", w.texts());
}

/// QC2-11: the keyboard on the icon rail is reported as the rail, not the Design context panel.
#[test]
fn the_rail_is_its_own_focus_region() {
    let (s, ..) = two_screens();
    let mut w = Win::new(s);
    w.open(Section::Brief);
    let rail = w.app.ui.nav.rail_rect.expect("the rail is drawn");
    let on_rail = w
        .nodes()
        .into_iter()
        .filter(|n| n.role() == Role::Button)
        .filter_map(|n| n.bounds())
        .map(|b| Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)))
        .find(|r| rail.contains(r.center()))
        .expect("a button on the rail");
    let id = w.ctx.viewport(|v| v.prev_pass.widgets.get_layer(egui::LayerId::background()).find(|r| r.rect == on_rail).map(|r| r.id)).unwrap();
    w.ctx.memory_mut(|m| m.request_focus(id));
    w.app.ui.nav.keyboard = true;
    w.app.ui.nav.region = crate::a11y::Region::Layers;
    w.frames(3);
    let ctx = w.ctx.clone();
    let v = crate::control::handle(&mut w.app, &ctx, "ui.inspect", &json!({})).unwrap();
    assert_eq!(v["focus"]["region"], json!("rail"), "{}", v["focus"]);
}

/// QC2-06: a new form's first field is scrolled into view with its whole focus ring.
#[test]
fn a_new_forms_first_field_shows_with_its_whole_focus_ring() {
    let (s, ..) = two_screens();
    let mut w = Win::sized(s, vec2(1440.0, 500.0));
    w.open(Section::Brief);
    w.button("Add something it must show");
    w.frames(4);
    let body = w.app.ui.context.body.expect("the panel's body");
    let field = w.controls("What must the design show, and where?")[0];
    let ring = field.expand(6.0 + crate::a11y::RING_MARGIN);
    assert!(body.contains_rect(ring), "the ring {ring:?} inside the body {body:?}");
}
