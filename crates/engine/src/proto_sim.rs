//! The prototype's state machine without a window: which screen shows, which overlays are open,
//! what the variables and instances have become, after a sequence of triggers. The player in the
//! UI crate animates and scrolls; this follows the same rules for what state comes next, so a
//! scenario can be run and checked headless.
//!
//! A [`Sim`] works on its own copy of the document, as the player does: the file it was made from
//! never changes. [`Sim::step`] takes one thing the viewer does ([`StepAction`]) and says what it
//! set off; [`Sim::check`] compares the state with what is expected; [`Sim::state`] reports it.
//!
//! # Where this follows the player, and where it can't
//!
//! The rules for navigation, history, overlays, "change to", variables, variable modes,
//! conditionals, section memory and "after delay" bookkeeping are the player's, action for action
//! (`ui-egui`'s `present/actions.rs` and `present/navigate.rs`), through the same
//! [`crate::proto_run`] helpers. What needs a pointer position, a clock or a screen size is
//! approximated, and each approximation is a documented limit:
//!
//! - **Which layer takes a gesture.** The player hit-tests: the top-most hotspot under the pointer
//!   that has the trigger takes it, and a layer without that trigger lets it through to what is
//!   under it. Without geometry, a gesture names a layer, and the interaction is the first one
//!   with the trigger on that layer or, failing that, on each layer around it up to the screen or
//!   overlay that shows. A hotspot that merely overlaps the layer (a sibling drawn over it) is not
//!   considered, and neither is whether the layer is scrolled or clipped out of view.
//! - **Hover and press are temporary.** In the player a "while hovering" or "while pressing"
//!   change reverts when the pointer leaves or lifts. Here it holds until the next step, which
//!   first reverts to the state before it, except a click, mouse down or mouse up on the hovered
//!   layer (or one inside it), where the pointer still is. That one is played as the player plays
//!   a click under a hover: the interaction is found on what shows (the hovered state, so the
//!   hovered variant's own click counts), it runs on the state under the hover, and when the same
//!   screen and overlays still show and the layer still plays that hover, the hover is laid back
//!   over the result and stays until the next step; otherwise it is over.
//! - **A gesture is one trigger.** A click fires "on click" only: the mouse down, "while
//!   pressing" and mouse up the player would also see for the same press are separate gestures.
//! - **A drag has no direction.** "On drag" fires whatever way the animation goes.
//! - **Time is not measured.** [`StepAction::Wait`] lets every pending "after delay" of what shows
//!   fire, the shortest first, and follows the chain they set off; the delay of a mouse enter,
//!   leave, down or up trigger is not waited out.
//! - **Scrolling, devices and responsive layout are not modelled.** "Scroll to" and "open link"
//!   change no state and are only reported; frames keep the size they have in the file.
//! - **The player's own keys** (R to restart, the arrows stepping through frames) are not part of
//!   the prototype: a key no interaction uses does nothing here.
//! - **Widgets** are not clicked.
//!
//! Where the two are known to give different answers for the same file (each pinned by the
//! `ui-egui` parity test named after it, `present/sim_parity_tests.rs`):
//!
//! - **A mouse down that does something takes the press.** In the player, pressing fires "mouse
//!   down" first, and when that did something the "while pressing" of the same press isn't
//!   fired. Here a press step fires "while pressing" whatever a "mouse down" on the layer would
//!   do (each gesture is one trigger). (Releasing is not like this: the player runs a "mouse up"
//!   and then the "on click" of the same release, which here are two steps, `mouseUp` then
//!   `click`, and agree.) `difference_a_mouse_down_that_does_something_takes_the_press_in_the_player`
//! - **A scroll-to that can't scroll.** The player counts a "scroll to" as having done nothing
//!   when no frame around its layer can scroll there, so a click whose only action is such a
//!   scroll falls through to whatever is under it. Here a "scroll to" a layer that shows counts as
//!   fired: scroll positions aren't known.
//!   `difference_a_scroll_to_that_cant_scroll_is_nothing_to_the_player_and_fired_to_the_simulator`
//! - **"Change to" something that isn't a component.** The player points the instance at the
//!   layer all the same, leaving it without a main component. Here the action is refused and
//!   reported as a problem. `difference_change_to_a_layer_that_is_not_a_component`
//! - **A delay that fires while hovering.** In the player an "after delay" that fires while a
//!   "while hovering" change is in effect runs under that change and lays it back. Here a wait
//!   is a step that takes the pointer away: it first ends the hover or press of the step before.
//!   (A key pressed while the pointer rests on a hovered layer is the same: a key step ends the
//!   hover first.) `difference_a_delay_fires_under_a_hover_in_the_player_and_after_it_in_the_simulator`
//! - **Restart.** The player's restart forgets which frame each section showed last and starts
//!   from the file as the editor has it. There is no step for it: a run that needs a fresh start
//!   is a new [`Sim`]. `difference_restart_forgets_the_sections_which_only_a_new_simulator_does`
//!
//! A "change to" settles as the player's does: variable bindings are applied again, so an instance
//! whose variant property is bound to a string variable (`componentProperties/<prop>`) is put
//! back to the variant the variable names, read in the instance's modes (a value that names no
//! option of the property changes nothing). The step's effect says so ("`State` is bound to
//! variable `state`, which keeps it at State=Default"), and the state shows the variant the
//! instance really shows (review X-02; `ui-egui`'s parity case
//! `change_to_on_an_instance_whose_variant_a_variable_keeps`).
//!
//! Layers that exist only after a "change to" (the layers of the variant an instance comes to
//! show) get their ids when the simulator's copy is changed, so a stored scenario can't name
//! them: name the instance, and say what it must show with `expect.variants`.
//!
//! `raise_fixed`, `relayout` and `settle` repeat what the player does to its own copy (`ui-egui`'s
//! `present`); the two are kept in step by hand until the player calls one shared implementation.

use std::collections::{HashMap, HashSet};

use figcraft_color::Color;
use figcraft_doc::design::{Expect, Gesture, StepAction};
use figcraft_doc::proto::{OverlayAnchor, OverlayPosition, ScrollPosition};
use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_geom::Rect;
use serde::Serialize;

mod actions;
mod expect;
mod render;
mod state;
mod steps;
mod vars;

#[cfg(test)]
pub(crate) mod tests;

pub use render::{render, render_png};
pub use state::{OverlayInfo, State, VariantInfo};
pub(crate) use steps::gesture_name;
pub use vars::find_variable;

/// The most "after delay" interactions one [`StepAction::Wait`] follows before it reports a loop.
pub const MAX_DELAY_CHAIN: usize = 100;

/// An open overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub id: NodeId,
    /// A click outside it closes it.
    pub close_on_outside: bool,
    pub position: OverlayPosition,
    /// The colour dimming what is under it.
    pub background: Option<Color>,
    /// The layer that opened it, as it sits on the screen (page units), when a gesture opened it.
    pub anchor: Option<Rect>,
    /// A manual position: the overlay's top-left from the anchor's.
    pub offset: Option<[f64; 2]>,
    pub anchor_to: OverlayAnchor,
}

/// What one step set off.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StepOutcome {
    /// Something happened: an action ran and did something.
    pub fired: bool,
    /// What happened, in order, in words.
    pub effects: Vec<String>,
    /// Why the step couldn't do what it says, or an action that couldn't run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// The state a "while hovering / pressing" interaction reverts to.
#[derive(Clone)]
struct Snapshot {
    screen: NodeId,
    design: Document,
    overlays: Vec<Overlay>,
    history: Vec<NodeId>,
}

/// A "while hovering / pressing" interaction in effect (the player's `Temp`).
#[derive(Clone)]
struct Temp {
    /// What it reverts to.
    snap: Snapshot,
    /// The layer it is on, as it plays (an instance playing its variant's interaction).
    node: NodeId,
    /// A press (else a hover).
    press: bool,
    /// Its actions, to lay it back over what a lasting interaction under it did.
    actions: Vec<figcraft_doc::proto::Action>,
}

/// A prototype being played headless.
pub struct Sim {
    /// The file as it was given, to tell what the prototype changed.
    original: Document,
    /// The player's copy as designed, with what the prototype changed (variables, variants).
    design: Document,
    /// `design` as it plays: instances carry their variants' interactions, fixed layers are on top.
    shown: Document,
    screen: NodeId,
    overlays: Vec<Overlay>,
    history: Vec<NodeId>,
    /// The frame shown last in each section (navigating to a section returns there).
    sections: HashMap<NodeId, NodeId>,
    /// The "after delay" interactions that fired since the screen arrived (an overlay's: since it
    /// opened).
    delays_fired: HashSet<(NodeId, usize)>,
    /// A hover or press in effect: the next step reverts to it first, unless the step is a click
    /// on the hovered layer (see the module docs).
    temp: Option<Temp>,
}

impl Sim {
    /// Start playing `doc` (a copy of it) at top-level frame `start`. The error says what `start`
    /// is instead.
    pub fn new(doc: &Document, start: NodeId) -> Result<Sim, String> {
        let Some(n) = doc.node(start) else { return Err(format!("no layer {start}")) };
        if doc.top_level_frame(start) != Some(start) {
            return Err(format!("`{}` ({start}) isn't a top-level frame", crate::error::prose(&n.name)));
        }
        let mut sim = Sim {
            original: doc.clone(),
            design: doc.clone(),
            shown: doc.clone(),
            screen: start,
            overlays: Vec::new(),
            history: Vec::new(),
            sections: HashMap::new(),
            delays_fired: HashSet::new(),
            temp: None,
        };
        sim.relayout();
        Ok(sim)
    }

    /// The document as it plays now: the copy with what the prototype changed.
    pub fn document(&self) -> &Document {
        &self.shown
    }

    /// The screen showing.
    pub fn screen(&self) -> NodeId {
        self.screen
    }

    /// The open overlays, bottom first.
    pub fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }

    /// The screens "back" returns to, oldest first.
    pub fn history(&self) -> &[NodeId] {
        &self.history
    }

    /// What takes gestures, keys and timers: the top overlay, else the screen.
    pub fn active_root(&self) -> NodeId {
        self.overlays.last().map_or(self.screen, |o| o.id)
    }

    /// What `expect` asks for that doesn't hold, each naming what was expected and what is.
    pub fn check(&self, expect: &Expect) -> Vec<String> {
        expect::check(self, expect)
    }

    /// The state, for reports.
    pub fn state(&self) -> State {
        state::of(self)
    }

    /// One thing the viewer does. A hover or press left by the step before is reverted first.
    pub fn step(&mut self, action: &StepAction) -> StepOutcome {
        steps::step(self, action)
    }

    /// `design` as it plays: instances play their variants' interactions and fixed layers sit on
    /// top (the player's `relayout`, without the resizing a device or responsive prototype does).
    fn relayout(&mut self) {
        let mut d = self.design.clone();
        crate::proto_run::play_interactions(&mut d);
        raise_fixed(&mut d);
        self.shown = d;
    }

    /// After an action changed `design` (since `before`): text resizes, instances follow and auto
    /// layout reflows as after an edit, then [`relayout`](Self::relayout) (the player's `settle`).
    fn settle(&mut self, before: &Document) {
        crate::normalize(&mut self.design, before);
        self.relayout();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { screen: self.screen, design: self.design.clone(), overlays: self.overlays.clone(), history: self.history.clone() }
    }

    /// Undo a hover or press (the player's `revert`): the screen, the document, the overlays and
    /// the history go back; what was remembered about sections and fired delays stays.
    fn revert(&mut self) -> bool {
        let Some(Temp { snap: s, .. }) = self.temp.take() else { return false };
        self.screen = s.screen;
        self.design = s.design;
        self.overlays = s.overlays;
        self.history = s.history;
        self.relayout();
        true
    }

    /// Whether the hover in effect stays for `action`: a click, a mouse down or a mouse up on the
    /// layer the hover is on or on one inside it, where the pointer still is. A press ends as the
    /// button comes up, and any other step takes the pointer away or lets time pass.
    fn hover_stays_for(&self, action: &StepAction) -> bool {
        let Some(t) = self.temp.as_ref().filter(|t| !t.press) else { return false };
        let StepAction::Gesture { node, gesture } = action else { return false };
        matches!(gesture, Gesture::Click | Gesture::MouseDown | Gesture::MouseUp) && (*node == t.node || self.shown.is_ancestor(t.node, *node))
    }

    /// Before a lasting interaction (a click, a mouse down or up) runs while a hover shows (the
    /// player's `lift_temp`): take the hover off, so that the interaction acts on the state under
    /// it and what it does outlives the hover. Only a hover that changed nothing but the document
    /// (the same screen and overlays show) is lifted.
    fn lift(&mut self) -> Option<Temp> {
        let t = self.temp.as_ref()?;
        if t.snap.screen != self.screen || !same_overlays(&t.snap.overlays, &self.overlays) {
            return None;
        }
        let t = self.temp.take()?;
        self.design = t.snap.design.clone();
        self.history.clone_from(&t.snap.history);
        self.relayout();
        Some(t)
    }
}

/// Whether two overlay stacks hold the same overlays in the same order.
fn same_overlays(a: &[Overlay], b: &[Overlay]) -> bool {
    a.iter().map(|o| o.id).eq(b.iter().map(|o| o.id))
}

/// `` `name` (id) `` for messages; `layer <id>` when it doesn't exist. The name is shown as the
/// engine's other messages show document text ([`crate::error::prose`]): one line, clipped, with
/// no backtick of its own, so a layer's name can't break the sentence or pass for part of it.
pub(crate) fn label(doc: &Document, id: NodeId) -> String {
    match doc.node(id) {
        Some(n) => format!("`{}` ({id})", crate::error::prose(&n.name)),
        None => format!("layer {id}"),
    }
}

/// Fixed layers take clicks (and draw) above the content that scrolls under them: each frame's
/// fixed children move to the top of its children, keeping their order. The player does the same
/// to its copy (`ui-egui`'s `present::raise_fixed`); the order decides which layer a key or a
/// timer finds first.
fn raise_fixed(doc: &mut Document) {
    let fixed = |c: &NodeId| doc.node(*c).is_some_and(|c| c.scroll_position == ScrollPosition::Fixed);
    let moves: Vec<(NodeId, Vec<NodeId>)> = doc
        .nodes()
        .filter(|n| !matches!(n.kind, NodeKind::Page { .. }) && n.children.iter().any(fixed))
        .filter_map(|n| {
            let (top, mut order): (Vec<NodeId>, Vec<NodeId>) = n.children.iter().partition(|c| fixed(c));
            order.extend(top);
            (order != n.children).then_some((n.id, order))
        })
        .collect();
    for (id, order) in moves {
        if let Some(n) = doc.node_mut(id) {
            n.children = order;
        }
    }
}
