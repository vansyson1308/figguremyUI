//! The prototype of one page as a static graph: its screens, every interaction on them as an
//! edge, what the flow starts reach, and the rules that read it (`flow.*`).
//!
//! The graph is what the file could do, not one run of it: every branch of a conditional counts
//! as possible, and so does every variant an instance can be changed to ("change to" is followed
//! into the variant it names, whose interactions the instance would then play). Whether a
//! condition can ever hold is not worked out, so "reachable" means "an interaction leads there".
//!
//! It is read from the file as the player has it ([`Facts::played`]): an instance plays its main
//! component's interactions, wherever that component lives, so an action that can't work is
//! found on the page that plays it, not only on the page that holds the component.
//!
//! What counts as a prototype. A page of frames with nothing but a hover state on a button isn't
//! one: the flow rules that ask "where does it start" and "can the viewer get out" apply only
//! once a screen (a top-level frame that isn't a component or component set) plays an action that
//! moves between screens (navigate, back, open, swap or close overlay, open link), a flow start
//! is declared, or an interaction on another page leads here.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use figcraft_doc::design::{EndStateStatus, Severity};
use figcraft_doc::proto::{Action, Trigger, walk};
use figcraft_doc::{Document, NodeId, NodeKind};

use crate::check::{Facts, Found};
use crate::components::owning_component;
use crate::proto_run;
use crate::proto_sim::label;

pub(super) const BROKEN_TARGET: &str = "flow.brokenTarget";
pub(super) const DEAD_END: &str = "flow.deadEnd";
pub(super) const TRAPPED_OVERLAY: &str = "flow.trappedOverlay";
pub(super) const UNREACHABLE: &str = "flow.unreachable";
pub(super) const NO_START: &str = "flow.noStart";

/// The most variants the "change to" actions of one screen are followed through, and the most
/// overlays one overlay's swaps.
const MAX_VARIANTS: usize = 256;
/// The most levels a layer is looked up through for the instance it is a copy in.
const MAX_DEPTH: usize = 64;
/// The most layers one finding lists.
const MAX_LISTED: usize = 8;

/// The subject of a finding about page `page` as a whole.
pub(super) fn page_subject(page: NodeId) -> String {
    format!("page:{page}")
}

/// One action of one interaction: something a screen can do.
pub(super) struct Edge {
    /// The layer whose interaction it is (for a variant reached by "change to": the layer in the
    /// variant, which the instance plays once it shows that variant).
    pub from: NodeId,
    pub trigger: Trigger,
    /// The action's type, as `proto.interaction.add` names it.
    pub action: &'static str,
    /// Where it leads when it has a target that works: the frame a navigate shows, the overlay
    /// opened, the variant changed to, the layer scrolled to.
    pub to: Option<NodeId>,
    /// An "open overlay" whose overlay stays open on a click outside it.
    stays_on_outside_click: bool,
}

/// Why a frame is where a prototype starts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartSource {
    /// A flow starting point declared on the page.
    Flow,
    /// The page has none, and the player falls back to its first frame.
    FirstFrame,
    /// An interaction on another page leads here.
    OtherPage,
}

impl StartSource {
    pub(super) fn id(self) -> &'static str {
        match self {
            StartSource::Flow => "flow",
            StartSource::FirstFrame => "firstFrame",
            StartSource::OtherPage => "otherPage",
        }
    }
}

/// Where a flow starts.
pub(super) struct Start {
    pub frame: NodeId,
    /// The flow's name; `None` when the frame isn't a declared flow start.
    pub flow: Option<String>,
    pub source: StartSource,
}

/// A top-level frame of the page.
pub(super) struct Screen {
    pub id: NodeId,
    pub edges: Vec<Edge>,
    /// A start, or a navigate action on something reachable leads here.
    pub reachable: bool,
    /// A person declared or accepted it as a place a flow may end.
    pub end_state: bool,
    /// The AI agent that declared it as an end state, while no person has accepted that.
    pub proposed_end: Option<String>,
    /// Reachable, with no way to leave it, and not an accepted end state (see [`graph`] for when
    /// that is asked at all).
    pub dead_end: bool,
    /// Some "open overlay" or "swap overlay" shows it.
    pub overlay: bool,
    /// A component or component set: never a screen of a flow.
    pub component: bool,
}

/// An overlay the viewer can't get out of.
struct Trap {
    /// The layers that open it (or the overlay it is swapped in from) with close-on-outside off.
    openers: Vec<NodeId>,
    /// The overlay opened first, when this one is reached from it by "swap overlay".
    through: Option<NodeId>,
}

/// The page's prototype as a graph.
pub(super) struct Graph {
    pub page: NodeId,
    pub starts: Vec<Start>,
    /// The page's top-level frames in reading order.
    pub screens: Vec<Screen>,
    /// The page has a flow starting point of its own.
    pub has_flow: bool,
    /// The page is a prototype: a flow start is declared, a screen plays an action that moves
    /// between screens, or another page leads here.
    pub prototyped: bool,
    /// An interaction on another page leads to a frame of this one.
    entered: bool,
    /// Overlays that can't be left, by overlay.
    trapped: BTreeMap<NodeId, Trap>,
}

/// The action's type, as `proto.interaction.add` names it.
fn action_id(a: &Action) -> &'static str {
    match a {
        Action::Navigate { .. } => "navigate",
        Action::Back => "back",
        Action::ScrollTo { .. } => "scrollTo",
        Action::OpenOverlay { .. } => "openOverlay",
        Action::SwapOverlay { .. } => "swapOverlay",
        Action::CloseOverlay => "closeOverlay",
        Action::OpenUrl { .. } => "openUrl",
        Action::ChangeTo { .. } => "changeTo",
        Action::SetVariable { .. } => "setVariable",
        Action::SetVariableMode { .. } => "setVariableMode",
        Action::Conditional { .. } => "conditional",
    }
}

/// The trigger's type, as `proto.interaction.add` names it.
pub(super) fn trigger_id(t: &Trigger) -> &'static str {
    match t {
        Trigger::OnClick => "onClick",
        Trigger::OnHover => "onHover",
        Trigger::OnPress => "onPress",
        Trigger::OnDrag => "onDrag",
        Trigger::MouseEnter => "mouseEnter",
        Trigger::MouseLeave => "mouseLeave",
        Trigger::MouseDown => "mouseDown",
        Trigger::MouseUp => "mouseUp",
        Trigger::AfterDelay { .. } => "afterDelay",
        Trigger::KeyPress { .. } => "keyPress",
    }
}

fn is_component(doc: &Document, id: NodeId) -> bool {
    doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. }))
}

/// Where action `a` leads when its target works (see [`Edge::to`]).
fn target(doc: &Document, a: &Action) -> Option<NodeId> {
    match a {
        // A section shows its first frame until one of its frames was visited: either way a frame
        // inside it, reached by the same edge.
        Action::Navigate { dest, .. } => proto_run::navigate_target(doc, *dest, &HashMap::new()).filter(|t| doc.top_level_frame(*t) == Some(*t)),
        _ => a.dest().filter(|d| doc.contains(*d)),
    }
}

/// Every action `root` (a screen or an overlay) can run: the interactions in it, and those of the
/// variants its instances can be changed to. `doc` plays interactive components already.
fn edges_of(doc: &Document, root: NodeId) -> Vec<Edge> {
    let mut out = Vec::new();
    // The variants showing now are in `root` already, as the instances' own layers.
    let mut variants: BTreeSet<NodeId> = doc
        .subtree(root)
        .into_iter()
        .filter_map(|id| match doc.node(id).map(|n| &n.kind) {
            Some(NodeKind::Instance { component, .. }) => Some(*component),
            _ => None,
        })
        .collect();
    let mut roots = vec![root];
    while let Some(r) = roots.pop() {
        for id in doc.subtree(r) {
            let Some(n) = doc.node(id) else { continue };
            for it in &n.interactions {
                for a in walk(&it.actions).filter(|a| !matches!(a, Action::Conditional { .. })) {
                    let to = target(doc, a);
                    let stays_on_outside_click = matches!(a, Action::OpenOverlay { close_on_outside: false, .. });
                    out.push(Edge { from: id, trigger: it.trigger.clone(), action: action_id(a), to, stays_on_outside_click });
                    if let (Action::ChangeTo { .. }, Some(variant)) = (a, to)
                        && variants.len() < MAX_VARIANTS
                        && variants.insert(variant)
                    {
                        roots.push(variant);
                    }
                }
            }
        }
    }
    out
}

/// Whether one of `edges` leaves screen `id` (`start`: the prototype starts there, so "back" has
/// nowhere to go).
fn leaves(edges: &[Edge], id: NodeId, start: bool) -> bool {
    edges.iter().any(|e| match e.action {
        "navigate" => e.to.is_some_and(|t| t != id),
        "back" => !start,
        "openOverlay" | "swapOverlay" => e.to.is_some(),
        "openUrl" => true,
        _ => false,
    })
}

/// Whether one of `edges` moves between screens or overlays at all: what makes a page a prototype.
fn moves(edges: &[Edge]) -> bool {
    edges.iter().any(|e| matches!(e.action, "navigate" | "back" | "openOverlay" | "swapOverlay" | "closeOverlay" | "openUrl"))
}

/// Whether one of an overlay's `edges` gets the viewer out of it: it closes the overlay, goes
/// back or navigates. A swap doesn't: it puts another overlay in this one's place, kept open the
/// same way.
fn exits(edges: &[Edge]) -> bool {
    edges.iter().any(|e| match e.action {
        "closeOverlay" | "back" => true,
        "navigate" => e.to.is_some(),
        _ => false,
    })
}

/// The overlays `edges` (an overlay's) swap it for.
fn swaps(edges: &[Edge], own: NodeId) -> Vec<NodeId> {
    edges.iter().filter(|e| e.action == "swapOverlay").filter_map(|e| e.to).filter(|to| *to != own).collect()
}

/// What interactions outside `page` lead to on it: the frames they navigate to, and the layers
/// they show as overlays.
fn led_to_from_elsewhere(doc: &Document, page: NodeId) -> (Vec<NodeId>, BTreeSet<NodeId>) {
    let (mut entries, mut overlays) = (Vec::new(), BTreeSet::new());
    for n in doc.nodes().filter(|n| !n.interactions.is_empty()) {
        if doc.page_of(n.id) == Some(page) {
            continue;
        }
        for a in n.interactions.iter().flat_map(|it| walk(&it.actions)) {
            let Some(to) = target(doc, a).filter(|to| doc.page_of(*to) == Some(page)) else { continue };
            match a {
                Action::Navigate { .. } if !entries.contains(&to) => entries.push(to),
                Action::OpenOverlay { .. } | Action::SwapOverlay { .. } => {
                    overlays.insert(to);
                }
                _ => {}
            }
        }
    }
    entries.sort();
    (entries, overlays)
}

/// The starts of `page`: its flows and the frames other pages lead to, else the frame the player
/// falls back to (the first that isn't a component: nobody presents a component set).
fn starts_of(doc: &Document, page: NodeId, frames: &[NodeId], entries: &[NodeId]) -> (Vec<Start>, bool) {
    let mut starts: Vec<Start> = match doc.node(page).map(|n| &n.kind) {
        Some(NodeKind::Page { flows, .. }) => flows
            .iter()
            .filter(|f| doc.contains(f.start))
            .map(|f| Start { frame: f.start, flow: Some(f.name.clone()), source: StartSource::Flow })
            .collect(),
        _ => Vec::new(),
    };
    let has_flow = !starts.is_empty();
    for entry in entries {
        if !starts.iter().any(|s| s.frame == *entry) {
            starts.push(Start { frame: *entry, flow: None, source: StartSource::OtherPage });
        }
    }
    if starts.is_empty()
        && let Some(first) = frames.iter().find(|f| !is_component(doc, **f))
    {
        starts.push(Start { frame: *first, flow: None, source: StartSource::FirstFrame });
    }
    (starts, has_flow)
}

/// Frames that aren't screens of a flow on purpose: components and recorded alternatives (and
/// the frames they were made from).
fn alternative(doc: &Document, id: NodeId) -> bool {
    doc.design.alternatives.iter().any(|v| v.source == id || v.members.iter().any(|m| m.node == id))
}

/// The overlays a viewer can't get out of. An overlay opened with close-on-outside off stays
/// until an action in it closes it, goes back or navigates; a swap keeps the slot open the same
/// way, so the overlays swapped in are followed: when none of them offers a way out, the ones
/// where the swapping ends (or goes round in circles) are traps.
fn traps(doc: &Document, edges: &mut BTreeMap<NodeId, Vec<Edge>>) -> BTreeMap<NodeId, Trap> {
    let opened: Vec<(NodeId, NodeId)> =
        edges.values().flatten().filter(|e| e.action == "openOverlay" && e.stays_on_outside_click).filter_map(|e| Some((e.to?, e.from))).collect();
    let mut out: BTreeMap<NodeId, Trap> = BTreeMap::new();
    for (first, opener) in opened {
        // Everything the slot can come to show, by swaps.
        let mut slot = vec![first];
        let mut at = 0;
        while let Some(overlay) = slot.get(at).copied() {
            at += 1;
            for next in swaps(edges.entry(overlay).or_insert_with(|| edges_of(doc, overlay)), overlay) {
                if slot.len() < MAX_VARIANTS && !slot.contains(&next) {
                    slot.push(next);
                }
            }
        }
        if slot.iter().any(|o| edges.get(o).is_some_and(|e| exits(e))) {
            continue;
        }
        // Where the viewer ends up: the overlays with no swap out; all of them when every one
        // swaps on (round in circles).
        let ends: Vec<NodeId> = slot.iter().copied().filter(|o| edges.get(o).is_none_or(|e| swaps(e, *o).is_empty())).collect();
        for overlay in if ends.is_empty() { slot.clone() } else { ends } {
            let trap = out.entry(overlay).or_insert_with(|| Trap { openers: Vec::new(), through: None });
            if !trap.openers.contains(&opener) {
                trap.openers.push(opener);
            }
            if overlay != first {
                trap.through.get_or_insert(first);
            }
        }
    }
    out
}

/// The flow graph of `page`.
///
/// A screen is a dead end when a start reaches it, it isn't an accepted end state, nothing on it
/// leaves it, and the question makes sense: some screen of the page moves between screens (or
/// the screen is reached from another page), and the page has another frame that counts as a
/// screen (not a component, a frame only shown as an overlay, or a recorded alternative) or the
/// viewer came from another page. A lone screen, or a page nobody linked up, has no dead ends.
pub(super) fn graph(facts: &Facts<'_>, page: NodeId) -> Graph {
    let (file, doc) = (facts.doc, facts.played());
    let frames = doc.top_frames(page);
    let (entries, overlays_elsewhere) = led_to_from_elsewhere(doc, page);
    let (starts, has_flow) = starts_of(doc, page, &frames, &entries);
    let mut edges: BTreeMap<NodeId, Vec<Edge>> = frames.iter().map(|f| (*f, edges_of(doc, *f))).collect();
    // What the starts reach: screens through navigate, and the overlays those screens open (an
    // overlay's own actions run too, but it isn't a screen for that).
    let mut reached: BTreeSet<NodeId> = starts.iter().map(|s| s.frame).collect();
    let mut seen = reached.clone();
    let mut queue: VecDeque<NodeId> = reached.iter().copied().collect();
    while let Some(root) = queue.pop_front() {
        for e in edges.entry(root).or_insert_with(|| edges_of(doc, root)).iter() {
            let Some(to) = e.to else { continue };
            let follows = match e.action {
                "navigate" => {
                    reached.insert(to);
                    true
                }
                "openOverlay" | "swapOverlay" => true,
                _ => false,
            };
            if follows && seen.insert(to) {
                queue.push_back(to);
            }
        }
    }
    let mut opened: BTreeSet<NodeId> =
        edges.values().flatten().filter(|e| matches!(e.action, "openOverlay" | "swapOverlay")).filter_map(|e| e.to).collect();
    opened.extend(overlays_elsewhere);
    let trapped = traps(doc, &mut edges);
    let screen_moves = frames.iter().filter(|f| !is_component(doc, **f)).any(|f| edges.get(f).is_some_and(|e| moves(e)));
    let entered = !entries.is_empty();
    let prototyped = has_flow || screen_moves || entered;
    // Whether `id` is a screen a viewer could be on: not a component, not a frame only ever
    // shown as an overlay, not an alternative kept beside the screen it was made from.
    let counts = |id: NodeId| !is_component(doc, id) && !(opened.contains(&id) && !reached.contains(&id)) && !alternative(file, id);
    let screens = frames
        .iter()
        .map(|id| {
            let own = edges.remove(id).unwrap_or_default();
            let component = is_component(doc, *id);
            let reachable = reached.contains(id);
            let declared = file.design.end_states.iter().find(|e| e.node == *id);
            let end_state = declared.is_some_and(|e| e.status == EndStateStatus::Accepted);
            let proposed_end = declared.filter(|e| e.status == EndStateStatus::Proposed).map(|e| e.author.clone());
            // "Back" has nowhere to go where the prototype itself starts; a viewer who came from
            // another page can go back there.
            let starts_here = starts.iter().any(|s| s.frame == *id && s.source != StartSource::OtherPage);
            let came_from_elsewhere = entries.contains(id);
            let elsewhere_to_go = came_from_elsewhere || frames.iter().any(|f| f != id && counts(*f));
            let dead_end =
                !component && reachable && !end_state && (screen_moves || came_from_elsewhere) && elsewhere_to_go && !leaves(&own, *id, starts_here);
            Screen { id: *id, edges: own, reachable, end_state, proposed_end, dead_end, overlay: opened.contains(id), component }
        })
        .collect();
    Graph { page, starts, screens, has_flow, prototyped, entered, trapped }
}

/// Why action `a` of an interaction on `node` can't work, when it can't.
fn broken(doc: &Document, node: NodeId, a: &Action) -> Option<String> {
    let missing = |d: NodeId| (!doc.contains(d)).then(|| format!("layer {d} doesn't exist"));
    match a {
        Action::Navigate { dest, .. } => missing(*dest).or_else(|| {
            let section = doc.node(*dest).is_some_and(|n| matches!(n.kind, NodeKind::Section { .. }));
            match target(doc, a) {
                Some(_) => None,
                None if section => Some(format!("section {} has no frame to show", label(doc, *dest))),
                None => Some(format!("{} isn't a top-level frame or inside one", label(doc, *dest))),
            }
        }),
        Action::ScrollTo { dest, .. } | Action::OpenOverlay { dest, .. } | Action::SwapOverlay { dest } => missing(*dest),
        Action::ChangeTo { dest } => missing(*dest).or_else(|| unchangeable(doc, node, *dest)),
        Action::SetVariable { variable, value, expr } => {
            let vars = &doc.variables;
            let Some(v) = vars.get_by_id(variable) else { return Some(format!("there is no variable `{variable}`")) };
            let kind = v.kind;
            match expr {
                None => proto_run::literal(kind, value)
                    .is_none()
                    .then(|| format!("{} isn't a {} value, which `{}` holds", figcraft_format::clip(&value.to_string(), 60), kind.id(), v.name)),
                Some(e) => {
                    unknown_variable(doc, e.variables()).map(|x| format!("its expression reads variable `{x}`, which doesn't exist")).or_else(|| {
                        // An expression that gives nothing now may give something later (a division
                        // by a variable that is still zero): only a value of the wrong type is certain.
                        let value = e.eval(vars, &doc.modes_for(node))?;
                        proto_run::fit(kind, value)
                            .is_none()
                            .then(|| format!("its expression doesn't give a {} value, which `{}` holds", kind.id(), v.name))
                    })
                }
            }
        }
        Action::SetVariableMode { collection, mode } => match doc.variables.collection_by_id(collection) {
            None => Some(format!("there is no collection `{collection}`")),
            Some(c) => c.mode_by_id(mode).is_none().then(|| format!("collection `{}` has no mode `{mode}`", c.name)),
        },
        Action::Conditional { blocks } => {
            let read = blocks.iter().filter_map(|b| b.condition.as_ref()).flat_map(|c| c.variables());
            unknown_variable(doc, read.collect()).map(|x| format!("a condition reads variable `{x}`, which doesn't exist"))
        }
        Action::Back | Action::CloseOverlay | Action::OpenUrl { .. } => None,
    }
}

/// Why "change to" `dest` on `node`, in an interaction whose actions are `actions`, shows nothing
/// for long: the instance it changes has the variant property that tells `dest` from what it shows
/// bound to a string variable that names another value, and the player applies bindings again
/// as the change settles, which puts that variant back (review X-02). Not when the same
/// interaction sets that variable (the binding then follows the change), nor when the variable's
/// value names no variant (a binding like that changes nothing).
fn bound_back(doc: &Document, node: NodeId, dest: NodeId, actions: &[Action]) -> Option<String> {
    let inst = proto_run::change_to_target(doc, node, dest)?;
    let n = doc.node(inst)?;
    let set = crate::components::variant_set(doc, dest)?;
    let wanted = doc.node(dest).map(|d| crate::components::variant_props(&d.name))?;
    let options = crate::components::variant_options(doc, set);
    let modes = doc.modes_for(inst);
    let sets_it = |var: &str| walk(actions).any(|a| matches!(a, Action::SetVariable { variable, .. } if variable == var));
    for (key, var) in &n.bound {
        let Some(prop) = key.strip_prefix(crate::components::PROP_BINDING) else { continue };
        let (Some(want), Some((_, values))) = (wanted.get(prop), options.iter().find(|(o, _)| o == prop)) else { continue };
        if sets_it(var) {
            continue;
        }
        let Some(figcraft_doc::VariableValue::String(held)) = doc.variables.resolve(var, &modes) else { continue };
        if held != *want && values.contains(&held) {
            let name = doc.variables.get_by_id(var).map_or(var.as_str(), |v| v.name.as_str());
            let held = figcraft_format::clip(&held, 60);
            return Some(format!(
                "`{prop}` of instance {} is bound to variable `{name}`, which holds \"{held}\" here: the player puts the instance back to {prop}={held} as soon as it changes, so it never shows {}",
                label(doc, inst),
                label(doc, dest)
            ));
        }
    }
    None
}

/// The first of `ids` that names no variable.
fn unknown_variable<'a>(doc: &Document, ids: Vec<&'a str>) -> Option<&'a str> {
    ids.into_iter().find(|id| doc.variables.get_by_id(id).is_none())
}

/// Why "change to" `dest` on `node` changes nothing: `dest` must be a component, and there must
/// be an instance to change. The player changes the nearest instance at or around the layer that
/// shows a variant of `dest`'s set, else the innermost instance holding the layer
/// ([`proto_run::change_to_target`]), whatever set `dest` is from; so only a layer with no
/// instance at or around it has nothing to change. An interaction kept on a main component is
/// played by its instances, which are then what changes.
fn unchangeable(doc: &Document, node: NodeId, dest: NodeId) -> Option<String> {
    if !doc.node(dest).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. })) {
        return Some(format!("{} isn't a component", label(doc, dest)));
    }
    if proto_run::change_to_target(doc, node, dest).is_some() || owning_component(doc, node).is_some() {
        return None;
    }
    Some(format!("there is no instance at or around {} to change to {}", label(doc, node), label(doc, dest)))
}

/// Where the interaction at index `k` of `layer` really lives: when `layer` is an instance, or a
/// layer inside one, that plays its main component's interaction, the main component's layer
/// and the interaction's index there; else `layer` itself. `doc` is the played file.
fn origin(doc: &Document, layer: NodeId, k: usize) -> (NodeId, usize) {
    let Some(it) = doc.node(layer).and_then(|n| n.interactions.get(k)) else { return (layer, k) };
    // The child indexes from the instance down to `layer`, innermost first.
    let mut path: Vec<usize> = Vec::new();
    let mut cur = layer;
    for _ in 0..MAX_DEPTH {
        let Some(n) = doc.node(cur) else { break };
        if let NodeKind::Instance { component, .. } = &n.kind {
            let twin = path.iter().rev().try_fold(*component, |at, i| doc.node(at).and_then(|m| m.children.get(*i)).copied());
            let found = twin.and_then(|t| Some((t, doc.node(t)?.interactions.iter().position(|x| x == it)?)));
            return found.unwrap_or((layer, k));
        }
        let Some(parent) = n.parent else { break };
        let Some(i) = doc.node(parent).and_then(|p| p.children.iter().position(|c| *c == cur)) else { break };
        path.push(i);
        cur = parent;
    }
    (layer, k)
}

/// One broken action, as it is reported once however many instances play it.
struct Broken {
    /// The layer whose interaction it is (a main component's layer when instances play it).
    owner: NodeId,
    /// The first layer of the page that plays it as an instance's copy.
    player: Option<NodeId>,
    /// The layer of the page whose "change to" leads to the variant (on another page) holding it.
    led_by: Option<NodeId>,
    message: String,
    evidence: String,
    /// It can't work because a binding puts the variant back ([`bound_back`]).
    unbind: bool,
}

/// `flow.brokenTarget` on `page`: the actions of every layer on it (as the player has them, so
/// with what instances play from their main components, on whatever page those are), and of the
/// variants on other pages its "change to" actions lead to. One finding per action, named by
/// where the action lives: a main component's broken action is reported once, with the first
/// instance layer that plays it.
fn broken_targets(doc: &Document, page: NodeId) -> Vec<Found> {
    let mut found: BTreeMap<String, Broken> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    // (the root to walk, the layer of the page that leads there by "change to")
    let mut roots: VecDeque<(NodeId, Option<NodeId>)> = VecDeque::from([(page, None)]);
    let mut variants: BTreeSet<NodeId> = BTreeSet::new();
    while let Some((root, led_by)) = roots.pop_front() {
        for id in doc.subtree(root) {
            let Some(n) = doc.node(id) else { continue };
            for (k, it) in n.interactions.iter().enumerate() {
                for (at, a) in walk(&it.actions).enumerate() {
                    // A variant kept on another page is played once an instance is changed to it.
                    if let Action::ChangeTo { dest } = a
                        && doc.node(*dest).is_some_and(|d| matches!(d.kind, NodeKind::Component { .. }))
                        && doc.page_of(*dest) != Some(page)
                        && variants.len() < MAX_VARIANTS
                        && variants.insert(*dest)
                    {
                        roots.push_back((*dest, led_by.or(Some(id))));
                    }
                    let bound = match a {
                        Action::ChangeTo { dest } => bound_back(doc, id, *dest, &it.actions),
                        _ => None,
                    };
                    let Some(why) = broken(doc, id, a).or_else(|| bound.clone()) else { continue };
                    let (owner, index) = origin(doc, id, k);
                    let subject = format!("action:{owner}:{index}:{at}");
                    let player = (owner != id).then_some(id);
                    match found.get_mut(&subject) {
                        // Met again through an instance: the first one that plays it is named.
                        Some(known) => known.player = known.player.or(player),
                        None => {
                            order.push(subject.clone());
                            found.insert(
                                subject,
                                Broken {
                                    owner,
                                    player,
                                    led_by,
                                    message: format!("{} on {} can't work: {why}", a.label(), label(doc, owner)),
                                    evidence: format!("{}, {} → {}: {why}", label(doc, owner), it.trigger.label(), a.label()),
                                    unbind: bound.is_some(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    order
        .into_iter()
        .filter_map(|subject| {
            let b = found.remove(&subject)?;
            let through = match (b.led_by, b.player) {
                (Some(by), _) => format!("; {} can be changed to the variant that holds it", label(doc, by)),
                (None, Some(player)) => format!("; played by instance layer {}", label(doc, player)),
                (None, None) => String::new(),
            };
            Some(Found {
                rule: BROKEN_TARGET,
                severity: Severity::Blocker,
                nodes: std::iter::once(b.owner).chain(b.led_by.or(b.player)).collect(),
                subject,
                message: b.message,
                evidence: format!("{}{through}", b.evidence),
                suggestion: if b.unbind {
                    "Set the variable instead of changing the variant (a setVariable action in the same interaction), or unbind the property (variable.bind with variable null).".into()
                } else {
                    "Point the action at something that exists (proto.interaction.set), or remove it (proto.interaction.remove).".into()
                },
            })
        })
        .collect()
}

/// What the `flow.*` rules find on the graph's page. `doc` is the played file.
pub(super) fn findings(doc: &Document, g: &Graph) -> Vec<Found> {
    let mut out = broken_targets(doc, g.page);
    let starts = g.starts.iter().map(|s| label(doc, s.frame)).collect::<Vec<_>>().join(", ");
    for s in g.screens.iter().filter(|s| s.dead_end) {
        let back = s.edges.iter().any(|e| e.action == "back");
        let proposed = match &s.proposed_end {
            Some(who) => format!("; declared as an end state by {who}, waiting for a person to accept (proto.endState.accept)"),
            None => String::new(),
        };
        out.push(Found {
            rule: DEAD_END,
            severity: Severity::Major,
            nodes: vec![s.id],
            subject: String::new(),
            message: format!("{} is a dead end: a viewer who gets there can't leave", label(doc, s.id)),
            evidence: format!(
                "{} action(s) on it, none of which navigates elsewhere, opens an overlay or opens a link{}{proposed}",
                s.edges.len(),
                if back { "; its \"back\" has nowhere to go, because a flow starts here" } else { "" }
            ),
            suggestion: if s.proposed_end.is_some() {
                "Add a way out, or have a person accept it as a place the flow may end (proto.endState.accept).".into()
            } else {
                "Add a way out, or declare it as a place the flow may end with proto.endState.set.".into()
            },
        });
    }
    for (overlay, trap) in &g.trapped {
        let openers: Vec<String> = trap.openers.iter().take(MAX_LISTED).map(|o| label(doc, *o)).collect();
        let more = if trap.openers.len() > MAX_LISTED { ", …" } else { "" };
        let evidence = match trap.through {
            Some(first) => format!(
                "{}{more} opens overlay {} with close-on-outside-click off; a swap overlay leads from there to {}, kept open the same way, and nothing in it closes the overlay, goes back or navigates",
                openers.join(", "),
                label(doc, first),
                label(doc, *overlay)
            ),
            None => format!(
                "{}{more} opens it with close-on-outside-click off, and nothing in it closes the overlay, goes back or navigates (nor does any overlay it swaps to)",
                openers.join(", ")
            ),
        };
        out.push(Found {
            rule: TRAPPED_OVERLAY,
            severity: Severity::Major,
            nodes: std::iter::once(*overlay).chain(trap.openers.iter().copied().take(1)).collect(),
            subject: format!("overlay:{overlay}"),
            message: format!("overlay {} can't be closed once it is open", label(doc, *overlay)),
            evidence,
            suggestion: "Add a close action inside the overlay, or let a click outside close it.".into(),
        });
    }
    if g.prototyped {
        out.extend(g.screens.iter().filter(|s| !s.reachable && !s.overlay && !s.component && !alternative(doc, s.id)).map(|s| {
            let own = !s.edges.is_empty();
            Found {
                rule: UNREACHABLE,
                severity: if own { Severity::Minor } else { Severity::Note },
                nodes: vec![s.id],
                subject: String::new(),
                message: format!("{} can't be reached from where the prototype starts", label(doc, s.id)),
                evidence: format!(
                    "no navigate action leads to it from {}; it has {} action(s) of its own",
                    if starts.is_empty() { "any screen".to_string() } else { starts.clone() },
                    s.edges.len()
                ),
                suggestion: "Link to it from a screen the flow reaches, start a flow on it (proto.flow.add), or leave it out of the prototype."
                    .into(),
            }
        }));
    }
    if g.prototyped && !g.has_flow && !g.entered {
        let first = g.starts.first().map(|s| s.frame);
        out.push(Found {
            rule: NO_START,
            severity: Severity::Minor,
            nodes: first.into_iter().collect(),
            subject: page_subject(g.page),
            message: "the page's screens are linked up but no flow starting point says where the prototype starts".into(),
            evidence: match first {
                Some(f) => format!("the player starts at the page's first frame, {}", label(doc, f)),
                None => "the page has no top-level frame to start at".into(),
            },
            suggestion: "Say where the prototype starts with proto.flow.add.".into(),
        });
    }
    out
}
