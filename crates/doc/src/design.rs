//! Design context: why the file is the way it is, and how to tell whether it works.
//!
//! A file can carry a brief (who it is for, what they need to get done, where and with what
//! limits), the assumptions the design rests on, the decisions taken and why, scenarios that walk
//! the prototype with expected results, the frames where a flow may legitimately end, findings
//! from checks and reviews, and sets of alternatives explored for a screen. Everything links to
//! layers by their stable [`NodeId`]; a link whose layer is gone stays in the file and is reported
//! as missing rather than dropped, so the reasoning isn't lost with the layer.
//!
//! Files without any of this serialize exactly as before (the field is skipped while empty).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// The longest text one field holds (chars); longer input is refused by the commands.
pub const MAX_TEXT: usize = 4000;
/// The most entries one list holds (assumptions, decisions, scenarios, findings, steps, links…).
pub const MAX_ENTRIES: usize = 500;

/// Everything a file records about its purpose and its verification.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesignContext {
    #[serde(skip_serializing_if = "Brief::is_empty")]
    pub brief: Brief,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<Assumption>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<Scenario>,
    /// Frames where a flow may end on purpose (a confirmation, a sent message). A frame with no
    /// way out that isn't listed here is a dead end.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub end_states: Vec<EndState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<AlternativeSet>,
    /// The last number given out per id prefix (`A`, `D`, `S`, `F`, `V`): ids are never reused, so
    /// a reference to a removed entry can't come to mean another one.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, u64>,
}

impl DesignContext {
    /// Nothing recorded: the file serializes without the field.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// A fresh id with `prefix` (`A` → `A1`, `A2`…).
    pub fn next_id(&mut self, prefix: &str) -> String {
        let n = self.counters.entry(prefix.to_string()).or_insert(0);
        *n = n.saturating_add(1);
        format!("{prefix}{n}")
    }

    /// Every layer the context links to, with what links to it (`"A1"`, `"S2 step 3"`…), in file
    /// order. For reporting links whose layer no longer exists.
    pub fn links(&self) -> Vec<(NodeId, String)> {
        let mut out = Vec::new();
        for a in &self.assumptions {
            out.extend(a.links.iter().map(|n| (*n, a.id.clone())));
        }
        for d in &self.decisions {
            out.extend(d.links.iter().map(|n| (*n, d.id.clone())));
        }
        for s in &self.scenarios {
            out.push((s.start, format!("{} start", s.id)));
            for (i, step) in s.steps.iter().enumerate() {
                let at = format!("{} step {}", s.id, i + 1);
                if let Some(n) = step.action.node() {
                    out.push((n, at.clone()));
                }
                out.extend(step.expect.nodes().into_iter().map(|n| (n, at.clone())));
            }
            out.extend(s.expect.nodes().into_iter().map(|n| (n, format!("{} result", s.id))));
        }
        out.extend(self.end_states.iter().map(|e| (e.node, "end state".to_string())));
        for f in &self.findings {
            out.extend(f.nodes.iter().map(|n| (*n, f.id.clone())));
        }
        for v in &self.alternatives {
            out.push((v.source, format!("{} source", v.id)));
            out.extend(v.members.iter().map(|m| (m.node, v.id.clone())));
        }
        out
    }
}

/// What the design is for. Every field is free text a person can read and edit; none is required.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Brief {
    /// Who uses it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub audience: String,
    /// What they need to get done.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub job: String,
    /// The circumstances of use: where, on what, under what pressure, after and before what.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub context: String,
    /// The content it has to carry (the message), in the words that matter.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// The voice the content should have for this audience in this context.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tone: String,
    /// Limits the design works within (technical, legal, brand, time, what can't change).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    /// How success would be recognised, as things that can be observed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub success: Vec<String>,
}

impl Brief {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Whether anything has been learned about an assumption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionStatus {
    /// Nobody has checked it. The honest default.
    #[default]
    Untested,
    /// Evidence from outside the file supports it (`evidence` says what).
    Supported,
    /// Evidence contradicts it.
    Refuted,
}

/// Something the design takes to be true without knowing it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Assumption {
    pub id: String,
    pub text: String,
    pub status: AssumptionStatus,
    /// Where the status comes from (a study, analytics, a conversation). An agent's own review
    /// of the design is not evidence about people.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    /// The screens, components or layers that depend on it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<NodeId>,
}

/// A choice that was made, and why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub rationale: String,
    /// What else was considered and why it lost.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rejected: String,
    /// Assumption ids the decision rests on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    /// The screens, components or layers it shaped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<NodeId>,
}

/// A frame where a flow may end on purpose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndState {
    pub node: NodeId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// A walk through the prototype with what should be true along the way and at the end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub name: String,
    /// What passing this scenario shows.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub purpose: String,
    /// The top-level frame it starts on.
    pub start: NodeId,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// What must hold after the last step.
    #[serde(default, skip_serializing_if = "Expect::is_empty")]
    pub expect: Expect,
}

/// One thing the viewer does, and what must hold right after it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub action: StepAction,
    #[serde(default, skip_serializing_if = "Expect::is_empty")]
    pub expect: Expect,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// A pointer gesture on a layer; each fires the layer's interactions with the matching trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gesture {
    #[default]
    Click,
    Hover,
    Press,
    Drag,
    MouseEnter,
    MouseLeave,
    MouseDown,
    MouseUp,
}

/// What the viewer does in one step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StepAction {
    /// A gesture on a layer of what is showing (the top overlay, else the screen).
    Gesture {
        node: NodeId,
        #[serde(default)]
        gesture: Gesture,
    },
    /// A key or combination (`Enter`, `Shift+K`) or controller button (`Gamepad A`).
    Key { key: String },
    /// Time passes: the "after delay" interactions of what is showing fire.
    Wait,
    /// A click outside the top overlay (closes it when it closes on outside clicks).
    ClickOutside,
    /// Stands in for input the prototype can't take (typing into a field): sets a variable as an
    /// interaction would. Scenarios that need this say what the real control would be in `note`.
    SetVariable { variable: String, value: serde_json::Value },
}

impl StepAction {
    /// The layer the step acts on.
    pub fn node(&self) -> Option<NodeId> {
        match self {
            StepAction::Gesture { node, .. } => Some(*node),
            _ => None,
        }
    }
}

/// What must hold in the prototype's state. Empty fields aren't checked.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Expect {
    /// The screen showing (a top-level frame).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<NodeId>,
    /// The open overlays, bottom first (`[]`: none open).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlays: Option<Vec<NodeId>>,
    /// Variable values, by variable name (`Collection/name` or a unique name) or id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Instances and the variant each must show.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantExpect>,
    /// Layers that must be showing (visible, with visible ancestors, on the screen or an overlay).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub visible: Vec<NodeId>,
    /// Layers that must not be showing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<NodeId>,
    /// Text layers and what each must read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text: Vec<TextExpect>,
    /// The screen is (`true`) or isn't (`false`) a declared end state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_state: Option<bool>,
}

impl Expect {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every layer the expectation names.
    pub fn nodes(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self.screen.into_iter().collect();
        out.extend(self.overlays.iter().flatten().copied());
        out.extend(self.variants.iter().map(|v| v.instance));
        out.extend(self.visible.iter().copied());
        out.extend(self.hidden.iter().copied());
        out.extend(self.text.iter().map(|t| t.node));
        out
    }
}

/// An instance and the variant it must show: the variant's name (`State=Error`), any of its
/// property values (`Error`), or the variant component's id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantExpect {
    pub instance: NodeId,
    pub variant: String,
}

/// A text layer and what it must read (`equals` exactly, or `contains`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextExpect {
    pub node: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

/// How a finding was arrived at. The three are never mixed: a measurement can be re-run, an
/// opinion can't, and what a person said is closed by that person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    /// Measured from the file by a named rule; running the rule again settles it.
    #[default]
    Check,
    /// A reviewer's (or an AI agent's) judgement: taste, inference, a heuristic read. Not a
    /// measurement and not research about people.
    Judgement,
    /// Something a person said about the design. Only a person confirms it is met.
    Feedback,
}

/// How much it matters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Blocks the task the design is for, or loses the viewer's work.
    Blocker,
    /// A real problem for some people or some content.
    #[default]
    Major,
    /// Worth fixing; nobody is stopped by it.
    Minor,
    /// An observation.
    Note,
}

/// Where a finding stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingStatus {
    #[default]
    Open,
    /// A fix is claimed (`resolution` says what changed) but nothing has confirmed it yet.
    Addressed,
    /// Confirmed: the rule passes again (checks), or a person confirmed it (`verified_by`).
    Verified,
    /// Left as it is on purpose (`resolution` says why).
    Accepted,
}

/// The last time a check finding's rule was run again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Recheck {
    pub passed: bool,
    /// What was measured this time.
    pub detail: String,
}

/// A problem found in the design, with what it rests on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Finding {
    pub id: String,
    pub kind: FindingKind,
    /// The rule that measured it (`contrast.text`, `flow.deadEnd`…), or a short label for a
    /// judgement or feedback.
    pub rule: String,
    pub severity: Severity,
    /// The layers, screens or flow starts affected.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeId>,
    pub message: String,
    /// What was observed: the measured numbers, the step that failed, the words of the person.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub suggestion: String,
    /// Who or what raised it: `check`, an agent's name, a person's name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    pub status: FindingStatus,
    /// What was changed, or why it stays.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resolution: String,
    /// Who confirmed it (feedback and judgements); empty for a check verified by its rule.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verified_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recheck: Option<Recheck>,
}

/// Alternatives explored for one screen or component, kept next to the original.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternativeSet {
    pub id: String,
    /// The frame or component the alternatives start from. It stays in the file untouched.
    pub source: NodeId,
    /// What the set is trying to find out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub question: String,
    #[serde(default)]
    pub members: Vec<Alternative>,
    /// The member (or the source) picked, once one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<NodeId>,
    /// The decision id recording why.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub decision: String,
}

/// One alternative: a copy of the source reworked under a stated constraint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alternative {
    pub node: NodeId,
    #[serde(default)]
    pub name: String,
    /// The limit this one was made under ("one thumb, no scrolling", "half the words").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub constraint: String,
    /// Who or what situation it serves best.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub intent: String,
    /// What it gives up.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tradeoffs: String,
}
