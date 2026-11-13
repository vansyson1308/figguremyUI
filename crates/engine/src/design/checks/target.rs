//! Pointer target sizes (WCAG 2.2 SC 2.5.8 and 2.5.5).
//!
//! A target is a layer that reacts to a click, press or drag in the prototype: through its own
//! interactions or, for an instance, those its main component gives it. Its size is the layer's
//! own width and height. Known limits: the exceptions of SC 2.5.8 aren't applied (a small target
//! with enough empty space around it, or one inside a sentence, is still reported), and a small
//! layer inside a larger layer that reacts the same way is reported although the larger one
//! catches the pointer.

use figcraft_doc::design::Severity;
use figcraft_doc::proto::Trigger;
use figcraft_doc::{Document, NodeId, NodeKind};

use super::{named, px, shown_layers};
use crate::check::Found;

pub(super) const SMALL: &str = "target.small";

/// WCAG 2.2 SC 2.5.8 Target Size (Minimum), level AA: 24 by 24 CSS px.
const MINIMUM: f64 = 24.0;
/// WCAG 2.2 SC 2.5.5 Target Size (Enhanced), level AAA: 44 by 44 CSS px.
const ENHANCED: f64 = 44.0;

/// Whether the page layer `id` is on is presented on a phone or tablet: a device held in the
/// hand (the ones that turn), where targets are hit with a finger.
fn touch_device(doc: &Document, id: NodeId) -> bool {
    let page = doc.page_of(id).and_then(|p| doc.node(p));
    matches!(page.map(|p| &p.kind), Some(NodeKind::Page { prototype, .. }) if prototype.model().is_some_and(|d| d.rotates))
}

/// `target.small`: layers that react to the pointer and are smaller than a target should be.
pub(super) fn small(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    // Instances play their main component's interactions, as the prototype player has them.
    let mut played = doc.clone();
    crate::proto_run::play_interactions(&mut played);
    let mut out = Vec::new();
    for id in shown_layers(&played, scope) {
        let Some(n) = played.node(id) else { continue };
        let pointer = |t: &Trigger| matches!(t, Trigger::OnClick | Trigger::OnPress | Trigger::OnDrag);
        let Some(reacts) = n.interactions.iter().find(|it| pointer(&it.trigger)) else { continue };
        let (w, h) = (n.width, n.height);
        if !(w.is_finite() && h.is_finite()) {
            continue;
        }
        let size = format!("{}×{} px", px(w), px(h));
        let (severity, evidence, least) = if w < MINIMUM || h < MINIMUM {
            (Severity::Major, format!("{size}, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)"), "24×24")
        } else if (w < ENHANCED || h < ENHANCED) && touch_device(&played, id) {
            (Severity::Note, format!("{size}, 44×44 recommended on a phone or tablet (WCAG 2.2 SC 2.5.5 target size, level AAA)"), "44×44")
        } else {
            continue;
        };
        out.push(Found {
            rule: SMALL,
            severity,
            nodes: vec![id],
            message: format!("{} reacts to the pointer ({}) and is small to hit", named(&played, id), reacts.trigger.label().to_lowercase()),
            evidence,
            suggestion: format!("make it, or a transparent layer around it that carries the interaction, at least {least} px"),
        });
    }
    out
}
