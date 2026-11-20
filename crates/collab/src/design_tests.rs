//! The design context in a live session. It syncs as one value (`DocProp::Design`): a change by
//! one client reaches the others whole, and of two changes made at the same time the one the
//! server takes last replaces the other (entries don't merge; known limit). A context a peer
//! sends is held to the limits the commands keep.

use std::sync::Arc;

use figcraft_doc::Document;
use figcraft_doc::design::{Assumption, Decision, DesignContext, MAX_ENTRIES, MAX_TEXT};

use crate::change::{Change, DocProp, Json};
use crate::replica::{Mode, Replica};
use crate::sim::Sim;
use crate::sim_tests::base;
use crate::{Error, NoRecompute, diff};

/// Client `i`'s document with its design context changed by `f`.
fn edited(sim: &Sim, i: usize, f: impl FnOnce(&mut DesignContext)) -> Document {
    let mut doc = sim.document(i).unwrap().clone();
    f(Arc::make_mut(&mut doc.design));
    doc
}

fn assumption(id: &str, text: &str) -> Assumption {
    Assumption { id: id.into(), text: text.into(), ..Assumption::default() }
}

#[test]
fn a_design_context_change_syncs_to_a_second_client() {
    let mut sim = Sim::new(base(), 2, 7, 3).unwrap();
    let doc = edited(&sim, 0, |ctx| {
        ctx.brief.audience = "Nurses on a night shift".into();
        ctx.assumptions.push(assumption("A1", "One hand is free"));
        ctx.counters.insert("A".into(), 1);
    });
    assert!(sim.edit(0, &doc).unwrap(), "the change goes out");
    sim.settle(&NoRecompute, 1000).unwrap();
    sim.converged().unwrap();
    let other = sim.document(1).unwrap();
    assert_eq!(other.design, doc.design);
    assert_eq!(other.design.brief.audience, "Nurses on a night shift");
    // It travels as one whole value.
    let (before, after) = (Replica::new(base()), Replica::new(doc));
    let d = diff(&before, &after.document().clone()).unwrap();
    assert!(matches!(d.changes.as_slice(), [Change::SetDoc { prop: DocProp::Design, .. }]), "{:?}", d.changes);
}

#[test]
fn of_two_concurrent_design_context_edits_the_last_writer_wins_and_the_other_is_lost() {
    let mut sim = Sim::new(base(), 2, 11, 3).unwrap();
    // Both edit before either hears of the other: one adds an assumption, one a decision.
    let first = edited(&sim, 0, |ctx| ctx.assumptions.push(assumption("A1", "One hand is free")));
    let second = edited(&sim, 1, |ctx| {
        ctx.decisions.push(Decision { id: "D1".into(), title: "Bottom actions".into(), rationale: "Thumb".into(), ..Decision::default() })
    });
    sim.edit(0, &first).unwrap();
    sim.edit(1, &second).unwrap();
    sim.settle(&NoRecompute, 1000).unwrap();
    sim.converged().unwrap();
    // Everyone agrees, on one side's context whole: the entries of the two were not merged.
    let (a, b) = (sim.document(0).unwrap().design.clone(), sim.document(1).unwrap().design.clone());
    assert_eq!(a, b);
    let kept = (a.assumptions.len(), a.decisions.len());
    assert!(kept == (1, 0) || kept == (0, 1), "one writer's context replaced the other's: {kept:?}");
}

#[test]
fn a_peers_design_context_past_the_limits_is_refused() {
    let host = Replica::new(base());
    let apply = |ctx: &DesignContext| {
        let mut r = host.clone();
        let change = Change::SetDoc { prop: DocProp::Design, value: Json::of(ctx).unwrap() };
        r.apply(&[change], Mode::Strict).map(|_| r.document().design.clone())
    };
    // At the limits: taken.
    let ctx = DesignContext {
        assumptions: (0..MAX_ENTRIES).map(|k| assumption(&format!("A{k}"), &"é".repeat(MAX_TEXT))).collect(),
        ..DesignContext::default()
    };
    assert_eq!(apply(&ctx).unwrap().assumptions.len(), MAX_ENTRIES);
    // One entry more, or one character more: refused whole, the document unchanged.
    let mut many = ctx.clone();
    many.assumptions.push(assumption("A-more", "x"));
    let mut long = DesignContext::default();
    long.brief.job = "é".repeat(MAX_TEXT + 1);
    for (over, names) in [(many, "`assumptions` from 501 to 500 entries"), (long, "1 text(s) to 4000 characters")] {
        match apply(&over) {
            Err(Error::Invalid(why)) => assert!(why.starts_with("design: larger than a file may hold") && why.contains(names), "{why}"),
            other => panic!("should be refused as invalid: {other:?}"),
        }
    }
    // A value that isn't a context is invalid too (and never reaches the document).
    let mut r = host.clone();
    let junk = Change::SetDoc { prop: DocProp::Design, value: Json::of(&serde_json::json!({"stage": "shipping"})).unwrap() };
    assert!(matches!(r.apply(&[junk], Mode::Strict), Err(Error::Invalid(_))));
}
