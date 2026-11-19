//! `context.requiredMissing`: the brief's required elements (`design.required.*`) looked for in
//! the file. An element is found through the layers linked to it, so one with none can't be
//! checked and says so; a linked layer that is gone, hidden, transparent, off the screens the
//! element must be on or cut out of view by a frame around it doesn't show what the brief
//! requires; and a linked text measured below the contrast it needs (WCAG 2.2 SC 1.4.3, the
//! threshold `contrast.text` uses) can't be read as required. No size threshold is applied: no
//! rule here has one for text, and none is invented for this one.

use figcraft_doc::design::{RequiredElement, Severity};
use figcraft_doc::{Document, NodeId};

use super::{clipping, contrast, named};
use crate::check::{Found, Scope};

pub(in crate::design) const MISSING: &str = "context.requiredMissing";

/// The most characters of an element's words a finding quotes.
const QUOTED: usize = 80;

fn quoted(r: &RequiredElement) -> String {
    format!("{} \"{}\"", r.id, figcraft_format::clip(&r.text, QUOTED))
}

/// Why linked layer `id` of `r` doesn't show it; `None`: it does.
fn not_shown(doc: &Document, r: &RequiredElement, id: NodeId) -> Option<String> {
    let Some(n) = doc.node(id) else { return Some(format!("its layer {id} no longer exists")) };
    let what = named(doc, id);
    if !n.visible {
        return Some(format!("{what} is hidden"));
    }
    if n.opacity <= 0.0 {
        return Some(format!("{what} has 0% opacity"));
    }
    for a in doc.ancestors(id) {
        let Some(an) = doc.node(a) else { continue };
        if an.is_page() {
            break;
        }
        if !an.visible || an.hides_contents() {
            return Some(format!("{what} is inside {}, which is hidden", named(doc, a)));
        }
        if an.opacity <= 0.0 {
            return Some(format!("{what} is inside {}, which has 0% opacity", named(doc, a)));
        }
    }
    if !r.screens.is_empty() && !doc.top_level_frame(id).is_some_and(|top| r.screens.contains(&top)) {
        let on: Vec<String> = r.screens.iter().filter(|s| doc.contains(**s)).map(|s| named(doc, *s)).collect();
        return Some(format!("{what} isn't on {}, where it must be", if on.is_empty() { "its screens".to_string() } else { on.join(" or ") }));
    }
    if let Some(cut) = clipping::cut_whole(doc, id) {
        return Some(format!("{what} is out of view: {cut}"));
    }
    None
}

/// What the rule finds for one element.
fn element(doc: &Document, r: &RequiredElement) -> Vec<Found> {
    let subject = |part: &str| if part.is_empty() { format!("required:{}", r.id) } else { format!("required:{}:{part}", r.id) };
    if r.nodes.is_empty() {
        return vec![Found {
            rule: MISSING,
            severity: Severity::Minor,
            nodes: r.screens.iter().copied().filter(|s| doc.contains(*s)).collect(),
            subject: subject(""),
            message: format!("Required element {} can't be checked until a layer is linked", quoted(r)),
            evidence: format!("{}: no layer linked", r.id),
            suggestion: format!("Link the layers that are it: design.required.set {{id: \"{}\", nodes: [layer ids]}}", r.id),
        }];
    }
    let mut out = Vec::new();
    for id in &r.nodes {
        if let Some(why) = not_shown(doc, r, *id) {
            out.push(Found {
                rule: MISSING,
                severity: Severity::Major,
                nodes: vec![*id].into_iter().filter(|n| doc.contains(*n)).collect(),
                subject: subject(&id.to_string()),
                message: format!("Required element {} doesn't show", quoted(r)),
                evidence: why,
                suggestion: "Show it where the brief requires it, at full opacity and inside the visible part of its screen; if the brief changed, change the element (design.required.set)".into(),
            });
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
    for screen in r.screens.iter().filter(|s| doc.contains(**s)) {
        if !r.nodes.iter().any(|n| doc.top_level_frame(*n) == Some(*screen)) {
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
    out
}

/// `context.requiredMissing` for the whole file; with layers in `scope`, the findings naming one
/// of them (or a layer inside one) and those naming none.
pub(super) fn missing(doc: &Document, scope: Scope<'_>) -> Vec<Found> {
    let found: Vec<Found> = doc.design.brief.required.iter().flat_map(|r| element(doc, r)).collect();
    if scope.is_page() {
        return found;
    }
    let within = |n: &NodeId| scope.layers.iter().any(|s| s == n || doc.is_ancestor(*s, *n));
    found.into_iter().filter(|f| f.nodes.is_empty() || f.nodes.iter().any(within)).collect()
}
