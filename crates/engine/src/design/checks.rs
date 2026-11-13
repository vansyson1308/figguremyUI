//! The design checks: which rules exist, which run at which stage, and running them.
//!
//! A rule measures the file and cites the standard its threshold comes from; what has no such
//! standard (too many font sizes, type that "feels" small) is a judgement and isn't a rule here.
//! A rule that can't measure with certainty says so in a finding of its own instead of guessing.
//! Hidden layers (invisible, or inside an invisible layer) are never looked at.

mod clipping;
mod context;
mod contrast;
mod target;

use std::collections::BTreeSet;

use figcraft_doc::design::{Finding, Stage};
use figcraft_doc::{Document, NodeId};

use crate::check::{Facts, Found, Rule, Scope};

pub(super) use context::missing_links;

/// Every rule `design.check` knows: the design module's own, then those other modules provide.
pub fn rules() -> Vec<Rule> {
    let mut all = vec![
        Rule {
            id: contrast::TEXT,
            about: "Text whose colour against what is behind it has a contrast ratio below 4.5:1, or below 3:1 for large text (at least 24 px, or at least 18.66 px and bold: weight 700 or more). WCAG 2.2 SC 1.4.3 Contrast (Minimum), level AA. Measured on solid fills composited with their opacities down to the page colour. A stored finding stays open while its text can no longer be measured.",
            run: |f, scope| contrast::failing(f.doc, &scope.roots(f.doc)),
            unsettled: Some(|f, scope| contrast::no_longer_measured(f.doc, &scope.roots(f.doc))),
        },
        Rule {
            id: contrast::UNMEASURED,
            about: "Text whose contrast (WCAG 2.2 SC 1.4.3) could not be measured with certainty: a gradient, image or pattern fill, a blend mode, a mask or another layer overlapping it is in the way, or the text is drawn by a stroke alone. Reported so it isn't taken as passing; check it by eye.",
            run: |f, scope| contrast::unmeasured(f.doc, &scope.roots(f.doc)),
            unsettled: None,
        },
        Rule {
            id: clipping::TEXT,
            about: "Text that doesn't fit: a fixed-size text box whose laid-out text is taller or wider than the box (boxes set to truncate are left alone), or a text layer cut by a frame around it that clips its content and doesn't scroll that way. A fact about the file, not a threshold from a standard: any cut counts, with 0.5 px allowed for rounding.",
            run: |f, scope| clipping::text(f.doc, &scope.roots(f.doc)),
            unsettled: None,
        },
        Rule {
            id: clipping::CONTENT,
            about: "A layer other than text that reaches outside a frame around it that clips its content and doesn't scroll that way: the part outside can't be seen or reached. A fact about the file, not a threshold from a standard: any cut counts, with 0.5 px allowed for rounding.",
            run: |f, scope| clipping::content(f.doc, &scope.roots(f.doc)),
            unsettled: None,
        },
        Rule {
            id: target::SMALL,
            about: "A layer with a click, press or drag interaction (its own, or the one its main component gives it) smaller than 24×24 px in either direction: WCAG 2.2 SC 2.5.8 Target Size (Minimum), level AA (major). On a page whose prototype device is a phone or tablet, also one smaller than 44×44 px: SC 2.5.5 Target Size (Enhanced), level AAA (note). Only the size is measured: the exceptions of SC 2.5.8 (enough spacing around the target, a target inside a sentence, an equivalent control elsewhere on the screen, a size that is essential) aren't evaluated, so a result says \"smaller than 24×24\" and is to be judged against those exceptions.",
            run: |f, scope| target::small(f.played(), &scope.roots(f.doc)),
            unsettled: None,
        },
        Rule {
            id: context::MISSING_LINK,
            about: "A link from the design context (an assumption, decision, scenario, end state, finding or alternative) to a layer that no longer exists.",
            run: |f, _| context::missing_link(f.doc),
            unsettled: None,
        },
        Rule {
            id: context::BRIEF_INCOMPLETE,
            about: "The brief has no audience, job, context or success signal, in a file that has at least one top-level frame.",
            run: |f, _| context::brief_incomplete(f.doc),
            unsettled: None,
        },
        Rule {
            id: context::UNTESTED_ASSUMPTION,
            about: "One finding listing the assumptions nobody has tested, so none is mistaken for a fact.",
            run: |f, _| context::untested_assumption(f.doc),
            unsettled: None,
        },
        Rule {
            id: context::STALE_DECISION,
            about: "A decision added or last accepted under an earlier revision of the brief than the current one.",
            run: |f, _| context::stale_decision(f.doc),
            unsettled: None,
        },
    ];
    all.extend(external_rules());
    all
}

/// Rules other modules provide: the prototype flow and scenario rules.
fn external_rules() -> Vec<Rule> {
    crate::proto_verify::rules()
}

/// The earliest stage at which rule `rule_id` runs unless it is asked for by name. While a design
/// is still being explored only what would break the file is worth reporting; a rule this table
/// doesn't know (another module's) runs from `converging`.
pub fn min_stage(rule_id: &str) -> Stage {
    match rule_id {
        "flow.brokenTarget" | context::MISSING_LINK => Stage::Exploring,
        context::BRIEF_INCOMPLETE | context::UNTESTED_ASSUMPTION | context::STALE_DECISION | "scenario.none" | "scenario.unvisited" => Stage::Ready,
        _ => Stage::Converging,
    }
}

/// What a run of the checks gave.
pub(super) struct Outcome {
    pub found: Vec<Found>,
    /// The rules that ran.
    pub ran: Vec<Rule>,
    /// The rules that didn't, each with why: never left out silently.
    pub skipped: Vec<(&'static str, String)>,
}

/// Run the rules on `scope`: exactly those named in `only`, else those the file's stage calls for.
pub(super) fn run(facts: &Facts<'_>, scope: Scope<'_>, only: Option<&[String]>) -> Outcome {
    let stage = facts.doc.design.stage;
    let mut out = Outcome { found: Vec::new(), ran: Vec::new(), skipped: Vec::new() };
    for rule in rules() {
        let skip = match only {
            Some(names) => (!names.iter().any(|n| n == rule.id)).then(|| "not among the `rules` asked for".to_string()),
            None => {
                let from = min_stage(rule.id);
                (from > stage).then(|| format!("runs from stage `{}`; the file is at `{}` (pass `rules` to run it now)", from.id(), stage.id()))
            }
        };
        match skip {
            Some(reason) => out.skipped.push((rule.id, reason)),
            None => {
                out.found.extend((rule.run)(facts, scope));
                out.ran.push(rule);
            }
        }
    }
    sort(&mut out.found);
    out
}

/// What rule `rule_id` finds in `scope` now, and what it can't call fixed there (see
/// [`Rule::unsettled`]); `None` when no such rule exists.
pub(super) fn run_rule(facts: &Facts<'_>, rule_id: &str, scope: Scope<'_>) -> Option<(Vec<Found>, Vec<Found>)> {
    let rule = rules().into_iter().find(|r| r.id == rule_id)?;
    let mut found = (rule.run)(facts, scope);
    sort(&mut found);
    let mut unsettled = rule.unsettled.map(|also| also(facts, scope)).unwrap_or_default();
    sort(&mut unsettled);
    Some((found, unsettled))
}

/// By severity, then rule, then layer and subject: the same file always lists its findings the
/// same way.
fn sort(found: &mut [Found]) {
    found.sort_by(|a, b| {
        (a.severity, a.rule, a.nodes.first(), &a.subject, &a.message, &a.evidence).cmp(&(
            b.severity,
            b.rule,
            b.nodes.first(),
            &b.subject,
            &b.message,
            &b.evidence,
        ))
    });
}

/// Whether stored finding `f` and result `found` are about the same problem.
///
/// A result that names its subject (a scenario, an overlay, a page, an entry of the context) is
/// the same problem as a stored finding of the same rule with the same subject, whatever layers
/// either lists: those are where to look this time. Without a subject, the layers are what the
/// finding is about and must be the same ones.
///
/// A finding stored before subjects existed has none. Against a result that has one it counts as
/// the same problem when they share a layer or, naming none, say the same: erring on the side of
/// "still there", since a finding wrongly kept open is re-read by a person and one wrongly
/// verified isn't.
pub(super) fn same_problem(f: &Finding, found: &Found) -> bool {
    if f.rule != found.rule {
        return false;
    }
    match (f.subject.is_empty(), found.subject.is_empty()) {
        (false, false) => f.subject == found.subject,
        (false, true) => false,
        (true, true) if f.nodes.is_empty() => found.nodes.is_empty() && f.message == found.message,
        (true, true) => f.nodes == found.nodes,
        (true, false) if f.nodes.is_empty() => f.message == found.message,
        (true, false) => f.nodes.iter().any(|n| found.nodes.contains(n)),
    }
}

/// Whether layer `id` shows: it and everything around it is visible.
fn shows(doc: &Document, id: NodeId) -> bool {
    let visible = |i: NodeId| doc.node(i).is_some_and(|n| n.visible);
    visible(id) && doc.ancestors(id).into_iter().all(|a| visible(a) && !doc.node(a).is_some_and(figcraft_doc::Node::hides_contents))
}

/// The layers of `scope` that show, each with everything showing inside it, in layer order and
/// once each (pages themselves aren't layers).
fn shown_layers(doc: &Document, scope: &[NodeId]) -> Vec<NodeId> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for root in scope.iter().copied().filter(|r| shows(doc, *r)) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(n) = doc.node(id) else { continue };
            // Each layer is walked once, so the walk ends even if a damaged file repeats a child.
            if !n.visible || !seen.insert(id) {
                continue;
            }
            if !n.is_page() {
                out.push(id);
            }
            if !n.hides_contents() {
                stack.extend(n.children.iter().rev());
            }
        }
    }
    out
}

/// A layer as findings name it: its name in backticks.
fn named(doc: &Document, id: NodeId) -> String {
    let name: String = doc.node(id).map(|n| n.name.chars().take(60).collect()).unwrap_or_default();
    format!("`{name}`")
}

/// A length for evidence: whole when it is, else to two decimals.
fn px(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
}
