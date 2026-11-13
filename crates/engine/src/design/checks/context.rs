//! Checks of the design context itself: links to layers that are gone, a brief that doesn't say
//! who the design is for, assumptions nobody tested, decisions older than the brief. They name
//! no layer; each message says which entry it is about, which is how a stored finding is matched
//! to a later run.

use figcraft_doc::design::{AssumptionStatus, FindingStatus, Severity};
use figcraft_doc::{Document, NodeId};

use crate::check::Found;

pub(super) const MISSING_LINK: &str = "context.missingLink";
pub(super) const BRIEF_INCOMPLETE: &str = "context.briefIncomplete";
pub(super) const UNTESTED_ASSUMPTION: &str = "context.untestedAssumption";
pub(super) const STALE_DECISION: &str = "context.staleDecision";

/// The most entry ids one finding spells out.
const MAX_LISTED: usize = 20;

/// The links of the design context whose layer no longer exists: (layer id, what links to it),
/// each once. A finding that is settled (verified or accepted) keeps the layers it was about as
/// history: those aren't links anyone needs to repair.
pub(in crate::design) fn missing_links(doc: &Document) -> Vec<(NodeId, String)> {
    let settled =
        |from: &str| doc.design.findings.iter().any(|f| f.id == from && matches!(f.status, FindingStatus::Verified | FindingStatus::Accepted));
    let mut out: Vec<(NodeId, String)> = Vec::new();
    for link in doc.design.links() {
        if !doc.contains(link.0) && !settled(&link.1) && !out.contains(&link) {
            out.push(link);
        }
    }
    out
}

/// `context.missingLink`: one finding per link to a layer that is gone.
pub(super) fn missing_link(doc: &Document, _: &[NodeId]) -> Vec<Found> {
    let found = |(node, from): (NodeId, String)| Found {
        rule: MISSING_LINK,
        severity: Severity::Minor,
        nodes: Vec::new(),
        message: format!("{from} links to layer {node}, which no longer exists"),
        evidence: format!("{from} → {node}"),
        suggestion: "point it at the layer that took its place, or remove the link".into(),
    };
    missing_links(doc).into_iter().map(found).collect()
}

/// `context.briefIncomplete`: a file with screens whose brief leaves out who it is for, what
/// they need, where they use it or how success would show.
pub(super) fn brief_incomplete(doc: &Document, _: &[NodeId]) -> Vec<Found> {
    let b = &doc.design.brief;
    let empty: Vec<&str> =
        [("audience", b.audience.is_empty()), ("job", b.job.is_empty()), ("context", b.context.is_empty()), ("success", b.success.is_empty())]
            .into_iter()
            .filter_map(|(name, empty)| empty.then_some(name))
            .collect();
    let has_frames = doc.pages.iter().any(|p| !doc.top_frames(*p).is_empty());
    if empty.is_empty() || !has_frames {
        return Vec::new();
    }
    vec![Found {
        rule: BRIEF_INCOMPLETE,
        severity: Severity::Note,
        nodes: Vec::new(),
        message: "The brief doesn't say everything the design is judged against".into(),
        evidence: format!("empty: {}", empty.join(", ")),
        suggestion: "fill them in with design.brief.set from what the person asking has said; ask rather than guess".into(),
    }]
}

/// `context.untestedAssumption`: one finding naming the assumptions nobody has tested.
pub(super) fn untested_assumption(doc: &Document, _: &[NodeId]) -> Vec<Found> {
    let all = &doc.design.assumptions;
    let untested: Vec<&str> = all.iter().filter(|a| a.status == AssumptionStatus::Untested).map(|a| a.id.as_str()).collect();
    if untested.is_empty() {
        return Vec::new();
    }
    let more = if untested.len() > MAX_LISTED { ", …" } else { "" };
    let listed: Vec<&str> = untested.iter().copied().take(MAX_LISTED).collect();
    vec![Found {
        rule: UNTESTED_ASSUMPTION,
        severity: Severity::Note,
        nodes: Vec::new(),
        message: "The design rests on assumptions nobody has tested".into(),
        evidence: format!("{} of {} untested: {}{more}", untested.len(), all.len(), listed.join(", ")),
        suggestion: "treat them as open questions, not facts; a person records what supports or refutes each (design.assumption.set)".into(),
    }]
}

/// `context.staleDecision`: decisions added or last accepted under an earlier brief.
pub(super) fn stale_decision(doc: &Document, _: &[NodeId]) -> Vec<Found> {
    let now = doc.design.brief_revision;
    let found = |d: &figcraft_doc::design::Decision| Found {
        rule: STALE_DECISION,
        severity: Severity::Note,
        nodes: Vec::new(),
        message: format!("Decision {} was made under an earlier brief; re-read it", d.id),
        evidence: format!(
            "{} `{}`: made at brief revision {}, the brief is at revision {now}",
            d.id,
            d.title.chars().take(60).collect::<String>(),
            d.brief_revision
        ),
        suggestion: "if it still holds, a person accepts it again (design.decision.accept); otherwise change or remove it".into(),
    };
    doc.design.decisions.iter().filter(|d| d.brief_revision < now).map(found).collect()
}
