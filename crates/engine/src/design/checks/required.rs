//! `context.requiredMissing`: the brief's required elements (`design.required.*`) looked for in
//! the file. An element is found through the layers linked to it, so one with none can't be
//! checked and says so; a linked layer that is gone, hidden, nearly transparent, off the screens
//! the element must be on or cut out of view by a frame around it doesn't show what the brief
//! requires; and a linked text measured below the contrast it needs (WCAG 2.2 SC 1.4.3, the
//! threshold `contrast.text` uses) can't be read as required. No size threshold is applied: no
//! rule here has one for text, and none is invented for this one.
//!
//! The file is read as it is, so a layer that only shows in some state of the prototype is hidden
//! in it. When every layer that hides it (the layer itself, and each layer around it that is
//! hidden) is what shows an accepted declared state of its screen (`proto.state.set`'s
//! `shownBy`, accepted as only a person's declaration is), the layer counts as shown in those
//! states: that is a fact about the design ([`shown_in_states`]), not a finding. A state an AI
//! agent only proposed excuses nothing, as only an accepted end state exempts a dead end; a
//! state of another screen excuses nothing, even when what shows it is the whole screen around
//! the layer; and an effective opacity too low to see is never excused.
//!
//! "On a screen" means the layer is the screen's frame or inside it. A screen the element names
//! that is gone, or no longer a top-level frame, is reported as such. Output and work are
//! bounded: the rule stops looking at an element after [`MAX_PER_ELEMENT`] findings and at the
//! elements after [`MAX_FINDINGS`] in all, then one note counts what it didn't look at; evidence
//! names at most [`NAMED`] screens and counts the rest; and each layer is measured once however
//! many elements link it.

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::design::{DeclaredState, RequiredElement, Severity, ShownBy};
use figcraft_doc::{Document, NodeId};

use super::{clipping, contrast, named};
use crate::check::{Found, Scope};

pub(in crate::design) const MISSING: &str = "context.requiredMissing";

/// The most characters of an element's words a finding quotes.
const QUOTED: usize = 80;
/// The effective opacity (a layer's own times those of the layers around it) below which it isn't
/// counted as shown: this product's choice, not a threshold from a standard.
pub(in crate::design) const MIN_OPACITY: f64 = 0.05;
/// The most findings one element gives, and the rule gives in all (then one note counts the rest).
const MAX_PER_ELEMENT: usize = 20;
const MAX_FINDINGS: usize = 200;
/// The most screens, or states, one text names (then it counts the rest).
const NAMED: usize = 3;
/// The most facts [`shown_in_states`] lists (then it counts the rest).
pub const MAX_FACTS: usize = 200;

fn quoted(r: &RequiredElement) -> String {
    format!("{} \"{}\"", r.id, figcraft_format::clip(&r.text, QUOTED))
}

/// At most [`NAMED`] of `names` joined by `or`/`and` (`word`), then how many more there are:
/// "`A`, `B`, `C` or 497 more".
fn few(names: &[String], word: &str) -> String {
    match names.len().checked_sub(NAMED) {
        Some(more) if more > 0 => format!("{} {word} {more} more", names.iter().take(NAMED).cloned().collect::<Vec<_>>().join(", ")),
        _ => names.join(&format!(" {word} ")),
    }
}

/// Layer `id` and the layers around it, up to (not including) the page, innermost first.
fn chain(doc: &Document, id: NodeId) -> Vec<NodeId> {
    std::iter::once(id).chain(doc.ancestors(id)).take_while(|a| doc.node(*a).is_some_and(|n| !n.is_page())).collect()
}

/// How the file shows a layer, as far as hiding goes.
#[derive(Clone, Debug)]
enum Showing {
    Shown,
    /// Hidden in the file, and every layer that hides it shows an accepted declared state of its
    /// screen: those states, in words.
    InStates(String),
    /// Not shown: why.
    Hidden(String),
}

/// A declared state in words: "the error state of `Sign-up form`".
fn state_words(doc: &Document, st: &DeclaredState) -> String {
    let kind = if st.name.is_empty() { st.kind.id().to_string() } else { format!("{} \"{}\"", st.kind.id(), figcraft_format::clip(&st.name, 40)) };
    format!("the {kind} state of {}", named(doc, st.frame))
}

/// What is learned of one linked layer, once a run however many elements link it.
struct Seen {
    chain: Vec<NodeId>,
    showing: Showing,
    /// Out of view, or below its contrast (`Err`): measured when first needed.
    measured: Option<Option<Result<String, String>>>,
}

/// One run over the required elements: the states that excuse hidden layers, and what is known
/// of each layer looked at so far.
struct Look<'a> {
    doc: &'a Document,
    /// The accepted declarations shown by a layer (`shownBy: {node}`), by that layer.
    states: BTreeMap<NodeId, Vec<&'a DeclaredState>>,
    seen: BTreeMap<NodeId, Seen>,
    /// Linked layers and screens looked at: what bounds the run's work.
    work: usize,
}

impl<'a> Look<'a> {
    fn new(doc: &'a Document) -> Look<'a> {
        let mut states: BTreeMap<NodeId, Vec<&DeclaredState>> = BTreeMap::new();
        for st in doc.design.states.iter().filter(|st| st.status.is_accepted()) {
            if let Some(ShownBy::Node { node }) = &st.shown_by {
                states.entry(*node).or_default().push(st);
            }
        }
        Look { doc, states, seen: BTreeMap::new(), work: 0 }
    }

    /// Why a layer whose chain is `chain` isn't shown in the file, or the states it shows in.
    fn showing(&self, chain: &[NodeId]) -> Showing {
        let doc = self.doc;
        let Some(&id) = chain.first() else { return Showing::Shown };
        let what = named(doc, id);
        let screen = doc.top_level_frame(id);
        let mut shown_in: Vec<String> = Vec::new();
        for a in chain {
            let Some(n) = doc.node(*a) else { continue };
            if n.visible && !n.hides_contents() {
                continue;
            }
            // Hidden here: excused only by an accepted state of this layer's screen that this very
            // layer shows. A section's hidden contents are a view of the canvas, not a state.
            let state =
                screen.filter(|_| !n.hides_contents()).and_then(|sc| self.states.get(a).and_then(|list| list.iter().find(|st| st.frame == sc)));
            match state {
                Some(st) => {
                    let words = state_words(doc, st);
                    if !shown_in.contains(&words) {
                        shown_in.push(words);
                    }
                }
                None => {
                    return Showing::Hidden(if *a == id {
                        format!("{what} is hidden")
                    } else {
                        format!("{what} is inside {}, which is hidden", named(doc, *a))
                    });
                }
            }
        }
        let opacity = |a: &NodeId| doc.node(*a).map_or(1.0, |n| if n.opacity.is_finite() { f64::from(n.opacity).clamp(0.0, 1.0) } else { 1.0 });
        let effective: f64 = chain.iter().map(opacity).product();
        if effective < MIN_OPACITY {
            let faintest = chain.iter().copied().min_by(|a, b| opacity(a).total_cmp(&opacity(b))).unwrap_or(id);
            let pct = (effective * 1000.0).round() / 10.0;
            return Showing::Hidden(format!(
                "{what} is at {pct}% effective opacity (its own times the layers' around it; {} is the faintest), below the {}% this product counts as shown (a product choice, not a standard)",
                named(doc, faintest),
                MIN_OPACITY * 100.0
            ));
        }
        if shown_in.is_empty() { Showing::Shown } else { Showing::InStates(few(&shown_in, "and")) }
    }

    /// What is known of layer `id` (which exists), learned the first time it is asked for.
    fn seen(&mut self, id: NodeId) -> &mut Seen {
        if !self.seen.contains_key(&id) {
            let chain = chain(self.doc, id);
            let showing = self.showing(&chain);
            self.seen.insert(id, Seen { chain, showing, measured: None });
        }
        self.seen.entry(id).or_insert_with(|| Seen { chain: Vec::new(), showing: Showing::Shown, measured: None })
    }

    /// Whether layer `id` is out of view (`Ok`, how) or below its contrast (`Err`, how), measured
    /// once.
    fn measured(&mut self, id: NodeId) -> Option<Result<String, String>> {
        let doc = self.doc;
        let seen = self.seen(id);
        seen.measured
            .get_or_insert_with(|| match clipping::cut_whole(doc, id) {
                Some(cut) => Some(Ok(format!("{} is out of view: {cut}", named(doc, id)))),
                None => contrast::fails(doc, id).map(Err),
            })
            .clone()
    }
}

/// What a run didn't look at once a cap was reached.
#[derive(Default)]
struct Left {
    layers: usize,
    screens: usize,
    elements: usize,
}

/// Look at element `r`, adding to `out` the findings `wants` keeps (by the layers they name), at
/// most `room` of them; what it didn't look at when `room` ran out is added to `left`.
fn element(look: &mut Look<'_>, r: &RequiredElement, wants: &dyn Fn(&[NodeId]) -> bool, room: usize, out: &mut Vec<Found>, left: &mut Left) {
    let doc = look.doc;
    let start = out.len();
    let full = |out: &Vec<Found>| out.len().saturating_sub(start) >= room;
    let subject = |part: &str| if part.is_empty() { format!("required:{}", r.id) } else { format!("required:{}:{part}", r.id) };
    // The screens it must be on, once: those that are screens, and what is wrong with the others.
    let mut screens: BTreeSet<NodeId> = BTreeSet::new();
    for (i, s) in r.screens.iter().enumerate() {
        if full(out) {
            left.screens = left.screens.saturating_add(r.screens.len() - i);
            left.layers = left.layers.saturating_add(r.nodes.len());
            return;
        }
        look.work = look.work.saturating_add(1);
        match crate::proto_verify::not_a_screen(doc, *s) {
            None => {
                screens.insert(*s);
            }
            Some(why) => {
                let nodes: Vec<NodeId> = [*s].into_iter().filter(|n| doc.contains(*n)).collect();
                if !wants(&nodes) {
                    continue;
                }
                let top = doc.top_level_frame(*s).filter(|t| t != s);
                out.push(Found {
                    rule: MISSING,
                    severity: Severity::Major,
                    nodes,
                    subject: subject(&format!("screen:{s}")),
                    message: format!("Required element {} names a screen that is no longer one", quoted(r)),
                    evidence: why,
                    suggestion: match top {
                        Some(t) => format!(
                            "Take it off the element's screens, or name {} instead (design.required.set {{id: \"{}\", screens: [...]}})",
                            named(doc, t),
                            r.id
                        ),
                        None => format!("Take it off the element's screens (design.required.set {{id: \"{}\", screens: [...]}})", r.id),
                    },
                });
            }
        }
    }
    if r.nodes.is_empty() {
        let nodes: Vec<NodeId> = screens.iter().copied().collect();
        if !full(out) && wants(&nodes) {
            out.push(Found {
                rule: MISSING,
                severity: Severity::Minor,
                nodes,
                subject: subject(""),
                message: format!("Required element {} can't be checked until a layer is linked", quoted(r)),
                evidence: format!("{}: no layer linked", r.id),
                suggestion: format!("Link the layers that are it: design.required.set {{id: \"{}\", nodes: [layer ids]}}", r.id),
            });
        }
        return;
    }
    let names = || -> Vec<String> { screens.iter().map(|s| named(doc, *s)).collect() };
    let mut on_screens: BTreeSet<NodeId> = BTreeSet::new();
    for (i, id) in r.nodes.iter().enumerate() {
        if full(out) {
            left.layers = left.layers.saturating_add(r.nodes.len() - i);
            left.screens = left.screens.saturating_add(screens.iter().filter(|s| !on_screens.contains(s)).count());
            return;
        }
        look.work = look.work.saturating_add(1);
        let gone = |nodes: Vec<NodeId>, why: String| {
            Found {
            rule: MISSING,
            severity: Severity::Major,
            nodes,
            subject: subject(&id.to_string()),
            message: format!("Required element {} doesn't show", quoted(r)),
            evidence: why,
            suggestion: "Show it where the brief requires it, at full opacity and inside the visible part of its screen; if the brief changed, change the element (design.required.set)".into(),
        }
        };
        if !doc.contains(*id) {
            if wants(&[]) {
                out.push(gone(Vec::new(), format!("its layer {id} no longer exists")));
            }
            continue;
        }
        let seen = look.seen(*id);
        on_screens.extend(seen.chain.iter().copied().filter(|a| screens.contains(a)));
        let on = screens.is_empty() || seen.chain.iter().any(|a| screens.contains(a));
        let showing = seen.showing.clone();
        if !wants(&[*id]) {
            continue;
        }
        if let Showing::Hidden(why) = showing {
            out.push(gone(vec![*id], why));
            continue;
        }
        if !on {
            out.push(gone(vec![*id], format!("{} isn't on {}, where it must be", named(doc, *id), few(&names(), "or"))));
            continue;
        }
        match look.measured(*id) {
            Some(Ok(cut)) => out.push(gone(vec![*id], cut)),
            Some(Err(fails)) => out.push(Found {
                rule: MISSING,
                severity: Severity::Major,
                nodes: vec![*id],
                subject: subject(&id.to_string()),
                message: format!(
                    "Required element {} is below the legibility threshold: text {} doesn't reach the contrast it needs",
                    quoted(r),
                    named(doc, *id)
                ),
                evidence: format!("{fails} (WCAG 2.2 SC 1.4.3 Contrast (Minimum), level AA)"),
                suggestion: "Give it the contrast it needs; required text is part of the design, not small print".into(),
            }),
            None => {}
        }
    }
    // A screen it must be on that none of its layers is on.
    let uncovered: Vec<NodeId> = screens.iter().copied().filter(|s| !on_screens.contains(s)).collect();
    for (i, screen) in uncovered.iter().enumerate() {
        if full(out) {
            left.screens = left.screens.saturating_add(uncovered.len() - i);
            return;
        }
        if !wants(&[*screen]) {
            continue;
        }
        out.push(Found {
            rule: MISSING,
            severity: Severity::Major,
            nodes: vec![*screen],
            subject: subject(&format!("screen:{screen}")),
            message: format!("Required element {} isn't on {}", quoted(r), named(doc, *screen)),
            evidence: format!("{} must be on {}; none of its linked layers is", r.id, named(doc, *screen)),
            suggestion: "Add it to that screen and link the layer, or take the screen off the element if the brief doesn't need it there".into(),
        });
    }
}

/// The layers of required elements that the file hides and accepted declared states show, each
/// with the element and the states in words: `(R1, layer, "the error state of `Form`")`, at most
/// `most` of them, and how many there are in all. A fact about the design, for `design.get` and
/// the panel; not a finding. Cheap: no measuring, and each layer looked at once.
pub fn shown_in_states(doc: &Document, most: usize) -> (Vec<(String, NodeId, String)>, usize) {
    let mut look = Look::new(doc);
    let mut out = Vec::new();
    let mut total = 0usize;
    for r in &doc.design.brief.required {
        for id in r.nodes.iter().filter(|n| doc.contains(**n)) {
            if let Showing::InStates(states) = &look.seen(*id).showing {
                total = total.saturating_add(1);
                if out.len() < most {
                    out.push((r.id.clone(), *id, states.clone()));
                }
            }
        }
    }
    (out, total)
}

/// The rule's findings, and the linked layers and screens it looked at.
fn run(doc: &Document, scope: Scope<'_>) -> (Vec<Found>, usize) {
    let within = |n: &NodeId| scope.layers.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    let wants = |nodes: &[NodeId]| scope.is_page() || nodes.is_empty() || nodes.iter().any(within);
    let mut look = Look::new(doc);
    let mut found: Vec<Found> = Vec::new();
    let mut left = Left::default();
    let required = &doc.design.brief.required;
    for (k, r) in required.iter().enumerate() {
        let Some(room) = MAX_FINDINGS.checked_sub(found.len()).filter(|room| *room > 0) else {
            left.elements = required.len() - k;
            break;
        };
        element(&mut look, r, &wants, room.min(MAX_PER_ELEMENT), &mut found, &mut left);
    }
    let mut parts: Vec<String> = Vec::new();
    for (n, what) in [(left.layers, "linked layer(s)"), (left.screens, "screen(s)")] {
        if n > 0 {
            parts.push(format!("{n} {what} of the elements listed"));
        }
    }
    if left.elements > 0 {
        parts.push(format!("{} element(s)", left.elements));
    }
    if !parts.is_empty() {
        found.push(Found {
            rule: MISSING,
            severity: Severity::Note,
            nodes: Vec::new(),
            subject: "required:more".into(),
            message: "Not every problem with required elements is listed: the rule stopped looking at the caps".into(),
            evidence: format!(
                "at most {MAX_PER_ELEMENT} findings per element and {MAX_FINDINGS} in all are listed; not looked at: {}",
                parts.join(", ")
            ),
            suggestion: "Fix those listed, or narrow the check with `scope`, and run it again".into(),
        });
    }
    (found, look.work)
}

/// `context.requiredMissing` for the whole file; with layers in `scope`, the findings naming one
/// of them (or a layer inside one) and those naming none. Bounded (see the module).
pub(super) fn missing(doc: &Document, scope: Scope<'_>) -> Vec<Found> {
    run(doc, scope).0
}

/// How many linked layers and screens a run over the whole file looks at.
#[cfg(test)]
pub(in crate::design) fn work(doc: &Document) -> usize {
    let page = doc.pages.first().copied().unwrap_or(NodeId(0));
    run(doc, Scope::of_page(page)).1
}
