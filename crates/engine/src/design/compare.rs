//! Comparing alternatives by what can be counted: how many layers, how much text, what a viewer
//! reads in what order, what reacts to a click, and in which of these two frames differ. Nothing
//! here ranks or scores: which alternative is better is a person's call, recorded as a decision.
//!
//! Only what shows is counted (hidden layers are left out, so hiding a block is a difference in
//! structure). The point of the differences is to tell a restyle from a rework: a copy whose
//! layers, text, actions and size match the source and that differs in paint alone is not a
//! structural alternative, whatever it is called.

use std::collections::BTreeSet;

use figcraft_color::{Paint, PaintKind};
use figcraft_doc::proto::Trigger;
use figcraft_doc::{Document, Node, NodeId, NodeKind};
use serde_json::{Value, json};

/// The most layers of one frame that are walked; a larger frame is compared on its first ones
/// (and says so).
const MAX_LAYERS: usize = 20_000;
/// How many texts `readingOrder` lists, and how long each may be.
const MAX_READING: usize = 40;
const MAX_READING_CHARS: usize = 80;

/// A text or a click target with where it sits, for ordering the way a viewer meets them.
struct Placed {
    y: f64,
    x: f64,
    id: NodeId,
    text: String,
}

/// Top to bottom, then left to right, by the top-left corner on the page.
fn in_visual_order(mut items: Vec<Placed>) -> Vec<String> {
    items.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)).then(a.id.cmp(&b.id)));
    items.into_iter().map(|p| p.text).collect()
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_of<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// A paint as it looks, whichever layer a pattern tiles (a copy's pattern tiles the copy's layer).
fn paint_look(p: &Paint) -> String {
    match &p.kind {
        PaintKind::Pattern(pattern) => {
            let mut p = p.clone();
            p.kind = PaintKind::Pattern(figcraft_color::PatternPaint { source: 0, ..pattern.clone() });
            json_of(&p)
        }
        _ => json_of(p),
    }
}

fn round2(v: f64) -> f64 {
    if v.is_finite() { (v * 100.0).round() / 100.0 } else { 0.0 }
}

/// What can be counted about one frame, and what it is compared by.
pub(super) struct Facts {
    width: f64,
    height: f64,
    layers: usize,
    depth: usize,
    text_layers: usize,
    words: usize,
    characters: usize,
    interactive: usize,
    instances: usize,
    components: usize,
    images: usize,
    density: f64,
    /// Every text that shows, one line each, in visual order.
    reading: Vec<String>,
    /// The names of the layers that react to a click, in visual order.
    click_targets: Vec<String>,
    /// Whether the walk stopped at [`MAX_LAYERS`].
    pub cut_short: bool,
    /// Per layer in layer order: kind and nesting; paint; type; geometry; what it reacts to.
    structure: Vec<(usize, &'static str)>,
    paint: Vec<String>,
    fonts: Vec<String>,
    layout: Vec<String>,
    reactions: Vec<String>,
}

impl Facts {
    /// The facts of frame `root` as the prototype shows it (`doc` with the interactions instances
    /// play); `None` when the layer is gone.
    pub(super) fn of(doc: &Document, root: NodeId) -> Option<Facts> {
        let top = doc.node(root)?;
        let frame = doc.world_bounds(root)?;
        let mut f = Facts {
            width: top.width,
            height: top.height,
            layers: 0,
            depth: 0,
            text_layers: 0,
            words: 0,
            characters: 0,
            interactive: 0,
            instances: 0,
            components: 0,
            images: 0,
            density: 0.0,
            reading: Vec::new(),
            click_targets: Vec::new(),
            cut_short: false,
            structure: Vec::new(),
            paint: Vec::new(),
            fonts: Vec::new(),
            layout: Vec::new(),
            reactions: Vec::new(),
        };
        let (mut texts, mut targets, mut mains, mut leaf_area) = (Vec::new(), Vec::new(), BTreeSet::new(), 0.0);
        let mut seen = BTreeSet::new();
        let mut stack = vec![(root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let Some(n) = doc.node(id).filter(|n| (n.visible || id == root) && seen.insert(id)) else { continue };
            if seen.len() > MAX_LAYERS {
                f.cut_short = true;
                break;
            }
            let at = doc.world_bounds(id).unwrap_or(frame);
            if id != root {
                f.layers += 1;
                f.depth = f.depth.max(depth);
                if n.children.is_empty() {
                    let shown = at.intersect(frame);
                    leaf_area += shown.width().max(0.0) * shown.height().max(0.0);
                }
            }
            if let NodeKind::Text(t) = &n.kind {
                f.text_layers += 1;
                f.words += t.characters.split_whitespace().count();
                f.characters += t.char_count();
                let line = one_line(&t.characters);
                if !line.is_empty() {
                    texts.push(Placed { y: at.y0, x: at.x0, id, text: line });
                }
                let mut style = t.style.clone();
                style.fills = None;
                let runs: Vec<_> = t.runs.iter().map(|r| (r.start, r.end, &r.style.font_family, &r.style.font_style, r.style.font_size)).collect();
                f.fonts.push(json_of(&(&style, runs)));
            }
            if let NodeKind::Instance { component, .. } = &n.kind {
                f.instances += 1;
                mains.insert(*component);
            }
            if n.fills.iter().chain(&n.strokes).any(|p| p.visible && matches!(p.kind, PaintKind::Image(_))) {
                f.images += 1;
            }
            if !n.interactions.is_empty() {
                f.interactive += 1;
                f.reactions.push(reaction(n));
            }
            if n.interactions.iter().any(|it| it.trigger == Trigger::OnClick) {
                targets.push(Placed { y: at.y0, x: at.x0, id, text: n.name.clone() });
            }
            f.structure.push((depth, n.kind.type_name()));
            f.paint.push(look(n));
            f.layout.push(geometry(n, id == root));
            stack.extend(n.children.iter().rev().map(|c| (*c, depth + 1)));
        }
        let area = frame.width().max(0.0) * frame.height().max(0.0);
        f.density = if area > 0.0 { round2((leaf_area / area).min(1.0)) } else { 0.0 };
        f.components = mains.len();
        f.reading = in_visual_order(texts);
        f.click_targets = in_visual_order(targets);
        f.reactions.sort();
        Some(f)
    }

    /// The counts, for the result.
    pub(super) fn metrics(&self) -> Value {
        json!({
            "width": round2(self.width),
            "height": round2(self.height),
            "layers": self.layers,
            "depth": self.depth,
            "textLayers": self.text_layers,
            "words": self.words,
            "characters": self.characters,
            "interactive": self.interactive,
            "instances": self.instances,
            "components": self.components,
            "images": self.images,
            "density": self.density,
        })
    }

    /// The first texts a viewer reads, each cut to a line's worth.
    pub(super) fn reading_order(&self) -> Vec<String> {
        self.reading.iter().take(MAX_READING).map(|t| t.chars().take(MAX_READING_CHARS).collect()).collect()
    }

    pub(super) fn actions(&self) -> &[String] {
        &self.click_targets
    }

    /// The texts as compared: case and spacing don't count.
    fn wording(&self) -> Vec<String> {
        self.reading.iter().map(|t| t.to_lowercase()).collect()
    }
}

/// How a layer is painted: fills (a text's stretches with their own included), strokes, effects,
/// opacity and blend.
fn look(n: &Node) -> String {
    let looks = |paints: &[Paint]| paints.iter().map(paint_look).collect::<Vec<String>>();
    let own = |fills: &Option<Vec<Paint>>| fills.as_deref().map(looks);
    let stretches = n.kind.text().map(|t| (own(&t.style.fills), t.runs.iter().map(|r| (r.start, r.end, own(&r.style.fills))).collect::<Vec<_>>()));
    json_of(&(looks(&n.fills), looks(&n.strokes), &n.stroke, &n.effects, n.opacity, n.blend, stretches))
}

/// Where a layer sits in its parent and how it is laid out (a frame's own place on the page
/// isn't part of it: alternatives stand side by side).
fn geometry(n: &Node, root: bool) -> String {
    let place = if root { [0.0; 6] } else { n.transform.as_coeffs().map(round2) };
    let text_box = n.kind.text().map(|t| (t.auto_resize, t.align_h, t.align_v));
    json_of(&(place, round2(n.width), round2(n.height), &n.corners, &n.layout, &n.child_layout, n.clip, text_box))
}

/// What a layer reacts to and what that does, without where it leads (a copy's interactions lead
/// into the copy).
fn reaction(n: &Node) -> String {
    let kinds: Vec<String> = n
        .interactions
        .iter()
        .map(|it| {
            let actions: Vec<String> = figcraft_doc::proto::walk(&it.actions)
                .map(|a| serde_json::to_value(a).ok().and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string)).unwrap_or_default())
                .collect();
            format!("{}: {}", it.trigger.label(), actions.join(", "))
        })
        .collect();
    format!("{} | {}", n.name, kinds.join("; "))
}

/// In what two frames differ.
pub(super) struct Difference {
    pub structure: bool,
    pub order: bool,
    pub content: bool,
    pub actions: bool,
    pub size: bool,
    /// `None` when the structure differs: the layers can't be matched one to one.
    pub layout: Option<bool>,
    pub paint: Option<bool>,
    pub paint_only: bool,
    pub identical: bool,
}

impl Difference {
    pub(super) fn between(a: &Facts, b: &Facts) -> Difference {
        let (wa, wb) = (a.wording(), b.wording());
        let set = |w: &[String]| w.iter().cloned().collect::<BTreeSet<String>>();
        let structure = a.structure != b.structure;
        let fonts = a.fonts != b.fonts;
        let mut d = Difference {
            structure,
            order: wa != wb,
            content: set(&wa) != set(&wb),
            actions: a.reactions != b.reactions,
            size: (a.width - b.width).abs() > 0.01 || (a.height - b.height).abs() > 0.01,
            layout: (!structure).then(|| a.layout != b.layout),
            paint: (!structure).then(|| a.paint != b.paint || fonts),
            paint_only: false,
            identical: false,
        };
        let reworked = d.structure || d.order || d.content || d.actions || d.size;
        let (layout, paint) = (d.layout == Some(true), d.paint == Some(true));
        // A change of type moves the layers around it by itself, so only without one does a
        // difference in geometry show that something was rearranged.
        d.paint_only = paint && !reworked && (fonts || !layout);
        d.identical = !(reworked || layout || paint);
        d
    }

    pub(super) fn json(&self, a: NodeId, b: NodeId) -> Value {
        json!({
            "a": a,
            "b": b,
            "structure": self.structure,
            "order": self.order,
            "content": self.content,
            "actions": self.actions,
            "size": self.size,
            "layout": self.layout,
            "paint": self.paint,
            "paintOnly": self.paint_only,
            "identical": self.identical,
        })
    }
}
