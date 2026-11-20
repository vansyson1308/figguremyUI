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
//!
//! A stored context is read leniently ([`read_stored`]): what can't be read is left out and what
//! passes the limits is cut, and both are reported ([`take_load_issues`]) instead of failing the
//! file, whose layers don't depend on it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

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
    /// How many times the brief has changed. Decisions remember the revision they were made
    /// under, so one made under an earlier brief can be pointed out for re-reading.
    #[serde(skip_serializing_if = "is_zero")]
    pub brief_revision: u64,
    /// How far along the design is: which checks are worth running yet.
    #[serde(skip_serializing_if = "Stage::is_exploring")]
    pub stage: Stage,
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
    /// The states each screen is declared to have because time passes (first use, nothing yet,
    /// waiting, failure, coming back) and its outcome: what the missing-states checks compare
    /// with what the file has. Nothing declared is reported as that, not as missing states.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<DeclaredState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<AlternativeSet>,
    /// The last number given out per id prefix (`A`, `D`, `S`, `F`, `V`, `R`): ids are never reused, so
    /// a reference to a removed entry can't come to mean another one.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, u64>,
    /// What reading the stored context left out or cut ([`read_stored`]), until whoever opened
    /// the file reports it ([`take_load_issues`]). Never written, and not part of what the
    /// context says.
    #[serde(skip)]
    pub load_issues: LoadIssues,
}

/// What was left out or cut when a stored context was read. Two contexts that say the same are
/// equal whatever was reported about reading them.
#[derive(Clone, Debug, Default)]
pub struct LoadIssues(pub Vec<String>);

impl PartialEq for LoadIssues {
    fn eq(&self, _: &Self) -> bool {
        true
    }
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
        for st in &self.states {
            let at = st.label();
            out.push((st.frame, at.clone()));
            if let Some(by) = &st.shown_by {
                out.push((by.node(), at));
            }
        }
        for r in &self.brief.required {
            out.extend(r.screens.iter().chain(&r.nodes).map(|n| (*n, r.id.clone())));
        }
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

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// How far along the design is. Early on only what would break the file is worth checking;
/// checks of the finish (contrast, clipping, target sizes) wait until the design is converging,
/// and the checks of the reasoning itself until it is about to be handed over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Trying directions: rough on purpose.
    #[default]
    Exploring,
    /// A direction is chosen and being worked out.
    Converging,
    /// About to be handed over or shown as the answer.
    Ready,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::Exploring, Stage::Converging, Stage::Ready];

    pub fn is_exploring(&self) -> bool {
        *self == Stage::Exploring
    }

    /// The name commands and files use.
    pub fn id(self) -> &'static str {
        match self {
            Stage::Exploring => "exploring",
            Stage::Converging => "converging",
            Stage::Ready => "ready",
        }
    }
}

/// What the design is for. Every field is free text a person can read and edit; none is required.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Brief {
    /// Who uses it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub audience: String,
    /// Who is asking for it and what they want from it. Not the audience: the two often want
    /// different things, and the design has to answer both.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sponsor: String,
    /// What they need to get done.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub job: String,
    /// The circumstances of use: where, on what, under what pressure, after and before what.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub context: String,
    /// What the design must say or offer: the promise it makes or the use it gives. Not the
    /// final copy.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// The form it takes and where people meet it (a phone app's first screen, a kiosk, an
    /// email).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub format: String,
    /// The voice the content should have for this audience in this context.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tone: String,
    /// Limits the design works within (technical, legal, brand, time, what can't change).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    /// How success would be recognised, as things that can be observed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub success: Vec<String>,
    /// What the design must show (consent text, a price, a permission prompt, an error), each
    /// linked to the layers that are it, so a check can find it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<RequiredElement>,
}

/// Whether a person stands behind a declaration (a state of a screen, a required element).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProposalStatus {
    /// Declared by an AI agent and not yet accepted by a person.
    Proposed,
    /// A person declared or accepted it.
    #[default]
    Accepted,
}

impl ProposalStatus {
    pub fn is_accepted(&self) -> bool {
        *self == ProposalStatus::Accepted
    }
}

/// Something the design must show, as the brief requires it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RequiredElement {
    /// `R1`, `R2`…
    pub id: String,
    /// What must be shown, in words ("consent to be contacted, before the form is sent").
    pub text: String,
    /// The top-level frames it must be on; empty: anywhere.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub screens: Vec<NodeId>,
    /// The layers that are it. Empty: not linked yet, so nothing can be checked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeId>,
    /// Who listed it: an agent's name, or `user`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Who last changed its words or screens, when that wasn't its author. Once a person has
    /// edited an AI agent's entry, the words are no longer the agent's to change.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub edited_by: String,
    #[serde(skip_serializing_if = "ProposalStatus::is_accepted")]
    pub status: ProposalStatus,
    /// The person who accepted an AI agent's entry.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub accepted_by: String,
}

/// Why a screen is in a state: the places where time passes, and how it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateKind {
    /// The first time somebody meets it, with nothing of theirs in it yet.
    FirstUse,
    /// Nothing to show (yet, or any more).
    Empty,
    /// Waiting for something to arrive.
    Loading,
    /// Something went wrong.
    #[default]
    Error,
    /// Coming back after a while.
    Returning,
    /// It worked.
    Success,
    /// Another state, named by `name`.
    Other,
}

impl StateKind {
    pub const ALL: [StateKind; 7] =
        [StateKind::FirstUse, StateKind::Empty, StateKind::Loading, StateKind::Error, StateKind::Returning, StateKind::Success, StateKind::Other];

    /// The name commands and files use.
    pub fn id(self) -> &'static str {
        match self {
            StateKind::FirstUse => "firstUse",
            StateKind::Empty => "empty",
            StateKind::Loading => "loading",
            StateKind::Error => "error",
            StateKind::Returning => "returning",
            StateKind::Success => "success",
            StateKind::Other => "other",
        }
    }
}

/// What shows a declared state: a frame or overlay, a layer of the screen, or an instance in the
/// screen showing one of its variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShownBy {
    /// A layer: a top-level frame (another screen, an overlay) or a layer in the screen.
    Node { node: NodeId },
    /// An instance in the screen and the variant it shows the state with: the variant's name
    /// (`State=Error`), one of its property values (`Error`), or the variant component's id.
    Variant { instance: NodeId, variant: String },
}

impl ShownBy {
    /// The layer it names (the instance, for a variant).
    pub fn node(&self) -> NodeId {
        match self {
            ShownBy::Node { node } => *node,
            ShownBy::Variant { instance, .. } => *instance,
        }
    }
}

/// A state a screen is declared to have.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredState {
    /// The screen: a top-level frame.
    pub frame: NodeId,
    pub kind: StateKind,
    /// Required for `other`; a label otherwise. One declaration per frame, kind and name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// What shows it; `None`: not designed yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown_by: Option<ShownBy>,
    /// Why the screen has this state.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Who declared it: an agent's name, or `user`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Who last changed what shows it or why, when that wasn't its author. Once a person has
    /// edited an AI agent's declaration, it is no longer the agent's to change.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edited_by: String,
    #[serde(default, skip_serializing_if = "ProposalStatus::is_accepted")]
    pub status: ProposalStatus,
    /// The person who accepted an AI agent's declaration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accepted_by: String,
}

impl DeclaredState {
    /// Whether this is the declaration of `kind` named `name` on `frame` (names compare without
    /// case or surrounding space).
    pub fn is(&self, frame: NodeId, kind: StateKind, name: &str) -> bool {
        self.frame == frame && self.kind == kind && self.name.trim().to_lowercase() == name.trim().to_lowercase()
    }

    /// How messages, links and finding subjects name it: `state error of 12`,
    /// `state other "Offline" of 12`.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("state {} of {}", self.kind.id(), self.frame)
        } else {
            format!("state {} {:?} of {}", self.kind.id(), self.name, self.frame)
        }
    }
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

/// Whether a person stands behind a decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionStatus {
    /// Put forward by an AI agent, or changed by one since a person accepted it.
    #[default]
    Proposed,
    /// A person made it or accepted it.
    Accepted,
}

/// A choice that was made, and why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub rationale: String,
    /// Who recorded it: an agent's name, or `user`. Set when it is added and never edited.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Who changed it last, when that wasn't its author (an agent's name, or `user`). The text
    /// is then no longer all the author's.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub edited_by: String,
    pub status: DecisionStatus,
    /// The person who accepted it, when someone else proposed it (`design.decision.accept`, or a
    /// choice among alternatives accepted). Empty when its author is the person who stands behind
    /// it, while it is proposed, and in a file from before this was recorded.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub accepted_by: String,
    /// The brief's revision when the decision was added or last accepted.
    #[serde(skip_serializing_if = "is_zero")]
    pub brief_revision: u64,
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

/// Whether a person stands behind an end state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndStateStatus {
    /// Declared by an AI agent: the frame is still a dead end until a person accepts it.
    Proposed,
    /// A person declared or accepted it. Also what a file from before end states had a status
    /// holds.
    #[default]
    Accepted,
}

impl EndStateStatus {
    pub fn is_accepted(&self) -> bool {
        *self == EndStateStatus::Accepted
    }
}

/// A frame where a flow may end on purpose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndState {
    pub node: NodeId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Who declared it: an agent's name, or `user`. Empty in a file from before end states had
    /// authors: the user's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Only an accepted end state exempts its frame from being a dead end.
    #[serde(default, skip_serializing_if = "EndStateStatus::is_accepted")]
    pub status: EndStateStatus,
    /// The person who accepted it, when an AI agent declared it. Empty when its author is the
    /// person who stands behind it, while it is proposed, and in a file from before this was
    /// recorded.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accepted_by: String,
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
    /// Who wrote it: an agent's name, or `user`. Empty in a file from before scenarios had
    /// authors: the user's. An agent changes and removes only its own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
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
    /// What a check is about when that isn't the layers in `nodes`, stable from one run of the
    /// rule to the next (`scenario:S1`, `overlay:12`, `page:1`): the same rule reporting the
    /// same subject is the same problem, whatever layers it names this time. Empty: the layers
    /// in `nodes` are the subject.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subject: String,
    pub message: String,
    /// What was observed: the measured numbers, the step that failed, the words of the person.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub suggestion: String,
    /// Who or what raised it: `check`, an agent's name, a person's name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Who ran the command that stored it, when that isn't `source`: the agent that passed on
    /// what a person said, whoever ran the checks.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recorded_by: String,
    pub status: FindingStatus,
    /// What was changed, or why it stays.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resolution: String,
    /// Who confirmed it (feedback and judgements); empty for a check verified by its rule.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verified_by: String,
    /// What that person looked at to confirm it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verification: String,
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
    /// The limits every member has to respect, stated before any is made (so they can't be
    /// fitted to the result afterwards).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub members: Vec<Alternative>,
    /// The member (or the source) picked, once one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<NodeId>,
    /// The decision id recording why.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub decision: String,
    /// A choice an AI agent put forward, waiting for a person: `chosen` changes only when a
    /// person chooses, or accepts the proposal's decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<ChoiceProposal>,
}

/// A member (or the source) an AI agent proposes to choose, with the proposed decision saying why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceProposal {
    pub node: NodeId,
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

/// The most problems one read of a stored context spells out; the rest are counted.
const MAX_LOAD_ISSUES: usize = 20;
/// The longest id-counter prefix (`A`, `D`, `S`, `F`, `V`, `R` are one letter) and the most counters.
const MAX_COUNTER_KEY: usize = 16;
const MAX_COUNTERS: usize = 64;

/// Read a document's stored `design` value without failing the document over it (the field's
/// `deserialize_with`). A value that reads whole is kept; of one that doesn't, the parts and
/// entries that read are kept and the rest left out; anything past [`MAX_ENTRIES`] or
/// [`MAX_TEXT`] is cut. What was left out or cut is in [`DesignContext::load_issues`] for the
/// loader to report: it is never written back, so saving the file drops what couldn't be read.
pub fn read_stored<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<DesignContext>, D::Error> {
    let raw = Value::deserialize(d)?;
    Ok(Arc::new(DesignContext::from_stored(raw)))
}

/// What reading the stored context left out or cut, taken out of it so it is reported once.
pub fn take_load_issues(design: &mut Arc<DesignContext>) -> Vec<String> {
    if design.load_issues.0.is_empty() {
        return Vec::new();
    }
    std::mem::take(&mut Arc::make_mut(design).load_issues.0)
}

/// Problems met while reading, the first [`MAX_LOAD_ISSUES`] spelt out.
#[derive(Default)]
struct Said {
    said: Vec<String>,
    more: usize,
}

impl Said {
    fn say(&mut self, what: String) {
        if self.said.len() < MAX_LOAD_ISSUES {
            self.said.push(what);
        } else {
            self.more = self.more.saturating_add(1);
        }
    }

    fn is_empty(&self) -> bool {
        self.said.is_empty()
    }

    fn joined(&self) -> String {
        let more = if self.more > 0 { format!("; and {} more", self.more) } else { String::new() };
        format!("{}{more}", self.said.join("; "))
    }
}

/// One part of a stored context into `slot`, or left as it is and reported.
fn part<T: DeserializeOwned>(slot: &mut T, value: Value, name: &str, lost: &mut Said) {
    match serde_json::from_value(value) {
        Ok(v) => *slot = v,
        Err(why) => lost.say(format!("`{name}`: {why}")),
    }
}

/// The entries of a stored list that read, into `out`; the others are reported.
fn entries<T: DeserializeOwned>(out: &mut Vec<T>, value: Value, name: &str, lost: &mut Said) {
    let Value::Array(items) = value else {
        lost.say(format!("`{name}` isn't a list"));
        return;
    };
    for (i, item) in items.into_iter().enumerate() {
        match serde_json::from_value(item) {
            Ok(v) => out.push(v),
            Err(why) => lost.say(format!("`{name}[{i}]`: {why}")),
        }
    }
}

/// The brief of a stored context: whole when it reads, else its required elements entry by entry
/// and the rest as one part.
fn brief_part(slot: &mut Brief, value: Value, lost: &mut Said) {
    let Value::Object(mut map) = value else { return part(slot, value, "brief", lost) };
    let required = map.remove("required");
    part(slot, Value::Object(map), "brief", lost);
    if let Some(required) = required {
        entries(&mut slot.required, required, "brief.required", lost);
    }
}

/// What is cut from a context that passes the limits.
#[derive(Default)]
struct Cut {
    texts: usize,
    values: usize,
    lists: Said,
}

impl Cut {
    fn text(&mut self, s: &mut String) {
        // A text of at most MAX_TEXT bytes has at most MAX_TEXT characters.
        if s.len() > MAX_TEXT
            && let Some((at, _)) = s.char_indices().nth(MAX_TEXT)
        {
            s.truncate(at);
            self.texts = self.texts.saturating_add(1);
        }
    }

    fn texts(&mut self, list: &mut Vec<String>, name: &str) {
        self.list(list, name);
        for t in list {
            self.text(t);
        }
    }

    fn list<T>(&mut self, list: &mut Vec<T>, name: &str) {
        if list.len() > MAX_ENTRIES {
            self.lists.say(format!("`{name}` from {} to {MAX_ENTRIES} entries", list.len()));
            list.truncate(MAX_ENTRIES);
        }
    }

    /// A variable's value: one whose JSON is longer than a text may be is dropped.
    fn value(&mut self, v: &mut Value) {
        if v.to_string().chars().count() > MAX_TEXT {
            *v = Value::Null;
            self.values = self.values.saturating_add(1);
        }
    }

    fn expect(&mut self, e: &mut Expect) {
        if let Some(overlays) = &mut e.overlays {
            self.list(overlays, "expect.overlays");
        }
        self.list(&mut e.visible, "expect.visible");
        self.list(&mut e.hidden, "expect.hidden");
        self.list(&mut e.variants, "expect.variants");
        self.list(&mut e.text, "expect.text");
        for v in &mut e.variants {
            self.text(&mut v.variant);
        }
        for t in &mut e.text {
            for wanted in [&mut t.equals, &mut t.contains].into_iter().flatten() {
                self.text(wanted);
            }
        }
        let before = e.variables.len();
        let mut kept = 0usize;
        e.variables.retain(|name, _| {
            kept += 1;
            name.chars().count() <= MAX_TEXT && kept <= MAX_ENTRIES
        });
        if e.variables.len() < before {
            self.lists.say(format!("`expect.variables` from {before} to {} entries", e.variables.len()));
        }
        for v in e.variables.values_mut() {
            self.value(v);
        }
    }

    fn said(self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.lists.is_empty() {
            out.push(self.lists.joined());
        }
        if self.texts > 0 {
            out.push(format!("{} text(s) to {MAX_TEXT} characters", self.texts));
        }
        if self.values > 0 {
            out.push(format!("{} variable value(s) longer than {MAX_TEXT} characters dropped", self.values));
        }
        out
    }
}

impl DesignContext {
    /// The context a stored `design` value holds, read leniently (see [`read_stored`]).
    pub fn from_stored(raw: Value) -> DesignContext {
        let mut issues = Vec::new();
        let mut ctx = match DesignContext::deserialize(&raw) {
            Ok(ctx) => ctx,
            Err(why) => DesignContext::salvaged(raw, &why.to_string(), &mut issues),
        };
        let cut = ctx.clamp();
        if !cut.is_empty() {
            issues.push(format!("the design context was larger than a file may hold and was cut: {}", cut.join("; ")));
        }
        if ctx.raise_counters() {
            issues.push("the design context's id counters were behind the ids in use and were raised, so no id is given out twice".into());
        }
        issues.extend(ctx.repair());
        ctx.load_issues = LoadIssues(issues);
        ctx
    }

    /// What of a stored value that doesn't read whole (`why`) can be read: each part by itself,
    /// lists entry by entry.
    fn salvaged(raw: Value, why: &str, issues: &mut Vec<String>) -> DesignContext {
        let mut ctx = DesignContext::default();
        let Value::Object(map) = raw else {
            issues.push(format!("the design context couldn't be read and was left out: {why}"));
            return ctx;
        };
        let mut lost = Said::default();
        for (key, value) in map {
            match key.as_str() {
                "brief" => brief_part(&mut ctx.brief, value, &mut lost),
                "briefRevision" => part(&mut ctx.brief_revision, value, "briefRevision", &mut lost),
                "stage" => part(&mut ctx.stage, value, "stage", &mut lost),
                "assumptions" => entries(&mut ctx.assumptions, value, "assumptions", &mut lost),
                "decisions" => entries(&mut ctx.decisions, value, "decisions", &mut lost),
                "scenarios" => entries(&mut ctx.scenarios, value, "scenarios", &mut lost),
                "endStates" => entries(&mut ctx.end_states, value, "endStates", &mut lost),
                "states" => entries(&mut ctx.states, value, "states", &mut lost),
                "findings" => entries(&mut ctx.findings, value, "findings", &mut lost),
                "alternatives" => entries(&mut ctx.alternatives, value, "alternatives", &mut lost),
                "counters" => part(&mut ctx.counters, value, "counters", &mut lost),
                _ => {}
            }
        }
        if lost.is_empty() {
            // Nothing in particular failed, yet the whole didn't read: say what the reader said.
            lost.say(why.to_string());
        }
        issues.push(format!("part of the design context couldn't be read and was left out ({}); the rest was kept", lost.joined()));
        ctx
    }

    /// Cut the context to what the commands would have let it grow to: every list to
    /// [`MAX_ENTRIES`] entries and every text to [`MAX_TEXT`] characters. Returns what was cut
    /// (empty: nothing). For a context that comes from outside: a file, a collaborator.
    pub fn clamp(&mut self) -> Vec<String> {
        let mut c = Cut::default();
        let b = &mut self.brief;
        for t in [&mut b.audience, &mut b.sponsor, &mut b.job, &mut b.context, &mut b.content, &mut b.format, &mut b.tone] {
            c.text(t);
        }
        c.texts(&mut b.constraints, "brief.constraints");
        c.texts(&mut b.success, "brief.success");
        c.list(&mut b.required, "brief.required");
        for r in &mut b.required {
            for t in [&mut r.id, &mut r.text, &mut r.author, &mut r.edited_by, &mut r.accepted_by] {
                c.text(t);
            }
            c.list(&mut r.screens, "required screens");
            c.list(&mut r.nodes, "required nodes");
        }
        c.list(&mut self.assumptions, "assumptions");
        for a in &mut self.assumptions {
            for t in [&mut a.id, &mut a.text, &mut a.evidence] {
                c.text(t);
            }
            c.list(&mut a.links, "assumption links");
        }
        c.list(&mut self.decisions, "decisions");
        for d in &mut self.decisions {
            for t in [&mut d.id, &mut d.title, &mut d.rationale, &mut d.author, &mut d.edited_by, &mut d.accepted_by, &mut d.rejected] {
                c.text(t);
            }
            c.texts(&mut d.assumptions, "decision assumptions");
            c.list(&mut d.links, "decision links");
        }
        c.list(&mut self.scenarios, "scenarios");
        for s in &mut self.scenarios {
            for t in [&mut s.id, &mut s.name, &mut s.purpose, &mut s.author] {
                c.text(t);
            }
            c.list(&mut s.steps, "scenario steps");
            for step in &mut s.steps {
                match &mut step.action {
                    StepAction::Key { key } => c.text(key),
                    StepAction::SetVariable { variable, value } => {
                        c.text(variable);
                        c.value(value);
                    }
                    StepAction::Gesture { .. } | StepAction::Wait | StepAction::ClickOutside => {}
                }
                c.text(&mut step.note);
                c.expect(&mut step.expect);
            }
            c.expect(&mut s.expect);
        }
        c.list(&mut self.end_states, "endStates");
        for e in &mut self.end_states {
            c.text(&mut e.reason);
            c.text(&mut e.author);
            c.text(&mut e.accepted_by);
        }
        c.list(&mut self.states, "states");
        for st in &mut self.states {
            for t in [&mut st.name, &mut st.reason, &mut st.author, &mut st.edited_by, &mut st.accepted_by] {
                c.text(t);
            }
            if let Some(ShownBy::Variant { variant, .. }) = &mut st.shown_by {
                c.text(variant);
            }
        }
        c.list(&mut self.findings, "findings");
        for f in &mut self.findings {
            for t in [
                &mut f.id,
                &mut f.rule,
                &mut f.subject,
                &mut f.message,
                &mut f.evidence,
                &mut f.suggestion,
                &mut f.source,
                &mut f.recorded_by,
                &mut f.resolution,
                &mut f.verified_by,
                &mut f.verification,
            ] {
                c.text(t);
            }
            if let Some(r) = &mut f.recheck {
                c.text(&mut r.detail);
            }
            c.list(&mut f.nodes, "finding nodes");
        }
        c.list(&mut self.alternatives, "alternatives");
        for v in &mut self.alternatives {
            for t in [&mut v.id, &mut v.question, &mut v.decision] {
                c.text(t);
            }
            if let Some(p) = &mut v.proposal {
                c.text(&mut p.decision);
            }
            c.texts(&mut v.constraints, "alternative constraints");
            c.list(&mut v.members, "alternative members");
            for m in &mut v.members {
                for t in [&mut m.name, &mut m.constraint, &mut m.intent, &mut m.tradeoffs] {
                    c.text(t);
                }
            }
        }
        let before = self.counters.len();
        let mut kept = 0usize;
        self.counters.retain(|prefix, _| {
            kept += 1;
            prefix.len() <= MAX_COUNTER_KEY && kept <= MAX_COUNTERS
        });
        if self.counters.len() < before {
            c.lists.say(format!("`counters` from {before} to {} entries", self.counters.len()));
        }
        c.said()
    }

    /// Make the declared states and required elements of a stored context what the commands
    /// would have made, so each can be edited and removed. Ids compare without case or
    /// surrounding space: the first element with an id keeps it (as the commands write it, in
    /// capitals), and one without an id, or with one an earlier element has, gets a fresh id no
    /// element has. Declarations that are named (any kind but `other`, or an `other` with a name)
    /// come first: a second one of the same frame, kind and name is dropped (the first is kept).
    /// Then an `other` without a name is named from its reason (else "unnamed"), with a number
    /// added when a declaration of its frame and kind has that name already, so it never pushes
    /// out one a person named. A list of screens or layers names each once. Returns what was
    /// changed, for the loader to report.
    fn repair(&mut self) -> Vec<String> {
        let mut said = Vec::new();
        let given: Vec<String> = self.brief.required.iter().map(|r| r.id.trim().to_ascii_uppercase()).collect();
        // The ids that stay: the first element with each.
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let keeps: Vec<bool> = given.iter().map(|id| !id.is_empty() && taken.insert(id.clone())).collect();
        let mut twice = 0usize;
        for (i, id) in given.into_iter().enumerate() {
            let fresh = if keeps.get(i).copied().unwrap_or(false) {
                None
            } else {
                // `raise_counters` has put the counter past every id it can read as a number; the
                // loop guards against the others. Each id taken blocks one candidate at most.
                let mut new = self.next_id("R");
                for _ in 0..taken.len() {
                    if !taken.contains(&new) {
                        break;
                    }
                    new = self.next_id("R");
                }
                taken.insert(new.clone());
                Some(new)
            };
            let Some(r) = self.brief.required.get_mut(i) else { continue };
            match fresh {
                Some(new) => {
                    said.push(if id.is_empty() {
                        format!("a required element had no id and was given {new}")
                    } else {
                        format!("a second required element {id} was given {new}")
                    });
                    r.id = new;
                }
                None => r.id = id,
            }
            let (screens, nodes) = (r.screens.len(), r.nodes.len());
            dedup(&mut r.screens);
            dedup(&mut r.nodes);
            if r.screens.len() < screens || r.nodes.len() < nodes {
                twice = twice.saturating_add(1);
            }
        }
        if twice > 0 {
            said.push(format!("{twice} required element(s) named a layer twice; each is named once"));
        }
        let nameless = |st: &DeclaredState| st.kind == StateKind::Other && st.name.trim().is_empty();
        // The named declarations first: the first of each frame, kind and name is kept.
        let mut kept: Vec<DeclaredState> = Vec::with_capacity(self.states.len());
        let mut dropped = 0usize;
        for st in std::mem::take(&mut self.states) {
            if !nameless(&st) && kept.iter().any(|k| !nameless(k) && k.is(st.frame, st.kind, &st.name)) {
                dropped = dropped.saturating_add(1);
            } else {
                kept.push(st);
            }
        }
        if dropped > 0 {
            said.push(format!(
                "{dropped} state(s) declared a second time for the same frame, kind and name were left out; the first of each is kept"
            ));
        }
        // Then those without a name, each given one no other declaration has.
        let mut named = 0usize;
        for i in 0..kept.len() {
            let Some(st) = kept.get(i).filter(|st| nameless(st)) else { continue };
            let (frame, kind) = (st.frame, st.kind);
            let from: String = st.reason.trim().chars().take(40).collect::<String>().trim().to_string();
            let base = if from.is_empty() { "unnamed".to_string() } else { from };
            let in_use = |name: &str| kept.iter().enumerate().any(|(j, k)| j != i && !nameless(k) && k.is(frame, kind, name));
            let mut name = base.clone();
            // Each other declaration blocks one candidate at most.
            for n in 2..kept.len().saturating_add(2) {
                if !in_use(&name) {
                    break;
                }
                name = format!("{base} {n}");
            }
            if let Some(st) = kept.get_mut(i) {
                st.name = name;
                named = named.saturating_add(1);
            }
        }
        if named > 0 {
            said.push(format!(
                "{named} state(s) of kind other had no name and were named from their reason (else \"unnamed\"), with a number added where a declaration of the same frame had that name"
            ));
        }
        self.states = kept;
        said
    }

    /// Raise the id counters to the ids in use, so [`next_id`](Self::next_id) never gives out one
    /// an entry already has (a file can say anything). Ids are read without case or surrounding
    /// space (`r7` is `R7`), as the commands look them up. True when a counter moved.
    fn raise_counters(&mut self) -> bool {
        let mut highest: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut see = |prefix: &'static str, id: &str| {
            let id = id.trim();
            let number = id.get(..prefix.len()).filter(|p| p.eq_ignore_ascii_case(prefix)).and_then(|_| id.get(prefix.len()..));
            if let Some(n) = number.and_then(|n| n.parse::<u64>().ok()) {
                let top = highest.entry(prefix).or_insert(0);
                *top = (*top).max(n);
            }
        };
        self.assumptions.iter().for_each(|a| see("A", &a.id));
        self.decisions.iter().for_each(|d| see("D", &d.id));
        self.scenarios.iter().for_each(|s| see("S", &s.id));
        self.findings.iter().for_each(|f| see("F", &f.id));
        self.alternatives.iter().for_each(|v| see("V", &v.id));
        self.brief.required.iter().for_each(|r| see("R", &r.id));
        let mut moved = false;
        for (prefix, top) in highest {
            let n = self.counters.entry(prefix.to_string()).or_insert(0);
            if *n < top {
                *n = top;
                moved = true;
            }
        }
        moved
    }
}

/// `list` with each entry once, in the order first given.
fn dedup<T: PartialEq + Copy>(list: &mut Vec<T>) {
    let mut seen: Vec<T> = Vec::with_capacity(list.len());
    list.retain(|x| {
        let fresh = !seen.contains(x);
        if fresh {
            seen.push(*x);
        }
        fresh
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        Brief, Decision, DecisionStatus, DesignContext, EndState, EndStateStatus, Finding, MAX_ENTRIES, MAX_TEXT, Scenario, Stage, take_load_issues,
    };
    use crate::{Document, NodeId};

    #[test]
    fn what_is_at_its_default_is_left_out_of_the_file() {
        let empty = DesignContext::default();
        assert!(empty.is_empty());
        assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
        // A stage or a revision alone is something recorded.
        assert!(!DesignContext { stage: Stage::Ready, ..DesignContext::default() }.is_empty());
        let touched = DesignContext { brief_revision: 2, stage: Stage::Converging, ..DesignContext::default() };
        assert_eq!(serde_json::to_value(&touched).unwrap(), json!({"briefRevision": 2, "stage": "converging"}));
        let decision = Decision { id: "D1".into(), title: "t".into(), rationale: "r".into(), ..Decision::default() };
        assert_eq!(serde_json::to_value(&decision).unwrap(), json!({"id": "D1", "title": "t", "rationale": "r", "status": "proposed"}));
        let brief = Brief { sponsor: "The ward manager".into(), format: "A phone app".into(), ..Brief::default() };
        assert_eq!(serde_json::to_value(&brief).unwrap(), json!({"sponsor": "The ward manager", "format": "A phone app"}));
    }

    #[test]
    fn a_context_without_the_newer_fields_loads_with_their_defaults() {
        let stored = json!({
            "brief": {"audience": "Nurses"},
            "decisions": [{"id": "D1", "title": "t", "rationale": "r"}],
            "findings": [{"id": "F1", "kind": "feedback", "rule": "wording", "severity": "minor", "message": "m", "status": "open"}],
            "alternatives": [{"id": "V1", "source": 5, "members": [{"node": 9, "name": "B"}]}],
            "counters": {"D": 1, "F": 1, "V": 1},
        });
        let ctx: DesignContext = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!((ctx.stage, ctx.brief_revision), (Stage::Exploring, 0));
        assert_eq!((ctx.decisions[0].status, ctx.decisions[0].author.as_str(), ctx.decisions[0].brief_revision), (DecisionStatus::Proposed, "", 0));
        assert!(ctx.findings[0].verification.is_empty() && ctx.alternatives[0].constraints.is_empty());
        assert!(ctx.brief.sponsor.is_empty() && ctx.brief.format.is_empty());
        assert_eq!(ctx.clone().next_id("D"), "D2", "ids go on from the stored counters");
        // Writing it back adds only the decision's status.
        let mut expected = stored;
        expected["decisions"][0]["status"] = json!("proposed");
        assert_eq!(serde_json::to_value(&ctx).unwrap(), expected);
    }

    #[test]
    fn end_states_scenarios_and_findings_from_before_they_had_authors_load_as_the_users() {
        // The shape written before end states had an author and a status, scenarios an author and
        // findings a subject: read as accepted and as the user's, and written back unchanged.
        let stored = json!({
            "scenarios": [{"id": "S1", "name": "Order", "start": 5, "steps": [{"action": {"type": "gesture", "node": 7, "gesture": "click"}}]}],
            "endStates": [{"node": 9, "reason": "The order is placed"}, {"node": 11}],
            "findings": [{"id": "F1", "kind": "check", "rule": "flow.noStart", "severity": "minor", "nodes": [5], "message": "m", "source": "check", "status": "open"}],
            "counters": {"F": 1, "S": 1},
        });
        let ctx: DesignContext = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(
            ctx.end_states[0],
            EndState {
                node: NodeId(9),
                reason: "The order is placed".into(),
                author: String::new(),
                status: EndStateStatus::Accepted,
                accepted_by: String::new()
            }
        );
        assert_eq!((ctx.end_states[1].status, ctx.end_states[1].author.as_str()), (EndStateStatus::Accepted, ""));
        assert_eq!(ctx.scenarios[0].author, "");
        assert_eq!((ctx.findings[0].subject.as_str(), ctx.findings[0].recorded_by.as_str()), ("", ""));
        assert_eq!(serde_json::to_value(&ctx).unwrap(), stored, "an old context is written back as it was");
        // The new fields are written only when they say something.
        let proposed = EndState {
            node: NodeId(9),
            reason: "r".into(),
            author: "Claude (agent)".into(),
            status: EndStateStatus::Proposed,
            accepted_by: String::new(),
        };
        assert_eq!(serde_json::to_value(&proposed).unwrap(), json!({"node": 9, "reason": "r", "author": "Claude (agent)", "status": "proposed"}));
        let back: EndState = serde_json::from_value(serde_json::to_value(&proposed).unwrap()).unwrap();
        assert_eq!(back, proposed);
        // Who accepted an agent's end state or decision is kept, and read back (QC-15).
        let accepted = EndState { status: EndStateStatus::Accepted, accepted_by: "user".into(), ..proposed.clone() };
        assert_eq!(serde_json::to_value(&accepted).unwrap(), json!({"node": 9, "reason": "r", "author": "Claude (agent)", "acceptedBy": "user"}));
        assert_eq!(serde_json::from_value::<EndState>(serde_json::to_value(&accepted).unwrap()).unwrap(), accepted);
        let decision = Decision {
            id: "D1".into(),
            author: "Claude (agent)".into(),
            status: DecisionStatus::Accepted,
            accepted_by: "user".into(),
            ..Decision::default()
        };
        let v = serde_json::to_value(&decision).unwrap();
        assert_eq!((&v["author"], &v["acceptedBy"]), (&json!("Claude (agent)"), &json!("user")));
        assert_eq!(serde_json::from_value::<Decision>(v).unwrap(), decision);
        let old: Decision = serde_json::from_value(json!({"id": "D2", "title": "t", "rationale": "r", "status": "accepted"})).unwrap();
        assert_eq!(old.accepted_by, "", "a file from before reads with nobody named");
        let finding = Finding {
            id: "F2".into(),
            rule: "scenario.fails".into(),
            subject: "scenario:S1".into(),
            recorded_by: "user".into(),
            ..Finding::default()
        };
        let v = serde_json::to_value(&finding).unwrap();
        assert_eq!((&v["subject"], &v["recordedBy"]), (&json!("scenario:S1"), &json!("user")));
        let scenario: Scenario = serde_json::from_value(json!({"id": "S2", "name": "n", "start": 1, "author": "user"})).unwrap();
        assert_eq!(serde_json::to_value(&scenario).unwrap()["author"], "user");
    }

    /// A document's JSON with `design` set to `design` (one page, one frame on it).
    fn document_with(design: serde_json::Value) -> serde_json::Value {
        let mut doc = Document::new("Lenient");
        let page = doc.first_page().unwrap();
        doc.create(page, crate::NodeKind::Frame, "Home", 0.0, 0.0, 100.0, 100.0).unwrap();
        let mut v = serde_json::to_value(&doc).unwrap();
        assert!(v.get("design").is_none(), "a file without a design context has no `design` key");
        v["design"] = design;
        v
    }

    #[test]
    fn a_document_without_a_design_context_loads_and_writes_none() {
        let mut v = document_with(json!(null));
        v.as_object_mut().unwrap().remove("design");
        let mut doc: Document = serde_json::from_value(v.clone()).unwrap();
        assert!(doc.design.is_empty() && take_load_issues(&mut doc.design).is_empty());
        assert_eq!(serde_json::to_value(&doc).unwrap(), v);
    }

    #[test]
    fn a_design_context_that_cant_be_read_doesnt_stop_the_document_from_loading() {
        // (the stored value, what the report names, what is kept all the same)
        let cases = [
            (
                json!({"brief": {"audience": "Nurses"}, "scenarios": [{"id": "S1", "name": "x"}, {"id": "S2", "name": "ok", "start": 4}], "counters": {"S": 2}}),
                "`scenarios[0]`",
                1,
            ),
            (json!({"brief": {"audience": "Nurses"}, "stage": "shipping"}), "`stage`", 0),
            (
                json!({"brief": {"audience": "Nurses"}, "scenarios": [{"id": "S1", "name": "n", "start": 4, "steps": [{"action": {"type": "swipe"}}]}]}),
                "`scenarios[0]`",
                0,
            ),
        ];
        for (design, names, scenarios) in cases {
            let mut doc: Document = serde_json::from_value(document_with(design.clone())).unwrap_or_else(|e| panic!("{design}: {e}"));
            assert_eq!(doc.nodes().filter(|n| n.name == "Home").count(), 1, "the layers are intact");
            let issues = take_load_issues(&mut doc.design);
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert!(
                issues[0].starts_with("part of the design context couldn't be read and was left out (") && issues[0].contains(names),
                "{issues:?}"
            );
            assert!(take_load_issues(&mut doc.design).is_empty(), "reported once");
            // What reads is kept; what doesn't is gone, also from what is written back.
            assert_eq!((doc.design.brief.audience.as_str(), doc.design.scenarios.len(), doc.design.stage), ("Nurses", scenarios, Stage::Exploring));
            let written = serde_json::to_value(&doc).unwrap();
            assert!(!written["design"].to_string().contains("swipe") && !written["design"].to_string().contains("shipping"), "{written}");
        }
        // Not an object at all: the whole context is left out, and so is the key when saved.
        for design in [json!(7), json!("brief"), json!([1, 2]), json!(null)] {
            let mut doc: Document = serde_json::from_value(document_with(design.clone())).unwrap();
            let issues = take_load_issues(&mut doc.design);
            assert!(issues.len() == 1 && issues[0].starts_with("the design context couldn't be read and was left out: "), "{design}: {issues:?}");
            assert!(doc.design.is_empty() && serde_json::to_value(&doc).unwrap().get("design").is_none());
        }
    }

    #[test]
    fn a_stored_context_past_the_limits_is_cut_and_says_so() {
        let long = "é".repeat(MAX_TEXT + 10);
        let findings: Vec<serde_json::Value> = (1..=MAX_ENTRIES + 3)
            .map(|k| json!({"id": format!("F{k}"), "kind": "judgement", "rule": "r", "message": long, "status": "open"}))
            .collect();
        let links: Vec<u64> = (0..100_000).collect();
        let design = json!({
            "brief": {"audience": long},
            "assumptions": [{"id": "A1", "text": "t", "links": links}],
            "findings": findings,
            "counters": {"A": 1},
        });
        let mut doc: Document = serde_json::from_value(document_with(design)).unwrap();
        let issues = take_load_issues(&mut doc.design);
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues[0].starts_with("the design context was larger than a file may hold and was cut: "), "{issues:?}");
        assert!(issues[0].contains(&format!("`findings` from {} to {MAX_ENTRIES} entries", MAX_ENTRIES + 3)), "{issues:?}");
        assert!(issues[0].contains(&format!("`assumption links` from 100000 to {MAX_ENTRIES} entries")), "{issues:?}");
        assert!(issues[0].contains(&format!("{} text(s) to {MAX_TEXT} characters", MAX_ENTRIES + 1)), "{issues:?}");
        assert!(issues[1].contains("id counters were behind"), "the file's counters said nothing of F: {issues:?}");
        let ctx = &doc.design;
        assert_eq!((ctx.findings.len(), ctx.assumptions[0].links.len()), (MAX_ENTRIES, MAX_ENTRIES));
        assert_eq!(ctx.brief.audience.chars().count(), MAX_TEXT, "cut between characters, not inside one");
        assert!(ctx.findings.iter().all(|f| f.message.chars().count() == MAX_TEXT));
        assert_eq!(DesignContext::clone(ctx).next_id("F"), format!("F{}", MAX_ENTRIES + 1), "ids go on after the ones kept");
        // A context within the limits is left alone.
        let mut small = DesignContext { brief: Brief { audience: "é".repeat(MAX_TEXT), ..Brief::default() }, ..DesignContext::default() };
        assert!(small.clamp().is_empty());
    }

    #[test]
    fn what_was_reported_about_reading_isnt_part_of_what_a_context_says() {
        let mut odd = DesignContext::from_stored(json!({"stage": "shipping"}));
        assert!(!odd.load_issues.0.is_empty());
        assert!(odd.is_empty() && odd == DesignContext::default());
        assert_eq!(serde_json::to_value(&odd).unwrap(), json!({}));
        odd.load_issues.0.clear();
        assert_eq!(odd, DesignContext::default());
    }

    #[test]
    fn declared_states_and_required_elements_round_trip_and_files_from_before_load_without_them() {
        use super::{DeclaredState, ProposalStatus, RequiredElement, ShownBy, StateKind};
        // A file from before has neither: they read empty and aren't written.
        let old: DesignContext = serde_json::from_value(json!({"brief": {"audience": "Nurses"}, "endStates": [{"node": 3}]})).unwrap();
        assert!(old.states.is_empty() && old.brief.required.is_empty());
        assert_eq!(serde_json::to_value(&old).unwrap(), json!({"brief": {"audience": "Nurses"}, "endStates": [{"node": 3}]}));
        let ctx = DesignContext {
            brief: Brief {
                required: vec![RequiredElement {
                    id: "R1".into(),
                    text: "Consent".into(),
                    screens: vec![NodeId(4)],
                    nodes: vec![NodeId(9)],
                    author: "Claude (agent)".into(),
                    edited_by: String::new(),
                    status: ProposalStatus::Proposed,
                    accepted_by: String::new(),
                }],
                ..Brief::default()
            },
            states: vec![
                DeclaredState {
                    frame: NodeId(4),
                    kind: StateKind::Error,
                    name: String::new(),
                    shown_by: Some(ShownBy::Node { node: NodeId(7) }),
                    reason: "the network fails".into(),
                    author: "user".into(),
                    edited_by: String::new(),
                    status: ProposalStatus::Accepted,
                    accepted_by: String::new(),
                },
                DeclaredState {
                    frame: NodeId(4),
                    kind: StateKind::Other,
                    name: "Offline".into(),
                    shown_by: Some(ShownBy::Variant { instance: NodeId(8), variant: "State=Offline".into() }),
                    reason: String::new(),
                    author: "Claude (agent)".into(),
                    edited_by: String::new(),
                    status: ProposalStatus::Proposed,
                    accepted_by: String::new(),
                },
            ],
            counters: [("R".to_string(), 1)].into_iter().collect(),
            ..DesignContext::default()
        };
        let v = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            v,
            json!({
                "brief": {"required": [{"id": "R1", "text": "Consent", "screens": [4], "nodes": [9], "author": "Claude (agent)", "status": "proposed"}]},
                "states": [
                    {"frame": 4, "kind": "error", "shownBy": {"node": 7}, "reason": "the network fails", "author": "user"},
                    {"frame": 4, "kind": "other", "name": "Offline", "shownBy": {"instance": 8, "variant": "State=Offline"}, "author": "Claude (agent)", "status": "proposed"},
                ],
                "counters": {"R": 1},
            })
        );
        assert_eq!(DesignContext::from_stored(v.clone()), ctx);
        assert!(DesignContext::from_stored(v).load_issues.0.is_empty());
        // Every layer they name is a link of the context, so a gone one is reported.
        let links: Vec<(NodeId, String)> = ctx.links();
        assert!(links.contains(&(NodeId(7), "state error of 4".into())) && links.contains(&(NodeId(8), "state other \"Offline\" of 4".into())));
        assert!(links.contains(&(NodeId(9), "R1".into())) && links.contains(&(NodeId(4), "R1".into())));
        // One declaration per frame, kind and name (case and spaces aside).
        assert!(ctx.states[1].is(NodeId(4), StateKind::Other, " offline ") && !ctx.states[1].is(NodeId(4), StateKind::Other, "Online"));
        let ids: Vec<&str> = StateKind::ALL.iter().map(|k| k.id()).collect();
        assert_eq!(ids, ["firstUse", "empty", "loading", "error", "returning", "success", "other"]);
        for k in StateKind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), json!(k.id()));
        }
    }

    #[test]
    fn a_declared_state_or_required_element_that_cant_be_read_is_left_out_alone() {
        let stored = json!({
            "brief": {"audience": "Nurses", "required": [{"id": "R1", "text": "Consent"}, {"id": "R2", "text": 5}, "junk"]},
            "states": [{"frame": 4, "kind": "error"}, {"frame": 4, "kind": "panic"}, {"frame": "x", "kind": "empty"}, {"frame": 4, "kind": "loading", "shownBy": {"node": "x"}}],
            "counters": {"R": 1},
        });
        let ctx = DesignContext::from_stored(stored);
        let issues = ctx.load_issues.0.join("; ");
        assert!(issues.contains("`brief.required[1]`") && issues.contains("`brief.required[2]`") && issues.contains("`states[1]`"), "{issues}");
        assert_eq!(ctx.brief.audience, "Nurses", "the rest of the brief is kept");
        assert_eq!(ctx.brief.required.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["R1"]);
        assert_eq!(ctx.states.len(), 1, "{:?}", ctx.states);
        // Past the limits, cut and said.
        let many: Vec<serde_json::Value> =
            (0..MAX_ENTRIES + 2).map(|k| json!({"frame": k, "kind": "empty", "reason": "é".repeat(MAX_TEXT + 1)})).collect();
        let ctx = DesignContext::from_stored(json!({"states": many}));
        assert_eq!(ctx.states.len(), MAX_ENTRIES);
        assert!(ctx.states.iter().all(|s| s.reason.chars().count() == MAX_TEXT));
        assert!(ctx.load_issues.0.join("; ").contains(&format!("`states` from {} to {MAX_ENTRIES} entries", MAX_ENTRIES + 2)));
        // Counters behind the ids in use are raised for required elements too.
        let ctx = DesignContext::from_stored(json!({"brief": {"required": [{"id": "R7", "text": "x"}]}}));
        assert_eq!(DesignContext::clone(&ctx).next_id("R"), "R8");
    }

    #[test]
    fn entries_that_couldnt_be_edited_or_removed_are_repaired_on_load_and_said() {
        use super::StateKind;
        let stored = json!({
            "brief": {"required": [
                {"id": "", "text": "Consent"},
                {"id": "R1", "text": "A price", "nodes": [4, 4, 5], "screens": [2, 2]},
                {"id": "r1", "text": "The same id again"},
                {},
            ]},
            "states": [
                {"frame": 2, "kind": "other", "reason": "Offline while on the bus"},
                {"frame": 2, "kind": "other"},
                {"frame": 2, "kind": "error", "reason": "first"},
                {"frame": 2, "kind": "error", "reason": "second"},
                {"frame": 3, "kind": "error"},
            ],
            "counters": {"R": 1},
        });
        let ctx = DesignContext::from_stored(stored);
        let ids: Vec<&str> = ctx.brief.required.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["R2", "R1", "R3", "R4"], "an empty or repeated id gets a fresh one; nothing a person wrote is dropped");
        assert_eq!((ctx.brief.required[1].nodes.clone(), ctx.brief.required[1].screens.clone()), (vec![NodeId(4), NodeId(5)], vec![NodeId(2)]));
        let states: Vec<(StateKind, &str, &str)> = ctx.states.iter().map(|s| (s.kind, s.name.as_str(), s.reason.as_str())).collect();
        assert_eq!(
            states,
            [
                (StateKind::Other, "Offline while on the bus", "Offline while on the bus"),
                (StateKind::Other, "unnamed", ""),
                (StateKind::Error, "", "first"),
                (StateKind::Error, "", ""),
            ],
            "an `other` without a name is named from its reason; a second declaration of the same frame, kind and name is left out"
        );
        let said = ctx.load_issues.0.join("; ");
        for part in [
            "had no id and was given R2",
            "a second required element R1 was given R3",
            "named a layer twice",
            "had no name",
            "declared a second time",
        ] {
            assert!(said.contains(part), "{part}: {said}");
        }
        assert_eq!(DesignContext::clone(&ctx).next_id("R"), "R5");
        // A context that needs nothing repaired says nothing.
        assert!(DesignContext::from_stored(json!({"states": [{"frame": 2, "kind": "error"}]})).load_issues.0.is_empty());
    }

    /// P3-6: ids compare without case when counters are raised and repeats are found, so a blank
    /// id never takes the id a person gave another element.
    #[test]
    fn a_blank_required_id_gets_one_no_entry_has_and_the_persons_id_stays() {
        let ctx = DesignContext::from_stored(json!({"brief": {"required": [{"id": "", "text": "a"}, {"id": "r1", "text": "b"}]}}));
        let ids: Vec<(&str, &str)> = ctx.brief.required.iter().map(|r| (r.id.as_str(), r.text.as_str())).collect();
        assert_eq!(ids, [("R2", "a"), ("R1", "b")], "the person's r1 is kept (as R1); the blank gets a fresh id");
        let said = ctx.load_issues.0.join("; ");
        assert!(said.contains("a required element had no id and was given R2"), "{said}");
        assert!(!said.contains("a second required element"), "nothing was given twice: {said}");
        assert_eq!(DesignContext::clone(&ctx).next_id("R"), "R3");
        // Counters of the other entries are raised by ids in any case too.
        let ctx = DesignContext::from_stored(json!({"assumptions": [{"id": "a4", "text": "t"}]}));
        assert_eq!(DesignContext::clone(&ctx).next_id("A"), "A5");
        // Near miss: a repeat in another case is still a repeat, and the later one moves.
        let ctx = DesignContext::from_stored(json!({"brief": {"required": [{"id": "R1", "text": "a"}, {"id": " r1 ", "text": "b"}]}}));
        let ids: Vec<&str> = ctx.brief.required.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["R1", "R2"]);
        assert!(ctx.load_issues.0.join("; ").contains("a second required element R1 was given R2"));
    }

    /// P3-5: the declarations a person named are kept first; one without a name gets a name no
    /// kept declaration of its frame and kind has.
    #[test]
    fn an_other_state_without_a_name_never_pushes_out_one_a_person_named() {
        use super::StateKind;
        let ctx = DesignContext::from_stored(json!({"states": [
            {"frame": 2, "kind": "other", "reason": "a"},
            {"frame": 2, "kind": "other", "reason": "b"},
            {"frame": 2, "kind": "other", "name": "A"},
            {"frame": 2, "kind": "other", "reason": "b"},
            {"frame": 2, "kind": "other", "name": " b 2 "},
        ]}));
        let names: Vec<&str> = ctx.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a 2", "b", "A", "b 3", " b 2 "], "every declaration is kept, each with a name of its own");
        assert!(ctx.states.iter().all(|s| s.kind == StateKind::Other));
        for (i, a) in ctx.states.iter().enumerate() {
            assert!(ctx.states.iter().skip(i + 1).all(|b| !b.is(a.frame, a.kind, &a.name)), "{:?}", ctx.states);
        }
        let said = ctx.load_issues.0.join("; ");
        assert!(said.contains("3 state(s) of kind other had no name") && !said.contains("declared a second time"), "{said}");
        // Two a person named alike are still one declaration: the first is kept.
        let ctx = DesignContext::from_stored(json!({"states": [
            {"frame": 2, "kind": "other", "name": "Offline", "reason": "first"},
            {"frame": 2, "kind": "other", "name": "offline ", "reason": "second"},
        ]}));
        assert_eq!(ctx.states.iter().map(|s| s.reason.as_str()).collect::<Vec<_>>(), ["first"]);
    }

    #[test]
    fn stages_come_in_order_and_read_as_their_ids() {
        assert!(Stage::Exploring < Stage::Converging && Stage::Converging < Stage::Ready);
        for stage in Stage::ALL {
            assert_eq!(serde_json::to_value(stage).unwrap(), json!(stage.id()));
        }
        assert!(Stage::default().is_exploring());
    }
}
