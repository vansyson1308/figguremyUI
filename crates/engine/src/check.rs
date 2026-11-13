//! What a design check reports, and how checks are listed. A check is a rule that measures
//! something in the file: the same file gives the same result, and running it again after a
//! fix settles whether the fix worked. Opinions don't belong here (they are findings of kind
//! `judgement`, recorded by whoever holds them).

use figcraft_doc::design::Severity;
use figcraft_doc::{Document, NodeId};

/// One thing a rule found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The rule's id (`contrast.text`, `flow.deadEnd`…).
    pub rule: &'static str,
    pub severity: Severity,
    /// The layers it is about, the most specific first.
    pub nodes: Vec<NodeId>,
    /// What is wrong, in one sentence a person can act on.
    pub message: String,
    /// What was measured (numbers, names, the step that failed).
    pub evidence: String,
    /// One way to fix it.
    pub suggestion: String,
}

/// A rule: `run` looks at the layers in `scope` (top-level frames or any layers; each with
/// everything inside it) and reports what it finds. An empty scope is the whole current page,
/// which the caller resolves before calling.
pub struct Rule {
    pub id: &'static str,
    /// What the rule measures and its threshold, for `design.check`'s rule list.
    pub about: &'static str,
    pub run: fn(&Document, &[NodeId]) -> Vec<Found>,
}
