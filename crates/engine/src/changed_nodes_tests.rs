//! `changed_nodes`, which every command an AI agent runs asks (where it works, `sharing::agent`;
//! performance budget PF1), answers as what it replaced: a lookup of every layer in the version
//! before.

use std::sync::Arc;

use figcraft_doc::{Document, NodeId};
use serde_json::json;

use crate::Session;
use crate::synthetic_ds::{DsSpec, design_system_with_instances};

/// A design system with light and dark screens and instances nested inside instances.
fn themed() -> Document {
    design_system_with_instances(&DsSpec::SMALL, 40).unwrap().0
}

/// What `changed_nodes` replaced: every layer of `doc` looked up in `before`.
fn looked_up(doc: &Document, before: &Document) -> Vec<NodeId> {
    doc.ids()
        .filter(|id| match (doc.node_arc(*id), before.node_arc(*id)) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        })
        .collect()
}

#[test]
fn changed_nodes_are_the_layers_whose_copy_changed() {
    let mut s = Session::with_document(themed());
    let before = s.doc().unwrap().clone();
    let ids: Vec<NodeId> = before.ids().collect();
    assert_eq!(crate::changed_nodes(&before, &before), Vec::<NodeId>::new());
    s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 4, "height": 4})).unwrap();
    let doc = s.doc_mut().unwrap();
    // Edited, removed, and replaced by an equal copy (a new `Arc` with the same content).
    doc.node_mut(ids[ids.len() / 2]).unwrap().name = "Edited".into();
    let last = *ids.last().unwrap();
    let _ = doc.remove(last);
    let _ = doc.node_mut(ids[3]);
    let doc = s.doc().unwrap().clone();
    let changed = crate::changed_nodes(&doc, &before);
    assert_eq!(changed, looked_up(&doc, &before));
    assert!(changed.len() >= 3, "{changed:?}");
    // And the other way round (what was removed is what is new).
    assert_eq!(crate::changed_nodes(&before, &doc), looked_up(&before, &doc));
    assert_eq!(crate::changed_nodes(&before, &Document::default()), before.ids().collect::<Vec<_>>());
}
