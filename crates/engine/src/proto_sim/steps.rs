//! One thing the viewer does: which interaction it reaches (the player's `input`, by ancestry
//! instead of hit-testing) and what that sets off.

use figcraft_doc::design::{Gesture, StepAction};
use figcraft_doc::proto::{KeyMods, Trigger, key_alias};
use figcraft_doc::{Document, NodeId};
use serde_json::Value;

use super::actions::{Log, fire, set_text, unset_reason};
use super::vars::find_variable;
use super::{MAX_DELAY_CHAIN, Sim, StepOutcome, label};
use crate::proto_run;

/// Whether a layer shows inside a root (the screen or an overlay).
pub(super) enum Showing {
    Yes,
    /// The layer, or this layer around it, is hidden.
    Hidden(NodeId),
    /// It isn't the root or inside it.
    Elsewhere,
}

/// Whether `id` shows inside `root`: it is `root` or inside it, and it and every layer around it
/// up to `root` are visible.
pub(super) fn showing(doc: &Document, root: NodeId, id: NodeId) -> Showing {
    let mut chain = vec![id];
    chain.extend(doc.ancestors(id));
    let Some(at) = chain.iter().position(|c| *c == root) else { return Showing::Elsewhere };
    match chain.iter().take(at + 1).find(|c| doc.node(**c).is_none_or(|n| !n.visible)) {
        Some(hidden) => Showing::Hidden(*hidden),
        None => Showing::Yes,
    }
}

/// The gesture as scenarios write it.
pub(crate) fn gesture_name(g: Gesture) -> &'static str {
    match g {
        Gesture::Click => "click",
        Gesture::Hover => "hover",
        Gesture::Press => "press",
        Gesture::Drag => "drag",
        Gesture::MouseEnter => "mouseEnter",
        Gesture::MouseLeave => "mouseLeave",
        Gesture::MouseDown => "mouseDown",
        Gesture::MouseUp => "mouseUp",
    }
}

/// The trigger a gesture fires.
fn trigger_of(g: Gesture) -> Trigger {
    match g {
        Gesture::Click => Trigger::OnClick,
        Gesture::Hover => Trigger::OnHover,
        Gesture::Press => Trigger::OnPress,
        Gesture::Drag => Trigger::OnDrag,
        Gesture::MouseEnter => Trigger::MouseEnter,
        Gesture::MouseLeave => Trigger::MouseLeave,
        Gesture::MouseDown => Trigger::MouseDown,
        Gesture::MouseUp => Trigger::MouseUp,
    }
}

/// The first interaction on `id` whose trigger satisfies `want` (the player's `find_trigger`).
fn find_trigger(doc: &Document, id: NodeId, want: impl Fn(&Trigger) -> bool) -> Option<usize> {
    doc.node(id)?.interactions.iter().position(|it| want(&it.trigger))
}

pub(super) fn step(sim: &mut Sim, action: &StepAction) -> StepOutcome {
    let mut log = Log::default();
    // A click on the hovered layer keeps the hover (the pointer hasn't left it); every other step
    // ends the hover or press of the step before first.
    if !sim.hover_stays_for(action) && sim.revert() {
        log.effects.push("the hover or press of the step before ended".into());
    }
    match action {
        StepAction::Gesture { node, gesture } => self::gesture(sim, *node, *gesture, log),
        StepAction::Key { key } => self::key(sim, key, log),
        StepAction::Wait => wait(sim, log),
        StepAction::ClickOutside => click_outside(sim, log),
        StepAction::SetVariable { variable, value } => set_variable(sim, variable, value, log),
    }
}

/// A step that couldn't be taken.
fn refused(log: Log, problem: String) -> StepOutcome {
    StepOutcome { fired: false, effects: log.effects, problem: Some(problem) }
}

/// The outcome of a gesture or key that reached an interaction: it is a problem when an action
/// couldn't run, and when nothing happened at all (`what` names the step).
fn reacted(fired: bool, log: Log, what: &str) -> StepOutcome {
    let problem = match (log.problems.is_empty(), fired) {
        (false, _) => Some(log.problems.join("; ")),
        (true, false) => Some(format!("{what} changed nothing: {}", log.idle.join("; "))),
        (true, true) => None,
    };
    StepOutcome { fired, effects: log.effects, problem }
}

fn gesture(sim: &mut Sim, node: NodeId, gesture: Gesture, mut log: Log) -> StepOutcome {
    let root = sim.active_root();
    let doc = &sim.shown;
    if !doc.contains(node) {
        return refused(log, format!("layer {node} doesn't exist"));
    }
    let (layer, on) = (label(doc, node), label(doc, root));
    match showing(doc, root, node) {
        Showing::Yes => {}
        Showing::Hidden(by) if by == node => return refused(log, format!("layer {layer} isn't on {on}, which is what shows: it is hidden")),
        Showing::Hidden(by) => {
            return refused(log, format!("layer {layer} isn't on {on}, which is what shows: {} around it is hidden", label(doc, by)));
        }
        Showing::Elsewhere if sim.on_show(node) => {
            return refused(log, format!("layer {layer} isn't on {on}, which is what shows: it is under the open overlay"));
        }
        Showing::Elsewhere => return refused(log, format!("layer {layer} isn't on {on}, which is what shows")),
    }
    let (name, trigger) = (gesture_name(gesture), trigger_of(gesture));
    // The layer itself, then each layer around it up to what shows: one without the trigger lets
    // the gesture through.
    let mut chain = vec![node];
    chain.extend(doc.ancestors(node).into_iter().take_while(|a| *a != root));
    if node != root {
        chain.push(root);
    }
    let Some((at, k)) = chain.into_iter().find_map(|id| find_trigger(doc, id, |t| *t == trigger).map(|k| (id, k))) else {
        return refused(log, format!("nothing reacts to {name} on {layer}"));
    };
    let via = if at == node { String::new() } else { format!(" (through {})", label(doc, at)) };
    let rect = sim.rect_on_screen(at);
    let fired = fire(sim, at, k, rect, &mut log);
    reacted(fired, log, &format!("{name} on {layer}{via}"))
}

/// A key or controller button: the first layer of what shows (in layer order, hidden ones too, as
/// the player looks them up) with a key trigger for it fires.
fn key(sim: &mut Sim, key: &str, mut log: Log) -> StepOutcome {
    let (mods, name) = KeyMods::split(key);
    if name.is_empty() {
        return refused(log, "`key` is empty".into());
    }
    // The player gets keys by their names; a scenario may spell them as triggers may.
    let name = key_alias(name).unwrap_or(name);
    let root = sim.active_root();
    let doc = &sim.shown;
    let found = doc.subtree(root).into_iter().find_map(|id| find_trigger(doc, id, |t| t.fires_on_key(name, mods)).map(|k| (id, k)));
    let Some((at, k)) = found else {
        return refused(log, format!("nothing reacts to key `{}` on {}", figcraft_format::clip(key, 60), label(doc, root)));
    };
    let what = format!("key `{}` on {}", figcraft_format::clip(key, 60), label(doc, at));
    let fired = fire(sim, at, k, None, &mut log);
    reacted(fired, log, &what)
}

/// The "after delay" interaction of what shows that is due first and hasn't fired (the player's
/// `next_delay`: the shortest delay, the first in layer order among equals).
fn next_delay(sim: &Sim) -> Option<(NodeId, usize)> {
    let doc = &sim.shown;
    let mut next: Option<((NodeId, usize), u32)> = None;
    for id in doc.subtree(sim.active_root()) {
        let Some(n) = doc.node(id) else { continue };
        for (k, it) in n.interactions.iter().enumerate() {
            if let Trigger::AfterDelay { ms } = it.trigger
                && !sim.delays_fired.contains(&(id, k))
                && next.is_none_or(|(_, due)| ms < due)
            {
                next = Some(((id, k), ms));
            }
        }
    }
    next.map(|(at, _)| at)
}

/// Time passes: every pending "after delay" of what shows fires, once each, and so do the ones
/// of whatever they bring up, until nothing is waiting.
fn wait(sim: &mut Sim, mut log: Log) -> StepOutcome {
    let mut fired = false;
    let mut chain = 0;
    while let Some((id, k)) = next_delay(sim) {
        if chain >= MAX_DELAY_CHAIN {
            log.problems
                .push(format!("after-delay loop: more than {MAX_DELAY_CHAIN} delays fired one after another, the last on {}", label(&sim.shown, id)));
            break;
        }
        chain += 1;
        sim.delays_fired.insert((id, k));
        log.effects.push(format!("after delay on {}", label(&sim.shown, id)));
        fired |= fire(sim, id, k, None, &mut log);
    }
    if chain == 0 {
        log.effects.push(format!("no delay is waiting on {}", label(&sim.shown, sim.active_root())));
    }
    let problem = (!log.problems.is_empty()).then(|| log.problems.join("; "));
    StepOutcome { fired, effects: log.effects, problem }
}

/// A click outside the top overlay closes it when it closes that way; otherwise it does nothing.
fn click_outside(sim: &mut Sim, mut log: Log) -> StepOutcome {
    let fired = match sim.overlays.last() {
        None => {
            log.effects.push("no overlay is open".into());
            false
        }
        Some(o) if !o.close_on_outside => {
            log.effects.push(format!("{} stays open: it doesn't close on a click outside", label(&sim.shown, o.id)));
            false
        }
        Some(o) => {
            log.effects.push(format!("close overlay {}", label(&sim.shown, o.id)));
            sim.overlays.pop();
            true
        }
    };
    StepOutcome { fired, effects: log.effects, problem: None }
}

/// Set a variable as an interaction would, for input the prototype can't take.
fn set_variable(sim: &mut Sim, variable: &str, value: &Value, mut log: Log) -> StepOutcome {
    let id = match find_variable(&sim.design, variable) {
        Ok(v) => v.id.clone(),
        Err(why) => return refused(log, format!("set variable: {why}")),
    };
    let before = sim.design.clone();
    if !proto_run::set_variable(&mut sim.design, sim.screen, &id, value, None) {
        let why = unset_reason(sim, &id, value, false);
        return refused(log, why);
    }
    sim.settle(&before);
    log.effects.push(set_text(sim, &id, sim.screen));
    StepOutcome { fired: true, effects: log.effects, problem: None }
}
