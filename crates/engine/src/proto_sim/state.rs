//! The simulator's state as a report: what shows, and what the prototype changed in its copy of
//! the file.

use std::collections::BTreeMap;

use figcraft_doc::design::EndStateStatus;
use figcraft_doc::{Document, NodeId, NodeKind};
use serde::Serialize;
use serde_json::Value;

use super::Sim;
use super::vars::{full_name, value_json, value_on};

/// An open overlay in a report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OverlayInfo {
    pub id: NodeId,
    pub name: String,
}

/// An instance the prototype changed to another variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VariantInfo {
    pub instance: NodeId,
    /// The variant (main component) it shows now.
    pub component: NodeId,
    /// That variant's name (`State=Pressed`).
    pub name: String,
}

/// The prototype's state.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub screen: NodeId,
    pub screen_name: String,
    /// The open overlays, bottom first.
    pub overlays: Vec<OverlayInfo>,
    /// The screens "back" returns to, oldest first.
    pub history: Vec<NodeId>,
    /// The variables whose value differs from the file's, as `Collection/name` → value (in the
    /// mode the screen is in).
    pub variables: BTreeMap<String, Value>,
    /// The collections the prototype switched to another mode, as collection → mode names.
    pub modes: BTreeMap<String, String>,
    /// The instances showing another variant than in the file.
    pub variants: Vec<VariantInfo>,
    /// The screen is an end state a person declared or accepted.
    pub end_state: bool,
}

pub(super) fn of(sim: &Sim) -> State {
    let doc = &sim.design;
    let name = |id: NodeId| doc.node(id).map(|n| n.name.clone()).unwrap_or_default();
    State {
        screen: sim.screen,
        screen_name: name(sim.screen),
        overlays: sim.overlays.iter().map(|o| OverlayInfo { id: o.id, name: name(o.id) }).collect(),
        history: sim.history.clone(),
        variables: changed_variables(sim),
        modes: changed_modes(sim),
        variants: changed_variants(sim),
        end_state: is_end_state(doc, sim.screen),
    }
}

/// Whether `screen` is a frame where a flow may end: declared as one, and a person stands behind
/// that (an AI agent's proposal doesn't count until it is accepted).
pub(super) fn is_end_state(doc: &Document, screen: NodeId) -> bool {
    doc.design.end_states.iter().any(|e| e.node == screen && e.status == EndStateStatus::Accepted)
}

fn changed_variables(sim: &Sim) -> BTreeMap<String, Value> {
    let (doc, file) = (&sim.design, &sim.original);
    doc.variables
        .variables
        .iter()
        .filter(|v| file.variables.get_by_id(&v.id).is_none_or(|old| old.values != v.values))
        .filter_map(|v| Some((full_name(doc, v), value_json(&value_on(doc, v, sim.screen)?))))
        .collect()
}

/// "Set variable mode" writes the mode on the screen's page: the collections whose mode there
/// differs from the file's.
fn changed_modes(sim: &Sim) -> BTreeMap<String, String> {
    let (doc, file) = (&sim.design, &sim.original);
    let Some(page) = doc.page_of(sim.screen) else { return BTreeMap::new() };
    let (Some(now), Some(was)) = (doc.node(page), file.node(page)) else { return BTreeMap::new() };
    now.var_modes
        .iter()
        .filter(|(c, m)| was.var_modes.get(*c) != Some(*m))
        .filter_map(|(c, m)| {
            let c = doc.variables.collection_by_id(c)?;
            Some((c.name.clone(), c.mode_by_id(m)?.name.clone()))
        })
        .collect()
}

fn changed_variants(sim: &Sim) -> Vec<VariantInfo> {
    let (doc, file) = (&sim.design, &sim.original);
    doc.nodes()
        .filter_map(|n| {
            let NodeKind::Instance { component, .. } = &n.kind else { return None };
            let NodeKind::Instance { component: was, .. } = &file.node(n.id)?.kind else { return None };
            (was != component).then(|| VariantInfo {
                instance: n.id,
                component: *component,
                name: doc.node(*component).map(|c| c.name.clone()).unwrap_or_default(),
            })
        })
        .collect()
}
