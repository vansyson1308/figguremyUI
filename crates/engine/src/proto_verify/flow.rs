//! The prototype of one page as a static graph: its screens, every interaction on them as an
//! edge, what the flow starts reach, and the rules that read it (`flow.*`).
//!
//! The graph is what the file could do, not one run of it: every branch of a conditional counts
//! as possible, and so does every variant an instance can be changed to ("change to" is followed
//! into the variant it names, whose interactions the instance would then play). Whether a
//! condition can ever hold is not worked out, so "reachable" means "an interaction leads there".

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use figcraft_doc::design::Severity;
use figcraft_doc::proto::{Action, Trigger, walk};
use figcraft_doc::{Document, NodeId, NodeKind};

use crate::check::Found;
use crate::components::variant_set;
use crate::proto_run;
use crate::proto_sim::label;

pub(super) const BROKEN_TARGET: &str = "flow.brokenTarget";
pub(super) const DEAD_END: &str = "flow.deadEnd";
pub(super) const TRAPPED_OVERLAY: &str = "flow.trappedOverlay";
pub(super) const UNREACHABLE: &str = "flow.unreachable";
pub(super) const NO_START: &str = "flow.noStart";

/// The most variants the "change to" actions of one screen are followed through.
const MAX_VARIANTS: usize = 256;

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

/// Where a flow starts.
pub(super) struct Start {
    pub frame: NodeId,
    /// The flow's name; `None` when the page has no flow and the player falls back to its first
    /// frame.
    pub flow: Option<String>,
}

/// A top-level frame of the page.
pub(super) struct Screen {
    pub id: NodeId,
    pub edges: Vec<Edge>,
    /// A flow start, or a navigate action on something reachable leads here.
    pub reachable: bool,
    pub end_state: bool,
    /// Reachable, with no way to leave it, and not declared as an end state (on a page that has a
    /// flow start or an interaction: a page nobody prototyped on has no dead ends).
    pub dead_end: bool,
    /// Some "open overlay" or "swap overlay" on the page shows it.
    pub overlay: bool,
}

/// The page's prototype as a graph.
pub(super) struct Graph {
    pub page: NodeId,
    pub starts: Vec<Start>,
    /// The page's top-level frames in reading order.
    pub screens: Vec<Screen>,
    /// The page has a flow starting point of its own.
    pub has_flow: bool,
    /// Some layer of the page has an interaction.
    pub has_interactions: bool,
    /// Overlays that can't be closed (overlay → the layer that opens it).
    trapped: BTreeMap<NodeId, NodeId>,
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

/// Whether one of `edges` leaves screen `id` (`start`: a flow starts there, so "back" has nowhere
/// to go).
fn leaves(edges: &[Edge], id: NodeId, start: bool) -> bool {
    edges.iter().any(|e| match e.action {
        "navigate" => e.to.is_some_and(|t| t != id),
        "back" => !start,
        "openOverlay" | "swapOverlay" => e.to.is_some(),
        "openUrl" => true,
        _ => false,
    })
}

/// Whether one of an overlay's `edges` closes or replaces it.
fn closes(edges: &[Edge]) -> bool {
    edges.iter().any(|e| match e.action {
        "closeOverlay" | "back" => true,
        "navigate" | "swapOverlay" => e.to.is_some(),
        _ => false,
    })
}

/// The flow starts of `page`: its flows, else the frame the player falls back to.
fn starts_of(doc: &Document, page: NodeId, frames: &[NodeId]) -> (Vec<Start>, bool) {
    let flows: Vec<Start> = match doc.node(page).map(|n| &n.kind) {
        Some(NodeKind::Page { flows, .. }) => {
            flows.iter().filter(|f| doc.contains(f.start)).map(|f| Start { frame: f.start, flow: Some(f.name.clone()) }).collect()
        }
        _ => Vec::new(),
    };
    if flows.is_empty() { (frames.first().map(|f| Start { frame: *f, flow: None }).into_iter().collect(), false) } else { (flows, true) }
}

pub(super) fn graph(file: &Document, page: NodeId) -> Graph {
    let mut doc = file.clone();
    proto_run::play_interactions(&mut doc);
    let frames = doc.top_frames(page);
    let (starts, has_flow) = starts_of(&doc, page, &frames);
    let mut edges: BTreeMap<NodeId, Vec<Edge>> = frames.iter().map(|f| (*f, edges_of(&doc, *f))).collect();
    // What the starts reach: screens through navigate, and the overlays those screens open (an
    // overlay's own actions run too, but it isn't a screen for that).
    let mut reached: BTreeSet<NodeId> = starts.iter().map(|s| s.frame).collect();
    let mut seen = reached.clone();
    let mut queue: VecDeque<NodeId> = reached.iter().copied().collect();
    while let Some(root) = queue.pop_front() {
        for e in edges.entry(root).or_insert_with(|| edges_of(&doc, root)).iter() {
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
    let opened: Vec<(NodeId, NodeId, bool)> = edges
        .values()
        .flatten()
        .filter(|e| matches!(e.action, "openOverlay" | "swapOverlay"))
        .filter_map(|e| Some((e.to?, e.from, e.stays_on_outside_click)))
        .collect();
    let mut trapped = BTreeMap::new();
    for (overlay, opener, stays) in &opened {
        if *stays && !trapped.contains_key(overlay) && !closes(edges.entry(*overlay).or_insert_with(|| edges_of(&doc, *overlay))) {
            trapped.insert(*overlay, *opener);
        }
    }
    let has_interactions = file.subtree(page).into_iter().any(|id| file.node(id).is_some_and(|n| !n.interactions.is_empty()));
    // A page nobody prototyped on has no flow to get stuck in: its first frame is no dead end.
    let prototyped = has_flow || has_interactions;
    let screens = frames
        .iter()
        .map(|id| {
            let own = edges.remove(id).unwrap_or_default();
            let reachable = reached.contains(id);
            let end_state = file.design.end_states.iter().any(|e| e.node == *id);
            let dead_end = prototyped && reachable && !end_state && !leaves(&own, *id, starts.iter().any(|s| s.frame == *id));
            Screen { id: *id, edges: own, reachable, end_state, dead_end, overlay: opened.iter().any(|(o, ..)| o == id) }
        })
        .collect();
    Graph { page, starts, screens, has_flow, has_interactions, trapped }
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

/// The first of `ids` that names no variable.
fn unknown_variable<'a>(doc: &Document, ids: Vec<&'a str>) -> Option<&'a str> {
    ids.into_iter().find(|id| doc.variables.get_by_id(id).is_none())
}

/// Why "change to" `dest` on `node` changes nothing sensible: `dest` must be a variant of the
/// component set an instance at or around `node` shows (or, for an interaction kept on a main
/// component, of that component's own set, which its instances play).
fn unchangeable(doc: &Document, node: NodeId, dest: NodeId) -> Option<String> {
    if !doc.node(dest).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. })) {
        return Some(format!("{} isn't a component", label(doc, dest)));
    }
    let Some(set) = variant_set(doc, dest) else { return Some(format!("{} isn't a variant of a component set", label(doc, dest))) };
    let in_set = |id: NodeId| match doc.node(id).map(|n| &n.kind) {
        Some(NodeKind::Instance { component, .. }) => variant_set(doc, *component) == Some(set),
        Some(NodeKind::Component { .. }) => variant_set(doc, id) == Some(set),
        _ => false,
    };
    let found = std::iter::once(node).chain(doc.ancestors(node)).any(in_set);
    (!found).then(|| {
        format!("no instance of {} is at or around {}, so {} isn't a variant it could show", label(doc, set), label(doc, node), label(doc, dest))
    })
}

fn broken_targets(doc: &Document, page: NodeId) -> Vec<Found> {
    let mut out = Vec::new();
    for id in doc.subtree(page) {
        let Some(n) = doc.node(id) else { continue };
        for it in &n.interactions {
            for a in walk(&it.actions) {
                let Some(why) = broken(doc, id, a) else { continue };
                out.push(Found {
                    rule: BROKEN_TARGET,
                    severity: Severity::Blocker,
                    nodes: vec![id],
                    message: format!("{} on {} can't work: {why}", a.label(), label(doc, id)),
                    evidence: format!("{}, {} → {}: {why}", label(doc, id), it.trigger.label(), a.label()),
                    suggestion: "Point the action at something that exists (proto.interaction.set), or remove it (proto.interaction.remove).".into(),
                });
            }
        }
    }
    out
}

/// What the `flow.*` rules find on the graph's page.
pub(super) fn findings(doc: &Document, g: &Graph) -> Vec<Found> {
    let mut out = broken_targets(doc, g.page);
    let starts = g.starts.iter().map(|s| label(doc, s.frame)).collect::<Vec<_>>().join(", ");
    for s in g.screens.iter().filter(|s| s.dead_end) {
        let back = s.edges.iter().any(|e| e.action == "back");
        out.push(Found {
            rule: DEAD_END,
            severity: Severity::Major,
            nodes: vec![s.id],
            message: format!("{} is a dead end: a viewer who gets there can't leave", label(doc, s.id)),
            evidence: format!(
                "{} action(s) on it, none of which navigates elsewhere, opens an overlay or opens a link{}",
                s.edges.len(),
                if back { "; its \"back\" has nowhere to go, because a flow starts here" } else { "" }
            ),
            suggestion: "Add a way out, or declare it as a place the flow may end with proto.endState.set.".into(),
        });
    }
    for (overlay, opener) in &g.trapped {
        out.push(Found {
            rule: TRAPPED_OVERLAY,
            severity: Severity::Major,
            nodes: vec![*overlay, *opener],
            message: format!("overlay {} can't be closed once it is open", label(doc, *overlay)),
            evidence: format!(
                "{} opens it with close-on-outside-click off, and nothing in it closes or swaps the overlay, goes back or navigates",
                label(doc, *opener)
            ),
            suggestion: "Add a close action inside the overlay, or let a click outside close it.".into(),
        });
    }
    if g.has_interactions {
        out.extend(g.screens.iter().filter(|s| !s.reachable && !skipped(doc, s)).map(|s| {
            let own = !s.edges.is_empty();
            Found {
                rule: UNREACHABLE,
                severity: if own { Severity::Minor } else { Severity::Note },
                nodes: vec![s.id],
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
    if g.has_interactions && !g.has_flow {
        let first = g.starts.first().map(|s| s.frame);
        out.push(Found {
            rule: NO_START,
            severity: Severity::Minor,
            nodes: first.into_iter().collect(),
            message: "the page has interactions but no flow starting point".into(),
            evidence: match first {
                Some(f) => format!("the player starts at the page's first frame, {}", label(doc, f)),
                None => "the page has no top-level frame to start at".into(),
            },
            suggestion: "Say where the prototype starts with proto.flow.add.".into(),
        });
    }
    out
}

/// Frames that aren't screens of a flow on purpose: overlays, components and recorded
/// alternatives (and the frames they were made from).
fn skipped(doc: &Document, s: &Screen) -> bool {
    let component = doc.node(s.id).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. } | NodeKind::ComponentSet { .. }));
    let alternative = doc.design.alternatives.iter().any(|v| v.source == s.id || v.members.iter().any(|m| m.node == s.id));
    s.overlay || component || alternative
}
