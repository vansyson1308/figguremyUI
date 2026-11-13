//! What a design check reports, and how checks are listed. A check is a rule that measures
//! something in the file: the same file gives the same result, and running it again after a
//! fix settles whether the fix worked. Opinions don't belong here (they are findings of kind
//! `judgement`, recorded by whoever holds them).
//!
//! One call of the checks (`design.check`, `proto.check`, `design.finding.recheck`) runs many
//! rules, and several of them need the same costly things: the document as the prototype player
//! has it, a page's flow graph, the runs of its stored scenarios. [`Facts`] holds those for the
//! length of the call, each worked out once, the first time a rule asks. Nothing is kept between
//! calls.

use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use figcraft_doc::design::Severity;
use figcraft_doc::{Document, NodeId};

use crate::proto_verify::PageFacts;

/// One thing a rule found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The rule's id (`contrast.text`, `flow.deadEnd`…).
    pub rule: &'static str,
    pub severity: Severity,
    /// The layers it is about, the most specific first. With a `subject`, they are for showing
    /// where to look and may differ from run to run.
    pub nodes: Vec<NodeId>,
    /// What the finding is about, stable from run to run (a scenario's id, an overlay's id, a
    /// page): a stored finding is the same problem when its rule and subject match. Empty: the
    /// layers in `nodes` are the subject.
    pub subject: String,
    /// What is wrong, in one sentence a person can act on.
    pub message: String,
    /// What was measured (numbers, names, the step that failed).
    pub evidence: String,
    /// One way to fix it.
    pub suggestion: String,
}

/// What a run of a rule looks at.
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    /// The page looked at when no layers are given: the one the session shows.
    pub page: NodeId,
    /// The layers asked for, each with everything inside it. Empty: the page.
    pub layers: &'a [NodeId],
}

impl Scope<'_> {
    /// The whole of `page`.
    pub fn of_page(page: NodeId) -> Scope<'static> {
        Scope { page, layers: &[] }
    }

    /// No layers were asked for: the page is looked at.
    pub fn is_page(&self) -> bool {
        self.layers.is_empty()
    }

    /// The layers a rule that measures layers walks (each with everything inside it): the ones
    /// asked for, else the page's top-level frames.
    pub fn roots(&self, doc: &Document) -> Vec<NodeId> {
        if self.layers.is_empty() { doc.top_frames(self.page) } else { self.layers.to_vec() }
    }
}

/// What one call of the checks has worked out so far, shared by the rules it runs.
pub struct Facts<'a> {
    /// The file being checked.
    pub doc: &'a Document,
    played: OnceCell<Document>,
    flows: RefCell<BTreeMap<NodeId, Rc<PageFacts>>>,
}

impl<'a> Facts<'a> {
    pub fn new(doc: &'a Document) -> Facts<'a> {
        Facts { doc, played: OnceCell::new(), flows: RefCell::new(BTreeMap::new()) }
    }

    /// The file as the prototype player has it: instances play their main components'
    /// interactions ([`crate::proto_run::play_interactions`]). Copied once per call.
    pub fn played(&self) -> &Document {
        self.played.get_or_init(|| {
            let mut played = self.doc.clone();
            crate::proto_run::play_interactions(&mut played);
            played
        })
    }

    /// The flow graph of `page` with what the flow rules find on it, built once per call.
    pub(crate) fn flow(&self, page: NodeId) -> Rc<PageFacts> {
        if let Some(known) = self.flows.borrow().get(&page) {
            return known.clone();
        }
        let built = Rc::new(PageFacts::build(self, page));
        self.flows.borrow_mut().insert(page, built.clone());
        built
    }
}

/// A rule: `run` looks at `scope` and reports what it finds.
pub struct Rule {
    pub id: &'static str,
    /// What the rule measures and its threshold, for `design.check`'s rule list.
    pub about: &'static str,
    pub run: fn(&Facts<'_>, Scope<'_>) -> Vec<Found>,
    /// What keeps a stored finding of this rule open although `run` no longer reports it: results
    /// (with this rule's id) for what can't be called fixed, such as text whose contrast can no
    /// longer be measured. Only `design.finding.recheck` asks.
    pub unsettled: Option<fn(&Facts<'_>, Scope<'_>) -> Vec<Found>>,
}
