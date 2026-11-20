//! Pictures of a simulated state: the screen's box at the scale asked for, with the open overlays
//! where the player places them.

use figcraft_geom::Rect;
use serde_json::json;

use super::{click, ex, frame, idv, rect, sim};
use crate::Session;
use crate::proto_sim::{Sim, render, render_png};

/// The colour at page point `(x, y)` of the screen in a picture taken at `scale`.
fn colour(p: &Sim, scale: f64, x: f64, y: f64) -> (u8, u8, u8) {
    let pix = render(p, scale).unwrap();
    let px = pix.pixel((x * scale) as u32, (y * scale) as u32).unwrap();
    (px.red(), px.green(), px.blue())
}

/// A white 300 × 400 screen with a button "Open" at (50, 50) that opens the red 100 × 80 frame
/// "Menu" as `overlay` says.
fn setup(overlay: serde_json::Value) -> (Session, figcraft_doc::NodeId, figcraft_doc::NodeId) {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    let menu = idv(&ex(&mut s, "frame.create", json!({"x": 500, "y": 0, "width": 100, "height": 80, "name": "Menu"})));
    ex(&mut s, "node.set", json!({"id": a, "props": {"fill": "#FFFFFF"}}));
    ex(&mut s, "node.set", json!({"id": menu, "props": {"fill": "#FF0000"}}));
    let opener = rect(&mut s, "Open", a, 50.0, 50.0);
    ex(&mut s, "node.set", json!({"id": opener, "props": {"fill": "#FFFFFF"}}));
    let mut action = json!({"type": "openOverlay", "dest": menu});
    action.as_object_mut().unwrap().extend(overlay.as_object().unwrap().clone());
    ex(&mut s, "proto.interaction.add", json!({"id": opener, "action": action}));
    (s, a, opener)
}

#[test]
fn a_picture_is_the_screen_at_scale_and_shows_the_open_overlay() {
    let (s, a, opener) = setup(json!({"background": "#00000080"}));
    let mut p = sim(&s, a);
    let alone = render(&p, 2.0).unwrap();
    assert_eq!((alone.width(), alone.height()), (600, 800), "300 × 400 at 2×");
    assert_eq!(colour(&p, 2.0, 150.0, 200.0), (255, 255, 255));
    click(&mut p, opener);
    let with_overlay = render(&p, 2.0).unwrap();
    assert_eq!((with_overlay.width(), with_overlay.height()), (600, 800), "the overlay doesn't change the picture's size");
    assert_ne!(with_overlay.data(), alone.data());
    assert_eq!(colour(&p, 2.0, 150.0, 200.0), (255, 0, 0), "centred: the overlay covers the screen's middle");
    let (r, g, b) = colour(&p, 2.0, 10.0, 10.0);
    assert!((120..=136).contains(&r) && r == g && g == b, "the screen is dimmed by the overlay's background: {r} {g} {b}");
    // The same picture as a PNG file, at another scale.
    let png = render_png(&p, 1.0).unwrap();
    let decoded = image::load_from_memory(&png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (300, 400));
    // A scale that makes no sense is 1.
    assert_eq!(render(&p, f64::NAN).unwrap().width(), 300);
    assert_eq!(render(&p, -3.0).unwrap().width(), 300);
}

#[test]
fn overlays_sit_where_their_position_says() {
    // (overlay settings, a page point of the screen inside the overlay, one outside it)
    let cases = [
        (json!({"position": "topLeft"}), (5.0, 5.0), (150.0, 200.0)),
        (json!({"position": "bottomRight"}), (295.0, 395.0), (150.0, 200.0)),
        (json!({"position": "bottomCenter"}), (150.0, 395.0), (5.0, 395.0)),
        // By hand: its top-left 20, 30 from the top-left of the layer that opened it (50, 50).
        (json!({"position": "manual", "offset": [20, 30]}), (75.0, 85.0), (65.0, 75.0)),
        // By hand without an offset: right below that layer (which is 40 tall).
        (json!({"position": "manual"}), (55.0, 95.0), (155.0, 95.0)),
        // On the layer that opened it, below it and lined up on the left.
        (json!({"position": "bottomLeft", "anchor": "triggerOutside"}), (55.0, 95.0), (155.0, 95.0)),
    ];
    for (overlay, inside, outside) in cases {
        let (s, a, opener) = setup(overlay.clone());
        let mut p = sim(&s, a);
        click(&mut p, opener);
        assert_eq!(p.overlays()[0].anchor, Some(Rect::new(50.0, 50.0, 150.0, 90.0)), "the layer that opened it, on the screen");
        assert_eq!(colour(&p, 1.0, inside.0, inside.1), (255, 0, 0), "{overlay}: inside");
        assert_eq!(colour(&p, 1.0, outside.0, outside.1), (255, 255, 255), "{overlay}: outside");
    }
}
