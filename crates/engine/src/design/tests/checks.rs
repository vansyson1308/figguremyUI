//! The check rules (a fixture that triggers each with the numbers it reports, and near misses
//! that must not), the stage profiles, and recording and re-running checks.

use figcraft_doc::NodeKind;
use figcraft_doc::design::{FindingStatus, Stage};
use serde_json::json;

use super::{NodeId, Session, Value, err, ex, ex_undoable, frame, label, nid, rect, reopened};

const CONTRAST: &str = "contrast.text";
const UNMEASURED: &str = "contrast.unmeasured";
const TEXT_CLIPPED: &str = "text.clipped";
const CLIPPED: &str = "layout.clippedContent";
const TARGET: &str = "target.small";

/// What `rule` finds on the current page.
fn found(s: &mut Session, rule: &str) -> Vec<Value> {
    let r = ex(s, "design.check", json!({"rules": [rule]}));
    assert_eq!(r["rules"].as_array().map(Vec::len), Some(1), "{r}");
    r["findings"].as_array().unwrap().clone()
}

/// The one finding `rule` gives.
fn only(s: &mut Session, rule: &str) -> Value {
    let all = found(s, rule);
    assert_eq!(all.len(), 1, "expected one `{rule}` finding: {all:#?}");
    all[0].clone()
}

fn set(s: &mut Session, id: NodeId, props: Value) {
    ex(s, "node.set", json!({"id": id, "props": props}));
}

/// A white 320×200 card with a 14 px text of colour `fill` on it.
fn card_with_text(s: &mut Session, fill: &str) -> (NodeId, NodeId) {
    let card = frame(s, "Card", 0.0, 320.0, 200.0);
    let text = label(s, card, "Read me", [20.0, 20.0], json!({"name": "Caption", "fill": fill, "fontSize": 14}));
    (card, text)
}

#[test]
fn low_contrast_text_is_reported_with_its_numbers_and_a_colour_that_passes() {
    let mut s = Session::new();
    let (_, text) = card_with_text(&mut s, "#9AA0A6");
    let f = only(&mut s, CONTRAST);
    assert_eq!((f["rule"].as_str(), f["severity"].as_str(), f["kind"].as_str()), (Some(CONTRAST), Some("major"), Some("check")));
    assert_eq!(f["nodes"], json!([text]));
    assert_eq!(f["evidence"], "2.64:1, needs 4.5:1; text #9AA0A6 on #FFFFFF, 14 px regular");
    assert_eq!(f["suggestion"], "darken the text to at least #73777C on #FFFFFF");
    assert!(f["message"].as_str().unwrap().contains("`Caption`"), "{f}");
    // The suggested colour passes; one step lighter than the threshold colour doesn't.
    set(&mut s, text, json!({"fill": "#73777C"}));
    assert!(found(&mut s, CONTRAST).is_empty());
}

#[test]
fn contrast_at_the_threshold_passes_and_just_under_it_fails() {
    let mut s = Session::new();
    let (_, text) = card_with_text(&mut s, "#767676");
    assert!(found(&mut s, CONTRAST).is_empty(), "#767676 on white is 4.54:1");
    set(&mut s, text, json!({"fill": "#777777"}));
    assert_eq!(only(&mut s, CONTRAST)["evidence"], "4.47:1, needs 4.5:1; text #777777 on #FFFFFF, 14 px regular");
    // Large text needs 3:1: from 24 px, or from 18.66 px when bold.
    set(&mut s, text, json!({"fill": "#949494", "fontSize": 24}));
    assert!(found(&mut s, CONTRAST).is_empty(), "#949494 on white is 3.03:1, enough at 24 px");
    set(&mut s, text, json!({"fill": "#959595"}));
    assert_eq!(only(&mut s, CONTRAST)["evidence"], "2.99:1, needs 3:1; text #959595 on #FFFFFF, 24 px regular");
    set(&mut s, text, json!({"fill": "#949494", "fontSize": 23.9}));
    assert!(only(&mut s, CONTRAST)["evidence"].as_str().unwrap().starts_with("3.03:1, needs 4.5:1"), "23.9 px regular isn't large text");
    set(&mut s, text, json!({"fontSize": 18.66, "fontWeight": 700}));
    assert!(found(&mut s, CONTRAST).is_empty(), "18.66 px bold is large text");
    set(&mut s, text, json!({"fontSize": 18.5}));
    assert_eq!(only(&mut s, CONTRAST)["evidence"], "3.03:1, needs 4.5:1; text #949494 on #FFFFFF, 18.5 px bold");
    set(&mut s, text, json!({"fontSize": 18.66, "fontWeight": 400}));
    assert_eq!(found(&mut s, CONTRAST).len(), 1, "18.66 px regular isn't large text");
}

#[test]
fn hidden_text_isnt_measured() {
    let mut s = Session::new();
    let (card, text) = card_with_text(&mut s, "#DDDDDD");
    assert_eq!(found(&mut s, CONTRAST).len(), 1);
    set(&mut s, text, json!({"visible": false}));
    assert!(found(&mut s, CONTRAST).is_empty(), "an invisible layer");
    set(&mut s, text, json!({"visible": true}));
    set(&mut s, card, json!({"visible": false}));
    assert!(found(&mut s, CONTRAST).is_empty(), "a layer in an invisible frame");
    set(&mut s, card, json!({"visible": true}));
    set(&mut s, text, json!({"opacity": 0}));
    assert!(found(&mut s, CONTRAST).is_empty(), "fully transparent text shows nothing to read");
    // Asked for by name, a hidden layer is still skipped.
    set(&mut s, text, json!({"opacity": 1, "visible": false}));
    let r = ex(&mut s, "design.check", json!({"rules": [CONTRAST], "scope": [text]}));
    assert_eq!(r["findings"], json!([]));
}

#[test]
fn opacity_and_what_lies_behind_are_composited() {
    let mut s = Session::new();
    // Black text at half opacity on white reads as mid grey.
    let (card, text) = card_with_text(&mut s, "#000000");
    assert!(found(&mut s, CONTRAST).is_empty());
    set(&mut s, text, json!({"opacity": 0.5}));
    assert_eq!(only(&mut s, CONTRAST)["evidence"], "3.94:1, needs 4.5:1; text #808080 on #FFFFFF, 14 px regular");
    // A frame without a fill shows the page colour behind the text.
    set(&mut s, text, json!({"opacity": 1, "fill": "#777777"}));
    set(&mut s, card, json!({"fill": "none"}));
    assert_eq!(only(&mut s, CONTRAST)["evidence"], "4.10:1, needs 4.5:1; text #777777 on #F5F5F5, 14 px regular");
    // A faded frame fades its fill and the text in it together, over the page.
    ex(&mut s, "page.setBackground", json!({"color": "#000000"}));
    set(&mut s, card, json!({"fill": "#808080", "opacity": 0.5}));
    set(&mut s, text, json!({"fill": "#C8C8C8"}));
    let e = only(&mut s, CONTRAST)["evidence"].as_str().unwrap().to_string();
    assert!(e.contains("text #646464 on #404040"), "{e}");
    // A solid box under the text, covering it, is what the text is read against.
    set(&mut s, card, json!({"fill": "#FFFFFF", "opacity": 1}));
    let plate = rect(&mut s, card, [10.0, 10.0, 200.0, 60.0], json!({"name": "Plate", "fill": "#222222"}));
    ex(&mut s, "arrange.back", json!({"ids": [plate]}));
    set(&mut s, text, json!({"fill": "#555555"}));
    let e = only(&mut s, CONTRAST)["evidence"].as_str().unwrap().to_string();
    assert!(e.contains("text #555555 on #222222"), "{e}");
    assert!(
        only(&mut s, CONTRAST)["suggestion"].as_str().unwrap().starts_with("lighten the text to at least #"),
        "on a dark ground the text gets lighter"
    );
    assert!(found(&mut s, UNMEASURED).is_empty());
}

#[test]
fn contrast_that_cant_be_measured_is_reported_as_unmeasured_not_guessed() {
    let mut s = Session::new();
    let (card, text) = card_with_text(&mut s, "#DDDDDD");
    assert!(found(&mut s, UNMEASURED).is_empty());
    // A gradient behind the text: no single colour to measure against.
    set(&mut s, card, json!({"fills": [{"type": "linearGradient", "stops": ["#FFFFFF", "#000000"]}]}));
    assert!(found(&mut s, CONTRAST).is_empty(), "nothing is guessed");
    let f = only(&mut s, UNMEASURED);
    assert_eq!((f["severity"].as_str(), f["nodes"].clone()), (Some("note"), json!([text])));
    assert!(f["evidence"].as_str().unwrap().contains("`Card` has a gradient, image or pattern fill"), "{f}");
    assert!(f["message"].as_str().unwrap().contains("wasn't measured"), "{f}");
    // An opaque panel between the gradient and the text makes it certain again.
    let panel = nid(&ex(&mut s, "frame.create", json!({"x": 10, "y": 10, "width": 200, "height": 100, "parent": card, "name": "Panel"})));
    ex(&mut s, "node.reparent", json!({"ids": [text], "parent": panel}));
    assert!(found(&mut s, UNMEASURED).is_empty());
    assert!(only(&mut s, CONTRAST)["evidence"].as_str().unwrap().contains("on #FFFFFF"));
    // Another layer partly behind the text, or in front of it, is in the way.
    let stripe = rect(&mut s, panel, [0.0, 0.0, 40.0, 100.0], json!({"name": "Stripe", "fill": "#000000"}));
    assert!(only(&mut s, UNMEASURED)["evidence"].as_str().unwrap().contains("`Stripe` is in front of the text"));
    ex(&mut s, "arrange.back", json!({"ids": [stripe]}));
    assert!(only(&mut s, UNMEASURED)["evidence"].as_str().unwrap().contains("`Stripe` is behind the text"));
    assert!(found(&mut s, CONTRAST).is_empty());
    // A blend mode on the text itself.
    ex(&mut s, "edit.delete", json!({"ids": [stripe]}));
    set(&mut s, text, json!({"blend": "multiply"}));
    assert!(only(&mut s, UNMEASURED)["evidence"].as_str().unwrap().contains("blend mode"));
}

/// The size the engine lays `id`'s text out at in its current box.
fn laid_out(s: &Session, id: NodeId) -> (f64, f64) {
    let n = s.doc().unwrap().node(id).unwrap().clone();
    let NodeKind::Text(t) = &n.kind else { panic!("not text") };
    let l = figcraft_text::layout_cached(t, Some(n.width), Some(n.height));
    (l.width, l.height)
}

fn size(s: &Session, id: NodeId) -> (f64, f64) {
    let n = s.doc().unwrap().node(id).unwrap();
    (n.width, n.height)
}

#[test]
fn text_that_doesnt_fit_its_fixed_box_is_reported() {
    let mut s = Session::new();
    let card = frame(&mut s, "Card", 0.0, 600.0, 400.0);
    let text =
        label(&mut s, card, "A headline that is long enough to wrap onto several lines", [20.0, 20.0], json!({"name": "Headline", "fontSize": 20}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "text that grows with its content fits");
    // A fixed box exactly as big as the text needs: it fits.
    let (w, h) = size(&s, text);
    set(&mut s, text, json!({"width": w, "height": h}));
    assert_eq!(s.doc().unwrap().node(text).unwrap().kind.text().unwrap().auto_resize, figcraft_doc::TextAutoResize::None);
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "text that fits exactly");
    // Narrower: it wraps and needs more height than the box has.
    set(&mut s, text, json!({"width": 200, "height": h}));
    let (need_w, need_h) = laid_out(&s, text);
    assert!(need_h > h + 1.0, "the fixture should wrap: needs {need_h}, has {h}");
    let f = only(&mut s, TEXT_CLIPPED);
    assert_eq!((f["severity"].as_str(), f["nodes"].clone()), (Some("major"), json!([text])));
    let px = |v: f64| {
        let r = (v * 100.0).round() / 100.0;
        if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r}") }
    };
    assert_eq!(f["evidence"], format!("the text needs {}×{} px, its box is 200×{} px", px(need_w), px(need_h), px(h)));
    // Tall enough again: fits. Truncating on purpose: not reported either.
    set(&mut s, text, json!({"height": need_h}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty());
    set(&mut s, text, json!({"height": h, "textAutoResize": "truncate"}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "truncation is on purpose");
    set(&mut s, text, json!({"textAutoResize": "none", "visible": false}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "hidden text");
}

#[test]
fn text_cut_by_a_clipping_frame_is_reported_unless_the_frame_scrolls() {
    let mut s = Session::new();
    let card = frame(&mut s, "Card", 0.0, 200.0, 100.0);
    let text = label(&mut s, card, "Total", [20.0, 20.0], json!({"name": "Total"}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "inside the frame");
    let (w, _) = size(&s, text);
    set(&mut s, text, json!({"x": 200.0 - w + 12.0}));
    let f = only(&mut s, TEXT_CLIPPED);
    assert_eq!(f["nodes"], json!([text]));
    assert_eq!(f["evidence"], format!("cut 12 px on the right by `Card` ({card}), which clips its content and doesn't scroll that way"));
    // The viewer can scroll there, or the frame doesn't clip: nothing is lost.
    set(&mut s, card, json!({"overflow": "horizontal"}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "a frame that scrolls that way");
    set(&mut s, card, json!({"overflow": "vertical"}));
    assert_eq!(found(&mut s, TEXT_CLIPPED).len(), 1, "scrolling the other way doesn't reach it");
    set(&mut s, card, json!({"overflow": "none", "clip": false}));
    assert!(found(&mut s, TEXT_CLIPPED).is_empty(), "a frame that doesn't clip");
}

#[test]
fn layers_cut_by_a_clipping_frame_are_reported_with_the_px_cut() {
    let mut s = Session::new();
    let screen = frame(&mut s, "Screen", 0.0, 200.0, 100.0);
    let inside = rect(&mut s, screen, [0.0, 0.0, 200.0, 100.0], json!({"name": "Fits"}));
    assert!(found(&mut s, CLIPPED).is_empty(), "a layer exactly filling the frame is inside it");
    let banner = rect(&mut s, screen, [150.0, 80.0, 100.0, 40.0], json!({"name": "Banner"}));
    let f = only(&mut s, CLIPPED);
    assert_eq!((f["severity"].as_str(), f["nodes"].clone()), (Some("minor"), json!([banner])));
    assert_eq!(
        f["evidence"],
        format!("cut 50 px on the right and 20 px at the bottom by `Screen` ({screen}), which clips its content and doesn't scroll that way")
    );
    // Scrolling down reaches the bottom, not the right.
    set(&mut s, screen, json!({"overflow": "vertical"}));
    assert_eq!(
        only(&mut s, CLIPPED)["evidence"],
        format!("cut 50 px on the right by `Screen` ({screen}), which clips its content and doesn't scroll that way")
    );
    set(&mut s, screen, json!({"overflow": "both"}));
    assert!(found(&mut s, CLIPPED).is_empty());
    set(&mut s, screen, json!({"overflow": "none"}));
    // Half a pixel is rounding.
    set(&mut s, banner, json!({"x": 100.4, "y": 60.4}));
    assert!(found(&mut s, CLIPPED).is_empty());
    // Wholly outside, to the left.
    set(&mut s, banner, json!({"x": -300, "y": 10}));
    assert_eq!(
        only(&mut s, CLIPPED)["evidence"],
        format!("entirely outside `Screen` ({screen}), which clips its content and doesn't scroll that way")
    );
    set(&mut s, banner, json!({"visible": false}));
    assert!(found(&mut s, CLIPPED).is_empty(), "hidden layers are ignored");
    // A group that is cut is reported once, for what is in it.
    set(&mut s, banner, json!({"visible": true, "x": 180, "y": 10}));
    let other = rect(&mut s, screen, [190.0, 60.0, 30.0, 30.0], json!({"name": "Dot"}));
    let group = nid(&ex(&mut s, "object.group", json!({"ids": [banner, other]})));
    let f = only(&mut s, CLIPPED);
    assert_eq!(f["nodes"], json!([group]));
    assert!(f["evidence"].as_str().unwrap().starts_with("cut 80 px on the right"), "{f}");
    let _ = inside;
}

#[test]
fn small_pointer_targets_are_reported_against_wcag_sizes() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let next = frame(&mut s, "Next", 400.0, 320.0, 480.0);
    let close = rect(&mut s, home, [10.0, 10.0, 18.0, 18.0], json!({"name": "Close"}));
    assert!(found(&mut s, TARGET).is_empty(), "a small layer that reacts to nothing isn't a target");
    ex(&mut s, "proto.interaction.add", json!({"id": close, "dest": next}));
    let f = only(&mut s, TARGET);
    assert_eq!((f["severity"].as_str(), f["nodes"].clone()), (Some("major"), json!([close])));
    assert_eq!(f["evidence"], "18×18 px, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)");
    // One dimension under the minimum is enough; exactly 24×24 passes.
    set(&mut s, close, json!({"width": 60, "height": 23}));
    assert_eq!(only(&mut s, TARGET)["evidence"], "60×23 px, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)");
    set(&mut s, close, json!({"width": 24, "height": 24}));
    assert!(found(&mut s, TARGET).is_empty(), "24×24 is the minimum");
    // On a phone, under 44×44 is a note (the enhanced size, level AAA); without a device it isn't.
    ex(&mut s, "proto.settings", json!({"device": "iPhone 16"}));
    let f = only(&mut s, TARGET);
    assert_eq!(f["severity"], "note");
    assert_eq!(f["evidence"], "24×24 px, 44×44 recommended on a phone or tablet (WCAG 2.2 SC 2.5.5 target size, level AAA)");
    set(&mut s, close, json!({"width": 44, "height": 44}));
    assert!(found(&mut s, TARGET).is_empty(), "44×44 meets the enhanced size");
    set(&mut s, close, json!({"width": 30, "height": 30}));
    ex(&mut s, "proto.settings", json!({"device": "Desktop"}));
    assert!(found(&mut s, TARGET).is_empty(), "a desktop isn't held in the hand");
    // A hover alone isn't a target; hidden targets are ignored.
    set(&mut s, close, json!({"width": 10, "height": 10}));
    ex(&mut s, "proto.interaction.clear", json!({"ids": [close]}));
    ex(&mut s, "proto.interaction.add", json!({"id": close, "trigger": {"type": "onHover"}, "action": {"type": "navigate", "dest": next}}));
    assert!(found(&mut s, TARGET).is_empty(), "hovering needs no aim like a click does");
    ex(&mut s, "proto.interaction.add", json!({"id": close, "trigger": {"type": "onPress"}, "action": {"type": "navigate", "dest": next}}));
    assert_eq!(found(&mut s, TARGET).len(), 1);
    set(&mut s, close, json!({"visible": false}));
    assert!(found(&mut s, TARGET).is_empty());
}

#[test]
fn an_instance_is_a_target_through_its_main_components_interaction() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let chip = frame(&mut s, "Chip", 400.0, 20.0, 20.0);
    let main = nid(&ex(&mut s, "component.create", json!({"ids": [chip]})));
    ex(&mut s, "proto.interaction.add", json!({"id": main, "dest": home}));
    let inst = nid(&ex(&mut s, "instance.create", json!({"component": main, "x": 10, "y": 10, "parent": home})));
    assert!(s.doc().unwrap().node(inst).unwrap().interactions.is_empty(), "the instance has no interaction of its own");
    let r = ex(&mut s, "design.check", json!({"rules": [TARGET], "scope": [home]}));
    assert_eq!(r["findings"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["findings"][0]["nodes"], json!([inst]));
    assert_eq!(r["findings"][0]["evidence"], "20×20 px, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)");
}

#[test]
fn context_rules_report_missing_links_an_incomplete_brief_untested_assumptions_and_stale_decisions() {
    let mut s = Session::new();
    let all = |s: &mut Session, rule: &str| found(s, rule);
    // No frame yet: an empty brief isn't worth a finding.
    assert!(all(&mut s, "context.briefIncomplete").is_empty());
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let button = rect(&mut s, home, [10.0, 10.0, 100.0, 40.0], json!({"name": "Button"}));
    let f = only(&mut s, "context.briefIncomplete");
    assert_eq!(
        (f["severity"].as_str(), f["nodes"].clone(), f["evidence"].as_str()),
        (Some("note"), json!([]), Some("empty: audience, job, context, success"))
    );
    ex(&mut s, "design.brief.set", json!({"audience": "Nurses", "job": "Hand over", "success": ["Under a minute"]}));
    assert_eq!(only(&mut s, "context.briefIncomplete")["evidence"], "empty: context");
    ex(&mut s, "design.brief.set", json!({"context": "At the bedside, at night"}));
    assert!(all(&mut s, "context.briefIncomplete").is_empty());

    assert!(all(&mut s, "context.untestedAssumption").is_empty(), "none when there are no assumptions");
    ex(&mut s, "design.assumption.add", json!({"text": "One hand is free", "links": [button]}));
    ex(&mut s, "design.assumption.add", json!({"text": "The ward has wifi", "status": "supported", "evidence": "IT confirmed"}));
    ex(&mut s, "design.assumption.add", json!({"text": "Gloves work on the screen"}));
    let f = only(&mut s, "context.untestedAssumption");
    assert_eq!((f["severity"].as_str(), f["evidence"].as_str()), (Some("note"), Some("2 of 3 untested: A1, A3")));

    // A decision is stale once the brief has moved on; accepting it again brings it up to date.
    ex(&mut s, "design.decision.add", json!({"title": "Bottom actions", "rationale": "Thumb reach", "links": [button]}));
    assert!(all(&mut s, "context.staleDecision").is_empty());
    ex(&mut s, "design.brief.set", json!({"audience": "Nurses and porters"}));
    let f = only(&mut s, "context.staleDecision");
    assert_eq!(f["evidence"], "D1 `Bottom actions`: made at brief revision 2, the brief is at revision 3");
    assert!(f["message"].as_str().unwrap().contains("made under an earlier brief; re-read it"), "{f}");
    assert_eq!((f["severity"].as_str(), f["nodes"].clone()), (Some("note"), json!([])));
    ex(&mut s, "design.decision.accept", json!({"id": "D1"}));
    assert!(all(&mut s, "context.staleDecision").is_empty());

    assert!(all(&mut s, "context.missingLink").is_empty());
    ex(&mut s, "edit.delete", json!({"ids": [button]}));
    let missing = all(&mut s, "context.missingLink");
    let evidence: Vec<&str> = missing.iter().map(|f| f["evidence"].as_str().unwrap()).collect();
    assert_eq!(evidence, [format!("A1 → {button}"), format!("D1 → {button}")]);
    assert!(missing.iter().all(|f| f["severity"] == "minor" && f["nodes"] == json!([])), "{missing:#?}");
}

#[test]
fn the_stage_decides_which_rules_run_and_the_rest_are_listed_as_skipped() {
    let mut s = Session::new();
    card_with_text(&mut s, "#DDDDDD");
    ex(&mut s, "design.assumption.add", json!({"text": "People read this"}));
    let ran = |r: &Value| -> Vec<String> { r["rules"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap().to_string()).collect() };
    let skipped = |r: &Value| -> Vec<String> { r["skipped"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap().to_string()).collect() };
    let converging = [CONTRAST, UNMEASURED, TEXT_CLIPPED, CLIPPED, TARGET];
    let ready = ["context.briefIncomplete", "context.untestedAssumption", "context.staleDecision"];

    // Exploring: only what would break the file.
    let r = ex(&mut s, "design.check", json!({}));
    assert_eq!(r["stage"], "exploring");
    assert_eq!(ran(&r), ["context.missingLink"]);
    assert_eq!(skipped(&r), [&converging[..], &ready[..]].concat());
    assert_eq!(r["findings"], json!([]), "the low contrast isn't reported while exploring");
    let why = r["skipped"][0]["reason"].as_str().unwrap();
    assert!(why.contains("runs from stage `converging`") && why.contains("the file is at `exploring`"), "{why}");
    assert!(r["skipped"][7]["reason"].as_str().unwrap().contains("runs from stage `ready`"));

    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let r = ex(&mut s, "design.check", json!({}));
    assert_eq!(ran(&r), [&[CONTRAST, UNMEASURED, TEXT_CLIPPED, CLIPPED, TARGET, "context.missingLink"][..]].concat());
    assert_eq!(skipped(&r), ready);
    assert_eq!(r["summary"]["byRule"], json!({CONTRAST: 1}));

    ex(&mut s, "design.stage.set", json!({"stage": "ready"}));
    let r = ex(&mut s, "design.check", json!({}));
    assert_eq!(ran(&r).len(), 9);
    assert_eq!(r["skipped"], json!([]));
    assert_eq!(r["summary"]["byRule"], json!({CONTRAST: 1, "context.briefIncomplete": 1, "context.untestedAssumption": 1}));
    assert_eq!(r["summary"]["bySeverity"], json!({"blocker": 0, "major": 1, "minor": 0, "note": 2}));
    assert_eq!(r["summary"]["total"], 3);

    // Rules asked for by name run whatever the stage, and exactly those.
    ex(&mut s, "design.stage.set", json!({"stage": "exploring"}));
    let r = ex(&mut s, "design.check", json!({"rules": [CONTRAST, "context.untestedAssumption"]}));
    assert_eq!(ran(&r), [CONTRAST, "context.untestedAssumption"]);
    assert_eq!(skipped(&r).len(), 7);
    assert!(r["skipped"][0]["reason"].as_str().unwrap().contains("not among the `rules` asked for"));
    assert_eq!(r["summary"]["total"], 2);

    let e = err(&mut s, "design.check", json!({"rules": ["contrast.txt"]}));
    assert!(e.contains("`rules`: no rule \"contrast.txt\"") && e.contains("contrast.text, contrast.unmeasured, text.clipped"), "{e}");
    assert!(err(&mut s, "design.check", json!({"rules": "contrast.text"})).contains("`rules` must be a list"));
    assert!(err(&mut s, "design.check", json!({"scope": [424242]})).contains("`scope`: no layer 424242"));
}

#[test]
fn the_rule_list_says_what_each_measures_and_from_which_stage() {
    let mut s = Session::new();
    let rules = ex(&mut s, "design.check.rules", json!({}));
    let ids: Vec<&str> = rules.as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            CONTRAST,
            UNMEASURED,
            TEXT_CLIPPED,
            CLIPPED,
            TARGET,
            "context.missingLink",
            "context.briefIncomplete",
            "context.untestedAssumption",
            "context.staleDecision"
        ]
    );
    let about = |id: &str| rules.as_array().unwrap().iter().find(|r| r["id"] == id).unwrap()["about"].as_str().unwrap().to_string();
    let contrast = about(CONTRAST);
    assert!(
        contrast.contains("WCAG 2.2 SC 1.4.3")
            && contrast.contains("4.5:1")
            && contrast.contains("3:1")
            && contrast.contains("24 px")
            && contrast.contains("18.66 px")
    );
    let target = about(TARGET);
    assert!(
        target.contains("SC 2.5.8") && target.contains("24×24") && target.contains("SC 2.5.5") && target.contains("44×44") && target.contains("AAA")
    );
    assert!(rules.as_array().unwrap().iter().all(|r| r["about"].as_str().is_some_and(|a| a.len() > 40)));
    // The profile is data: unknown rules (another module's) run from `converging`.
    assert_eq!(crate::design::min_stage("flow.brokenTarget"), Stage::Exploring);
    assert_eq!(crate::design::min_stage("context.missingLink"), Stage::Exploring);
    assert_eq!(crate::design::min_stage("flow.deadEnd"), Stage::Converging);
    assert_eq!(crate::design::min_stage(CONTRAST), Stage::Converging);
    assert_eq!(crate::design::min_stage("context.staleDecision"), Stage::Ready);
    assert_eq!(rules[0]["stage"], "converging");
    assert_eq!(rules[8]["stage"], "ready");
    // The retired rules are gone: their thresholds had no standard behind them.
    assert!(!ids.contains(&"text.small") && !ids.contains(&"type.manySizes"));
    assert_eq!(crate::design::rules().len(), 9);
}

/// A page with one finding of each of three severities and rules.
fn mixed_page(s: &mut Session) -> (NodeId, NodeId, NodeId) {
    let (card, text) = card_with_text(s, "#DDDDDD");
    let banner = rect(s, card, [300.0, 150.0, 100.0, 40.0], json!({"name": "Banner"}));
    let dot = rect(s, card, [10.0, 100.0, 12.0, 12.0], json!({"name": "Dot"}));
    ex(s, "proto.interaction.add", json!({"id": dot, "dest": card}));
    ex(s, "design.stage.set", json!({"stage": "ready"}));
    (text, banner, dot)
}

#[test]
fn checks_are_deterministic_and_ordered_by_severity_rule_and_layer() {
    let mut s = Session::new();
    let (text, banner, dot) = mixed_page(&mut s);
    let card = s.doc().unwrap().node(text).unwrap().parent.unwrap();
    let other = label(&mut s, card, "Also faint", [20.0, 60.0], json!({"fill": "#CCCCCC"}));
    let first = ex(&mut s, "design.check", json!({}));
    let second = ex(&mut s, "design.check", json!({}));
    assert_eq!(first, second, "the same file gives the same result");
    let order: Vec<(String, Value)> =
        first["findings"].as_array().unwrap().iter().map(|f| (f["rule"].as_str().unwrap().to_string(), f["nodes"].clone())).collect();
    assert_eq!(
        order,
        [
            (CONTRAST.to_string(), json!([text])),
            (CONTRAST.to_string(), json!([other])),
            (TARGET.to_string(), json!([dot])),
            (CLIPPED.to_string(), json!([banner])),
            ("context.briefIncomplete".to_string(), json!([])),
        ]
    );
    // A check reads the file: no undo step, nothing stored.
    assert!(s.doc().unwrap().design.findings.is_empty());
    // A scope limits what is looked at; a layer inside a scoped frame isn't reported twice.
    let scoped = ex(&mut s, "design.check", json!({"scope": [other, text, other], "rules": [CONTRAST, TARGET]}));
    assert_eq!(scoped["findings"].as_array().unwrap().len(), 2, "{scoped}");
}

#[test]
fn recording_stores_each_problem_once() {
    let mut s = Session::new();
    let (text, banner, dot) = mixed_page(&mut s);
    let r = ex_undoable(&mut s, "design.check.record", json!({}));
    assert_eq!(r["added"], json!(["F1", "F2", "F3", "F4"]));
    assert_eq!((r["existing"].clone(), r["total"].as_u64(), r["skipped"].clone()), (json!([]), Some(4), json!([])));
    let stored = s.doc().unwrap().design.findings.clone();
    assert!(stored.iter().all(|f| f.kind == figcraft_doc::design::FindingKind::Check && f.source == "check" && f.status == FindingStatus::Open));
    assert_eq!(
        stored.iter().map(|f| (f.rule.as_str(), f.nodes.clone())).collect::<Vec<_>>(),
        [(CONTRAST, vec![text]), (TARGET, vec![dot]), (CLIPPED, vec![banner]), ("context.briefIncomplete", vec![]),]
    );
    assert_eq!(stored[0].evidence, "1.35:1, needs 4.5:1; text #DDDDDD on #FFFFFF, 14 px regular");
    // Again: the same problems are already stored, whatever their status short of verified.
    let depth = s.undo_depth();
    ex(&mut s, "design.finding.address", json!({"id": "F2", "resolution": "Will enlarge"}));
    ex(&mut s, "design.finding.accept", json!({"id": "F3", "reason": "Bleeds off on purpose"}));
    let r = ex(&mut s, "design.check.record", json!({}));
    assert_eq!((r["added"].clone(), r["existing"].clone(), r["total"].as_u64()), (json!([]), json!(["F1", "F2", "F3", "F4"]), Some(4)));
    assert_eq!(s.undo_depth().0, depth.0 + 2, "recording nothing new is no undo step");
    // A new problem is added next to them; the stage and `rules` apply as in design.check.
    let parent = s.doc().unwrap().node(text).unwrap().parent.unwrap();
    rect(&mut s, parent, [-50.0, 10.0, 100.0, 20.0], json!({"name": "Ribbon"}));
    let r = ex(&mut s, "design.check.record", json!({"rules": [CLIPPED]}));
    assert_eq!((r["added"].clone(), r["existing"].clone()), (json!(["F5"]), json!(["F3"])));
    assert_eq!(r["skipped"].as_array().unwrap().len(), 8);
    assert!(err(&mut s, "design.check.record", json!({"rules": ["nope"]})).contains("`rules`: no rule"));
}

#[test]
fn a_fixed_check_is_verified_by_running_it_again() {
    let mut s = Session::new();
    let (text, banner, dot) = mixed_page(&mut s);
    ex(&mut s, "design.check.record", json!({}));
    // Fix the contrast and re-run that finding.
    set(&mut s, text, json!({"fill": "#333333"}));
    let r = ex_undoable(&mut s, "design.finding.recheck", json!({"id": "F1"}));
    assert_eq!(r["results"], json!([{"id": "F1", "passed": true, "detail": "`contrast.text` no longer reports it", "status": "verified"}]));
    let f = s.doc().unwrap().design.findings[0].clone();
    assert_eq!((f.status, f.verified_by.as_str()), (FindingStatus::Verified, ""));
    assert_eq!(f.recheck.map(|r| (r.passed, r.detail)), Some((true, "`contrast.text` no longer reports it".to_string())));

    // Marked addressed without a fix: the re-run says it is still there and opens it again.
    ex(&mut s, "design.finding.address", json!({"id": "F2", "resolution": "Made the dot bigger"}));
    let r = ex_undoable(&mut s, "design.finding.recheck", json!({"id": "F2"}));
    assert_eq!(
        r["results"],
        json!([{"id": "F2", "passed": false, "detail": "12×12 px, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)", "status": "open"}])
    );
    assert_eq!(s.doc().unwrap().design.findings[1].status, FindingStatus::Open);
    assert_eq!(s.doc().unwrap().design.findings[1].resolution, "Made the dot bigger", "what was claimed stays on record");

    // Really fixed, partly: the evidence of the re-run is what is measured now.
    set(&mut s, dot, json!({"width": 30, "height": 20}));
    let r = ex(&mut s, "design.finding.recheck", json!({"id": "F2"}));
    assert_eq!(r["results"][0]["detail"], "30×20 px, minimum 24×24 (WCAG 2.2 SC 2.5.8 target size)");
    set(&mut s, dot, json!({"height": 30}));

    // Without an id: every stored check that isn't accepted. The layer of F3 is deleted.
    ex(&mut s, "edit.delete", json!({"ids": [banner]}));
    let r = ex_undoable(&mut s, "design.finding.recheck", json!({}));
    assert_eq!(
        r["results"],
        json!([
            {"id": "F1", "passed": true, "detail": "`contrast.text` no longer reports it", "status": "verified"},
            {"id": "F2", "passed": true, "detail": "`target.small` no longer reports it", "status": "verified"},
            {"id": "F3", "passed": true, "detail": "the layer no longer exists", "status": "verified"},
            {"id": "F4", "passed": false, "detail": "empty: audience, job, context, success", "status": "open"},
        ])
    );
    // A verified finding whose problem comes back is opened by the next re-run...
    set(&mut s, text, json!({"fill": "#DDDDDD"}));
    let r = ex(&mut s, "design.finding.recheck", json!({"id": "F1"}));
    assert_eq!((r["results"][0]["passed"].as_bool(), r["results"][0]["status"].as_str()), (Some(false), Some("open")));
    // ...and one that stayed verified is recorded anew when it is found again.
    set(&mut s, dot, json!({"width": 10}));
    let r = ex(&mut s, "design.check.record", json!({"rules": [TARGET, CONTRAST]}));
    assert_eq!((r["added"].clone(), r["existing"].clone()), (json!(["F5"]), json!(["F1"])));
    // A settled finding about a deleted layer isn't a broken link.
    assert!(found(&mut s, "context.missingLink").is_empty());
}

#[test]
fn an_accepted_check_stays_accepted_and_is_left_out_of_recheck_all() {
    let mut s = Session::new();
    mixed_page(&mut s);
    ex(&mut s, "design.check.record", json!({}));
    ex(&mut s, "design.finding.accept", json!({"id": "F3", "reason": "The banner bleeds off the edge on purpose"}));
    let r = ex(&mut s, "design.finding.recheck", json!({}));
    let ids: Vec<&str> = r["results"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["F1", "F2", "F4"]);
    // Asked for by id it is measured, and stays as the person left it while it is still there.
    let r = ex(&mut s, "design.finding.recheck", json!({"id": "F3"}));
    assert_eq!((r["results"][0]["passed"].as_bool(), r["results"][0]["status"].as_str()), (Some(false), Some("accepted")));
    // A context finding is matched by what it is about: fill the brief and it is verified.
    ex(&mut s, "design.brief.set", json!({"audience": "a", "job": "b", "context": "c", "success": ["d"]}));
    let r = ex(&mut s, "design.finding.recheck", json!({"id": "F4"}));
    assert_eq!(r["results"][0]["status"], "verified");
}

#[test]
fn missing_links_are_recorded_one_finding_each_and_rechecked_each() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let a = rect(&mut s, home, [10.0, 10.0, 50.0, 50.0], json!({}));
    let b = rect(&mut s, home, [80.0, 10.0, 50.0, 50.0], json!({}));
    ex(&mut s, "design.assumption.add", json!({"text": "First", "links": [a]}));
    ex(&mut s, "design.assumption.add", json!({"text": "Second", "links": [b]}));
    ex(&mut s, "edit.delete", json!({"ids": [a, b]}));
    // The default profile of a file that is still exploring runs this rule.
    let r = ex(&mut s, "design.check.record", json!({}));
    assert_eq!(r["added"], json!(["F1", "F2"]), "two links, two findings although neither names a layer");
    ex(&mut s, "design.assumption.set", json!({"id": "A1", "links": []}));
    let r = ex(&mut s, "design.finding.recheck", json!({}));
    assert_eq!(r["results"][0]["status"], "verified");
    assert_eq!((r["results"][1]["status"].as_str(), r["results"][1]["detail"].as_str()), (Some("open"), Some(format!("A2 → {b}").as_str())));
}

#[test]
fn recorded_checks_survive_save_and_reopen() {
    let mut s = Session::new();
    let (text, ..) = mixed_page(&mut s);
    ex(&mut s, "design.check.record", json!({}));
    set(&mut s, text, json!({"fill": "#000000"}));
    ex(&mut s, "design.finding.recheck", json!({}));
    let back = reopened(&s);
    assert_eq!(back.design.findings, s.doc().unwrap().design.findings);
    assert_eq!(back.design.findings[0].recheck.as_ref().map(|r| r.passed), Some(true));
    assert_eq!(back.design.stage, Stage::Ready);
}
