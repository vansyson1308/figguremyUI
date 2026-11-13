//! `proto.check`: the page's flow graph as facts, with what the flow and scenario rules find on
//! it; and the same rules as `design.check` lists them.

use std::collections::BTreeSet;

use figcraft_doc::design::Severity;
use figcraft_doc::proto::Trigger;
use figcraft_doc::{Document, Node, NodeId};
use serde_json::{Value, json};

use super::flow::{self, Edge, Graph};
use super::scenario;
use crate::check::{Found, Rule};
use crate::params::opt_id;
use crate::{Result, Session, bad};

/// The order findings are reported in: the worst first, then by rule, layer and wording, so the
/// same file always gives the same list.
fn sort(found: &mut [Found]) {
    found.sort_by(|a, b| (a.severity, a.rule, &a.nodes, &a.message).cmp(&(b.severity, b.rule, &b.nodes, &b.message)));
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
    json!({"rule": f.rule, "severity": f.severity, "nodes": f.nodes, "message": f.message, "evidence": f.evidence, "suggestion": f.suggestion})
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
    let g = flow::graph(doc, page);
    let mut problems = flow::findings(doc, &g);
    problems.extend(scenario::findings(doc, &g));
    sort(&mut problems);
    let name = |id: NodeId| doc.node(id).map(|n| n.name.clone());
    let starts: Vec<Value> = g
        .starts
        .iter()
        .map(
            |st| json!({"frame": st.frame, "name": name(st.frame), "source": if st.flow.is_some() { "flow" } else { "firstFrame" }, "flow": st.flow}),
        )
        .collect();
    let screens: Vec<Value> = g
        .screens
        .iter()
        .map(|sc| {
            json!({
                "id": sc.id,
                "name": name(sc.id),
                "reachable": sc.reachable,
                "endState": sc.end_state,
                "deadEnd": sc.dead_end,
                "overlay": sc.overlay,
                "out": sc.edges.iter().map(|e| edge_json(doc, e)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let ok = !problems.iter().any(|f| matches!(f.severity, Severity::Blocker | Severity::Major));
    Ok(json!({"page": page, "starts": starts, "screens": screens, "problems": problems.iter().map(found_json).collect::<Vec<_>>(), "ok": ok}))
}

/// What rule `rule` (one of those `find` reports) finds for `scope`: the pages the scoped layers
/// are on are checked, and a finding is kept when it is about a layer in scope (or about no layer
/// in particular). An empty scope is the current page, whole.
fn scoped(doc: &Document, scope: &[NodeId], rule: &str, find: fn(&Document, &Graph) -> Vec<Found>) -> Vec<Found> {
    let pages: BTreeSet<NodeId> = match scope {
        [] => doc.current_page.or_else(|| doc.first_page()).into_iter().collect(),
        ids => ids.iter().filter_map(|id| doc.page_of(*id)).collect(),
    };
    let within = |n: &NodeId| scope.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    let mut out: Vec<Found> = pages
        .into_iter()
        .flat_map(|page| find(doc, &flow::graph(doc, page)))
        .filter(|f| f.rule == rule && (scope.is_empty() || f.nodes.is_empty() || f.nodes.iter().any(within)))
        .collect();
    sort(&mut out);
    out
}

/// The flow and scenario rules, for `design.check`.
pub(super) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: flow::BROKEN_TARGET,
            about: "Blocker: an action that can't work. Navigate, open overlay, swap overlay, scroll to or change to a layer that no longer exists; navigate to a layer outside every top-level frame, or to a section without frames; change to a component that isn't a variant of the set an instance at or around the layer shows; set variable naming a missing variable or a value of another type; set variable mode naming a missing collection or mode; a condition reading a missing variable.",
            run: |doc, scope| scoped(doc, scope, flow::BROKEN_TARGET, flow::findings),
        },
        Rule {
            id: flow::DEAD_END,
            about: "Major: a screen a flow start reaches through navigate actions on which no action navigates to another screen, opens an overlay or opens a link, and none goes back either (back doesn't count on a screen where a flow starts). Screens declared with proto.endState.set are exempt, and so is a page with 0 interactions and 0 flow starting points. Whether a condition can ever hold isn't worked out.",
            run: |doc, scope| scoped(doc, scope, flow::DEAD_END, flow::findings),
        },
        Rule {
            id: flow::TRAPPED_OVERLAY,
            about: "Major: an overlay opened with close-on-outside-click off in which no action closes or swaps the overlay, goes back or navigates.",
            run: |doc, scope| scoped(doc, scope, flow::TRAPPED_OVERLAY, flow::findings),
        },
        Rule {
            id: flow::UNREACHABLE,
            about: "Minor when the frame has interactions of its own, else note: a top-level frame no flow start reaches through navigate actions, on a page with at least 1 interaction. Frames shown as overlays, components, component sets and recorded alternatives (and the frames they were made from) are skipped.",
            run: |doc, scope| scoped(doc, scope, flow::UNREACHABLE, flow::findings),
        },
        Rule {
            id: flow::NO_START,
            about: "Minor: the page has at least 1 interaction and 0 flow starting points (the player then starts at the page's first frame).",
            run: |doc, scope| scoped(doc, scope, flow::NO_START, flow::findings),
        },
        Rule {
            id: scenario::FAILS,
            about: "Major: a stored scenario that doesn't pass when it is run headless: a step can't run, a gesture or key sets nothing off, or an expectation doesn't hold. The evidence is the first failing step.",
            run: |doc, scope| scoped(doc, scope, scenario::FAILS, scenario::findings),
        },
        Rule {
            id: scenario::NONE,
            about: "Note: the page has at least 1 flow starting point and 0 stored scenarios starting on it.",
            run: |doc, scope| scoped(doc, scope, scenario::NONE, scenario::findings),
        },
        Rule {
            id: scenario::UNVISITED,
            about: "Note: screens a flow start reaches that no stored scenario shows, as its start or as the screen or an overlay after any of its steps. Reported only when at least 1 stored scenario starts on the page.",
            run: |doc, scope| scoped(doc, scope, scenario::UNVISITED, scenario::findings),
        },
    ]
}
