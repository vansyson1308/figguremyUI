//! Content that is cut off: text that doesn't fit its own box, and layers reaching outside a
//! frame that clips its content and can't be scrolled that way.
//!
//! Boxes are compared as upright bounds on the page, so a rotated layer is measured by the box
//! around it; effects (a shadow reaching past the frame) aren't content and aren't counted.
//! Known limits: a shape meant to bleed off the edge of its frame is reported like any other
//! layer that is cut, and masks are not taken into account.

use std::collections::BTreeSet;

use figcraft_doc::design::Severity;
use figcraft_doc::proto::Overflow;
use figcraft_doc::{Document, Node, NodeId, NodeKind, TextAutoResize};

use super::{named, px, shown_layers};
use crate::check::Found;

pub(super) const TEXT: &str = "text.clipped";
pub(super) const CONTENT: &str = "layout.clippedContent";
pub(super) const NO_FONT: &str = "text.noFont";

/// `text.noFont`: text that has no font to be laid out with. Its size, whether it fits and its
/// contrast can't be measured, so the checks that read them are blind to it (review X-01).
pub(super) fn no_font(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    shown_layers(doc, scope)
        .into_iter()
        .filter(|id| {
            doc.node(*id).is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if !t.characters.trim().is_empty() && !crate::fonts::lays_out(t)))
        })
        .map(|id| Found {
            rule: NO_FONT,
            severity: Severity::Note,
            nodes: vec![id],
            subject: String::new(),
            message: format!(
                "Text {} has no font to be laid out with: it draws nothing here, and its size, fit and contrast weren't measured",
                named(doc, id)
            ),
            evidence: "no font in this session gives a face for its family or the fallback one".into(),
            suggestion:
                "run with fonts (the craft-fonts build input, or system fonts) and check again; text.missingFonts lists the families the file names"
                    .into(),
        })
        .collect()
}

/// Differences up to this many px are rounding, not clipping.
const SLACK: f64 = 0.5;

/// How much of a layer a clipping frame around it cuts off.
struct Cut {
    by: NodeId,
    /// Px cut on the left, top, right and bottom.
    sides: [f64; 4],
    /// Nothing of the layer is inside.
    whole: bool,
}

impl Cut {
    fn evidence(&self, doc: &Document) -> String {
        let frame = format!("{} ({}), which clips its content and doesn't scroll that way", named(doc, self.by), self.by);
        if self.whole {
            return format!("entirely outside {frame}");
        }
        let parts: Vec<String> = ["on the left", "at the top", "on the right", "at the bottom"]
            .iter()
            .zip(self.sides)
            .filter(|(_, cut)| *cut > SLACK)
            .map(|(side, cut)| format!("{} px {side}", px(cut)))
            .collect();
        format!("cut {} by {frame}", parts.join(" and "))
    }
}

/// The nearest frame around layer `id` that cuts part of it off where the viewer can't scroll to:
/// a frame that clips its content, on the axes it doesn't scroll. A frame that scrolls an axis
/// makes what is past it on that axis reachable, also past the frames further out.
fn cut(doc: &Document, id: NodeId) -> Option<Cut> {
    let b = doc.world_bounds(id).filter(|b| b.width() > 0.0 || b.height() > 0.0)?;
    let (mut check_x, mut check_y) = (true, true);
    for a in doc.ancestors(id) {
        let Some(an) = doc.node(a).filter(|an| !an.is_page()) else { break };
        if !(an.clip && an.kind.is_frame_like()) {
            continue;
        }
        let Some(ab) = doc.world_bounds(a) else { continue };
        check_x = check_x && !matches!(an.overflow, Overflow::Horizontal | Overflow::Both);
        check_y = check_y && !matches!(an.overflow, Overflow::Vertical | Overflow::Both);
        if !check_x && !check_y {
            break;
        }
        let over = |on: bool, by: f64| if on && by.is_finite() { by.max(0.0) } else { 0.0 };
        let sides = [over(check_x, ab.x0 - b.x0), over(check_y, ab.y0 - b.y0), over(check_x, b.x1 - ab.x1), over(check_y, b.y1 - ab.y1)];
        if sides.iter().any(|s| *s > SLACK) {
            let whole = (check_x && (b.x1 <= ab.x0 || b.x0 >= ab.x1)) || (check_y && (b.y1 <= ab.y0 || b.y0 >= ab.y1));
            return Some(Cut { by: a, sides, whole });
        }
    }
    None
}

/// The size the text of fixed-size text layer `n` needs when it is more than its box has:
/// (needed, available), each width × height.
fn overflow(n: &Node) -> Option<((f64, f64), (f64, f64))> {
    let NodeKind::Text(t) = &n.kind else { return None };
    // Only a fixed box can be too small: the others grow with their text, and a truncating one
    // cuts its text on purpose. Text on a path has no box.
    if t.auto_resize != TextAutoResize::None || t.path.is_some() || !(n.width.is_finite() && n.height.is_finite()) {
        return None;
    }
    let l = figcraft_text::layout_cached(t, Some(n.width), Some(n.height));
    let (needed, available) = ((l.width, l.height), (n.width, n.height));
    let fits = needed.0 <= available.0 + SLACK && needed.1 <= available.1 + SLACK;
    (!fits && needed.0.is_finite() && needed.1.is_finite()).then_some((needed, available))
}

/// `text.clipped`: text that doesn't fit its box, or that a frame around it cuts off.
pub(super) fn text(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    let mut out = Vec::new();
    for id in shown_layers(doc, scope) {
        let Some(n) = doc.node(id).filter(|n| matches!(n.kind, NodeKind::Text(_))) else { continue };
        // Without a font the text isn't laid out: whether it fits is unknown (`text.noFont` says
        // so), never a pass and never a cut.
        if matches!(&n.kind, NodeKind::Text(t) if !crate::fonts::lays_out(t)) {
            continue;
        }
        let too_big =
            overflow(n).map(|(need, have)| format!("the text needs {}×{} px, its box is {}×{} px", px(need.0), px(need.1), px(have.0), px(have.1)));
        let clipped = cut(doc, id);
        if too_big.is_none() && clipped.is_none() {
            continue;
        }
        let suggestion = match (&too_big, &clipped) {
            (Some(_), _) => "make the box bigger, let it grow with its text (auto height), shorten the text, or truncate it on purpose".to_string(),
            (None, Some(c)) => format!("move it inside {}, or let {} scroll or stop clipping its content", named(doc, c.by), named(doc, c.by)),
            (None, None) => String::new(),
        };
        let evidence: Vec<String> = too_big.into_iter().chain(clipped.map(|c| c.evidence(doc))).collect();
        out.push(Found {
            rule: TEXT,
            severity: Severity::Major,
            nodes: vec![id],
            subject: String::new(),
            message: format!("Text {} is cut off: part of it can't be read", named(doc, id)),
            evidence: evidence.join("; "),
            suggestion,
        });
    }
    out
}

/// Whether a layer is reported as a whole when it is cut: something that paints, or a group
/// (which is only as big as what it holds). A frame that paints nothing itself is looked into
/// instead, since what is cut is what is inside it.
fn unit(n: &Node) -> bool {
    let painted = n.fills.iter().any(|p| p.visible && p.opacity > 0.0) || n.has_visible_stroke();
    painted || n.children.is_empty() || n.kind.bounds_follow_children()
}

/// `layout.clippedContent`: layers other than text that a frame around them cuts off. A layer
/// reported as cut stands for what is inside it.
pub(super) fn content(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut covered: BTreeSet<NodeId> = BTreeSet::new();
    for id in shown_layers(doc, scope) {
        let Some(n) = doc.node(id) else { continue };
        if n.parent.is_some_and(|p| covered.contains(&p)) {
            covered.insert(id);
            continue;
        }
        if matches!(n.kind, NodeKind::Text(_) | NodeKind::Slice | NodeKind::Section { .. }) || n.is_mask || !unit(n) {
            continue;
        }
        let Some(c) = cut(doc, id) else { continue };
        covered.insert(id);
        out.push(Found {
            rule: CONTENT,
            severity: Severity::Minor,
            nodes: vec![id],
            subject: String::new(),
            message: format!("{} is cut off by {}: the part outside can't be seen or reached", named(doc, id), named(doc, c.by)),
            evidence: c.evidence(doc),
            suggestion: format!("move or resize it inside {}, or let {} scroll or stop clipping its content", named(doc, c.by), named(doc, c.by)),
        });
    }
    out
}
