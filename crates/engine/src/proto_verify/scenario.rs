//! Stored scenarios (`proto.scenario.*`): walks through the prototype kept in the file with what
//! should be true along the way, run headless on the simulator; and the rules that report on
//! them (`scenario.*`).

use std::collections::BTreeSet;
use std::sync::Arc;

use figcraft_doc::design::{MAX_ENTRIES, Scenario, Severity, StepAction};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use super::flow::Graph;
use super::parse;
use super::run::{Run, run, steps_json};
use crate::check::Found;
use crate::params::{get, req_id, req_string};
use crate::proto_sim::label;
use crate::{Result, Session, bad};

pub(super) const FAILS: &str = "scenario.fails";
pub(super) const NONE: &str = "scenario.none";
pub(super) const UNVISITED: &str = "scenario.unvisited";

/// The longest evidence one failing scenario gives (characters).
const MAX_EVIDENCE: usize = 600;

/// `key` as trimmed text, when given.
fn text(cmd: &str, p: &Value, key: &str) -> Result<Option<String>> {
    get(p, key).map(|v| parse::text(cmd, v, key).map(|s| s.trim().to_string())).transpose()
}

fn name(cmd: &str, p: &Value) -> Result<Option<String>> {
    match text(cmd, p, "name")? {
        Some(n) if n.is_empty() => Err(bad(cmd, "`name` can't be empty")),
        n => Ok(n),
    }
}

fn find<'a>(cmd: &str, doc: &'a Document, id: &str) -> Result<&'a Scenario> {
    let known = || doc.design.scenarios.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join(", ");
    doc.design.scenarios.iter().find(|s| s.id == id).ok_or_else(|| {
        let id = figcraft_format::clip(id, 40);
        bad(
            cmd,
            if doc.design.scenarios.is_empty() {
                format!("`id`: no scenario `{id}` (none are stored)")
            } else {
                format!("`id`: no scenario `{id}` (stored: {})", known())
            },
        )
    })
}

pub(super) fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.scenario.add";
    let mut scenario = Scenario {
        id: String::new(),
        name: name(cmd, p)?.ok_or_else(|| bad(cmd, "`name` is required"))?,
        purpose: text(cmd, p, "purpose")?.unwrap_or_default(),
        start: req_id(cmd, p, "start")?,
        steps: parse::steps(cmd, get(p, "steps").ok_or_else(|| bad(cmd, "`steps` is required (a list, which may be empty)"))?, "steps")?,
        expect: get(p, "expect").map(|v| parse::expect(cmd, v, "expect")).transpose()?.unwrap_or_default(),
    };
    let doc = s.doc()?;
    parse::validate(cmd, doc, &scenario)?;
    if doc.design.scenarios.len() >= MAX_ENTRIES {
        return Err(bad(cmd, format!("the file holds {MAX_ENTRIES} scenarios already, the most it keeps")));
    }
    let design = Arc::make_mut(&mut s.doc_mut()?.design);
    scenario.id = design.next_id("S");
    let id = scenario.id.clone();
    design.scenarios.push(scenario);
    Ok(json!({"id": id}))
}

pub(super) fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.scenario.set";
    let id = req_string(cmd, p, "id")?;
    let mut scenario = find(cmd, s.doc()?, &id)?.clone();
    let before = scenario.clone();
    if let Some(n) = name(cmd, p)? {
        scenario.name = n;
    }
    if let Some(purpose) = text(cmd, p, "purpose")? {
        scenario.purpose = purpose;
    }
    if get(p, "start").is_some() {
        scenario.start = req_id(cmd, p, "start")?;
    }
    if let Some(v) = get(p, "steps") {
        scenario.steps = parse::steps(cmd, v, "steps")?;
    }
    if let Some(v) = get(p, "expect") {
        scenario.expect = parse::expect(cmd, v, "expect")?;
    }
    parse::validate(cmd, s.doc()?, &scenario)?;
    // Nothing to change is no edit (and no undo step).
    if scenario != before
        && let Some(slot) = Arc::make_mut(&mut s.doc_mut()?.design).scenarios.iter_mut().find(|x| x.id == id)
    {
        *slot = scenario.clone();
    }
    Ok(json!({"id": id, "scenario": scenario}))
}

pub(super) fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.scenario.remove";
    let id = req_string(cmd, p, "id")?;
    find(cmd, s.doc()?, &id)?;
    Arc::make_mut(&mut s.doc_mut()?.design).scenarios.retain(|x| x.id != id);
    Ok(json!({"removed": id}))
}

pub(super) fn list(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({"scenarios": s.doc()?.design.scenarios}))
}

fn play(doc: &Document, s: &Scenario) -> Run {
    run(doc, s.start, &s.steps, &s.expect, true)
}

pub(super) fn run_stored(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.scenario.run";
    let doc = s.doc()?;
    let chosen: Vec<&Scenario> = match get(p, "id") {
        Some(_) => vec![find(cmd, doc, &req_string(cmd, p, "id")?)?],
        None => doc.design.scenarios.iter().collect(),
    };
    let mut failed = 0;
    let results: Vec<Value> = chosen
        .iter()
        .map(|sc| {
            let r = play(doc, sc);
            failed += usize::from(!r.passed);
            let mut v = json!({
                "id": sc.id,
                "name": sc.name,
                "passed": r.passed,
                "failures": r.first_failures(),
                "steps": steps_json(&sc.steps, &r),
                "stepsTaken": r.steps.len(),
            });
            if let (Some(o), Some(i)) = (v.as_object_mut(), r.failed_step) {
                o.insert("failedStep".into(), json!(i));
            }
            v
        })
        .collect();
    Ok(json!({"passed": failed == 0, "total": results.len(), "failed": failed, "results": results}))
}

/// What a step does, in a few words.
fn describe(doc: &Document, a: &StepAction) -> String {
    match a {
        StepAction::Gesture { node, gesture } => format!("{} on {}", crate::proto_sim::gesture_name(*gesture), label(doc, *node)),
        StepAction::Key { key } => format!("key `{}`", figcraft_format::clip(key, 40)),
        StepAction::Wait => "wait".into(),
        StepAction::ClickOutside => "click outside the overlay".into(),
        StepAction::SetVariable { variable, .. } => format!("set variable `{}`", figcraft_format::clip(variable, 40)),
    }
}

/// Why scenario `sc` doesn't pass, as one finding.
fn failure(doc: &Document, sc: &Scenario, r: &Run) -> Found {
    let step = r.failed_step.and_then(|i| Some((i, sc.steps.get(i)?)));
    let (place, layer) = match step {
        Some((i, step)) => (format!("steps[{i}] ({})", describe(doc, &step.action)), step.action.node()),
        None if r.state.is_none() => ("before the first step".to_string(), None),
        None => ("after the last step".to_string(), None),
    };
    // The most specific layer first; one that is gone would put the finding out of every scope.
    let mut nodes: Vec<NodeId> = layer.into_iter().chain([sc.start]).filter(|n| doc.contains(*n)).collect();
    nodes.dedup();
    Found {
        rule: FAILS,
        severity: Severity::Major,
        nodes,
        message: format!("scenario {} \"{}\" doesn't pass", sc.id, figcraft_format::clip(&sc.name, 80)),
        evidence: figcraft_format::clip(&format!("{place}: {}", r.first_failures().join("; ")), MAX_EVIDENCE),
        suggestion: format!(
            "Run proto.scenario.run {{id: \"{}\"}} to see every step; fix the prototype, or the scenario if what it expects has changed on purpose.",
            sc.id
        ),
    }
}

/// What the `scenario.*` rules find for the graph's page: the stored scenarios starting on it are
/// run (a scenario whose start is gone belongs to every page, so that it is reported somewhere).
pub(super) fn findings(doc: &Document, g: &Graph) -> Vec<Found> {
    let here: Vec<&Scenario> = doc.design.scenarios.iter().filter(|s| doc.page_of(s.start) == Some(g.page)).collect();
    let orphans = doc.design.scenarios.iter().filter(|s| !doc.contains(s.start));
    let mut out = Vec::new();
    let mut shown = BTreeSet::new();
    for sc in here.iter().copied().chain(orphans) {
        let r = play(doc, sc);
        shown.extend(r.shown(sc.start));
        if !r.passed {
            out.push(failure(doc, sc, &r));
        }
    }
    if here.is_empty() {
        if let Some(start) = g.starts.first().filter(|_| g.has_flow) {
            out.push(Found {
                rule: NONE,
                severity: Severity::Note,
                nodes: vec![start.frame],
                message: "the page has a flow but no stored scenario, so nothing checks that the flow works".into(),
                evidence: format!("{} flow start(s), the first at {}; 0 scenarios start on this page", g.starts.len(), label(doc, start.frame)),
                suggestion: "Store the walk a viewer is meant to take, with what should be true at the end, with proto.scenario.add.".into(),
            });
        }
        return out;
    }
    let unvisited: Vec<NodeId> = g.screens.iter().filter(|s| s.reachable && !shown.contains(&s.id)).map(|s| s.id).collect();
    if !unvisited.is_empty() {
        let names: Vec<String> = unvisited.iter().map(|id| label(doc, *id)).collect();
        out.push(Found {
            rule: UNVISITED,
            severity: Severity::Note,
            nodes: unvisited.clone(),
            message: format!("{} screen(s) the flows reach are never shown by a stored scenario", unvisited.len()),
            evidence: figcraft_format::clip(&names.join(", "), MAX_EVIDENCE),
            suggestion: "Add a scenario that goes there (proto.scenario.add), or check by hand what no scenario covers.".into(),
        });
    }
    out
}
