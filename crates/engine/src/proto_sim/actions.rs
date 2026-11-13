//! Running an interaction's actions: the player's `fire`, `perform`, `go` and `close_overlay`
//! (`ui-egui`'s `present/actions.rs` and `present/navigate.rs`) without animation or scrolling.

use figcraft_doc::proto::{Action, MAX_ACTION_DEPTH, OverlayAnchor, OverlayPosition, Trigger};
use figcraft_doc::{NodeId, NodeKind};
use figcraft_geom::Rect;
use serde_json::Value;

use super::vars::{full_name, value_on, value_text};
use super::{Overlay, Sim, Temp, label};
use crate::proto_run;

/// What the actions of one step did, in words.
#[derive(Default)]
pub(super) struct Log {
    /// What happened, and what ran without changing anything.
    pub effects: Vec<String>,
    /// Actions that couldn't run.
    pub problems: Vec<String>,
    /// Why actions that ran changed nothing.
    pub idle: Vec<String>,
}

impl Log {
    /// An action did `what`.
    fn did(&mut self, what: String) -> bool {
        self.effects.push(what);
        true
    }

    /// An action ran and changed nothing, because `why`.
    fn nothing(&mut self, why: String) -> bool {
        self.effects.push(why.clone());
        self.idle.push(why);
        false
    }

    /// An action couldn't run, because `why`.
    fn failed(&mut self, why: String) -> bool {
        self.problems.push(why);
        false
    }
}

impl Sim {
    /// Show `frame` (the player's `go` and `arrive`): overlays close, timers start over, and with
    /// `remember` the screen being left goes on the history.
    fn go(&mut self, frame: NodeId, remember: bool) {
        if remember {
            self.history.push(self.screen);
        }
        self.overlays.clear();
        self.screen = frame;
        proto_run::remember_sections(&self.shown, self.screen, &mut self.sections);
        self.delays_fired.clear();
    }

    /// Whether `id` is the screen, an open overlay, or a layer inside one of them.
    pub(super) fn on_show(&self, id: NodeId) -> bool {
        std::iter::once(self.screen).chain(self.overlays.iter().map(|o| o.id)).any(|root| root == id || self.shown.is_ancestor(root, id))
    }

    /// Where layer `id` sits on the screen (page units of the screen): its own bounds on the
    /// screen, or moved with the overlay holding it to where that overlay is placed.
    pub(super) fn rect_on_screen(&self, id: NodeId) -> Option<Rect> {
        let b = self.shown.world_bounds(id)?;
        if id == self.screen || self.shown.is_ancestor(self.screen, id) {
            return Some(b);
        }
        let o = self.overlays.iter().rev().find(|o| o.id == id || self.shown.is_ancestor(o.id, id))?;
        let placed = super::render::overlay_rect(self, o)?;
        let at = self.shown.world_bounds(o.id)?;
        Some(b + figcraft_geom::Vec2::new(placed.x0 - at.x0, placed.y0 - at.y0))
    }

    /// A newly opened overlay's timers start over.
    fn restart_delays_in(&mut self, overlay: NodeId) {
        let doc = &self.shown;
        self.delays_fired.retain(|(id, _)| !doc.is_ancestor(overlay, *id));
    }
}

/// Run the actions of interaction `k` on `node` (the player's `fire`). `rect` is where the layer
/// sits on the screen when a pointer gesture fired it. A "while hovering / pressing" interaction
/// that did something leaves the state to revert to in `sim.temp`. The interaction is the one
/// `node` plays in what shows; a lasting one that fires while a hover is in effect (a click on
/// the hovered layer: the hovered variant's own click, say) runs on the state under the hover,
/// which is laid back over the result afterwards (the player's `lift_temp` and `lay_temp`).
pub(super) fn fire(sim: &mut Sim, node: NodeId, k: usize, rect: Option<Rect>, log: &mut Log) -> bool {
    let Some(it) = sim.shown.node(node).and_then(|n| n.interactions.get(k)).cloned() else { return false };
    let temporary = matches!(it.trigger, Trigger::OnHover | Trigger::OnPress);
    let lifted = if temporary { None } else { sim.lift() };
    let snap = (temporary && sim.temp.is_none()).then(|| sim.snapshot());
    if it.actions.is_empty() {
        log.nothing(format!("the interaction on {} has no actions", label(&sim.shown, node)));
    }
    let mut did = false;
    for a in &it.actions {
        did |= perform(sim, a, node, rect, 0, log);
    }
    if let Some(t) = lifted {
        lay_back(sim, t, log);
    }
    if did
        && rect.is_some()
        && let Some(s) = snap
    {
        sim.temp = Some(Temp { snap: s, node, press: it.trigger == Trigger::OnPress, actions: it.actions.clone() });
    }
    did
}

/// After a lasting interaction ran under a lifted hover: when the same screen and overlays still
/// show and the layer still plays that hover, it goes back over what the interaction left (its
/// actions run again, and the state to revert to is the interaction's result); otherwise the
/// hover is over (an instance the click changed to a variant without it, a click that opened an
/// overlay or left the screen).
fn lay_back(sim: &mut Sim, mut t: Temp, log: &mut Log) {
    let trigger = if t.press { Trigger::OnPress } else { Trigger::OnHover };
    let plays = sim.shown.node(t.node).is_some_and(|n| n.interactions.iter().any(|it| it.trigger == trigger && it.actions == t.actions));
    let what = label(&sim.shown, t.node);
    if sim.temp.is_some() || t.snap.screen != sim.screen || !super::same_overlays(&t.snap.overlays, &sim.overlays) || !plays {
        log.effects.push(format!("the hover on {what} ended"));
        return;
    }
    let snap = sim.snapshot();
    let rect = sim.rect_on_screen(t.node);
    // Laying it back is not something the step did: what its actions say isn't reported again.
    let mut again = Log::default();
    for a in &t.actions.clone() {
        perform(sim, a, t.node, rect, 0, &mut again);
    }
    t.snap = snap;
    sim.temp = Some(t);
    log.effects.push(format!("the hover on {what} shows again over the result: the pointer is still over it"));
}

/// One action of an interaction on `node` (the player's `perform_at`); `depth` counts the
/// conditional actions it is inside. True when it did something.
fn perform(sim: &mut Sim, a: &Action, node: NodeId, rect: Option<Rect>, depth: usize, log: &mut Log) -> bool {
    match a {
        Action::Navigate { dest, .. } => navigate(sim, *dest, log),
        Action::Back => back(sim, log),
        Action::ScrollTo { dest, .. } => scroll_to(sim, *dest, log),
        Action::OpenOverlay { dest, position, close_on_outside, background, offset, anchor } => {
            let overlay = Overlay {
                id: *dest,
                close_on_outside: *close_on_outside,
                position: *position,
                background: *background,
                anchor: rect,
                offset: *offset,
                anchor_to: *anchor,
            };
            open_overlay(sim, overlay, log)
        }
        Action::SwapOverlay { dest } => swap_overlay(sim, *dest, rect, log),
        Action::CloseOverlay => match sim.overlays.pop() {
            Some(o) => log.did(format!("close overlay {}", label(&sim.shown, o.id))),
            None => log.nothing("close overlay: no overlay is open".into()),
        },
        Action::OpenUrl { url, .. } => open_url(url, log),
        Action::ChangeTo { dest } => change_to(sim, node, *dest, log),
        Action::SetVariable { variable, value, expr } => {
            let before = sim.design.clone();
            if !proto_run::set_variable(&mut sim.design, node, variable, value, expr.as_ref()) {
                return log.failed(unset_reason(sim, variable, value, expr.is_some()));
            }
            sim.settle(&before);
            log.did(set_text(sim, variable, node))
        }
        Action::SetVariableMode { collection, mode } => set_mode(sim, collection, mode, log),
        Action::Conditional { blocks } => {
            if depth >= MAX_ACTION_DEPTH {
                return log.failed(format!("conditional: nested more than {MAX_ACTION_DEPTH} deep, the player stops there"));
            }
            // Evaluated now: earlier actions of the interaction may have changed variables.
            let inner = proto_run::branch(&sim.shown, node, blocks);
            if inner.is_empty() {
                return log.nothing("conditional: no branch with actions applies".into());
            }
            let mut did = false;
            for a in &inner {
                did |= perform(sim, a, node, rect, depth + 1, log);
            }
            did
        }
    }
}

/// "Scroll to" moves no state here: it is reported, and refused when its layer isn't on what
/// shows (the player would scroll frames nobody sees).
fn scroll_to(sim: &Sim, dest: NodeId, log: &mut Log) -> bool {
    if !sim.shown.contains(dest) {
        return log.failed(format!("scroll to: layer {dest} doesn't exist"));
    }
    if !sim.on_show(dest) {
        let (dest, root) = (label(&sim.shown, dest), label(&sim.shown, sim.active_root()));
        return log.failed(format!("scroll to {dest}: it isn't on {root} or anything else that shows"));
    }
    log.did(format!("scroll to {} (scroll positions aren't simulated)", label(&sim.shown, dest)))
}

fn open_overlay(sim: &mut Sim, overlay: Overlay, log: &mut Log) -> bool {
    let dest = overlay.id;
    if !sim.shown.contains(dest) {
        return log.failed(format!("open overlay: layer {dest} doesn't exist"));
    }
    sim.restart_delays_in(dest);
    sim.overlays.push(overlay);
    log.did(format!("open overlay {}", label(&sim.shown, dest)))
}

/// "Open link" leaves the prototype: it is reported. The player counts the action as run even
/// for an address it refuses to open, which is a problem to report all the same.
fn open_url(url: &str, log: &mut Log) -> bool {
    let shown = figcraft_format::clip(url, 200);
    if crate::doc_urls::openable(url) {
        return log.did(format!("open link {shown}"));
    }
    log.failed(format!("open link: `{shown}` isn't a web or mail address, so the player opens nothing"));
    true
}

fn set_mode(sim: &mut Sim, collection: &str, mode: &str, log: &mut Log) -> bool {
    let before = sim.design.clone();
    if !proto_run::set_mode_by_id(&mut sim.design, sim.screen, collection, mode) {
        return log.failed(match sim.design.variables.collection_by_id(collection) {
            Some(c) => format!("set variable mode: collection `{}` has no mode `{mode}`", c.name),
            None => format!("set variable mode: there is no collection `{collection}`"),
        });
    }
    sim.settle(&before);
    let names = sim.design.variables.collection_by_id(collection).map(|c| (c.name.as_str(), c.mode_by_id(mode).map_or(mode, |m| m.name.as_str())));
    let (c, m) = names.unwrap_or((collection, mode));
    log.did(format!("set mode of `{c}` to `{m}`"))
}

fn navigate(sim: &mut Sim, dest: NodeId, log: &mut Log) -> bool {
    let Some(top) = proto_run::navigate_target(&sim.shown, dest, &sim.sections).filter(|t| sim.shown.contains(*t)) else {
        return log.failed(match sim.shown.node(dest) {
            Some(n) => format!("navigate: section `{}` ({dest}) has no frame to show", n.name),
            None => format!("navigate: layer {dest} doesn't exist"),
        });
    };
    if top == sim.screen && sim.overlays.is_empty() {
        return log.nothing(format!("navigate to {}: it is already showing", label(&sim.shown, top)));
    }
    sim.go(top, true);
    log.did(format!("navigate to {}", label(&sim.shown, top)))
}

/// "Back" closes the top overlay first, else returns to the screen before this one.
fn back(sim: &mut Sim, log: &mut Log) -> bool {
    if let Some(o) = sim.overlays.pop() {
        return log.did(format!("back: close overlay {}", label(&sim.shown, o.id)));
    }
    match sim.history.pop() {
        Some(frame) => {
            sim.go(frame, false);
            log.did(format!("back to {}", label(&sim.shown, frame)))
        }
        None => log.nothing(format!("back: {} is where the prototype started, there is no screen to go back to", label(&sim.shown, sim.screen))),
    }
}

/// "Swap overlay" replaces the top overlay, keeping its place and how it closes; with none open
/// it opens `dest` centred, closing on outside clicks.
fn swap_overlay(sim: &mut Sim, dest: NodeId, rect: Option<Rect>, log: &mut Log) -> bool {
    if !sim.shown.contains(dest) {
        return log.failed(format!("swap overlay: layer {dest} doesn't exist"));
    }
    sim.restart_delays_in(dest);
    match sim.overlays.last_mut() {
        Some(o) => o.id = dest,
        None => sim.overlays.push(Overlay {
            id: dest,
            close_on_outside: true,
            position: OverlayPosition::Center,
            background: None,
            anchor: rect,
            offset: None,
            anchor_to: OverlayAnchor::Screen,
        }),
    }
    log.did(format!("swap overlay to {}", label(&sim.shown, dest)))
}

/// "Change to" `dest`: the instance [`proto_run::change_to_target`] picks shows that variant.
fn change_to(sim: &mut Sim, node: NodeId, dest: NodeId, log: &mut Log) -> bool {
    match sim.shown.node(dest).map(|n| &n.kind) {
        None => return log.failed(format!("change to: layer {dest} doesn't exist")),
        Some(NodeKind::Component { .. }) => {}
        // The player would point the instance at it all the same, leaving it without a main
        // component; a run that reports the mistake is worth more than one that follows it.
        Some(_) => return log.failed(format!("change to: {} isn't a component", label(&sim.shown, dest))),
    }
    let Some(inst) = proto_run::change_to_target(&sim.shown, node, dest) else {
        return log.failed(format!("change to {}: there is no instance at or around {} to change", label(&sim.shown, dest), label(&sim.shown, node)));
    };
    let before = sim.design.clone();
    if let Some(n) = sim.design.node_mut(inst)
        && let NodeKind::Instance { component, .. } = &mut n.kind
    {
        *component = dest;
    }
    crate::components::sync_instance(&mut sim.design, inst);
    sim.settle(&before);
    log.did(format!("change {} to {}", label(&sim.shown, inst), label(&sim.shown, dest)))
}

/// "set `Collection/name` to value", after the variable was set.
pub(super) fn set_text(sim: &Sim, variable: &str, node: NodeId) -> String {
    let doc = &sim.design;
    match doc.variables.get_by_id(variable) {
        Some(v) => {
            let value = value_on(doc, v, node).map_or_else(|| "nothing".to_string(), |x| value_text(&x));
            format!("set `{}` to {value}", full_name(doc, v))
        }
        None => format!("set `{variable}`"),
    }
}

/// Why [`proto_run::set_variable`] set nothing.
pub(super) fn unset_reason(sim: &Sim, variable: &str, value: &Value, by_expression: bool) -> String {
    let doc = &sim.design;
    let Some(v) = doc.variables.get_by_id(variable) else { return format!("set variable: there is no variable `{variable}`") };
    let (name, kind) = (full_name(doc, v), v.kind.id());
    if by_expression {
        format!("set variable `{name}`: its expression gives no {kind} value")
    } else {
        format!("set variable `{name}`: {} isn't a {kind} value", figcraft_format::clip(&value.to_string(), 60))
    }
}
