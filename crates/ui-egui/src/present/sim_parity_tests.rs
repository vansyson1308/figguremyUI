//! One set of rules: the player a person watches and the headless simulator an agent verifies
//! with (`figcraft_engine::proto_sim::Sim`) are given the same file and the same things to do,
//! and must agree after every step on the screen showing, the open overlays (which, in what
//! order), the history, the variables, variable modes and instance variants the prototype
//! changed, and the whole document as it plays.
//!
//! The player is driven as a person drives it: a pointer and keys through egui, headless, and
//! its clock for delays. The simulator is driven with scenario steps. What a step means on each
//! side is in [`Pair::step`]; the simulator's documented limits (`proto_sim`'s module docs: a
//! gesture is one trigger, hover and press last until the next step unless it is a click on the
//! hovered layer, time isn't measured) decide
//! how a player gesture is written as steps, and nothing here hides a difference outside them.

use std::collections::BTreeMap;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use figcraft_doc::design::{Gesture, StepAction};
use figcraft_doc::{Document, NodeId, NodeKind, VariableValue};
use figcraft_engine::Session;
use figcraft_engine::proto_sim::{Sim, StepOutcome};
use serde_json::{Value, json};

use super::Presenter;
use crate::present_tests::{Ui as Harness, id};

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

/// A 300 × 400 top-level frame at `x`.
fn frame(s: &mut Session, name: &str, x: f64) -> NodeId {
    id(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": 300, "height": 400, "name": name})))
}

/// A 150 × 100 top-level frame at `x`, below the screens: something to show as an overlay.
fn sheet(s: &mut Session, name: &str, x: f64) -> NodeId {
    id(&ex(s, "frame.create", json!({"x": x, "y": 600, "width": 150, "height": 100, "name": name})))
}

/// A 60 × 30 rectangle in `parent` at `(x, y)` from its top-left.
fn rect(s: &mut Session, name: &str, parent: NodeId, x: f64, y: f64) -> NodeId {
    id(&ex(s, "shape.rectangle", json!({"x": x, "y": y, "width": 60, "height": 30, "parent": parent, "props": {"name": name}})))
}

/// An interaction on `layer`: `trigger` (a trigger object) runs `actions` at once.
fn on(s: &mut Session, layer: NodeId, trigger: Value, actions: Value) {
    let actions = if actions.is_array() { actions } else { json!([actions]) };
    ex(s, "proto.interaction.add", json!({"id": layer, "trigger": trigger, "actions": actions, "transition": {"type": "instant"}}));
}

fn t(kind: &str) -> Value {
    json!({"type": kind})
}

/// A rectangle named `name` in `parent` at `(x, y)` that runs `actions` on click.
fn button(s: &mut Session, name: &str, parent: NodeId, [x, y]: [f64; 2], actions: Value) -> NodeId {
    let b = rect(s, name, parent, x, y);
    on(s, b, t("onClick"), actions);
    b
}

fn navigate(dest: NodeId) -> Value {
    json!({"type": "navigate", "dest": dest})
}

fn open(dest: NodeId, close_on_outside: bool) -> Value {
    json!({"type": "openOverlay", "dest": dest, "closeOnOutside": close_on_outside})
}

fn add_one() -> Value {
    json!({"type": "setVariable", "variable": "count", "expression": "count + 1"})
}

/// A collection `State` with `count` (number, 0) and `label` (string, "Hi").
fn state_variables(s: &mut Session) {
    ex(s, "variable.createCollection", json!({"name": "State"}));
    ex(s, "variable.create", json!({"collection": "State", "name": "count", "type": "number", "value": 0}));
    ex(s, "variable.create", json!({"collection": "State", "name": "label", "type": "string", "value": "Hi"}));
}

/// A component set of 100 × 40 variants `State=<name>` for each of `states`, below the screens.
/// Returns the set and its variants in that order.
fn variants(s: &mut Session, states: &[&str]) -> (NodeId, Vec<NodeId>) {
    let mut made = Vec::new();
    for (k, state) in states.iter().enumerate() {
        let f = id(&ex(s, "frame.create", json!({"x": k as f64 * 150.0, "y": 900, "width": 100, "height": 40, "name": format!("State={state}")})));
        made.push(id(&ex(s, "component.create", json!({"ids": [f]}))));
    }
    (id(&ex(s, "component.combineVariants", json!({"ids": made.clone()}))), made)
}

fn instance(s: &mut Session, set: NodeId, parent: NodeId) -> NodeId {
    id(&ex(s, "instance.create", json!({"component": set, "x": 20, "y": 200, "parent": parent})))
}

/// What a viewer does, in the terms both sides take.
#[derive(Clone, Debug)]
enum Do {
    /// Press and release on the layer. On the layer [`Do::Press`] holds, only the release; on
    /// the layer [`Do::Hover`] rests over, in place.
    Click(NodeId),
    /// The same for the player; for the simulator, where a gesture is one trigger, a mouse up
    /// and then a click.
    ClickWithMouseUp(NodeId),
    /// Move the pointer over the layer and leave it there.
    Hover(NodeId),
    /// Press on the layer and keep the button down.
    Press(NodeId),
    /// A key, with Shift when `shift`.
    Key(Key, bool),
    /// Time passes with the pointer off every layer (and the button up).
    Wait,
    /// A click on the screen outside the open overlay.
    Outside,
}

/// What both sides must agree on.
#[derive(Debug, PartialEq)]
struct Facts {
    screen: NodeId,
    overlays: Vec<NodeId>,
    history: Vec<NodeId>,
    /// The variables whose values differ from the file's: `Collection/name` → value on the screen.
    variables: BTreeMap<String, Value>,
    /// The collections switched to another mode: collection → mode.
    modes: BTreeMap<String, String>,
    /// The instances showing another variant than in the file: (instance, variant).
    variants: Vec<(NodeId, NodeId)>,
}

fn value_json(v: &VariableValue) -> Value {
    match v {
        VariableValue::Number(n) => json!(n),
        VariableValue::String(s) => json!(s),
        VariableValue::Boolean(b) => json!(b),
        VariableValue::Color(c) => json!(c.to_css_hex()),
        other => json!(other.display()),
    }
}

/// What the player changed in its copy `doc` of `file`, read the way the simulator reports its
/// own state (`proto_sim::state`).
fn player_facts(p: &Presenter) -> Facts {
    let (doc, file) = (p.design.as_ref().expect("the player has a document"), p.original.as_ref().expect("and the file it copied"));
    let variables = doc
        .variables
        .variables
        .iter()
        .filter(|v| file.variables.get_by_id(&v.id).is_none_or(|old| old.values != v.values))
        .filter_map(|v| {
            let collection = doc.variables.collection_by_id(&v.collection)?;
            Some((format!("{}/{}", collection.name, v.name), value_json(&doc.variables.resolve(&v.id, &doc.modes_for(p.current))?)))
        })
        .collect();
    let page = doc.page_of(p.current).expect("the screen is on a page");
    let (now, was) = (doc.node(page).unwrap(), file.node(page).unwrap());
    let modes = now
        .var_modes
        .iter()
        .filter(|(c, m)| was.var_modes.get(*c) != Some(*m))
        .filter_map(|(c, m)| {
            let c = doc.variables.collection_by_id(c)?;
            Some((c.name.clone(), c.mode_by_id(m)?.name.clone()))
        })
        .collect();
    Facts {
        screen: p.current,
        overlays: p.overlays.iter().map(|o| o.id).collect(),
        history: p.history.iter().map(|v| v.frame).collect(),
        variables,
        modes,
        variants: changed_variants(doc, file),
    }
}

fn changed_variants(doc: &Document, file: &Document) -> Vec<(NodeId, NodeId)> {
    doc.nodes()
        .filter_map(|n| {
            let NodeKind::Instance { component, .. } = &n.kind else { return None };
            let NodeKind::Instance { component: was, .. } = &file.node(n.id)?.kind else { return None };
            (was != component).then_some((n.id, *component))
        })
        .collect()
}

fn sim_facts(sim: &Sim) -> Facts {
    let st = sim.state();
    Facts {
        screen: sim.screen(),
        overlays: sim.overlays().iter().map(|o| o.id).collect(),
        history: sim.history().to_vec(),
        variables: st.variables,
        modes: st.modes,
        variants: st.variants.iter().map(|v| (v.instance, v.component)).collect(),
    }
}

/// The player and the simulator on the same file.
struct Pair {
    what: &'static str,
    u: Harness,
    sim: Sim,
    /// The layer the player's pointer holds the button down on, and where.
    held: Option<(NodeId, Pos2)>,
    /// The layer the player's pointer rests over (a hover), and where.
    over: Option<(NodeId, Pos2)>,
    steps: usize,
}

impl Pair {
    /// Present `s` from `start` in the app and start the simulator on the same document.
    fn new(what: &'static str, s: Session, start: NodeId) -> Pair {
        let mut u = Harness::new(s);
        u.run("proto.present", json!({"start": start}));
        u.frames(2, Vec::new);
        let sim = Sim::new(u.app.session.doc().unwrap(), start).unwrap();
        let pair = Pair { what, u, sim, held: None, over: None, steps: 0 };
        pair.agree("at the start");
        pair
    }

    fn player(&self) -> &Presenter {
        self.u.app.ui.present.as_ref().expect("the presenter is open")
    }

    /// The middle of the player's hotspot on `layer` (a layer of what shows on top, with an
    /// interaction of its own).
    fn hotspot(&self, layer: NodeId) -> Pos2 {
        let p = self.player();
        let hit = p.hot.iter().find(|(id, _)| *id == layer);
        hit.unwrap_or_else(|| panic!("{}: step {}: the player has no hotspot on {layer}: {:?}", self.what, self.steps, p.hot)).1.center()
    }

    /// A point on screen `dx, dy` from the top-left of the screen showing.
    fn on_screen(&self, dx: f64, dy: f64) -> Pos2 {
        let p = self.player();
        let (_, place, _) = p.shown.expect("the player drew a frame");
        let b = p.doc.as_ref().and_then(|d| d.world_bounds(p.current)).expect("the screen has bounds");
        place.pt(b.x0 + dx, b.y0 + dy)
    }

    /// Beside the screen: over the player, on no layer.
    fn away(&self) -> Pos2 {
        self.on_screen(-40.0, 200.0)
    }

    fn pointer(&mut self, at: Pos2, pressed: bool) {
        self.u.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }]);
    }

    /// The pointer lets go of whatever it holds or hovers and rests beside the screen. A held
    /// button comes up after dragging off the layer, which is no click.
    fn rest(&mut self) {
        let away = self.away();
        self.over = None;
        if let Some((_, at)) = self.held.take() {
            self.u.frame(vec![Event::PointerMoved(at + vec2(0.0, 20.0))]);
            self.u.frame(vec![Event::PointerMoved(away)]);
            self.pointer(away, false);
        }
        self.u.frame(vec![Event::PointerMoved(away)]);
        self.u.frames(2, Vec::new);
    }

    /// One thing the viewer does, on both sides, then the two must agree.
    ///
    /// | step | the player | the simulator |
    /// |---|---|---|
    /// | `Click` | pointer to the layer's hotspot, button down, button up | gesture `click` |
    /// | `Click` on the layer `Press` holds | button up | gesture `click` (the press ends first) |
    /// | `Hover` | pointer to the hotspot, left there | gesture `hover` |
    /// | `Press` | pointer to the hotspot, button down and held | gesture `press` |
    /// | `Key` | the key, pressed and released | `key` |
    /// | `Wait` | pointer off every layer, 1.5 s on the clock | `wait` |
    /// | `Outside` | a click at the screen's corner, outside the overlay | `clickOutside` |
    ///
    /// Before any step but the release of a held press, the player's pointer first lets go and
    /// moves off the layers, as the simulator ends a hover or press when the next step begins.
    fn step(&mut self, what: Do) {
        self.steps += 1;
        self.player_does(&what);
        let outcomes = self.sim_does(&what);
        self.agree(&format!("after step {} {what:?} (the simulator: {outcomes:?})", self.steps));
    }

    /// The simulator's side of a step.
    fn sim_does(&mut self, what: &Do) -> Vec<StepOutcome> {
        let gesture = |node, gesture| StepAction::Gesture { node, gesture };
        let actions = match what {
            Do::Click(n) => vec![gesture(*n, Gesture::Click)],
            Do::ClickWithMouseUp(n) => vec![gesture(*n, Gesture::MouseUp), gesture(*n, Gesture::Click)],
            Do::Hover(n) => vec![gesture(*n, Gesture::Hover)],
            Do::Press(n) => vec![gesture(*n, Gesture::Press)],
            Do::Key(k, shift) => vec![StepAction::Key { key: if *shift { format!("Shift+{}", k.name()) } else { k.name().to_string() } }],
            Do::Wait => vec![StepAction::Wait],
            Do::Outside => vec![StepAction::ClickOutside],
        };
        actions.iter().map(|a| self.sim.step(a)).collect()
    }

    /// The player's side of a step.
    fn player_does(&mut self, what: &Do) {
        match what.clone() {
            Do::Click(n) | Do::ClickWithMouseUp(n) if self.held.is_some_and(|(held, _)| held == n) => {
                let (_, at) = self.held.take().unwrap();
                self.pointer(at, false);
            }
            // The pointer is over the layer already (a hover step put it there): it clicks in place.
            Do::Click(n) | Do::ClickWithMouseUp(n) if self.over.is_some_and(|(over, _)| over == n) => {
                let (_, at) = self.over.take().unwrap();
                self.pointer(at, true);
                self.pointer(at, false);
            }
            Do::Click(n) | Do::ClickWithMouseUp(n) => {
                self.rest();
                let at = self.hotspot(n);
                self.u.frame(vec![Event::PointerMoved(at)]);
                self.pointer(at, true);
                self.pointer(at, false);
            }
            Do::Hover(n) => {
                self.rest();
                let at = self.hotspot(n);
                self.u.frame(vec![Event::PointerMoved(at)]);
                self.over = Some((n, at));
            }
            Do::Press(n) => {
                self.rest();
                let at = self.hotspot(n);
                self.u.frame(vec![Event::PointerMoved(at)]);
                self.pointer(at, true);
                self.held = Some((n, at));
            }
            Do::Key(key, shift) => {
                self.rest();
                let modifiers = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
                let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
                self.u.frame(vec![k(true), k(false)]);
            }
            Do::Wait => {
                self.rest();
                self.u.frames(90, Vec::new);
            }
            Do::Outside => {
                self.rest();
                let at = self.on_screen(4.0, 4.0);
                self.u.frame(vec![Event::PointerMoved(at)]);
                self.pointer(at, true);
                self.pointer(at, false);
            }
        }
        self.u.frames(3, Vec::new);
    }

    /// Time passes in the player while the pointer stays where it is (over a layer it hovers).
    fn player_lingers(&mut self) {
        self.u.frames(90, Vec::new);
    }

    fn run(&mut self, steps: impl IntoIterator<Item = Do>) {
        for s in steps {
            self.step(s);
        }
    }

    /// The player and the simulator say the same.
    fn agree(&self, when: &str) {
        let (player, sim) = (player_facts(self.player()), sim_facts(&self.sim));
        assert_eq!(player, sim, "{}: {when}: the player (left) and the simulator (right) disagree", self.what);
        // Everything else a layer can show (text, visibility, the layers of a variant): the
        // document as it plays is the same on both sides.
        let shown = serde_json::to_value(self.player().doc.as_ref().expect("the player has a document")).unwrap();
        let simulated = serde_json::to_value(self.sim.document()).unwrap();
        if shown != simulated {
            let differing: Vec<String> = match (shown["nodes"].as_object(), simulated["nodes"].as_object()) {
                (Some(a), Some(b)) => {
                    a.iter().filter(|(k, v)| b.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: {v} ≠ {}", b.get(k).unwrap_or(&Value::Null))).collect()
                }
                _ => Vec::new(),
            };
            panic!("{}: {when}: the document the player shows differs from the simulator's: {differing:#?}", self.what);
        }
    }

    fn screen(&self) -> NodeId {
        self.sim.screen()
    }

    fn count(&self) -> Option<f64> {
        self.sim.state().variables.get("State/count").and_then(Value::as_f64)
    }
}

#[test]
fn navigate_and_back() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let go = button(&mut s, "Go", a, [20.0, 20.0], navigate(b));
    let stay = button(&mut s, "Stay", a, [100.0, 20.0], navigate(a));
    let start_back = button(&mut s, "Back", a, [180.0, 20.0], json!({"type": "back"}));
    let back = button(&mut s, "Back", b, [20.0, 20.0], json!({"type": "back"}));
    let mut p = Pair::new("navigate and back", s, a);
    p.run([Do::Click(go)]);
    assert_eq!((p.screen(), p.sim.history()), (b, &[a][..]));
    p.run([Do::Click(back)]);
    assert_eq!((p.screen(), p.sim.history().len()), (a, 0));
    // Back with nothing to go back to, and navigating to the screen that shows, change nothing.
    p.run([Do::Click(start_back), Do::Click(stay), Do::Click(go), Do::Click(back)]);
    assert_eq!((p.screen(), p.sim.history().len()), (a, 0));
}

#[test]
fn an_overlay_opens_takes_clicks_and_closes() {
    // (the overlay closes on a click outside it)
    for close in [true, false] {
        let mut s = Session::new();
        let a = frame(&mut s, "A", 0.0);
        let m = sheet(&mut s, "Menu", 0.0);
        state_variables(&mut s);
        let show = button(&mut s, "Open", a, [20.0, 20.0], open(m, close));
        let inc = button(&mut s, "Add", m, [10.0, 10.0], add_one());
        let shut = button(&mut s, "Close", m, [80.0, 10.0], json!({"type": "closeOverlay"}));
        let mut p = Pair::new(if close { "overlay closing on outside clicks" } else { "overlay staying on outside clicks" }, s, a);
        p.run([Do::Click(show), Do::Click(inc), Do::Click(inc)]);
        assert_eq!((p.sim.overlays().len(), p.count()), (1, Some(2.0)), "clicks inside reach the overlay's layers");
        p.run([Do::Outside]);
        assert_eq!(p.sim.overlays().len(), usize::from(!close), "a click outside closes it only when it closes that way");
        if close {
            p.run([Do::Click(show)]);
        }
        p.run([Do::Click(shut)]);
        assert!(p.sim.overlays().is_empty());
        // With no overlay open a click on the screen where nothing reacts changes nothing.
        p.run([Do::Outside, Do::Click(show), Do::Click(inc)]);
        assert_eq!(p.count(), Some(3.0));
    }
}

#[test]
fn overlays_stack_and_swap() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let [m1, m2, m3] = [("M1", 0.0), ("M2", 200.0), ("M3", 400.0)].map(|(n, x)| sheet(&mut s, n, x));
    let show = button(&mut s, "Open", a, [20.0, 20.0], open(m1, true));
    let swap_none = button(&mut s, "Swap", a, [100.0, 20.0], json!({"type": "swapOverlay", "dest": m3}));
    let more = button(&mut s, "More", m1, [10.0, 10.0], open(m2, false));
    let back1 = button(&mut s, "Back", m1, [80.0, 10.0], json!({"type": "back"}));
    let swap = button(&mut s, "Swap", m2, [10.0, 10.0], json!({"type": "swapOverlay", "dest": m3}));
    let back3 = button(&mut s, "Back", m3, [10.0, 10.0], json!({"type": "back"}));
    let mut p = Pair::new("overlays stacking and swapping", s, a);
    let open_now = |p: &Pair| p.sim.overlays().iter().map(|o| o.id).collect::<Vec<_>>();
    p.run([Do::Click(show), Do::Click(more)]);
    assert_eq!(open_now(&p), [m1, m2], "an overlay opened from an overlay stacks on it");
    p.run([Do::Click(swap)]);
    assert_eq!(open_now(&p), [m1, m3], "swap replaces the top one");
    // The swapped-in overlay keeps how the one it replaced closes: not on outside clicks.
    p.run([Do::Outside]);
    assert_eq!(open_now(&p), [m1, m3]);
    p.run([Do::Click(back3)]);
    assert_eq!(open_now(&p), [m1], "back closes the top overlay first");
    p.run([Do::Click(back1)]);
    assert!(open_now(&p).is_empty());
    // Swap with none open opens it, closing on outside clicks.
    p.run([Do::Click(swap_none)]);
    assert_eq!(open_now(&p), [m3]);
    p.run([Do::Outside]);
    assert!(open_now(&p).is_empty());
}

#[test]
fn navigating_from_an_overlay() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let m = sheet(&mut s, "Menu", 0.0);
    let show = button(&mut s, "Open", a, [20.0, 20.0], open(m, true));
    let back_a = button(&mut s, "Back", a, [100.0, 20.0], json!({"type": "back"}));
    let go = button(&mut s, "Go", m, [10.0, 10.0], navigate(b));
    let same = button(&mut s, "Same", m, [80.0, 10.0], navigate(a));
    let back = button(&mut s, "Back", b, [20.0, 20.0], json!({"type": "back"}));
    let mut p = Pair::new("navigating from an overlay", s, a);
    p.run([Do::Click(show), Do::Click(go)]);
    assert_eq!((p.screen(), p.sim.overlays().len(), p.sim.history()), (b, 0, &[a][..]), "the screen under the overlay is replaced and it closes");
    p.run([Do::Click(back)]);
    assert_eq!(p.screen(), a);
    // Mirrored on purpose: navigating to the screen that shows, from an overlay over it, closes
    // the overlay and still puts the screen on the history, so the next Back stays on it.
    p.run([Do::Click(show), Do::Click(same)]);
    assert_eq!((p.screen(), p.sim.overlays().len(), p.sim.history()), (a, 0, &[a][..]));
    p.run([Do::Click(back_a)]);
    assert_eq!((p.screen(), p.sim.history().len()), (a, 0));
}

#[test]
fn change_to_toggles_an_interactive_component() {
    let mut s = Session::new();
    let (set, v) = variants(&mut s, &["Off", "On"]);
    on(&mut s, v[0], t("onClick"), json!({"type": "changeTo", "dest": v[1]}));
    on(&mut s, v[1], t("onClick"), json!({"type": "changeTo", "dest": v[0]}));
    let a = frame(&mut s, "A", 0.0);
    let toggle = instance(&mut s, set, a);
    let mut p = Pair::new("change to, twice", s, a);
    p.run([Do::Click(toggle)]);
    assert_eq!(sim_facts(&p.sim).variants, [(toggle, v[1])], "off → on");
    p.run([Do::Click(toggle)]);
    assert!(sim_facts(&p.sim).variants.is_empty(), "on → off: it played the interaction of the variant it showed");
    p.run([Do::Click(toggle)]);
    assert_eq!(sim_facts(&p.sim).variants, [(toggle, v[1])]);
}

#[test]
fn set_variable_and_a_conditional_reading_it_in_the_same_interaction() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let five = button(&mut s, "Five", a, [20.0, 20.0], json!({"type": "setVariable", "variable": "count", "value": 5}));
    let step = button(
        &mut s,
        "Step",
        a,
        [100.0, 20.0],
        json!([
            add_one(),
            {"type": "conditional", "blocks": [
                {"expression": "count >= 6", "actions": [navigate(b)]},
                {"actions": [{"type": "setVariable", "variable": "label", "value": "again"}]},
            ]},
        ]),
    );
    let mut p = Pair::new("set variable and conditional", s, a);
    p.run([Do::Click(step)]);
    assert_eq!((p.screen(), p.count()), (a, Some(1.0)), "an expression; the condition doesn't hold, so the else branch ran");
    assert_eq!(p.sim.state().variables.get("State/label"), Some(&json!("again")));
    p.run([Do::Click(five)]);
    assert_eq!(p.count(), Some(5.0), "a literal");
    p.run([Do::Click(step)]);
    assert_eq!((p.screen(), p.count()), (b, Some(6.0)), "the condition read what the same interaction had just set");
}

#[test]
fn set_variable_mode() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    ex(&mut s, "variable.createCollection", json!({"name": "Theme", "modes": ["Light", "Dark"]}));
    ex(&mut s, "variable.create", json!({"collection": "Theme", "name": "bg", "type": "color", "value": "#FFFFFF"}));
    ex(&mut s, "variable.set", json!({"variable": "bg", "mode": "Dark", "value": "#000000"}));
    ex(&mut s, "variable.bind", json!({"ids": [a], "field": "fill", "variable": "bg"}));
    let dark = button(&mut s, "Dark", a, [20.0, 20.0], json!({"type": "setVariableMode", "collection": "Theme", "mode": "Dark"}));
    let light = button(&mut s, "Light", a, [100.0, 20.0], json!({"type": "setVariableMode", "collection": "Theme", "mode": "Light"}));
    let mut p = Pair::new("set variable mode", s, a);
    p.run([Do::Click(dark)]);
    assert_eq!(p.sim.state().modes, BTreeMap::from([("Theme".to_string(), "Dark".to_string())]));
    let fill = |p: &Pair| p.player().doc.as_ref().unwrap().node(a).and_then(|n| n.fills.first()?.solid_color()).map(|c| c.to_hex());
    assert_eq!(fill(&p).as_deref(), Some("000000"), "the layer bound to the variable shows the mode's value in the player");
    p.run([Do::Click(light)]);
    assert_eq!(fill(&p).as_deref(), Some("FFFFFF"));
}

#[test]
fn a_key_trigger() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let first = rect(&mut s, "First", a, 20.0, 20.0);
    on(&mut s, first, json!({"type": "keyPress", "key": "K"}), navigate(b));
    // Another layer with the same key, later in layer order: the first one wins on both sides.
    let second = rect(&mut s, "Second", a, 100.0, 20.0);
    on(&mut s, second, json!({"type": "keyPress", "key": "K"}), navigate(a));
    let home = rect(&mut s, "Home", b, 20.0, 20.0);
    on(&mut s, home, json!({"type": "keyPress", "key": "Shift+K"}), json!({"type": "back"}));
    let mut p = Pair::new("a key trigger", s, a);
    p.run([Do::Key(Key::K, false)]);
    assert_eq!(p.screen(), b);
    // On B the plain key is nobody's; the combination goes back.
    p.run([Do::Key(Key::K, false)]);
    assert_eq!(p.screen(), b);
    p.run([Do::Key(Key::K, true)]);
    assert_eq!(p.screen(), a);
}

#[test]
fn after_delay_fires_in_order_and_chains_onto_the_next_screen() {
    let mut s = Session::new();
    let [a, b, c] = [("A", 0.0), ("B", 400.0), ("C", 800.0)].map(|(n, x)| frame(&mut s, n, x));
    state_variables(&mut s);
    on(&mut s, a, json!({"type": "afterDelay", "ms": 100}), navigate(b));
    // On B the later delay is listed first: the shorter one still fires first.
    on(&mut s, b, json!({"type": "afterDelay", "ms": 400}), navigate(c));
    let tick = rect(&mut s, "Tick", b, 20.0, 20.0);
    on(&mut s, tick, json!({"type": "afterDelay", "ms": 150}), add_one());
    let mut p = Pair::new("after delay", s, a);
    p.run([Do::Wait]);
    assert_eq!((p.screen(), p.count(), p.sim.history()), (c, Some(1.0), &[a, b][..]), "A's delay, then B's two, the shorter first");
    p.run([Do::Wait]);
    assert_eq!((p.screen(), p.count()), (c, Some(1.0)), "nothing is waiting on C");
}

#[test]
fn a_hover_shows_while_the_pointer_is_over_the_layer_and_reverts() {
    let mut s = Session::new();
    let (set, v) = variants(&mut s, &["Idle", "Hovered"]);
    on(&mut s, v[0], t("onHover"), json!({"type": "changeTo", "dest": v[1]}));
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let peek = rect(&mut s, "Peek", a, 20.0, 20.0);
    on(&mut s, peek, t("onHover"), navigate(b));
    let chip = instance(&mut s, set, a);
    let mut p = Pair::new("hover and its revert", s, a);
    p.run([Do::Hover(peek)]);
    assert_eq!((p.screen(), p.sim.history()), (b, &[a][..]), "while hovering");
    p.run([Do::Wait]);
    assert_eq!((p.screen(), p.sim.history().len()), (a, 0), "the pointer left: back as it was, the history too");
    p.run([Do::Hover(chip)]);
    assert_eq!(sim_facts(&p.sim).variants, [(chip, v[1])], "the instance shows its hovered variant");
    p.run([Do::Wait]);
    assert!(sim_facts(&p.sim).variants.is_empty());
}

#[test]
fn a_press_and_a_click_on_one_layer() {
    let mut s = Session::new();
    let [a, b, c] = [("A", 0.0), ("B", 400.0), ("C", 800.0)].map(|(n, x)| frame(&mut s, n, x));
    let hot = rect(&mut s, "Hot", a, 20.0, 20.0);
    on(&mut s, hot, t("onPress"), navigate(b));
    on(&mut s, hot, t("onClick"), navigate(c));
    let back = button(&mut s, "Back", c, [20.0, 100.0], json!({"type": "back"}));
    let mut p = Pair::new("press and click on one layer", s, a);
    p.run([Do::Press(hot)]);
    assert_eq!(p.screen(), b, "while the button is down");
    p.run([Do::Click(hot)]);
    assert_eq!((p.screen(), p.sim.history()), (c, &[a][..]), "the release ends the press and is the click");
    p.run([Do::Click(back)]);
    assert_eq!(p.screen(), a);
    // A press that is let go off the layer only reverts.
    p.run([Do::Press(hot), Do::Wait]);
    assert_eq!((p.screen(), p.sim.history().len()), (a, 0));
}

#[test]
fn an_instance_pressed_then_clicked() {
    let mut s = Session::new();
    let (set, v) = variants(&mut s, &["Default", "Pressed"]);
    on(&mut s, v[0], t("onPress"), json!({"type": "changeTo", "dest": v[1]}));
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    let inst = instance(&mut s, set, a);
    on(&mut s, inst, t("onClick"), navigate(b));
    let mut p = Pair::new("an instance pressed then clicked", s, a);
    p.run([Do::Press(inst)]);
    assert_eq!((p.screen(), sim_facts(&p.sim).variants), (a, vec![(inst, v[1])]), "the pressed variant while the button is down");
    p.run([Do::Click(inst)]);
    assert_eq!((p.screen(), sim_facts(&p.sim).variants), (b, Vec::new()), "the instance's own click, on the variant it showed before");
}

/// Home, and a section "Steps" holding One and Two. Home goes to the section, One to Two, Two
/// home. Returns the session, `[home, one, two]` and the three buttons in that order.
fn section_file() -> (Session, [NodeId; 3], [NodeId; 3]) {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0);
    let section = id(&ex(&mut s, "node.create", json!({"type": "section", "x": 1000, "y": 0, "width": 900, "height": 500, "name": "Steps"})));
    let (one, two) = (frame(&mut s, "One", 1020.0), frame(&mut s, "Two", 1420.0));
    ex(&mut s, "node.reparent", json!({"ids": [one, two], "parent": section}));
    let to_section = button(&mut s, "Steps", home, [20.0, 20.0], navigate(section));
    let next = button(&mut s, "Next", one, [20.0, 20.0], navigate(two));
    let leave = button(&mut s, "Home", two, [20.0, 100.0], navigate(home));
    (s, [home, one, two], [to_section, next, leave])
}

#[test]
fn navigating_to_a_section_shows_its_first_frame_then_the_one_shown_last() {
    let (s, [home, one, two], [to_section, next, leave]) = section_file();
    let mut p = Pair::new("navigate to a section", s, home);
    p.run([Do::Click(to_section)]);
    assert_eq!(p.screen(), one, "not visited yet: its first frame");
    p.run([Do::Click(next), Do::Click(leave), Do::Click(to_section)]);
    assert_eq!(p.screen(), two, "then the frame of it shown last");
    assert_eq!(p.sim.history(), &[home, one, two, home][..]);
}

#[test]
fn the_frame_a_prototype_starts_on_is_not_remembered_for_its_section() {
    // Mirrored on purpose: the section remembers frames as they arrive, and the start frame
    // doesn't arrive. Started on Two, the section still opens on its first frame.
    let (s, [home, one, two], [to_section, _, leave]) = section_file();
    let mut p = Pair::new("start frame and section memory", s, two);
    p.run([Do::Click(leave), Do::Click(to_section)]);
    assert_eq!((p.screen(), p.sim.history()), (one, &[two, home][..]));
}

#[test]
fn a_reverted_hover_is_still_remembered_by_the_section() {
    // Mirrored on purpose: a revert puts back the screen, the document, the overlays and the
    // history, not which frame each section showed last.
    let (mut s, [home, _, two], [to_section, ..]) = section_file();
    let peek = rect(&mut s, "Peek", home, 20.0, 200.0);
    on(&mut s, peek, t("onHover"), navigate(two));
    let mut p = Pair::new("a hover revert and section memory", s, home);
    p.run([Do::Hover(peek)]);
    assert_eq!(p.screen(), two);
    p.run([Do::Wait]);
    assert_eq!((p.screen(), p.sim.history().len()), (home, 0));
    p.run([Do::Click(to_section)]);
    assert_eq!(p.screen(), two, "the section opens on the frame the hover showed");
}

#[test]
fn a_reverted_hover_leaves_the_screens_delays_to_fire_again() {
    // Mirrored on purpose: arriving on a screen starts its delays over, and a revert doesn't put
    // back which had fired. A hover that showed another screen makes this one's delay fire twice.
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    on(&mut s, a, json!({"type": "afterDelay", "ms": 100}), add_one());
    let peek = rect(&mut s, "Peek", a, 20.0, 20.0);
    on(&mut s, peek, t("onHover"), navigate(b));
    let mut p = Pair::new("a hover revert and fired delays", s, a);
    p.run([Do::Wait]);
    assert_eq!(p.count(), Some(1.0), "once per arrival");
    p.run([Do::Wait]);
    assert_eq!(p.count(), Some(1.0));
    p.run([Do::Hover(peek)]);
    assert_eq!(p.screen(), b);
    p.run([Do::Wait]);
    assert_eq!((p.screen(), p.count()), (a, Some(2.0)), "back on A, whose delay fired again");
}

// ---------------------------------------------------------------------------------------------
// A lasting interaction (a click, a key, a delay) under a hover or press. In the player it acts
// on the state under the hover, as the simulator's steps do: what it did outlives the hover.

/// A with "Hot": while hovering, the label variable reads "hovered"; on click, what `click`
/// gives (it may add layers first).
fn hover_and_click(click: impl FnOnce(&mut Session) -> Value) -> (Session, NodeId, NodeId) {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    let hot = rect(&mut s, "Hot", a, 20.0, 20.0);
    on(&mut s, hot, t("onHover"), json!({"type": "setVariable", "variable": "label", "value": "hovered"}));
    let action = click(&mut s);
    on(&mut s, hot, t("onClick"), action);
    (s, a, hot)
}

fn label_of(f: &Facts) -> Option<&str> {
    f.variables.get("State/label").and_then(Value::as_str)
}

fn count_of(f: &Facts) -> Option<f64> {
    f.variables.get("State/count").and_then(Value::as_f64)
}

#[test]
fn a_click_that_opens_an_overlay_under_a_hover_keeps_it_open_when_the_pointer_leaves() {
    let mut menu = NodeId(0);
    let (s, a, hot) = hover_and_click(|s| {
        menu = sheet(s, "Menu", 0.0);
        open(menu, true)
    });
    let mut p = Pair::new("hover, then a click that opens an overlay", s, a);
    p.run([Do::Hover(hot)]);
    assert_eq!(label_of(&sim_facts(&p.sim)), Some("hovered"));
    p.run([Do::Click(hot)]);
    assert_eq!(sim_facts(&p.sim).overlays, [menu], "the overlay opened");
    assert_eq!(label_of(&sim_facts(&p.sim)), None, "under the overlay the layer is no longer hovered");
    // The pointer leaves the layer: the overlay stays (it used to close with the hover).
    p.run([Do::Wait]);
    assert_eq!(sim_facts(&p.sim).overlays, [menu]);
}

#[test]
fn a_click_that_leaves_the_screen_under_a_hover_takes_the_hover_off() {
    let mut b = NodeId(0);
    let (s, a, hot) = hover_and_click(|s| {
        b = frame(s, "B", 400.0);
        navigate(b)
    });
    let mut p = Pair::new("hover, then a click that navigates", s, a);
    p.run([Do::Hover(hot), Do::Click(hot)]);
    assert_eq!((p.screen(), label_of(&sim_facts(&p.sim))), (b, None), "on B, and the hover's change didn't come along");
}

#[test]
fn a_mouse_up_and_a_click_on_one_layer() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let hot = rect(&mut s, "Hot", a, 20.0, 20.0);
    on(&mut s, hot, t("onClick"), navigate(b));
    on(&mut s, hot, t("mouseUp"), add_one());
    let mut p = Pair::new("mouse up and click on one layer", s, a);
    // One click in the player; a mouse up and a click for the simulator (a gesture is one trigger).
    p.run([Do::ClickWithMouseUp(hot)]);
    assert_eq!((p.screen(), p.count()), (b, Some(1.0)), "both ran");
}

// ---------------------------------------------------------------------------------------------
// Documented differences. Each case states what the player does and what the simulator does,
// and asserts both, so a change on either side shows up here.

#[test]
fn a_hover_outlasts_a_click_on_the_same_screen() {
    // Once a documented difference: the simulator ended the hover when the click step began. Now
    // a click on the hovered layer keeps it there too: the pointer hasn't left, the hover shows
    // over the click's result, and when the pointer leaves the click's count stays.
    let (s, a, hot) = hover_and_click(|_| add_one());
    let mut p = Pair::new("hover, then a click on the same screen", s, a);
    p.run([Do::Hover(hot), Do::Click(hot)]);
    let sim = sim_facts(&p.sim);
    assert_eq!((count_of(&sim), label_of(&sim)), (Some(1.0), Some("hovered")), "the click counted; the hover still shows over it");
    // A second click while the pointer stays counts again, under the same hover.
    p.run([Do::Click(hot)]);
    assert_eq!((p.count(), label_of(&sim_facts(&p.sim))), (Some(2.0), Some("hovered")));
    p.run([Do::Wait]);
    assert_eq!((p.count(), label_of(&sim_facts(&p.sim))), (Some(2.0), None), "the hover went with the pointer, the count stayed");
}

#[test]
fn difference_a_delay_fires_under_a_hover_in_the_player_and_after_it_in_the_simulator() {
    // The player's clock runs while the pointer rests on a layer; the simulator's `wait` is a
    // step, so it ends the hover first. What the delay did is the same.
    let (mut s, a, hot) = hover_and_click(|_| json!({"type": "back"}));
    on(&mut s, a, json!({"type": "afterDelay", "ms": 600}), add_one());
    let mut p = Pair::new("a delay under a hover", s, a);
    p.run([Do::Hover(hot)]);
    p.player_lingers();
    p.sim_does(&Do::Wait);
    let (player, sim) = (player_facts(p.player()), sim_facts(&p.sim));
    assert_eq!((count_of(&player), count_of(&sim)), (Some(1.0), Some(1.0)), "the delay fired once on both");
    assert_eq!((label_of(&player), label_of(&sim)), (Some("hovered"), None), "the player still shows the hover; the simulator's wait ended it");
    // Once the pointer leaves they agree, and the delay's count stayed.
    p.run([Do::Wait]);
    assert_eq!((p.count(), label_of(&sim_facts(&p.sim))), (Some(1.0), None));
}

#[test]
fn a_click_the_hovered_variant_holds_runs() {
    // Once a documented difference ("can't be simulated"): the instance shows its hovered variant
    // while the pointer is over it, and only that variant holds the click. The player runs it;
    // the simulator ended the hover first and found nothing to react. Now both find the click on
    // what shows and run it on the state under the hover: the instance is selected, and since the
    // selected variant has no hover of its own, the hover is over.
    let mut s = Session::new();
    let (set, v) = variants(&mut s, &["Idle", "Hovered", "Selected"]);
    on(&mut s, v[0], t("onHover"), json!({"type": "changeTo", "dest": v[1]}));
    on(&mut s, v[1], t("onClick"), json!({"type": "changeTo", "dest": v[2]}));
    let a = frame(&mut s, "A", 0.0);
    let chip = instance(&mut s, set, a);
    let mut p = Pair::new("a click on the hovered variant", s, a);
    p.run([Do::Hover(chip)]);
    assert_eq!(sim_facts(&p.sim).variants, [(chip, v[1])], "hovered");
    p.run([Do::Click(chip)]);
    assert_eq!(sim_facts(&p.sim).variants, [(chip, v[2])], "the hovered variant's click ran: the instance is selected");
    // The pointer leaves: the instance stays selected on both sides.
    p.run([Do::Wait]);
    assert_eq!(sim_facts(&p.sim).variants, [(chip, v[2])]);
}

#[test]
fn a_click_under_a_hover_that_the_variant_keeps_lays_the_hover_back() {
    // A toggle whose two variants hover the same way: clicking while hovered flips it, and since
    // the variant it flipped to plays the same hover, the hover is laid back over the result.
    let mut s = Session::new();
    state_variables(&mut s);
    let (set, v) = variants(&mut s, &["Off", "On"]);
    for k in [0, 1] {
        on(&mut s, v[k], t("onHover"), json!({"type": "setVariable", "variable": "label", "value": "hovered"}));
    }
    on(&mut s, v[0], t("onClick"), json!({"type": "changeTo", "dest": v[1]}));
    on(&mut s, v[1], t("onClick"), json!({"type": "changeTo", "dest": v[0]}));
    let a = frame(&mut s, "A", 0.0);
    let toggle = instance(&mut s, set, a);
    let mut p = Pair::new("a toggle clicked under its hover", s, a);
    p.run([Do::Hover(toggle), Do::Click(toggle)]);
    let f = sim_facts(&p.sim);
    assert_eq!((f.variants.clone(), label_of(&f)), (vec![(toggle, v[1])], Some("hovered")), "on, and still hovered");
    p.run([Do::Click(toggle), Do::Wait]);
    let f = sim_facts(&p.sim);
    assert_eq!((f.variants.is_empty(), label_of(&f)), (true, None), "off again, and the hover went with the pointer");
}

#[test]
fn difference_a_mouse_down_that_does_something_takes_the_press_in_the_player() {
    // The player fires "mouse down" first when the button goes down, and when that did something
    // the "while pressing" of the same press isn't fired. A simulator step is one trigger: a
    // press step fires "while pressing" only (and a `mouseDown` step "mouse down" only).
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    let hot = rect(&mut s, "Hot", a, 20.0, 20.0);
    on(&mut s, hot, t("mouseDown"), add_one());
    on(&mut s, hot, t("onPress"), json!({"type": "setVariable", "variable": "label", "value": "pressed"}));
    let mut p = Pair::new("a mouse down and a press on one layer", s, a);
    p.player_does(&Do::Press(hot));
    p.sim_does(&Do::Press(hot));
    let (player, sim) = (player_facts(p.player()), sim_facts(&p.sim));
    assert_eq!((count_of(&player), label_of(&player)), (Some(1.0), None), "the player: the mouse down ran and took the press");
    assert_eq!((count_of(&sim), label_of(&sim)), (None, Some("pressed")), "the simulator: the press step fired while pressing only");
}

#[test]
fn difference_a_scroll_to_that_cant_scroll_is_nothing_to_the_player_and_fired_to_the_simulator() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let target = rect(&mut s, "Target", a, 20.0, 300.0);
    let jump = button(&mut s, "Jump", a, [20.0, 20.0], json!({"type": "scrollTo", "dest": target}));
    let mut p = Pair::new("a scroll-to with nothing to scroll", s, a);
    p.player_does(&Do::Click(jump));
    let outcome = p.sim_does(&Do::Click(jump)).remove(0);
    // The player: no frame around the target scrolls, so the action did nothing and the click
    // counts as one nothing reacted to (the hotspots flash).
    assert!(p.player().flash_until > p.player().clock, "the player flashes the hotspots");
    // The simulator doesn't model scroll positions: it reports the action as run.
    assert!(outcome.fired && outcome.problem.is_none() && outcome.effects.iter().any(|e| e.contains("scroll to `Target`")), "{outcome:?}");
    p.agree("the state is the same either way");
}

#[test]
fn difference_change_to_a_layer_that_is_not_a_component() {
    let mut s = Session::new();
    let (set, _) = variants(&mut s, &["Off", "On"]);
    let plain = sheet(&mut s, "Not a component", 600.0);
    let a = frame(&mut s, "A", 0.0);
    let toggle = instance(&mut s, set, a);
    on(&mut s, toggle, t("onClick"), json!({"type": "changeTo", "dest": plain}));
    let mut p = Pair::new("change to a non-component", s, a);
    p.player_does(&Do::Click(toggle));
    let outcome = p.sim_does(&Do::Click(toggle)).remove(0);
    // The simulator refuses and says why.
    assert!(!outcome.fired && outcome.problem.as_deref().is_some_and(|why| why.contains("isn't a component")), "{outcome:?}");
    assert!(sim_facts(&p.sim).variants.is_empty());
    // The player points the instance at the layer all the same.
    assert_eq!(player_facts(p.player()).variants, [(toggle, plain)], "the player repoints the instance");
}

#[test]
fn difference_restart_forgets_the_sections_which_only_a_new_simulator_does() {
    let (s, [home, one, two], [to_section, next, leave]) = section_file();
    let mut p = Pair::new("restart and section memory", s, home);
    p.run([Do::Click(to_section), Do::Click(next), Do::Click(leave)]);
    // The player's own R key restarts it: the screen it started on, no history, no memory of
    // which frame each section showed. The simulator has no restart step.
    p.player_does(&Do::Key(Key::R, false));
    assert_eq!((p.player().current, p.player().history.len()), (home, 0));
    p.player_does(&Do::Click(to_section));
    assert_eq!(p.player().current, one, "the player: the section opens on its first frame again");
    p.sim_does(&Do::Click(to_section));
    assert_eq!(p.sim.screen(), two, "the simulator that kept running remembers the frame shown last");
    // A restart is a new run: a new simulator agrees with the restarted player.
    p.sim = Sim::new(p.u.app.session.doc().unwrap(), home).unwrap();
    p.sim_does(&Do::Click(to_section));
    p.agree("after a restart, against a new simulator");
}
