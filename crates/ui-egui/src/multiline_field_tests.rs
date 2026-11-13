//! A multiline field commits what was typed when the keyboard leaves it, and never later: a draft
//! left behind when the field was hidden while it had the keyboard is dropped, not committed over
//! what the file holds when the field shows again.

use egui::{Context, Event, Id, RawInput};

use crate::widgets::multiline_field;

/// One pass drawing the field (when `shown`) with `value`; what it committed.
fn pass(ctx: &Context, shown: bool, value: &str, events: Vec<Event>) -> Option<String> {
    let mut out = None;
    let input = RawInput { events, ..Default::default() };
    let mut full = ctx.run_ui(input, |ui| {
        if shown {
            out = multiline_field(ui, Id::unique("reason"), value, "", 1);
        }
    });
    full.textures_delta.clear();
    out
}

fn editor() -> Id {
    Id::unique("reason").with("te")
}

#[test]
fn a_draft_left_by_a_hidden_field_is_not_committed_later() {
    let ctx = Context::default();
    pass(&ctx, true, "Sent", Vec::new());
    ctx.memory_mut(|m| m.request_focus(editor()));
    pass(&ctx, true, "Sent", Vec::new());
    assert_eq!(pass(&ctx, true, "Sent", vec![Event::Text(" for good".into())]), None, "typing commits nothing yet");
    // The field goes out of sight while it has the keyboard (another section is shown).
    for _ in 0..4 {
        pass(&ctx, false, "Sent", Vec::new());
    }
    // Meanwhile the value changed elsewhere; the field shows again: the old draft isn't written.
    assert_eq!(pass(&ctx, true, "Sent by the agent", Vec::new()), None);
    assert_eq!(pass(&ctx, true, "Sent by the agent", Vec::new()), None);
}

#[test]
fn what_was_typed_is_committed_when_the_keyboard_leaves() {
    let ctx = Context::default();
    pass(&ctx, true, "Sent", Vec::new());
    ctx.memory_mut(|m| m.request_focus(editor()));
    pass(&ctx, true, "Sent", Vec::new());
    pass(&ctx, true, "Sent", vec![Event::Text("!".into())]);
    // The keyboard moves on before the field is drawn (Tab to the next field).
    ctx.memory_mut(|m| m.surrender_focus(editor()));
    let committed = pass(&ctx, true, "Sent", Vec::new());
    assert!(committed.as_deref().is_some_and(|t| t.contains('!')), "{committed:?}");
}
