//! `proto.check`: the page's flow graph as facts, with what the flow and scenario rules find on
//! it; and the same rules as `design.check` lists them.
//!
//! One call builds a page's graph, its flow findings and the runs of its stored scenarios once
//! ([`PageFacts`], held by the call's [`Facts`]): every rule, and every stored finding a recheck
//! goes through, reads the same ones.

use std::cell::OnceCell;
use std::collections::BTreeSet;

use figcraft_doc::design::Severity;
use figcraft_doc::proto::Trigger;
use figcraft_doc::{Document, Node, NodeId};
use serde_json::{Value, json};

use super::flow::{self, Edge, Graph};
use super::scenario;
use crate::check::{Facts, Found, Rule, Scope};
use crate::params::opt_id;
use crate::{Result, Session, bad};

/// What one call knows about one page's prototype.
pub(crate) struct PageFacts {
    pub(super) graph: Graph,
    /// What the `flow.*` rules find.
    pub(super) flow: Vec<Found>,
    /// What the `scenario.*` rules find: the stored scenarios are run the first time it is asked.
    scenarios: OnceCell<Vec<Found>>,
}

impl PageFacts {
    pub(crate) fn build(facts: &Facts<'_>, page: NodeId) -> PageFacts {
        let graph = flow::graph(facts, page);
        let flow = flow::findings(facts.played(), &graph);
        PageFacts { graph, flow, scenarios: OnceCell::new() }
    }

    pub(super) fn scenarios(&self, doc: &Document) -> &[Found] {
        self.scenarios.get_or_init(|| scenario::findings(doc, &self.graph))
    }
}

/// The order findings are reported in: the worst first, then by rule, layer and wording, so the
/// same file always gives the same list.
fn sort(found: &mut [Found]) {
    found.sort_by(|a, b| (a.severity, a.rule, &a.nodes, &a.subject, &a.message).cmp(&(b.severity, b.rule, &b.nodes, &b.subject, &b.message)));
}

fn edge_json(doc: &Document, e: &Edge) -> Value {
    let name = |id: NodeId| doc.node(id).map(|n| n.name.clone());
    let mut v = json!({"from": e.from, "fromName": name(e.from), "trigger": flow::trigger_id(&e.trigger), "action": e.action});
    if let Some(o) = v.as_object_mut() {
        match &e.trigger {
            Trigger::AfterDelay { ms } => {
                o.insert("ms".into(), json!(ms));
            }
            Trigger::KeyPress { key } => {
                o.insert("key".into(), json!(key));
            }
            _ => {}
        }
        if let Some(to) = e.to {
            o.insert("to".into(), json!(to));
            o.insert("toName".into(), json!(name(to)));
        }
    }
    v
}

fn found_json(f: &Found) -> Value {
    let mut v =
        json!({"rule": f.rule, "severity": f.severity, "nodes": f.nodes, "message": f.message, "evidence": f.evidence, "suggestion": f.suggestion});
    if let (Some(o), false) = (v.as_object_mut(), f.subject.is_empty()) {
        o.insert("subject".into(), json!(f.subject));
    }
    v
}

pub(super) fn check(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.check";
    let doc = s.doc()?;
    let page = match opt_id(cmd, p, "page")? {
        Some(id) if doc.node(id).is_some_and(Node::is_page) => id,
        Some(id) if doc.contains(id) => return Err(bad(cmd, format!("`page`: {} isn't a page", crate::proto_sim::label(doc, id)))),
        Some(id) => return Err(bad(cmd, format!("`page`: no layer {id}"))),
        None => s.page()?,
    };
    let facts = Facts::new(doc);
    let page_facts = facts.flow(page);
    let g = &page_facts.graph;
    let mut problems = page_facts.flow.clone();
    problems.extend(page_facts.scenarios(doc).iter().cloned());
    sort(&mut problems);
    let name = |id: NodeId| doc.node(id).map(|n| n.name.clone());
    let starts: Vec<Value> =
        g.starts.iter().map(|st| json!({"frame": st.frame, "name": name(st.frame), "source": st.source.id(), "flow": st.flow})).collect();
    let screens: Vec<Value> = g
        .screens
        .iter()
        .map(|sc| {
            let mut v = json!({
                "id": sc.id,
                "name": name(sc.id),
                "reachable": sc.reachable,
                "endState": sc.end_state,
                "deadEnd": sc.dead_end,
                "overlay": sc.overlay,
                "component": sc.component,
                "out": sc.edges.iter().map(|e| edge_json(doc, e)).collect::<Vec<_>>(),
            });
            if let (Some(o), Some(who)) = (v.as_object_mut(), &sc.proposed_end) {
                o.insert("endStateProposedBy".into(), json!(who));
            }
            v
        })
        .collect();
    let ok = !problems.iter().any(|f| matches!(f.severity, Severity::Blocker | Severity::Major));
    Ok(json!({
        "page": page,
        "prototyped": g.prototyped,
        "starts": starts,
        "screens": screens,
        "problems": problems.iter().map(found_json).collect::<Vec<_>>(),
        "ok": ok,
    }))
}

/// Where a finding with subject `subject` would be reported again.
pub(crate) enum Where {
    /// On this page.
    Page(NodeId),
    /// Nowhere: what it was about is gone (why).
    Gone(String),
    /// On whichever page leads to it: every page has to be looked at.
    EveryPage,
    /// On any page alike (the subject isn't a place).
    Anywhere,
}

/// Where the rules would report subject `subject` again: a page-level finding on its page, a
/// scenario's on the page it starts on, a broken action's on the page of the layer that holds
/// it, a trapped overlay's on any page that opens it.
pub(crate) fn subject_where(doc: &Document, subject: &str) -> Where {
    let (kind, rest) = subject.split_once(':').unwrap_or((subject, ""));
    let layer = || rest.split(':').next().and_then(|id| id.parse::<NodeId>().ok());
    match kind {
        "page" => match layer().filter(|p| doc.node(*p).is_some_and(Node::is_page)) {
            Some(page) => Where::Page(page),
            None => Where::Gone("the page no longer exists".into()),
        },
        "scenario" => match doc.design.scenarios.iter().find(|s| s.id == rest) {
            None => Where::Gone(format!("scenario {} no longer exists", figcraft_format::clip(rest, 40))),
            // A scenario whose start is gone is reported on every page alike.
            Some(s) => doc.page_of(s.start).map_or(Where::Anywhere, Where::Page),
        },
        "action" => match layer().and_then(|l| doc.page_of(l)) {
            Some(page) => Where::Page(page),
            None => Where::Gone("the layer no longer exists".into()),
        },
        "overlay" => match layer().filter(|l| doc.contains(*l)) {
            Some(_) => Where::EveryPage,
            None => Where::Gone("the layer no longer exists".into()),
        },
        _ => Where::Anywhere,
    }
}

/// Which of a page's findings a rule gives.
#[derive(Clone, Copy)]
enum Kind {
    Flow,
    Scenario,
}

/// What rule `rule` finds for `scope`.
///
/// Without layers in the scope, the scope's page is checked whole: everything the rule finds on
/// it, also what is about the page itself and about layers lying outside every frame. With
/// layers, the pages they are on are checked, and a finding is kept when one of the layers it
/// names is in scope (a scoped layer or inside one); a finding about a page as a whole (no start,
/// no scenario, screens no scenario shows) is kept when the scope holds that page or one of its
/// top-level frames.
fn scoped(facts: &Facts<'_>, scope: Scope<'_>, rule: &str, kind: Kind) -> Vec<Found> {
    let doc = facts.doc;
    let pages: BTreeSet<NodeId> =
        if scope.is_page() { std::iter::once(scope.page).collect() } else { scope.layers.iter().filter_map(|id| doc.page_of(*id)).collect() };
    let within = |n: &NodeId| scope.layers.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    let mut out: Vec<Found> = Vec::new();
    for page in pages {
        let page_facts = facts.flow(page);
        let all = match kind {
            Kind::Flow => page_facts.flow.as_slice(),
            Kind::Scenario => page_facts.scenarios(doc),
        };
        let holds_page = scope.layers.iter().any(|s| *s == page || (doc.top_level_frame(*s) == Some(*s) && doc.page_of(*s) == Some(page)));
        let kept = |f: &&Found| {
            f.rule == rule
                && (scope.is_page() || if f.nodes.is_empty() || f.subject.starts_with("page:") { holds_page } else { f.nodes.iter().any(within) })
        };
        out.extend(all.iter().filter(kept).cloned());
    }
    sort(&mut out);
    out
}

/// The flow and scenario rules, for `design.check`.
pub(super) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: flow::BROKEN_TARGET,
            about: "Blocker: an action that can't work. Navigate, open overlay, swap overlay, scroll to or change to a layer that no longer exists; navigate to a layer outside every top-level frame, or to a section without frames; change to a layer that isn't a component, or on a layer with no instance at or around it to change; set variable naming a missing variable or a value of another type; set variable mode naming a missing collection or mode; a condition reading a missing variable. Looked for on every layer of the page as the player has it: what an instance plays from its main component counts wherever that component lives (reported once per action, on the component's layer), and so do the variants its change-to actions lead to.",
            run: |f, scope| scoped(f, scope, flow::BROKEN_TARGET, Kind::Flow),
            unsettled: None,
        },
        Rule {
            id: flow::DEAD_END,
            about: "Major: a screen a start reaches through navigate actions on which no action navigates to another screen, opens an overlay or opens a link, and none goes back either (back doesn't count on the screen the prototype starts on). Asked only where it makes sense: a screen of the page plays an action that moves between screens (or the screen is reached from another page), and the page has another frame that counts as a screen (not a component, a frame only shown as an overlay, or a recorded alternative). Screens a person declared or accepted as end states (proto.endState.set, proto.endState.accept) are exempt; one an AI agent declared isn't until a person accepts it. Components and component sets are never dead ends. Whether a condition can ever hold isn't worked out.",
            run: |f, scope| scoped(f, scope, flow::DEAD_END, Kind::Flow),
            unsettled: None,
        },
        Rule {
            id: flow::TRAPPED_OVERLAY,
            about: "Major: an overlay shown with close-on-outside-click off that the viewer can't get out of: no action in it closes the overlay, goes back or navigates, and neither does any overlay it can be swapped for (a swap keeps the overlay open the same way).",
            run: |f, scope| scoped(f, scope, flow::TRAPPED_OVERLAY, Kind::Flow),
            unsettled: None,
        },
        Rule {
            id: flow::UNREACHABLE,
            about: "Minor when the frame has interactions of its own, else note: a top-level frame no start reaches through navigate actions (starts: the page's flow starting points and the frames interactions on other pages navigate to; without either, the first frame), on a page that is a prototype: a flow start is declared, a screen plays an action that moves between screens, or another page leads here. Frames shown as overlays, components, component sets and recorded alternatives (and the frames they were made from) are skipped.",
            run: |f, scope| scoped(f, scope, flow::UNREACHABLE, Kind::Flow),
            unsettled: None,
        },
        Rule {
            id: flow::NO_START,
            about: "Minor: a screen of the page plays an action that moves between screens (navigate, back, open, swap or close overlay, open link), yet the page has 0 flow starting points and no other page leads to it (the player then starts at the page's first frame). A page whose only interactions are hover states, change-to or variables isn't reported.",
            run: |f, scope| scoped(f, scope, flow::NO_START, Kind::Flow),
            unsettled: None,
        },
        Rule {
            id: scenario::FAILS,
            about: "Major: a stored scenario that doesn't pass when it is run headless: a step can't run, a gesture or key sets nothing off, or an expectation doesn't hold. The evidence is the first failing step. One finding per scenario, whichever step fails.",
            run: |f, scope| scoped(f, scope, scenario::FAILS, Kind::Scenario),
            unsettled: None,
        },
        Rule {
            id: scenario::NONE,
            about: "Note: the page has at least 1 flow starting point and 0 stored scenarios starting on it.",
            run: |f, scope| scoped(f, scope, scenario::NONE, Kind::Scenario),
            unsettled: None,
        },
        Rule {
            id: scenario::UNVISITED,
            about: "Note: screens a start reaches that no stored scenario shows, as its start or as the screen or an overlay after any of its steps. One finding per page, listing them, while any is left. Reported only when at least 1 stored scenario starts on the page.",
            run: |f, scope| scoped(f, scope, scenario::UNVISITED, Kind::Scenario),
            unsettled: None,
        },
    ]
}
