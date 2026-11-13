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
//!   first reverts to the state before it.
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

use std::collections::{HashMap, HashSet};

use figcraft_color::Color;
use figcraft_doc::design::{Expect, StepAction};
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
    /// A hover or press in effect: the next step reverts to this first.
    temp: Option<Snapshot>,
}

impl Sim {
    /// Start playing `doc` (a copy of it) at top-level frame `start`. The error says what `start`
    /// is instead.
    pub fn new(doc: &Document, start: NodeId) -> Result<Sim, String> {
        let Some(n) = doc.node(start) else { return Err(format!("no layer {start}")) };
        if doc.top_level_frame(start) != Some(start) {
            return Err(format!("`{}` ({start}) isn't a top-level frame", n.name));
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
        let Some(s) = self.temp.take() else { return false };
        self.screen = s.screen;
        self.design = s.design;
        self.overlays = s.overlays;
        self.history = s.history;
        self.relayout();
        true
    }
}

/// `` `name` (id) `` for messages; `layer <id>` when it doesn't exist.
pub(crate) fn label(doc: &Document, id: NodeId) -> String {
    match doc.node(id) {
        Some(n) => format!("`{}` ({id})", n.name),
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
