//! Declared states (`proto.state.*`): the states a screen is meant to have because time passes
//! (first use, nothing yet, waiting, failure, coming back) and how it ends, each with what shows
//! it; and the rules that compare the declarations with what the file has (`states.*`).
//!
//! Whether a screen has states is a guess until somebody says which it should have, so nothing is
//! inferred here: the rules compare what is declared with what exists, and with nothing declared
//! they say that (`states.none`), not that states are missing.
//!
//! What a declaration says about the design is a person's to stand behind, as an end state is: an
//! AI agent's declaration is a proposal (`status: proposed`) until a person accepts it, and an
//! agent changes and takes back only its own proposals. A proposal counts in the checks all the
//! same (it is what somebody says the screen needs), and the list says who proposed it. As
//! everywhere in the design commands this is a convention between cooperating parties, not a
//! security boundary (see [`crate::design`]).
//!
//! What counts, exactly:
//! - **designed**: `shownBy` is given and what it names is there: a top-level frame (another
//!   screen, an overlay) or a layer inside the screen; or an instance inside the screen whose
//!   component set has the variant named.
//! - **reachable**: the flow graph reaches the frame that shows it (the top-level frame `shownBy`
//!   names, else the screen itself: a layer or a variant shows on its screen), or a stored
//!   scenario shows it (`coveredBy`). The graph doesn't follow variables or bindings, so for a
//!   layer or a variant it says only that its screen can be reached; whether something on that
//!   screen makes it show is what a scenario shows.
//! - **coveredBy**: the stored scenarios whose headless run shows it, at the start or after a step
//!   that passed (a step that failed and what follows it don't count): the frame is the screen or
//!   an open overlay, the layer is showing, or the instance is showing with that variant.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use figcraft_doc::design::{DeclaredState, Expect, MAX_ENTRIES, ProposalStatus, Severity, ShownBy, StateKind, VariantExpect};
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::{Map, Value, json};

use super::parse;
use crate::check::{Facts, Found, Scope};
use crate::design::{USER, agent, author_name};
use crate::params::{get, id_of, req_id};
use crate::proto_sim::{Sim, label};
use crate::{Result, Session, bad};

pub(super) const NONE: &str = "states.none";
pub(super) const NOT_DESIGNED: &str = "states.notDesigned";
pub(super) const UNREACHABLE: &str = "states.unreachable";
pub(super) const ERROR_EMPTY: &str = "states.errorEmpty";

/// The most declarations one finding names.
const MAX_LISTED: usize = 12;

fn kinds() -> String {
    StateKind::ALL.iter().map(|k| k.id()).collect::<Vec<_>>().join("|")
}

fn shown(v: &Value) -> String {
    figcraft_format::clip(&v.to_string(), 60)
}

/// Which declaration a command is about: its screen, kind and name.
struct Key {
    frame: NodeId,
    kind: StateKind,
    name: String,
}

fn key(cmd: &str, p: &Value) -> Result<Key> {
    let frame = req_id(cmd, p, "frame")?;
    let kind = match get(p, "kind") {
        None => return Err(bad(cmd, format!("`kind` is required (one of {})", kinds()))),
        Some(v) => {
            let name = v.as_str().map(str::trim).unwrap_or_default();
            StateKind::ALL
                .into_iter()
                .find(|k| k.id().eq_ignore_ascii_case(name))
                .ok_or_else(|| bad(cmd, format!("`kind` must be one of {} (got {})", kinds(), shown(v))))?
        }
    };
    let name = match get(p, "name") {
        Some(v) => parse::text(cmd, v, "name")?.trim().to_string(),
        None => String::new(),
    };
    if kind == StateKind::Other && name.is_empty() {
        return Err(bad(cmd, "`name` is required for kind `other`: say which state it is"));
    }
    Ok(Key { frame, kind, name })
}

impl Key {
    fn label(&self) -> String {
        DeclaredState { frame: self.frame, kind: self.kind, name: self.name.clone(), ..declared(self.frame) }.label()
    }
}

/// An empty declaration on `frame`, to build others from.
fn declared(frame: NodeId) -> DeclaredState {
    DeclaredState {
        frame,
        kind: StateKind::default(),
        name: String::new(),
        shown_by: None,
        reason: String::new(),
        author: String::new(),
        status: ProposalStatus::Accepted,
        accepted_by: String::new(),
    }
}

/// `shownBy` as given: `Some(None)` takes it away (`null`), `None`: not given.
fn shown_by(cmd: &str, p: &Value) -> Result<Option<Option<ShownBy>>> {
    let o: &Map<String, Value> = match p.get("shownBy") {
        None => return Ok(None),
        Some(Value::Null) => return Ok(Some(None)),
        Some(Value::Object(o)) => o,
        Some(v) => return Err(bad(cmd, format!("`shownBy` must be {{node}} or {{instance, variant}}, or null (got {})", shown(v)))),
    };
    if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "node" | "instance" | "variant")) {
        return Err(bad(cmd, format!("`shownBy`: unknown key `{}` (it takes node, or instance and variant)", figcraft_format::clip(k, 40))));
    }
    let layer = |key: &str| -> Result<Option<NodeId>> {
        match o.get(key).filter(|v| !v.is_null()) {
            None => Ok(None),
            Some(v) => id_of(v).map(Some).ok_or_else(|| bad(cmd, format!("`shownBy.{key}` must be a layer id (got {})", shown(v)))),
        }
    };
    match (layer("node")?, layer("instance")?, o.get("variant").filter(|v| !v.is_null())) {
        (Some(node), None, None) => Ok(Some(Some(ShownBy::Node { node }))),
        (None, Some(instance), Some(v)) => {
            let variant = parse::text(cmd, v, "shownBy.variant")?.trim().to_string();
            if variant.is_empty() {
                return Err(bad(cmd, "`shownBy.variant` can't be empty: name the variant the instance shows the state with"));
            }
            Ok(Some(Some(ShownBy::Variant { instance, variant })))
        }
        (None, Some(_), None) => Err(bad(cmd, "`shownBy.variant` is required with `shownBy.instance`: the variant it shows the state with")),
        (Some(_), _, _) => Err(bad(cmd, "`shownBy` takes `node`, or `instance` with `variant`, not both")),
        (None, None, _) => Err(bad(cmd, "`shownBy` needs `node`, or `instance` with `variant`")),
    }
}

/// The variants instance `id`'s main component is one of (its component set's, else itself),
/// with their names.
fn variants_of(doc: &Document, component: NodeId) -> Vec<(NodeId, String)> {
    let set = doc.node(component).and_then(|c| c.parent).and_then(|p| doc.node(p)).filter(|p| matches!(p.kind, NodeKind::ComponentSet { .. }));
    match set {
        Some(set) => set
            .children
            .iter()
            .filter_map(|c| doc.node(*c))
            .filter(|c| matches!(c.kind, NodeKind::Component { .. }))
            .map(|c| (c.id, c.name.clone()))
            .collect(),
        None => doc.node(component).map(|c| vec![(c.id, c.name.clone())]).unwrap_or_default(),
    }
}

/// Why `by` doesn't show a state of screen `frame` in `doc`; `None`: it does (see the module).
fn not_shown_by(doc: &Document, frame: NodeId, by: &ShownBy) -> Option<String> {
    if !doc.contains(frame) {
        return Some(format!("the screen (layer {frame}) no longer exists"));
    }
    let inside = |id: NodeId| id == frame || doc.is_ancestor(frame, id);
    match by {
        ShownBy::Node { node } if !doc.contains(*node) => Some(format!("layer {node}, which showed it, no longer exists")),
        ShownBy::Node { node } if doc.top_level_frame(*node) == Some(*node) || inside(*node) => None,
        ShownBy::Node { node } => Some(format!("{} is neither a top-level frame nor a layer in {}", label(doc, *node), label(doc, frame))),
        ShownBy::Variant { instance, .. } if !doc.contains(*instance) => Some(format!("instance {instance}, which showed it, no longer exists")),
        ShownBy::Variant { instance, .. } if !inside(*instance) => Some(format!("{} isn't in {}", label(doc, *instance), label(doc, frame))),
        ShownBy::Variant { instance, variant } => {
            let Some(NodeKind::Instance { component, .. }) = doc.node(*instance).map(|n| &n.kind) else {
                return Some(format!("{} isn't an instance", label(doc, *instance)));
            };
            let variants = variants_of(doc, *component);
            if variants.iter().any(|(id, name)| crate::proto_sim::names_variant(variant, *id, name)) {
                return None;
            }
            let names: Vec<String> = variants.iter().take(MAX_LISTED).map(|(_, n)| format!("`{}`", crate::error::prose(n))).collect();
            Some(format!(
                "{} has no variant `{}` (its variants: {})",
                label(doc, *instance),
                figcraft_format::clip(variant, 60),
                if names.is_empty() { "none".to_string() } else { names.join(", ") }
            ))
        }
    }
}

/// Why declaration `st` isn't designed; `None`: it is.
fn not_designed(doc: &Document, st: &DeclaredState) -> Option<String> {
    match &st.shown_by {
        None if !doc.contains(st.frame) => Some(format!("the screen (layer {}) no longer exists", st.frame)),
        None => Some("nothing is said to show it (no `shownBy`)".into()),
        Some(by) => not_shown_by(doc, st.frame, by),
    }
}

/// The frame whose reachability counts for `st`: the top-level frame `shownBy` names, else the
/// screen (a layer or a variant shows on it).
fn shows_on(doc: &Document, st: &DeclaredState) -> NodeId {
    match &st.shown_by {
        Some(ShownBy::Node { node }) if doc.top_level_frame(*node) == Some(*node) => *node,
        _ => st.frame,
    }
}

/// Mark in `seen` the declarations of `doc` the simulator shows now (see the module's
/// "coveredBy"). Only what is designed is looked for.
pub(super) fn showing(doc: &Document, sim: &Sim, seen: &mut BTreeSet<usize>) {
    let open: Vec<NodeId> = std::iter::once(sim.screen()).chain(sim.overlays().iter().map(|o| o.id)).collect();
    for (i, st) in doc.design.states.iter().enumerate() {
        if seen.contains(&i) {
            continue;
        }
        let Some(by) = &st.shown_by else { continue };
        let shows = match by {
            ShownBy::Node { node } if doc.top_level_frame(*node) == Some(*node) => open.contains(node),
            // A layer or a variant shows on its screen: asked only while the screen is open.
            _ if !open.contains(&st.frame) || not_shown_by(doc, st.frame, by).is_some() => false,
            ShownBy::Node { node } => sim.check(&Expect { visible: vec![*node], ..Expect::default() }).is_empty(),
            ShownBy::Variant { instance, variant } => sim
                .check(&Expect {
                    visible: vec![*instance],
                    variants: vec![VariantExpect { instance: *instance, variant: variant.clone() }],
                    ..Expect::default()
                })
                .is_empty(),
        };
        if shows {
            seen.insert(i);
        }
    }
}

/// What can be said about one declaration.
pub(crate) struct StateFacts {
    /// Why it isn't designed; `None`: it is.
    pub not_designed: Option<String>,
    /// The flow graph reaches the frame that shows it.
    pub by_flow: bool,
    /// The scenarios that show it, in file order.
    pub covered_by: Vec<String>,
}

impl StateFacts {
    pub fn designed(&self) -> bool {
        self.not_designed.is_none()
    }

    pub fn reachable(&self) -> bool {
        self.by_flow || !self.covered_by.is_empty()
    }
}

/// The facts of every declaration of the file, in file order.
pub(crate) fn facts(f: &Facts<'_>) -> Vec<StateFacts> {
    let doc = f.doc;
    let states = &doc.design.states;
    if states.is_empty() {
        return Vec::new();
    }
    // The scenarios are run on the page they start on, each once per call.
    let pages: BTreeSet<NodeId> = doc.design.scenarios.iter().filter_map(|s| doc.page_of(s.start)).collect();
    let mut covered: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for page in pages {
        for (i, ids) in &f.flow(page).scenarios(doc).covered {
            covered.entry(*i).or_default().extend(ids.iter().cloned());
        }
    }
    let order = |id: &String| doc.design.scenarios.iter().position(|s| s.id == *id);
    states
        .iter()
        .enumerate()
        .map(|(i, st)| {
            let not_designed = not_designed(doc, st);
            let target = shows_on(doc, st);
            let by_flow = not_designed.is_none()
                && doc.page_of(target).is_some_and(|page| f.flow(page).graph.screens.iter().any(|sc| sc.id == target && sc.reachable));
            let mut covered_by: Vec<String> = covered.remove(&i).map(|ids| ids.into_iter().collect()).unwrap_or_default();
            covered_by.sort_by_key(order);
            StateFacts { not_designed, by_flow, covered_by }
        })
        .collect()
}

/// One declaration as the list and the flow check give it.
fn state_json(doc: &Document, st: &DeclaredState, fact: Option<&StateFacts>) -> Value {
    let name = |id: NodeId| doc.node(id).map(|n| n.name.clone());
    let mut v = json!({
        "frame": st.frame,
        "frameName": name(st.frame),
        "kind": st.kind,
        "author": author_name(&st.author),
        "status": st.status,
        "exists": doc.contains(st.frame),
    });
    let Some(o) = v.as_object_mut() else { return v };
    if !st.name.is_empty() {
        o.insert("name".into(), json!(st.name));
    }
    if let Some(by) = &st.shown_by {
        o.insert("shownBy".into(), json!(by));
        o.insert("shownByName".into(), json!(name(by.node())));
    }
    if !st.reason.is_empty() {
        o.insert("reason".into(), json!(st.reason));
    }
    if !st.accepted_by.is_empty() {
        o.insert("acceptedBy".into(), json!(st.accepted_by));
    }
    if let Some(f) = fact {
        o.insert("designed".into(), json!(f.designed()));
        if let Some(why) = &f.not_designed {
            o.insert("notDesigned".into(), json!(why));
        }
        o.insert("reachable".into(), json!(f.reachable()));
        let mut by: Vec<&str> = Vec::new();
        if f.by_flow {
            by.push("flow");
        }
        if !f.covered_by.is_empty() {
            by.push("scenario");
        }
        o.insert("reachedBy".into(), json!(by));
        o.insert("coveredBy".into(), json!(f.covered_by));
    }
    v
}

/// The declarations on the top-level frames of `page`, with their facts, for `proto.check`.
pub(super) fn on_page(f: &Facts<'_>, page: NodeId) -> Vec<Value> {
    let doc = f.doc;
    let all = facts(f);
    doc.design
        .states
        .iter()
        .zip(&all)
        .filter(|(st, _)| doc.page_of(st.frame) == Some(page))
        .map(|(st, fact)| state_json(doc, st, Some(fact)))
        .collect()
}

/// Refuse an AI agent (`me`) a change to a declaration that isn't its own pending proposal.
fn own_proposal(cmd: &str, me: Option<&str>, st: &DeclaredState, doing: &str) -> Result<()> {
    match me {
        Some(me) if st.author != me || st.status.is_accepted() => Err(bad(
            cmd,
            format!(
                "{} was declared by {}{}; an AI agent ({me}) {doing} only its own proposals. Ask them to change it",
                st.label(),
                author_name(&st.author),
                if st.status.is_accepted() { " and stands accepted" } else { "" }
            ),
        )),
        _ => Ok(()),
    }
}

pub(super) fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.state.set";
    let k = key(cmd, p)?;
    let by = shown_by(cmd, p)?;
    let reason = get(p, "reason").map(|v| parse::text(cmd, v, "reason").map(|r| r.trim().to_string())).transpose()?;
    let me = agent(s).map(str::to_string);
    let doc = s.doc()?;
    if !doc.contains(k.frame) {
        return Err(bad(cmd, format!("`frame`: no layer {}", k.frame)));
    }
    if doc.top_level_frame(k.frame) != Some(k.frame) {
        return Err(bad(cmd, format!("`frame`: {} isn't a top-level frame (a state belongs to a screen)", label(doc, k.frame))));
    }
    if let Some(Some(b)) = &by
        && let Some(why) = not_shown_by(doc, k.frame, b)
    {
        return Err(bad(cmd, format!("`shownBy`: {why}")));
    }
    let mut states = doc.design.states.clone();
    let full = states.len() >= MAX_ENTRIES;
    match states.iter_mut().find(|st| st.is(k.frame, k.kind, &k.name)) {
        Some(st) => {
            own_proposal(cmd, me.as_deref(), st, "changes")?;
            if let Some(b) = by {
                st.shown_by = b;
            }
            if let Some(r) = reason {
                st.reason = r;
            }
        }
        None if full => {
            return Err(bad(cmd, format!("the file holds {MAX_ENTRIES} declared states already, the most it keeps")));
        }
        None => states.push(DeclaredState {
            kind: k.kind,
            name: k.name.clone(),
            shown_by: by.flatten(),
            reason: reason.unwrap_or_default(),
            author: me.clone().unwrap_or_else(|| USER.to_string()),
            status: if me.is_some() { ProposalStatus::Proposed } else { ProposalStatus::Accepted },
            ..declared(k.frame)
        }),
    }
    let changed = states != doc.design.states;
    let out = states.iter().find(|st| st.is(k.frame, k.kind, &k.name)).map(|st| state_json(doc, st, None)).unwrap_or(Value::Null);
    // Declaring what is declared already is no edit (and no undo step).
    if changed {
        Arc::make_mut(&mut s.doc_mut()?.design).states = states;
    }
    Ok(json!({"state": out}))
}

pub(super) fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.state.remove";
    let k = key(cmd, p)?;
    let me = agent(s).map(str::to_string);
    let doc = s.doc()?;
    let Some(st) = doc.design.states.iter().find(|st| st.is(k.frame, k.kind, &k.name)) else {
        return Err(bad(cmd, format!("no {} is declared", k.label())));
    };
    own_proposal(cmd, me.as_deref(), st, "takes back")?;
    let removed = st.label();
    Arc::make_mut(&mut s.doc_mut()?.design).states.retain(|st| !st.is(k.frame, k.kind, &k.name));
    Ok(json!({"removed": removed}))
}

pub(super) fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.state.accept";
    let k = key(cmd, p)?;
    if let Some(me) = agent(s) {
        return Err(bad(cmd, format!("a person accepts a declared state: {} stays a proposal until the user accepts it ({me} can't)", k.label())));
    }
    let doc = s.doc()?;
    let mut states = doc.design.states.clone();
    let Some(st) = states.iter_mut().find(|st| st.is(k.frame, k.kind, &k.name)) else {
        return Err(bad(cmd, format!("no {} is declared (declare it with proto.state.set)", k.label())));
    };
    if st.status == ProposalStatus::Proposed {
        st.status = ProposalStatus::Accepted;
        if author_name(&st.author) != USER {
            st.accepted_by = USER.into();
        }
    }
    let out = state_json(doc, st, None);
    // Accepting what is accepted already is no edit (and no undo step).
    if states != doc.design.states {
        Arc::make_mut(&mut s.doc_mut()?.design).states = states;
    }
    Ok(json!({"state": out}))
}

pub(super) fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.state.list";
    let frame = crate::params::opt_id(cmd, p, "frame")?;
    let doc = s.doc()?;
    let f = Facts::new(doc);
    let all = facts(&f);
    let states: Vec<Value> = doc
        .design
        .states
        .iter()
        .zip(&all)
        .filter(|(st, _)| frame.is_none_or(|fr| st.frame == fr))
        .map(|(st, fact)| state_json(doc, st, Some(fact)))
        .collect();
    let count = |pick: fn(&Value) -> bool| states.iter().filter(|v| pick(v)).count();
    let summary = json!({
        "total": states.len(),
        "designed": count(|v| v["designed"] == json!(true)),
        "reachable": count(|v| v["reachable"] == json!(true)),
        "proposed": count(|v| v["status"] == json!("proposed")),
    });
    Ok(json!({"states": states, "summary": summary}))
}

/// The subject of a finding about declaration `st`.
fn subject(st: &DeclaredState) -> String {
    format!("state:{}:{}:{}", st.frame, st.kind.id(), st.name.trim().to_lowercase())
}

/// The layers a finding about `st` points at: the screen, then what shows it, those that exist.
fn nodes_of(doc: &Document, st: &DeclaredState) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = std::iter::once(st.frame).chain(st.shown_by.as_ref().map(ShownBy::node)).filter(|n| doc.contains(*n)).collect();
    out.dedup();
    out
}

/// Keep what is in `scope`: with layers asked for, the findings naming one of them (or a layer
/// inside one) and those naming none; else all of them. Declarations are the file's, so the
/// page alone narrows nothing.
fn in_scope(doc: &Document, scope: Scope<'_>, found: Vec<Found>) -> Vec<Found> {
    if scope.is_page() {
        return found;
    }
    let within = |n: &NodeId| scope.layers.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    found.into_iter().filter(|f| f.nodes.is_empty() || f.nodes.iter().any(within)).collect()
}

/// `states.none`: nothing declared, in a file with screens: said once, as that.
pub(super) fn none(f: &Facts<'_>, _: Scope<'_>) -> Vec<Found> {
    let doc = f.doc;
    let screens = doc.pages.iter().map(|p| doc.top_frames(*p).len()).sum::<usize>();
    if !doc.design.states.is_empty() || screens == 0 {
        return Vec::new();
    }
    vec![Found {
        rule: NONE,
        severity: Severity::Note,
        nodes: Vec::new(),
        subject: "states".into(),
        message: "No states are declared, so missing states can't be checked: this is not a finding about the design".into(),
        evidence: format!("0 declared states; {screens} top-level frame(s) in the file"),
        suggestion: "Say which states each screen has where time passes (first use, empty, loading, error, returning, success) with proto.state.set {frame, kind, shownBy?}; the states.* rules then compare them with the file".into(),
    }]
}

/// `states.notDesigned`: a declaration with nothing that shows it, or whose layer or variant is gone.
pub(super) fn not_designed_rule(f: &Facts<'_>, scope: Scope<'_>) -> Vec<Found> {
    let doc = f.doc;
    let found = doc
        .design
        .states
        .iter()
        .filter_map(|st| {
            let why = not_designed(doc, st)?;
            // Not designed yet is a gap the declaration names; a link that broke is a state the
            // file says it has and no longer does.
            let severity = if st.shown_by.is_none() && doc.contains(st.frame) { Severity::Minor } else { Severity::Major };
            Some(Found {
                rule: NOT_DESIGNED,
                severity,
                nodes: nodes_of(doc, st),
                subject: subject(st),
                message: format!("The {} state declared for {} isn't designed", kind_words(st), label(doc, st.frame)),
                evidence: format!("{}: {why}", st.label()),
                suggestion: "Design what shows it (a frame, a layer in the screen, or a variant of an instance in it) and link it with proto.state.set {frame, kind, name?, shownBy}".into(),
            })
        })
        .collect();
    in_scope(doc, scope, found)
}

/// `states.unreachable`: designed, and neither the flow nor a scenario gets there.
pub(super) fn unreachable(f: &Facts<'_>, scope: Scope<'_>) -> Vec<Found> {
    let doc = f.doc;
    let all = facts(f);
    let found = doc
        .design
        .states
        .iter()
        .zip(&all)
        .filter(|(_, fact)| fact.designed() && !fact.reachable())
        .map(|(st, _)| {
            let target = shows_on(doc, st);
            Found {
                rule: UNREACHABLE,
                severity: Severity::Minor,
                nodes: nodes_of(doc, st),
                subject: subject(st),
                message: format!("Nothing in the prototype gets to the {} state of {}", kind_words(st), label(doc, st.frame)),
                evidence: format!(
                    "{}: shown on {}, which no flow start reaches through the page's interactions, and no stored scenario shows it",
                    st.label(),
                    label(doc, target)
                ),
                suggestion: "Wire the interaction that leads there, or store a scenario that shows it (proto.scenario.add), so it can be seen in the prototype".into(),
            }
        })
        .collect();
    in_scope(doc, scope, found)
}

/// `states.errorEmpty`: a screen with declared states, none of them an error or an empty one.
pub(super) fn error_empty(f: &Facts<'_>, scope: Scope<'_>) -> Vec<Found> {
    let doc = f.doc;
    let mut by_frame: BTreeMap<NodeId, Vec<&DeclaredState>> = BTreeMap::new();
    for st in doc.design.states.iter().filter(|st| doc.contains(st.frame)) {
        by_frame.entry(st.frame).or_default().push(st);
    }
    let context = doc.design.brief.context.trim();
    let found = by_frame
        .into_iter()
        .filter(|(_, list)| !list.iter().any(|st| matches!(st.kind, StateKind::Error | StateKind::Empty)))
        .map(|(frame, list)| {
            let declared: Vec<String> = list.iter().take(MAX_LISTED).map(|st| kind_words(st)).collect();
            let brief = if context.is_empty() {
                "the brief doesn't say where and how it is used".to_string()
            } else {
                format!("the brief says it is used: {}", figcraft_format::clip(context, 160))
            };
            Found {
                rule: ERROR_EMPTY,
                severity: Severity::Note,
                nodes: vec![frame],
                subject: format!("stateKinds:{frame}"),
                message: format!(
                    "Can {} fail, or have nothing to show, where the brief says it is used? It declares states, but neither an error nor an empty one",
                    label(doc, frame)
                ),
                evidence: format!("declared: {}; {brief}", declared.join(", ")),
                suggestion: "If it can, declare and design that state (proto.state.set {frame, kind: error|empty, shownBy}); if it can't, a person says so by accepting this finding".into(),
            }
        })
        .collect();
    in_scope(doc, scope, found)
}

/// A declaration's kind as a finding says it: `error`, `other "Offline"`.
fn kind_words(st: &DeclaredState) -> String {
    if st.name.is_empty() { st.kind.id().to_string() } else { format!("{} \"{}\"", st.kind.id(), figcraft_format::clip(&st.name, 40)) }
}
