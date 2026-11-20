//! `context.requiredMissing`: the brief's required elements (`design.required.*`) looked for in
//! the file. An element is found through the layers linked to it, so one with none can't be
//! checked and says so; a linked layer that is gone, hidden, nearly transparent, off the screens
//! the element must be on or cut out of view by a frame around it doesn't show what the brief
//! requires; and a linked text measured below the contrast it needs (WCAG 2.2 SC 1.4.3, the
//! threshold `contrast.text` uses) can't be read as required. No size threshold is applied: no
//! rule here has one for text, and none is invented for this one.
//!
//! The file is read as it is, so a layer that only shows in some state of the prototype is hidden
//! in it. When what hides it (the layer, or the layer around it that is hidden) is, or is inside,
//! what shows a declared state (`proto.state.set`'s `shownBy`), the layer counts as shown in that
//! state: that is a fact about the design ([`shown_in_states`]), not a finding.
//!
//! "On a screen" means the layer is the screen's frame or inside it. A screen the element names
//! that is gone, or no longer a top-level frame, is reported as such. Output is bounded: at most
//! [`MAX_PER_ELEMENT`] findings for one element and [`MAX_FINDINGS`] for the rule, then one note
//! that counts the rest.

use std::collections::BTreeSet;

use figcraft_doc::design::{RequiredElement, Severity, ShownBy};
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

fn quoted(r: &RequiredElement) -> String {
    format!("{} \"{}\"", r.id, figcraft_format::clip(&r.text, QUOTED))
}

/// Layer `id` and the layers around it, up to (not including) the page, innermost first.
fn chain(doc: &Document, id: NodeId) -> Vec<NodeId> {
    std::iter::once(id).chain(doc.ancestors(id)).take_while(|a| doc.node(*a).is_some_and(|n| !n.is_page())).collect()
}

/// Why a layer whose chain is `chain` isn't shown in the file, and the layer that hides it.
fn hidden(doc: &Document, chain: &[NodeId]) -> Option<(String, NodeId)> {
    let id = *chain.first()?;
    let what = named(doc, id);
    for a in chain {
        let Some(n) = doc.node(*a) else { continue };
        if !n.visible || n.hides_contents() {
            return Some((if *a == id { format!("{what} is hidden") } else { format!("{what} is inside {}, which is hidden", named(doc, *a)) }, *a));
        }
    }
    let opacity = |a: &NodeId| doc.node(*a).map_or(1.0, |n| if n.opacity.is_finite() { f64::from(n.opacity).clamp(0.0, 1.0) } else { 1.0 });
    let effective: f64 = chain.iter().map(opacity).product();
    if effective < MIN_OPACITY {
        let faintest = chain.iter().copied().min_by(|a, b| opacity(a).total_cmp(&opacity(b))).unwrap_or(id);
        let pct = (effective * 1000.0).round() / 10.0;
        return Some((
            format!(
                "{what} is at {pct}% effective opacity (its own times the layers' around it; {} is the faintest), below the {}% this product counts as shown (a product choice, not a standard)",
                named(doc, faintest),
                MIN_OPACITY * 100.0
            ),
            faintest,
        ));
    }
    None
}

/// The declared state that shows layer `hider` (it is, or is inside, the state's `shownBy`), as
/// words: "the error state of `Sign-up form`".
fn state_showing(doc: &Document, hider: NodeId, hider_chain: &[NodeId]) -> Option<String> {
    doc.design
        .states
        .iter()
        .find_map(|st| match &st.shown_by {
            Some(ShownBy::Node { node }) if hider_chain.contains(node) => {
                let kind = if st.name.is_empty() { st.kind.id().to_string() } else { format!("{} \"{}\"", st.kind.id(), st.name) };
                Some(format!("the {kind} state of {}", named(doc, st.frame)))
            }
            _ => None,
        })
        .filter(|_| doc.contains(hider))
}

/// What the rule finds for one element, and which of its layers show only in a declared state
/// (`(layer, the state in words)`).
fn element(doc: &Document, r: &RequiredElement) -> (Vec<Found>, Vec<(NodeId, String)>) {
    let subject = |part: &str| if part.is_empty() { format!("required:{}", r.id) } else { format!("required:{}:{part}", r.id) };
    let mut out = Vec::new();
    let mut facts = Vec::new();
    // The screens it must be on, once: those that are screens, and what is wrong with the others.
    let mut screens: BTreeSet<NodeId> = BTreeSet::new();
    for s in &r.screens {
        match crate::proto_verify::not_a_screen(doc, *s) {
            None => {
                screens.insert(*s);
            }
            Some(why) => {
                let top = doc.top_level_frame(*s).filter(|t| t != s);
                out.push(Found {
                    rule: MISSING,
                    severity: Severity::Major,
                    nodes: [*s].into_iter().filter(|n| doc.contains(*n)).collect(),
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
        out.push(Found {
            rule: MISSING,
            severity: Severity::Minor,
            nodes: screens.iter().copied().collect(),
            subject: subject(""),
            message: format!("Required element {} can't be checked until a layer is linked", quoted(r)),
            evidence: format!("{}: no layer linked", r.id),
            suggestion: format!("Link the layers that are it: design.required.set {{id: \"{}\", nodes: [layer ids]}}", r.id),
        });
        return (out, facts);
    }
    let mut on_screens: BTreeSet<NodeId> = BTreeSet::new();
    for id in &r.nodes {
        let gone = |why: String| {
            Found {
            rule: MISSING,
            severity: Severity::Major,
            nodes: [*id].into_iter().filter(|n| doc.contains(*n)).collect(),
            subject: subject(&id.to_string()),
            message: format!("Required element {} doesn't show", quoted(r)),
            evidence: why,
            suggestion: "Show it where the brief requires it, at full opacity and inside the visible part of its screen; if the brief changed, change the element (design.required.set)".into(),
        }
        };
        if !doc.contains(*id) {
            out.push(gone(format!("its layer {id} no longer exists")));
            continue;
        }
        let chain = chain(doc, *id);
        on_screens.extend(chain.iter().copied().filter(|a| screens.contains(a)));
        if let Some((why, hider)) = hidden(doc, &chain) {
            let hider_chain: Vec<NodeId> = chain.iter().copied().skip_while(|a| *a != hider).collect();
            match state_showing(doc, hider, &hider_chain) {
                Some(state) => facts.push((*id, state)),
                None => {
                    out.push(gone(why));
                    continue;
                }
            }
        }
        if !screens.is_empty() && !chain.iter().any(|a| screens.contains(a)) {
            let on: Vec<String> = screens.iter().map(|s| named(doc, *s)).collect();
            out.push(gone(format!("{} isn't on {}, where it must be", named(doc, *id), on.join(" or "))));
        } else if let Some(cut) = clipping::cut_whole(doc, *id) {
            out.push(gone(format!("{} is out of view: {cut}", named(doc, *id))));
        } else if let Some(fails) = contrast::fails(doc, *id) {
            out.push(Found {
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
            });
        }
    }
    // A screen it must be on that none of its layers is on.
    for screen in screens.iter().filter(|s| !on_screens.contains(s)) {
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
    (out, facts)
}

/// The layers of required elements that the file hides and a declared state shows, each with the
/// element and the state in words: `(R1, layer, "the error state of `Form`")`. A fact about the
/// design, for `design.get` and the panel; not a finding. Cheap: no measuring.
pub fn shown_in_states(doc: &Document) -> Vec<(String, NodeId, String)> {
    let mut out = Vec::new();
    for r in &doc.design.brief.required {
        for id in r.nodes.iter().filter(|n| doc.contains(**n)) {
            let chain = chain(doc, *id);
            let Some((_, hider)) = hidden(doc, &chain) else { continue };
            let hider_chain: Vec<NodeId> = chain.iter().copied().skip_while(|a| *a != hider).collect();
            if let Some(state) = state_showing(doc, hider, &hider_chain) {
                out.push((r.id.clone(), *id, state));
            }
        }
    }
    out
}

/// `context.requiredMissing` for the whole file; with layers in `scope`, the findings naming one
/// of them (or a layer inside one) and those naming none. Bounded (see the module).
pub(super) fn missing(doc: &Document, scope: Scope<'_>) -> Vec<Found> {
    let within = |n: &NodeId| scope.layers.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    let mut found: Vec<Found> = Vec::new();
    let mut left_out = 0usize;
    for r in &doc.design.brief.required {
        let (mine, _shown_in_states) = element(doc, r);
        let mine: Vec<Found> = mine.into_iter().filter(|f| scope.is_page() || f.nodes.is_empty() || f.nodes.iter().any(within)).collect();
        for (i, f) in mine.into_iter().enumerate() {
            if i < MAX_PER_ELEMENT && found.len() < MAX_FINDINGS {
                found.push(f);
            } else {
                left_out = left_out.saturating_add(1);
            }
        }
    }
    if left_out > 0 {
        found.push(Found {
            rule: MISSING,
            severity: Severity::Note,
            nodes: Vec::new(),
            subject: "required:more".into(),
            message: format!("{left_out} more problem(s) with required elements weren't listed"),
            evidence: format!("at most {MAX_PER_ELEMENT} findings per element and {MAX_FINDINGS} in all are listed"),
            suggestion: "Fix those listed, or narrow the check with `scope`, and run it again".into(),
        });
    }
    found
}
