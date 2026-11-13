//! A picture of a simulated state: the screen with the open overlays over it where the player
//! puts them (`ui-egui`'s `present::anchored_overlay_rect` and `overlay_rect`), each over its
//! dimming colour. The picture is the screen's own box: no device, no scrolling.

use figcraft_color::Color;
use figcraft_doc::proto::OverlayPosition;
use figcraft_doc::{ExportFormat, NodeId, NodeKind};
use figcraft_geom::Rect;
use figcraft_render::Pixmap;

use super::{Overlay, Sim};

/// Where overlay `o` sits (page units of the screen): by its position on the screen, or on the
/// layer that opened it when it is anchored to that layer or placed by hand. `None` when the
/// screen or the overlay has no bounds.
pub(super) fn overlay_rect(sim: &Sim, o: &Overlay) -> Option<Rect> {
    let doc = sim.document();
    let base = doc.world_bounds(sim.screen())?;
    let size = doc.world_bounds(o.id)?;
    let (w, h) = (size.width(), size.height());
    // Anchored to the layer that opened it: the preset is measured on that layer.
    let on_trigger = o.anchor.and_then(|a| o.anchor_to.offset(o.position, [a.width(), a.height()], [w, h]));
    let (position, offset) = match on_trigger {
        Some(at) => (OverlayPosition::Manual, Some(at)),
        None => (o.position, o.offset),
    };
    let (cx, cy) = ((base.x0 + base.x1 - w) / 2.0, (base.y0 + base.y1 - h) / 2.0);
    let (x, y) = match position {
        OverlayPosition::Center => (cx, cy),
        OverlayPosition::TopLeft => (base.x0, base.y0),
        OverlayPosition::TopCenter => (cx, base.y0),
        OverlayPosition::TopRight => (base.x1 - w, base.y0),
        OverlayPosition::BottomLeft => (base.x0, base.y1 - h),
        OverlayPosition::BottomCenter => (cx, base.y1 - h),
        OverlayPosition::BottomRight => (base.x1 - w, base.y1 - h),
        OverlayPosition::Manual => match (o.anchor, offset) {
            (Some(a), Some([dx, dy])) => (a.x0 + dx, a.y0 + dy),
            // Without an offset: right below the layer that opened it.
            (Some(a), None) => (a.x0, a.y1),
            // Opened by a key or a timer: no layer to sit on.
            (None, _) => (cx, cy),
        },
    };
    (x.is_finite() && y.is_finite()).then(|| Rect::new(x, y, x + w, y + h))
}

/// The prototype background of the page holding `id`.
fn background(sim: &Sim, id: NodeId) -> Option<Color> {
    let doc = sim.document();
    match &doc.node(doc.page_of(id)?)?.kind {
        NodeKind::Page { prototype, .. } => Some(prototype.background),
        _ => None,
    }
}

/// A colour as premultiplied RGBA bytes, as a pixmap holds its pixels.
fn premultiplied(c: Color) -> [u8; 4] {
    let [r, g, b, a] = c.to_rgba8();
    let pm = |v: u8| ((u32::from(v) * u32::from(a) + 127) / 255) as u8;
    [pm(r), pm(g), pm(b), a]
}

/// `src` over `dst` (both premultiplied).
fn over(dst: &mut [u8], src: &[u8]) {
    let keep = 255 - u32::from(src.get(3).copied().unwrap_or(0));
    for (d, s) in dst.iter_mut().zip(src) {
        *d = (u32::from(*s) + (u32::from(*d) * keep + 127) / 255).min(255) as u8;
    }
}

/// `dst` over `src` (both premultiplied): what is already drawn stays on top.
fn under(dst: &mut [u8], src: &[u8]) {
    let keep = 255 - u32::from(dst.get(3).copied().unwrap_or(0));
    for (d, s) in dst.iter_mut().zip(src) {
        *d = (u32::from(*d) + (u32::from(*s) * keep + 127) / 255).min(255) as u8;
    }
}

/// Draw `top` over `pix` with its top-left pixel at `(x, y)`; what falls outside is cut.
fn blend(pix: &mut Pixmap, top: &Pixmap, x: i64, y: i64) {
    let (w, h) = (i64::from(pix.width()), i64::from(pix.height()));
    let (tw, stride) = (top.width() as usize, pix.width() as usize);
    let data = pix.data_mut();
    for (row, line) in top.data().chunks_exact(tw.max(1) * 4).enumerate() {
        let py = y + row as i64;
        if !(0..h).contains(&py) {
            continue;
        }
        for (col, src) in line.chunks_exact(4).enumerate() {
            let px = x + col as i64;
            if !(0..w).contains(&px) {
                continue;
            }
            let at = (py as usize * stride + px as usize) * 4;
            if let Some(dst) = data.get_mut(at..at + 4) {
                over(dst, src);
            }
        }
    }
}

/// The simulated state as an image at `scale` pixels per page unit: the screen's box (its width
/// and height times `scale`, rounded up) on the page's prototype background, then each open
/// overlay, bottom first, over its dimming colour. `None` when the screen has no size or the
/// image would be larger than the renderer draws.
pub fn render(sim: &Sim, scale: f64) -> Option<Pixmap> {
    let doc = sim.document();
    // The scale the renderer would settle on itself: overlays are placed with the same one.
    let scale = if scale.is_finite() && scale > 0.0 { scale.min(64.0) } else { 1.0 };
    let base = doc.world_bounds(sim.screen())?;
    let mut pix = figcraft_render::render_area(doc, &[sim.screen()], base, scale)?;
    if let Some(bg) = background(sim, sim.screen()) {
        let bg = premultiplied(bg);
        pix.data_mut().chunks_exact_mut(4).for_each(|px| under(px, &bg));
    }
    for o in sim.overlays() {
        if let Some(dim) = o.background {
            let dim = premultiplied(dim);
            pix.data_mut().chunks_exact_mut(4).for_each(|px| over(px, &dim));
        }
        // The overlay with what it paints past its box (a shadow), placed by its box.
        let (Some(placed), Some(own), Some(painted)) = (overlay_rect(sim, o), doc.world_bounds(o.id), figcraft_render::export_bounds(doc, o.id))
        else {
            continue;
        };
        let Some(top) = figcraft_render::render_area(doc, &[o.id], painted, scale) else { continue };
        let px = |page: f64| (page * scale).round() as i64;
        blend(&mut pix, &top, px(placed.x0 + painted.x0 - own.x0 - base.x0), px(placed.y0 + painted.y0 - own.y0 - base.y0));
    }
    Some(pix)
}

/// [`render`] as a PNG file (tagged with the document's colour profile).
pub fn render_png(sim: &Sim, scale: f64) -> Option<Vec<u8>> {
    figcraft_render::encode_export(&render(sim, scale)?, ExportFormat::Png, sim.document().color_profile).ok()
}
