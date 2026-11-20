//! While a prototype plays over the canvas, the canvas methods refuse instead of editing a
//! selection nobody sees; and `app.info` says which run of the app answers.

use egui::{Pos2, pos2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::present_tests::{Ui as Harness, id};
use crate::{FigApp, NoServices};

fn handle(u: &mut Harness, method: &str, p: Value) -> Result<Value, String> {
    let ctx = u.ctx.clone();
    super::handle(&mut u.app, &ctx, method, &p)
}

/// Screen A (300×400 at the page's origin) with a rectangle at (50, 50), and screen B.
fn two_screens() -> (Harness, NodeId, NodeId) {
    let mut s = Session::new();
    let ex = |s: &mut Session, c: &str, p: Value| s.execute(c, &p).unwrap();
    let a = id(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 300, "height": 400, "name": "A"})));
    ex(&mut s, "frame.create", json!({"x": 400, "y": 0, "width": 300, "height": 400, "name": "B"}));
    let rect = id(&ex(&mut s, "shape.rectangle", json!({"x": 50, "y": 50, "width": 100, "height": 60, "parent": a})));
    ex(&mut s, "select.set", json!({"ids": []}));
    (Harness::new(s), a, rect)
}

#[test]
fn the_canvas_methods_refuse_while_the_presenter_plays() {
    let (mut u, a, rect) = two_screens();
    // In the editor a click on the rectangle selects it and Delete removes it: the baseline.
    assert_eq!(handle(&mut u, "ui.pointer", json!({"kind": "click", "x": 100, "y": 80})).unwrap()["selection"], json!([rect]));
    u.run("select.set", json!({"ids": []}));
    u.run("proto.present", json!({"start": a}));
    assert!(u.app.ui.present.is_some());
    let before = serde_json::to_value(u.app.session.doc().unwrap()).unwrap();
    for (method, params) in [
        ("ui.pointer", json!({"kind": "click", "x": 100, "y": 80})),
        ("ui.pointer", json!({"kind": "drag", "sx": 500, "sy": 300, "to": [600, 400]})),
        ("ui.key", json!({"key": "A", "cmd": true})),
        ("ui.key", json!({"key": "Delete"})),
        ("ui.type", json!({"text": "x"})),
    ] {
        let e = handle(&mut u, method, params.clone()).unwrap_err();
        assert!(e.contains("a prototype is playing") && e.contains(&format!("`{method}`")), "{method} {params}: {e}");
        assert!(e.contains("`ui.input`") && e.contains("`proto.simulate`"), "{method}: the error says where the input goes instead: {e}");
    }
    assert!(u.app.session.selection().is_empty(), "nothing was selected behind the presenter");
    assert_eq!(serde_json::to_value(u.app.session.doc().unwrap()).unwrap(), before, "and nothing was edited");
    // The player's own input still goes through, as do commands and inspection.
    assert!(handle(&mut u, "ui.input", json!({"events": [{"move": [700, 450]}]})).is_ok());
    assert!(handle(&mut u, "ui.inspect", json!({})).unwrap()["presenting"].as_bool().unwrap());
    assert!(handle(&mut u, "engine.execute", json!({"command": "select.set", "params": {"ids": [rect]}})).is_ok());
}

#[test]
fn with_the_preview_pane_the_canvas_stays_reachable_except_under_the_pane() {
    let (mut u, a, rect) = two_screens();
    u.run("proto.preview", json!({"start": a}));
    let pane = u.pane();
    // The canvas shows beside the pane: page coordinates and screen points off the pane work.
    assert_eq!(handle(&mut u, "ui.pointer", json!({"kind": "click", "x": 100, "y": 80})).unwrap()["selection"], json!([rect]));
    assert!(handle(&mut u, "ui.key", json!({"key": "Escape"})).is_ok());
    let off: Pos2 = pos2(pane.min.x - 30.0, pane.center().y);
    assert!(handle(&mut u, "ui.pointer", json!({"kind": "hover", "sx": off.x, "sy": off.y})).is_ok(), "beside the pane");
    // A screen point on the pane would land on the canvas hidden under it.
    let on = pane.center();
    let e = handle(&mut u, "ui.pointer", json!({"kind": "click", "sx": on.x, "sy": on.y})).unwrap_err();
    assert!(e.contains("preview pane") && e.contains("`ui.input`") && e.contains("`proto.simulate`"), "{e}");
}

#[test]
fn app_info_names_this_run_of_the_app() {
    let ctx = egui::Context::default();
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    let info = super::handle(&mut app, &ctx, "app.info", &json!({})).unwrap();
    let instance = info["instance"].as_str().unwrap().to_string();
    assert_eq!(instance.len(), 32, "{instance}");
    assert!(instance.chars().all(|c| c.is_ascii_hexdigit()), "{instance}");
    assert_eq!(info["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(info["pid"], json!(std::process::id()));
    // The same app answers the same; another app (a restart) answers differently.
    assert_eq!(super::handle(&mut app, &ctx, "app.info", &json!({})).unwrap(), info);
    let mut other = FigApp::new(Session::new(), Box::new(NoServices));
    assert_ne!(super::handle(&mut other, &ctx, "app.info", &json!({})).unwrap()["instance"], json!(instance));
    // No file needs to be open.
    app.session.docs.clear();
    assert!(super::handle(&mut app, &ctx, "app.info", &json!({})).is_ok());
}
