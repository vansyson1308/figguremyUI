//! `proto.snapshot`: the picture of a simulated state, what it takes and refuses, and that it
//! only reads. Files are built with commands.

use std::sync::Arc;

use base64::Engine as _;
use figcraft_doc::NodeId;
use figcraft_doc::design::MAX_ENTRIES;
use serde_json::{Value, json};

use crate::params::id_of;
use crate::{EngineError, Session};

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {e}"))
}

fn err(s: &mut Session, p: Value) -> EngineError {
    match s.execute("proto.snapshot", &p) {
        Err(e) => e,
        Ok(v) => panic!("proto.snapshot {p} ran: {}", figcraft_format::clip(&v.to_string(), 300)),
    }
}

fn idv(v: &Value) -> NodeId {
    id_of(&v["id"]).unwrap()
}

fn frame(s: &mut Session, name: &str, x: f64, size: (f64, f64), fill: &str) -> NodeId {
    let f = idv(&ex(s, "frame.create", json!({"x": x, "y": 0, "width": size.0, "height": size.1, "name": name})));
    ex(s, "node.set", json!({"id": f, "props": {"fill": fill}}));
    f
}

fn button(s: &mut Session, name: &str, parent: NodeId, y: f64, action: Value) -> NodeId {
    let b = idv(&ex(
        s,
        "shape.rectangle",
        json!({"x": 20, "y": y, "width": 100, "height": 40, "parent": parent, "props": {"name": name, "fill": "#FFFFFF"}}),
    ));
    ex(s, "proto.interaction.add", json!({"id": b, "trigger": {"type": "onClick"}, "action": action}));
    b
}

/// The decoded picture a result carries as `data`.
fn picture(r: &Value) -> image::RgbaImage {
    let bytes = base64::engine::general_purpose::STANDARD.decode(r["data"].as_str().unwrap()).unwrap();
    image::load_from_memory(&bytes).unwrap().to_rgba8()
}

fn rgb(img: &image::RgbaImage, x: u32, y: u32) -> (u8, u8, u8) {
    let p = img.get_pixel(x, y);
    (p[0], p[1], p[2])
}

/// What is saved, the undo history and the revision: none of them may move when a picture is taken.
fn untouched(s: &Session) -> (Value, (usize, usize), u64) {
    (serde_json::to_value(s.doc().unwrap()).unwrap(), s.undo_depth(), s.ds().unwrap().doc_revision())
}

struct Proto {
    s: Session,
    /// White 300 × 400.
    home: NodeId,
    /// Blue 300 × 400.
    details: NodeId,
    /// Red 100 × 80, opened centred over a half-black dim.
    menu: NodeId,
    open: NodeId,
    next: NodeId,
    close: NodeId,
}

/// Home (white) with "Open" (opens Menu, a red overlay with a "Close" button) and "Next" (goes
/// to Details, blue), a number variable `State/count` that "Next" raises, and a key that goes
/// back from Details.
fn proto() -> Proto {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, (300.0, 400.0), "#FFFFFF");
    let details = frame(&mut s, "Details", 400.0, (300.0, 400.0), "#0000FF");
    let menu = frame(&mut s, "Menu", 800.0, (100.0, 80.0), "#FF0000");
    ex(&mut s, "variable.createCollection", json!({"name": "State"}));
    ex(&mut s, "variable.create", json!({"collection": "State", "name": "count", "type": "number", "value": 0}));
    let open = button(&mut s, "Open", home, 20.0, json!({"type": "openOverlay", "dest": menu, "background": "#00000080"}));
    let next = idv(&ex(&mut s, "shape.rectangle", json!({"x": 20, "y": 300, "width": 100, "height": 40, "parent": home, "props": {"name": "Next"}})));
    let actions = json!([{"type": "setVariable", "variable": "count", "expression": "count + 1"}, {"type": "navigate", "dest": details}]);
    ex(&mut s, "proto.interaction.add", json!({"id": next, "actions": actions}));
    let close = button(&mut s, "Close", menu, 20.0, json!({"type": "closeOverlay"}));
    ex(&mut s, "proto.interaction.add", json!({"id": details, "trigger": {"type": "keyPress", "key": "Escape"}, "action": {"type": "back"}}));
    ex(&mut s, "proto.flow.add", json!({"start": home}));
    ex(&mut s, "select.none", json!({}));
    Proto { s, home, details, menu, open, next, close }
}

#[test]
fn the_picture_is_the_state_the_steps_lead_to_and_the_file_is_not_touched() {
    let Proto { mut s, home, details, menu, open, next, .. } = proto();
    let before = untouched(&s);
    // No steps: the start screen as it opens, from the first flow.
    let r = ex(&mut s, "proto.snapshot", json!({}));
    assert_eq!((&r["format"], &r["width"], &r["height"], &r["scale"]), (&json!("png"), &json!(300), &json!(400), &json!(1.0)), "{r}");
    assert_eq!((&r["start"], &r["state"]["screen"], &r["state"]["screenName"]), (&json!(home), &json!(home), &json!("Home")));
    assert_eq!((&r["steps"], &r["stepsTaken"], &r["simulated"]), (&json!([]), &json!(0), &json!(true)));
    let img = picture(&r);
    assert_eq!((img.width(), img.height()), (300, 400));
    assert_eq!(rgb(&img, 150, 200), (255, 255, 255), "Home is white");
    // An overlay open: the same box, the overlay in the middle over the dimmed screen, at 2×.
    let r = ex(&mut s, "proto.snapshot", json!({"start": home, "steps": [{"click": open}], "scale": 2}));
    assert_eq!((&r["width"], &r["height"], &r["scale"]), (&json!(600), &json!(800), &json!(2.0)));
    assert_eq!(r["state"]["overlays"], json!([{"id": menu, "name": "Menu"}]));
    assert_eq!(r["steps"][0]["state"], json!({"screen": home, "screenName": "Home", "overlays": [{"id": menu, "name": "Menu"}]}));
    assert_eq!((&r["steps"][0]["index"], &r["steps"][0]["fired"]), (&json!(0), &json!(true)));
    assert_eq!(r["steps"][0]["action"], json!({"type": "gesture", "node": open, "gesture": "click"}));
    let img = picture(&r);
    assert_eq!(rgb(&img, 220, 340), (255, 0, 0), "the red overlay sits in the middle: its corner is 100, 160 on the screen");
    let (red, green, blue) = rgb(&img, 20, 700);
    assert!((120..=136).contains(&red) && red == green && green == blue, "the screen under it is dimmed: {red} {green} {blue}");
    // Another screen, with what the prototype changed on the way.
    let r = ex(&mut s, "proto.snapshot", json!({"start": home, "steps": [{"type": "gesture", "node": next}]}));
    assert_eq!(
        (&r["state"]["screen"], &r["state"]["history"], &r["state"]["variables"]),
        (&json!(details), &json!([home]), &json!({"State/count": 1.0}))
    );
    assert_eq!(rgb(&picture(&r), 150, 200), (0, 0, 255), "Details is blue");
    assert_eq!(untouched(&s), before, "a picture reads only: the file, its history and its revision are as they were");
}

/// The steps are `proto.simulate`'s: every form it takes gives the same actions, outcomes and
/// state here, and what it refuses is refused here.
#[test]
fn steps_are_read_as_proto_simulate_reads_them() {
    let Proto { mut s, home, details, open, next, close, .. } = proto();
    let steps = json!([
        {"type": "gesture", "node": open, "gesture": "click", "note": "open the menu", "expect": {"overlays": []}},
        {"type": "clickOutside"},
        {"click": open.to_string()},
        {"action": {"type": "gesture", "node": close}},
        {"hover": open},
        {"type": "wait"},
        {"type": "setVariable", "variable": "State/count", "value": 5},
        {"click": next},
        {"key": "Escape"},
        {"type": "key", "key": "Q"},
        {"mouseDown": 424242},
    ]);
    let simulated = ex(&mut s, "proto.simulate", json!({"start": home, "steps": steps, "stopOnFailure": false}));
    let pictured = ex(&mut s, "proto.snapshot", json!({"start": home, "steps": steps}));
    assert_eq!(pictured["stepsTaken"], json!(11));
    assert_eq!(pictured["state"], simulated["state"], "the state pictured is the state simulated");
    assert_eq!(pictured["state"]["variables"], json!({"State/count": 6.0}), "set to 5, then Next added one");
    let (ours, theirs) = (pictured["steps"].as_array().unwrap(), simulated["steps"].as_array().unwrap());
    assert_eq!(ours.len(), theirs.len());
    for (a, b) in ours.iter().zip(theirs) {
        for key in ["index", "action", "fired", "effects", "problem"] {
            assert_eq!(a.get(key), b.get(key), "step {} `{key}`", a["index"]);
        }
        for key in ["screen", "screenName", "overlays"] {
            assert_eq!(a["state"][key], b["state"][key], "step {} state `{key}`", a["index"]);
        }
        assert!(a.get("failures").is_none() && a.get("note").is_none(), "expectations and notes are proto.simulate's: {a}");
    }
    assert_eq!(ours[7]["state"]["screen"], json!(details));
    assert_eq!(ours[8]["state"]["screen"], json!(home), "the key went back");
    assert_eq!(
        (&ours[9]["fired"], &ours[10]["fired"]),
        (&json!(false), &json!(false)),
        "a step that sets nothing off is reported, and the next still runs"
    );
    assert!(ours[10]["problem"].as_str().unwrap().contains("424242"), "{}", ours[10]);
    // Refused by both, each naming the step.
    let refused = [
        json!([{}]),
        json!([{"click": open, "type": "wait"}]),
        json!([{"click": open, "hover": open}]),
        json!([{"swipe": open}]),
        json!([{"type": "swipe"}]),
        json!([{"type": "gesture"}]),
        json!([{"type": "gesture", "node": open, "gesture": "poke"}]),
        json!([{"click": "the button"}]),
        json!([{"click": open, "expect": "home"}]),
        json!([{"action": {"type": "wait", "expect": {}}}]),
        json!([{"type": "key"}]),
        json!([{"type": "setVariable", "variable": "State/count"}]),
        json!([{"type": "key", "key": "x".repeat(4001)}]),
        json!(["click"]),
        json!({"click": open}),
        Value::Array(vec![json!({"type": "wait"}); MAX_ENTRIES + 1]),
    ];
    for steps in refused {
        let shown = figcraft_format::clip(&steps.to_string(), 80);
        let ours = err(&mut s, json!({"start": home, "steps": steps}));
        assert!(matches!(ours, EngineError::BadParams { .. }), "{shown}: {ours:?}");
        assert!(ours.to_string().contains("steps"), "{shown}: {ours}");
        assert!(s.execute("proto.simulate", &json!({"start": home, "steps": steps})).is_err(), "proto.simulate takes {shown}");
    }
}

#[test]
fn the_picture_is_held_to_a_size_and_the_scale_to_a_range() {
    let Proto { mut s, home, .. } = proto();
    let size = |s: &mut Session, p: Value| {
        let r = ex(s, "proto.snapshot", p);
        (r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap(), r["scale"].as_f64().unwrap())
    };
    assert_eq!(size(&mut s, json!({"start": home, "scale": 100})), (1200, 1600, 4.0), "at most 4×");
    assert_eq!(size(&mut s, json!({"start": home, "scale": 0.0001})), (30, 40, 0.1), "at least 0.1×");
    assert_eq!(size(&mut s, json!({"start": home, "scale": "2"})), (600, 800, 2.0), "a number as text, as other commands read one");
    // A screen larger than the limit is drawn smaller than asked for, and the result says at what scale.
    let wide = frame(&mut s, "Wide", 2000.0, (10_000.0, 500.0), "#FFFFFF");
    let (w, h, scale) = size(&mut s, json!({"start": wide, "scale": 4}));
    assert_eq!((w, h), (4096, 205), "the larger side is 4096 pixels");
    assert!((scale - 0.4096).abs() < 1e-9, "{scale}");
    for bad in [json!(0), json!(-1), json!("big"), json!([2]), json!({"x": 2})] {
        let e = err(&mut s, json!({"start": home, "scale": bad}));
        assert_eq!(e.detail()["param"], "scale", "{bad}: {e}");
    }
}

#[test]
fn a_start_that_is_not_a_screen_is_refused_by_name() {
    let Proto { mut s, home, open, .. } = proto();
    let e = err(&mut s, json!({"start": open}));
    assert_eq!(e.detail()["param"], "start");
    assert!(e.to_string().contains("`Open`") && e.to_string().contains("isn't a top-level frame"), "{e}");
    assert_eq!(err(&mut s, json!({"start": 424_242})).detail()["param"], "start");
    assert_eq!(err(&mut s, json!({"start": [home]})).detail()["param"], "start");
    assert!(err(&mut s, json!({"start": home, "nope": 1})).to_string().contains("unknown parameter `nope`"));
    let mut empty = Session::new();
    assert!(err(&mut empty, json!({})).to_string().contains("no frame to start at"));
}

#[test]
fn with_a_path_the_png_is_written_there_within_the_automation_roots() {
    let Proto { mut s, home, open, .. } = proto();
    let dir = std::env::temp_dir().join(format!("figcraft-proto-snapshot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let inside = dir.join("out").join("menu.png").to_string_lossy().into_owned();
    let outside = dir.join("elsewhere.png").to_string_lossy().into_owned();
    let before = untouched(&s);
    let r = ex(&mut s, "proto.snapshot", json!({"start": home, "steps": [{"click": open}], "path": inside}));
    assert!(r.get("data").is_none(), "written, not returned");
    assert_eq!((&r["path"], &r["width"], &r["height"]), (&json!(inside), &json!(300), &json!(400)));
    let written = std::fs::read(&inside).unwrap();
    assert_eq!(r["bytes"], json!(written.len()));
    let img = image::load_from_memory(&written).unwrap();
    assert_eq!((img.width(), img.height()), (300, 400));
    // Confined to the automation roots like every other file a command writes.
    let roots = Arc::new(crate::file_access::Roots::new(&[], &[dir.join("out").to_string_lossy().into_owned()]).unwrap());
    crate::file_access::confined(Some(&roots), || {
        let e = err(&mut s, json!({"start": home, "path": outside}));
        assert!(matches!(e, EngineError::PathRejected(_)), "{e:?}");
        ex(&mut s, "proto.snapshot", json!({"start": home, "path": inside}));
    });
    assert!(!std::path::Path::new(&outside).exists());
    for bad in [json!(7), json!(""), json!(["a.png"]), json!({"to": "a.png"})] {
        assert_eq!(err(&mut s, json!({"start": home, "path": bad})).detail()["param"], "path", "{bad}");
    }
    assert_eq!(untouched(&s), before);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hostile parameters: nothing panics, nothing is written and the file stays as it was.
#[test]
fn hostile_parameters_are_refused_or_run_without_a_crash() {
    let Proto { mut s, home, .. } = proto();
    let deep = (0..200).fold(json!({"click": 1}), |inner, _| json!({"action": inner, "expect": {"screen": 1}}));
    let wide: Vec<Value> = (0..5000).map(|k| json!({"click": k})).collect();
    let junk = [
        json!({"steps": deep, "start": deep, "scale": deep}),
        json!({"steps": [deep]}),
        json!({"steps": wide}),
        json!({"steps": [{"click": 18_446_744_073_709_551_615u64}, {"click": -1}, {"click": 1.5}, {"click": "0:9999999999999999999999"}], "start": home}),
        json!({"steps": [{"type": "key", "key": "+++"}, {"key": "\u{0}"}, {"key": "Gamepad \u{202e}A"}, {"key": 1e308}], "start": home}),
        json!({"steps": [{"type": "setVariable", "variable": "\u{0}", "value": {"a": [1e308, null]}}, {"type": "setVariable", "variable": 7, "value": 1e308}], "start": home}),
        json!({"steps": [{"type": 7}, {"type": null, "click": null}, {"action": []}], "start": home}),
        json!({"start": "0:0", "scale": 1e308, "steps": null}),
        json!({"start": home, "scale": 1e-308, "path": null}),
    ];
    let before = untouched(&s);
    for j in &junk {
        if let Err(EngineError::Internal { cmd, msg }) = s.execute("proto.snapshot", j) {
            panic!("{cmd} panicked on {}: {msg}", figcraft_format::clip(&j.to_string(), 200));
        }
    }
    assert_eq!(untouched(&s), before);
}
