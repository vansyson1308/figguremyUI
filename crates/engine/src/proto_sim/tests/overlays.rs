//! Overlays: opening, stacking, swapping, closing, and what takes input while one is open.

use figcraft_doc::NodeId;
use figcraft_doc::design::StepAction;
use serde_json::{Value, json};

use super::{after_delay, button, click, frame, navigate, open, problem, sim};
use crate::Session;

fn overlay(dest: NodeId, close_on_outside: bool) -> Value {
    json!({"type": "openOverlay", "dest": dest, "closeOnOutside": close_on_outside})
}

/// Screens A and B, overlays Menu and Dialog, and on A a button "Open" that opens Menu
/// (`close_on_outside`).
fn setup(close_on_outside: bool) -> (Session, [NodeId; 4], NodeId) {
    let mut s = Session::new();
    let frames = [frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0), frame(&mut s, "Menu", 800.0), frame(&mut s, "Dialog", 1200.0)];
    let opener = button(&mut s, "Open", frames[0], 20.0, overlay(frames[2], close_on_outside));
    (s, frames, opener)
}

#[test]
fn an_overlay_takes_the_input_until_it_closes() {
    let (mut s, [a, _, menu, _], opener) = setup(true);
    let close = button(&mut s, "Close", menu, 20.0, json!({"type": "closeOverlay"}));
    let mut p = sim(&s, a);
    let o = click(&mut p, opener);
    assert!(o.fired && o.effects.iter().any(|e| e.contains("open overlay `Menu`")), "{o:?}");
    assert_eq!((p.screen(), open(&p), p.active_root()), (a, vec![menu], menu));
    // The screen underneath doesn't take clicks while the overlay is open.
    let o = click(&mut p, opener);
    assert!(!o.fired && problem(&o).contains("isn't on `Menu`") && problem(&o).contains("under the open overlay"), "{o:?}");
    assert_eq!(open(&p), vec![menu], "and nothing changed");
    let o = click(&mut p, close);
    assert!(o.fired, "{o:?}");
    assert_eq!((open(&p), p.active_root()), (vec![], a));
    assert!(p.history().is_empty(), "overlays aren't screens: nothing to go back to");
    // Closing with none open does nothing.
    let mut p = sim(&s, menu);
    let o = click(&mut p, close);
    assert!(!o.fired && problem(&o).contains("no overlay is open"), "{o:?}");
}

#[test]
fn overlays_stack_and_back_closes_the_top_one_first() {
    let (mut s, [a, b, menu, dialog], _) = setup(true);
    let to_b = button(&mut s, "To B", a, 80.0, navigate(b));
    let deeper = button(&mut s, "More", menu, 20.0, overlay(dialog, true));
    let back = button(&mut s, "Back", dialog, 20.0, json!({"type": "back"}));
    let menu_back = button(&mut s, "Back", menu, 80.0, json!({"type": "back"}));
    let b_open = button(&mut s, "Open", b, 20.0, overlay(menu, true));
    let mut p = sim(&s, a);
    click(&mut p, to_b);
    click(&mut p, b_open);
    click(&mut p, deeper);
    assert_eq!((p.screen(), open(&p)), (b, vec![menu, dialog]), "overlays stack, the newest on top");
    let o = click(&mut p, deeper);
    assert!(problem(&o).contains("isn't on `Dialog`"), "only the top overlay takes input: {o:?}");
    click(&mut p, back);
    assert_eq!((p.screen(), open(&p), p.history()), (b, vec![menu], &[a][..]), "back closes the top overlay, not the screen");
    click(&mut p, menu_back);
    assert_eq!((p.screen(), open(&p)), (b, vec![]));
}

#[test]
fn swap_replaces_the_top_overlay_and_keeps_how_it_closes() {
    let (mut s, [a, _, menu, dialog], opener) = setup(false);
    let swap = button(&mut s, "Swap", menu, 20.0, json!({"type": "swapOverlay", "dest": dialog}));
    let direct = button(&mut s, "Direct", a, 80.0, json!({"type": "swapOverlay", "dest": dialog}));
    let mut p = sim(&s, a);
    click(&mut p, opener);
    let o = click(&mut p, swap);
    assert!(o.fired, "{o:?}");
    assert_eq!(open(&p), vec![dialog], "one overlay, the new one");
    let o = p.step(&StepAction::ClickOutside);
    assert!(!o.fired && open(&p) == vec![dialog], "it stays as the overlay it replaced was opened: {o:?}");
    // With no overlay open, swap opens one that closes on a click outside.
    let mut p = sim(&s, a);
    click(&mut p, direct);
    assert_eq!(open(&p), vec![dialog]);
    assert!(p.step(&StepAction::ClickOutside).fired);
    assert!(open(&p).is_empty());
}

#[test]
fn navigating_from_an_overlay_closes_every_overlay() {
    let (mut s, [a, b, menu, dialog], opener) = setup(true);
    let deeper = button(&mut s, "More", menu, 20.0, overlay(dialog, true));
    let go = button(&mut s, "Go", dialog, 20.0, navigate(b));
    let home = button(&mut s, "Home", menu, 80.0, navigate(a));
    let mut p = sim(&s, a);
    click(&mut p, opener);
    click(&mut p, deeper);
    click(&mut p, go);
    assert_eq!((p.screen(), open(&p), p.history()), (b, vec![], &[a][..]));
    // To the screen under the overlay too: the overlays close, and the screen is remembered as
    // any screen navigated away from is (the player's rule).
    let mut p = sim(&s, a);
    click(&mut p, opener);
    let o = click(&mut p, home);
    assert!(o.fired, "{o:?}");
    assert_eq!((p.screen(), open(&p), p.history()), (a, vec![], &[a][..]));
}

#[test]
fn a_click_outside_closes_only_an_overlay_that_closes_that_way() {
    for close_on_outside in [true, false] {
        let (s, [a, _, menu, _], opener) = setup(close_on_outside);
        let mut p = sim(&s, a);
        let o = p.step(&StepAction::ClickOutside);
        assert!(!o.fired && o.problem.is_none(), "no overlay: nothing to close, and no mistake either: {o:?}");
        click(&mut p, opener);
        let o = p.step(&StepAction::ClickOutside);
        assert_eq!((o.fired, o.problem.clone()), (close_on_outside, None), "{o:?}");
        assert_eq!(open(&p), if close_on_outside { vec![] } else { vec![menu] });
    }
}

#[test]
fn an_overlays_timer_starts_each_time_it_opens() {
    let (mut s, [a, _, menu, _], opener) = setup(true);
    after_delay(&mut s, menu, 500, json!({"type": "closeOverlay"}));
    let mut p = sim(&s, a);
    for round in 0..2 {
        click(&mut p, opener);
        assert_eq!(open(&p), vec![menu]);
        let o = p.step(&StepAction::Wait);
        assert!(o.fired && open(&p).is_empty(), "round {round}: {o:?}");
    }
}

#[test]
fn keys_go_to_the_top_overlay_only() {
    let (mut s, [a, b, menu, _], opener) = setup(true);
    let key = json!({"id": a, "trigger": {"type": "keyPress", "key": "Enter"}, "action": navigate(b)});
    s.execute("proto.interaction.add", &key).unwrap();
    let enter = StepAction::Key { key: "Enter".into() };
    let mut p = sim(&s, a);
    click(&mut p, opener);
    let o = p.step(&enter);
    assert!(!o.fired && problem(&o).contains("nothing reacts to key `Enter` on `Menu`"), "{o:?}");
    assert_eq!((p.screen(), open(&p)), (a, vec![menu]));
    p.step(&StepAction::ClickOutside);
    assert!(p.step(&enter).fired);
    assert_eq!(p.screen(), b);
}
