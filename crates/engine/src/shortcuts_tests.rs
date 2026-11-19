//! Shortcuts for applying variable bindings, which a prototype's run does after nearly every action
//! (performance budget PF2), answer as what they replace: `ModesCache` as `Document::modes_for` on
//! every layer, and a bound colour taken as in place as the copy it saves.

use figcraft_doc::{Document, ModesCache, NodeId};
use serde_json::json;

use crate::Session;
use crate::synthetic_ds::{DsSpec, design_system_with_instances};

/// A design system with light and dark screens and instances nested inside instances.
fn themed() -> Document {
    design_system_with_instances(&DsSpec::SMALL, 40).unwrap().0
}

#[test]
fn the_modes_cache_answers_as_modes_for_on_every_layer() {
    let doc = themed();
    let ids: Vec<NodeId> = doc.ids().collect();
    let mut cache = ModesCache::default();
    // In id order, then the other way round (other chains are worked out first), then again from
    // what is cached.
    for id in ids.iter().chain(ids.iter().rev()).chain(ids.iter()) {
        assert_eq!(cache.modes_for(&doc, *id), doc.modes_for(*id), "layer {id}");
    }
    let themed = ids.iter().filter(|i| !doc.modes_for(**i).is_empty()).count();
    assert!(themed > ids.len() / 4, "{themed} of {} layers are in a mode", ids.len());
    // A layer that isn't there has no modes.
    assert!(cache.modes_for(&doc, NodeId(u64::MAX)).is_empty());
}

#[test]
fn a_loop_of_parents_is_answered_as_modes_for_answers_it() {
    let mut s = Session::new();
    let outer = NodeId(s.execute("node.create", &json!({"type": "frame", "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap());
    let inner =
        NodeId(s.execute("node.create", &json!({"type": "frame", "parent": outer.0, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap());
    let leaf =
        NodeId(s.execute("node.create", &json!({"type": "rectangle", "parent": inner.0, "width": 5, "height": 5})).unwrap()["id"].as_u64().unwrap());
    let doc = s.doc_mut().unwrap();
    doc.node_mut(outer).unwrap().var_modes.insert("c:1".into(), "m:1".into());
    doc.node_mut(inner).unwrap().var_modes.insert("c:2".into(), "m:2".into());
    let mut cache = ModesCache::default();
    for id in [leaf, inner, outer] {
        assert_eq!(cache.modes_for(doc, id), doc.modes_for(id));
    }
    assert_eq!(cache.modes_for(doc, leaf).len(), 2);
    // A broken file: the outer frame inside the leaf.
    let mut broken = doc.clone();
    broken.node_mut(outer).unwrap().parent = Some(leaf);
    let mut cache = ModesCache::default();
    for id in [leaf, inner, outer] {
        assert_eq!(cache.modes_for(&broken, id), broken.modes_for(id), "layer {id}");
    }
}

#[test]
fn a_bound_colour_is_taken_as_in_place_only_where_setting_it_changes_nothing() {
    use crate::cmd::styles::{paint_color_in_place, set_paint_color};
    use figcraft_color::{Color, Gradient, GradientKind, Paint, PaintKind};
    let blue = Color::from_rgb8_const(0x33, 0x66, 0xFF);
    let red = Color::from_rgb8_const(0xFF, 0, 0);
    let mut faded = Paint::solid(blue);
    faded.opacity = 0.5;
    let mut gradient = Paint::solid(blue);
    gradient.kind = PaintKind::Gradient(Gradient::default_for(GradientKind::Linear, blue));
    let nan = Color { r: f32::NAN, ..blue };
    let lists: Vec<Vec<Paint>> = vec![vec![], vec![Paint::solid(blue)], vec![faded, Paint::solid(red)], vec![gradient], vec![Paint::solid(nan)]];
    let mut taken = 0;
    for list in &lists {
        for i in 0..3 {
            for stop in [None, Some(0), Some(1), Some(2)] {
                for c in [blue, red, nan] {
                    let mut set = list.clone();
                    set_paint_color(&mut set, i, stop, c);
                    // Exactly where the copy it saves would have compared equal.
                    assert_eq!(paint_color_in_place(list, i, stop, c), set == *list, "{list:?} {i} {stop:?} {c:?}");
                    taken += usize::from(set == *list);
                }
            }
        }
    }
    assert!(taken > 20, "{taken}");
}
