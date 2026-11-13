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
    w.press("Remove this link", 1);
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
    w.button("Remove this link");
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
    assert_eq!(run["notModelled"].as_array().map(Vec::len), Some(5));
    let line = "Simulated (not modelled: animation and timing; scroll position; drag direction; the device frame and responsive resizing; pointer hit-testing)";
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
