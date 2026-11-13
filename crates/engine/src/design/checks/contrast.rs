//! Text contrast (WCAG 2.2 SC 1.4.3), measured only where the answer is certain.
//!
//! The colour of the text and the colour behind it are worked out the way the canvas paints
//! them: solid fills laid over each other with their opacities, each frame around the text
//! faded as a whole by its layer opacity, down to the page colour. Anything that makes the
//! colour behind the text depend on where you look (a gradient, an image, another layer
//! overlapping the text) or that this arithmetic doesn't cover (blend modes, masks) is reported
//! as unmeasured rather than guessed. Known limits: effects (shadows, blurs), strokes on text,
//! rounded corners and rotation of what is behind the text are not taken into account.

use figcraft_color::{BlendMode, Color, Paint, PaintKind};
use figcraft_doc::design::Severity;
use figcraft_doc::{Document, Node, NodeId, NodeKind};
use figcraft_geom::Rect;

use super::{named, px, shown_layers};
use crate::check::Found;

pub(super) const TEXT: &str = "contrast.text";
pub(super) const UNMEASURED: &str = "contrast.unmeasured";

/// WCAG 2.2 SC 1.4.3: the ratio normal text needs, and the one large text needs.
const NEED: f64 = 4.5;
const NEED_LARGE: f64 = 3.0;
/// Large text: at least this size, or at least [`LARGE_BOLD_PX`] and bold (18 pt and 14 pt).
const LARGE_PX: f64 = 24.0;
const LARGE_BOLD_PX: f64 = 18.66;
const BOLD: u16 = 700;

/// The most fills of one layer, style spans of one text and layers sharing a container that are
/// looked at: beyond that the text is reported as unmeasured.
const MAX_FILLS: usize = 64;
const MAX_SPANS: usize = 64;
const MAX_SIBLINGS: usize = 2000;

/// An alpha this close to 1 is opaque (8-bit colour can't tell the difference).
const OPAQUE: f64 = 0.999;

/// A colour with its alpha multiplied in: how layers are laid over each other.
#[derive(Clone, Copy)]
struct Px([f64; 4]);

impl Px {
    const CLEAR: Px = Px([0.0; 4]);
    const WHITE: Px = Px([1.0; 4]);

    fn of(c: Color, alpha: f64) -> Px {
        let part = |v: f32| if v.is_finite() { f64::from(v).clamp(0.0, 1.0) } else { 0.0 };
        let a = part(c.a) * if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
        Px([part(c.r) * a, part(c.g) * a, part(c.b) * a, a])
    }

    fn alpha(self) -> f64 {
        self.0[3]
    }

    fn over(self, under: Px) -> Px {
        let k = 1.0 - self.alpha();
        let [a, b] = [self.0, under.0];
        Px([a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k, a[3] + b[3] * k])
    }

    fn faded(self, opacity: f64) -> Px {
        let o = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
        Px(self.0.map(|v| v * o))
    }

    /// As the 8-bit colour a screen shows (the alpha is taken as opaque: every stack ends on the
    /// page colour).
    fn rgb8(self) -> [u8; 3] {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.0[0]), q(self.0[1]), q(self.0[2])]
    }
}

/// What `fills` paint together, the first at the bottom; an error says what can't be measured.
fn flat(fills: &[Paint]) -> Result<Px, &'static str> {
    if fills.len() > MAX_FILLS {
        return Err("more fills than are measured");
    }
    let mut px = Px::CLEAR;
    for p in fills.iter().filter(|p| p.visible && p.opacity > 0.0) {
        let PaintKind::Solid { color } = &p.kind else { return Err("a gradient, image or pattern fill") };
        if !matches!(p.blend, BlendMode::Normal | BlendMode::PassThrough) {
            return Err("a fill with a blend mode");
        }
        px = Px::of(*color, f64::from(p.opacity)).over(px);
    }
    Ok(px)
}

fn plain_blend(n: &Node) -> bool {
    matches!(n.blend, BlendMode::Normal | BlendMode::PassThrough)
}

/// Whether two boxes share more than a sliver (boxes that only touch don't cover each other).
fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 1.0 && i.height() > 1.0
}

fn contains(outer: Rect, inner: Rect) -> bool {
    const SLACK: f64 = 0.5;
    outer.x0 <= inner.x0 + SLACK && outer.y0 <= inner.y0 + SLACK && outer.x1 >= inner.x1 - SLACK && outer.y1 >= inner.y1 - SLACK
}

/// Whether a layer puts anything on the canvas.
fn paints(n: &Node) -> bool {
    n.fills.iter().any(|p| p.visible && p.opacity > 0.0) || n.has_visible_stroke() || !n.children.is_empty()
}

/// The one colour layer `n` puts behind all of `area`, when that is certain: an upright box with
/// nothing inside it and solid fills, covering the whole area.
fn flat_backdrop(doc: &Document, n: &Node, area: Rect) -> Option<Px> {
    let boxy = matches!(n.kind, NodeKind::Rectangle | NodeKind::Frame | NodeKind::Component { .. } | NodeKind::Instance { .. });
    let [_, b, c, ..] = doc.world_transform(n.id).as_coeffs();
    let upright = b.abs() < 1e-6 && c.abs() < 1e-6;
    if !boxy || !upright || !n.children.is_empty() || !plain_blend(n) || !contains(doc.world_bounds(n.id)?, area) {
        return None;
    }
    flat(&n.fills).ok().map(|px| px.faded(f64::from(n.opacity)))
}

/// One frame or group around the text: what it paints behind the text, and the opacity it (with
/// the text in it) is shown at.
struct Level {
    behind: Px,
    opacity: f64,
}

/// Everything behind a text, innermost container first, ending on the page colour.
struct Backdrop {
    levels: Vec<Level>,
    page: Px,
}

impl Backdrop {
    /// The colour seen where `ink` (the text's own colour, or nothing) is painted.
    fn seen(&self, ink: Px) -> [u8; 3] {
        let mut inner = ink;
        for l in &self.levels {
            inner = inner.over(l.behind).faded(l.opacity);
        }
        inner.over(self.page).rgb8()
    }
}

/// What container `c` paints behind `area` under its child `child`: its own fills, then the
/// layers below `child` that cover the area. `Err` says why that isn't one certain colour.
fn behind_child(doc: &Document, c: &Node, child: NodeId, area: Rect) -> Result<Px, String> {
    let mut px = match &c.kind {
        NodeKind::Page { background, .. } => Px::of(*background, 1.0).over(Px::WHITE),
        _ => {
            let own = flat(&c.fills).map_err(|why| format!("{} has {why}", named(doc, c.id)))?;
            let inside = c.clip || doc.world_bounds(c.id).is_some_and(|b| contains(b, area));
            if own.alpha() > 0.0 && !inside {
                return Err(format!("the text reaches outside {}, which has a fill", named(doc, c.id)));
            }
            own
        }
    };
    for s in c.paint_order().take_while(|s| *s != child) {
        let Some(sn) = doc.node(s).filter(|sn| sn.visible && sn.opacity > 0.0 && paints(sn)) else { continue };
        if !doc.world_bounds(s).is_some_and(|b| overlaps(b, area)) {
            continue;
        }
        match flat_backdrop(doc, sn, area) {
            Some(layer) => px = layer.over(px),
            None => return Err(format!("{} is behind the text and isn't one solid colour across it", named(doc, s))),
        }
    }
    Ok(px)
}

/// The first layer painted over `child` in container `c` that covers part of `area`.
fn in_front(doc: &Document, c: &Node, child: NodeId, area: Rect) -> Option<NodeId> {
    c.paint_order().skip_while(|s| *s != child).skip(1).find(|s| {
        doc.node(*s).is_some_and(|sn| sn.visible && sn.opacity > 0.0 && paints(sn)) && doc.world_bounds(*s).is_some_and(|b| overlaps(b, area))
    })
}

/// What is behind text `id` (which covers `area`). `Ok(None)`: the text doesn't show (something
/// around it is fully transparent). `Err`: why the colour behind it isn't certain.
fn backdrop(doc: &Document, id: NodeId, area: Rect) -> Result<Option<Backdrop>, String> {
    let mut out = Backdrop { levels: Vec::new(), page: Px::WHITE };
    // Once something opaque is behind the text, what lies further back can't show through
    // (until a container around it is itself faded).
    let mut covered = false;
    let mut child = id;
    for a in doc.ancestors(id) {
        let Some(c) = doc.node(a) else { break };
        if c.children.len() > MAX_SIBLINGS {
            return Err(format!("{} holds more layers than are looked at", named(doc, a)));
        }
        if c.children.iter().any(|k| doc.node(*k).is_some_and(|kn| kn.is_mask && kn.visible)) {
            return Err(format!("{} holds a mask", named(doc, a)));
        }
        if !c.is_page() && !plain_blend(c) {
            return Err(format!("{} has a blend mode", named(doc, a)));
        }
        let behind = if covered { None } else { Some(behind_child(doc, c, child, area)?) };
        covered = covered || behind.is_some_and(|b| b.alpha() >= OPAQUE);
        if let Some(front) = in_front(doc, c, child, area) {
            return Err(format!("{} is in front of the text", named(doc, front)));
        }
        let behind = behind.unwrap_or(Px::CLEAR);
        if c.is_page() {
            out.page = behind.over(Px::WHITE);
            break;
        }
        let opacity = f64::from(c.opacity);
        if opacity <= 0.0 {
            return Ok(None);
        }
        covered = covered && opacity >= OPAQUE;
        out.levels.push(Level { behind, opacity });
        child = a;
    }
    Ok(Some(out))
}

/// WCAG relative luminance of an 8-bit sRGB colour.
fn luminance(c: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio of two opaque colours (1 to 21).
fn ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a) + 0.05, luminance(b) + 0.05);
    if la > lb { la / lb } else { lb / la }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// One measurement that fails.
struct Sample {
    ratio: f64,
    need: f64,
    text: [u8; 3],
    on: [u8; 3],
    size: f64,
    style: String,
}

impl Sample {
    fn evidence(&self) -> String {
        // Cut, not rounded: 4.499 must not read as the 4.5 it fails to reach.
        let shown = (self.ratio * 100.0).floor() / 100.0;
        format!("{shown:.2}:1, needs {}:1; text {} on {}, {} px {}", px(self.need), hex(self.text), hex(self.on), px(self.size), self.style)
    }

    /// The nearest colour that passes, moving the text away from what is behind it (darker on a
    /// light ground, lighter on a dark one), else the other way.
    fn suggestion(&self) -> String {
        let darker_first = luminance(self.text) <= luminance(self.on);
        let toward = |target: u8| {
            (0..=255u32).find_map(|step| {
                let mix = |v: u8| {
                    let (from, to) = (f64::from(v), f64::from(target));
                    (from + (to - from) * f64::from(step) / 255.0).round().clamp(0.0, 255.0) as u8
                };
                let c = self.text.map(mix);
                (ratio(c, self.on) >= self.need).then_some(c)
            })
        };
        let (first, second) = if darker_first { ((0, "darken"), (255, "lighten")) } else { ((255, "lighten"), (0, "darken")) };
        match [first, second].into_iter().find_map(|(target, verb)| toward(target).map(|c| (verb, c))) {
            Some((verb, c)) => format!("{verb} the text to at least {} on {}", hex(c), hex(self.on)),
            None => format!("change the colour behind the text: no text colour reaches {}:1 on {}", px(self.need), hex(self.on)),
        }
    }
}

/// What measuring one text gave.
enum Reading {
    Fails(Sample),
    Unmeasured(String),
    Passes,
}

/// Measure text layer `n`: each stretch of text with its own fill, size and weight against what
/// is behind the layer; the stretch furthest below what it needs is the one reported. `None`
/// when there is nothing to read (not text, no characters, not showing).
fn read(doc: &Document, n: &Node) -> Option<Reading> {
    let NodeKind::Text(t) = &n.kind else { return None };
    let area = doc.world_bounds(n.id).filter(|b| b.width() > 0.0 && b.height() > 0.0)?;
    if t.characters.trim().is_empty() || n.opacity <= 0.0 {
        return None;
    }
    if !plain_blend(n) {
        return Some(Reading::Unmeasured("the text has a blend mode".into()));
    }
    let behind = match backdrop(doc, n.id, area) {
        Ok(b) => b?,
        Err(why) => return Some(Reading::Unmeasured(why)),
    };
    let on = behind.seen(Px::CLEAR);
    let spans = t.spans();
    if spans.len() > MAX_SPANS {
        return Some(Reading::Unmeasured("the text has more differently styled stretches than are measured".into()));
    }
    let (mut worst, mut unsure): (Option<Sample>, Option<String>) = (None, None);
    for (start, end, style) in spans {
        if t.characters.chars().skip(start).take(end.saturating_sub(start)).all(char::is_whitespace) {
            continue;
        }
        let ink = match flat(style.fills.as_deref().unwrap_or(&n.fills)) {
            Ok(ink) if ink.alpha() > 0.0 => ink.faded(f64::from(n.opacity)),
            Ok(_) => continue,
            Err(why) => {
                unsure = Some(format!("the text has {why}"));
                continue;
            }
        };
        let size = style.font_size;
        let large = size >= LARGE_PX || (size >= LARGE_BOLD_PX && style.weight() >= BOLD);
        let need = if large { NEED_LARGE } else { NEED };
        let text = behind.seen(ink);
        let name = style.font_style.trim().to_lowercase();
        let style = if name.is_empty() { format!("weight {}", style.weight()) } else { name };
        let sample = Sample { ratio: ratio(text, on), need, text, on, size, style };
        if sample.ratio < need && worst.as_ref().is_none_or(|w| sample.ratio / need < w.ratio / w.need) {
            worst = Some(sample);
        }
    }
    Some(match (worst, unsure) {
        (Some(sample), _) => Reading::Fails(sample),
        (None, Some(why)) => Reading::Unmeasured(why),
        (None, None) => Reading::Passes,
    })
}

fn readings(doc: &Document, scope: &[NodeId]) -> Vec<(NodeId, Reading)> {
    shown_layers(doc, scope).into_iter().filter_map(|id| Some((id, read(doc, doc.node(id)?)?))).collect()
}

/// `contrast.text`: texts measured below the ratio they need.
pub(super) fn failing(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    let found = |(id, reading)| {
        let Reading::Fails(sample) = reading else { return None };
        Some(Found {
            rule: TEXT,
            severity: Severity::Major,
            nodes: vec![id],
            message: format!("Text {} has too little contrast with what is behind it", named(doc, id)),
            evidence: sample.evidence(),
            suggestion: sample.suggestion(),
        })
    };
    readings(doc, scope).into_iter().filter_map(found).collect()
}

/// `contrast.unmeasured`: texts whose contrast can't be measured with certainty.
pub(super) fn unmeasured(doc: &Document, scope: &[NodeId]) -> Vec<Found> {
    let found = |(id, reading)| {
        let Reading::Unmeasured(why) = reading else { return None };
        Some(Found {
            rule: UNMEASURED,
            severity: Severity::Note,
            nodes: vec![id],
            message: format!("The contrast of text {} wasn't measured: what is behind it isn't one certain colour", named(doc, id)),
            evidence: why,
            suggestion: "check it by eye where the background is least favourable, or put a solid fill behind the text".into(),
        })
    };
    readings(doc, scope).into_iter().filter_map(found).collect()
}
