//! End states (`proto.endState.*`): the frames where a flow may end on purpose, so that a screen
//! with no way out is a finding only when nobody meant it.

use std::sync::Arc;

use figcraft_doc::design::{EndState, MAX_ENTRIES};
use serde_json::{Value, json};

use super::parse;
use crate::params::{bool_of, get, req_id};
use crate::proto_sim::label;
use crate::{Result, Session, bad};

pub(super) fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.endState.set";
    let id = req_id(cmd, p, "id")?;
    let on = match get(p, "on") {
        Some(v) => bool_of(v).ok_or_else(|| bad(cmd, format!("`on` must be true or false (got {})", figcraft_format::clip(&v.to_string(), 40))))?,
        None => true,
    };
    let reason = get(p, "reason").map(|v| parse::text(cmd, v, "reason").map(|r| r.trim().to_string())).transpose()?;
    let doc = s.doc()?;
    let mut states = doc.design.end_states.clone();
    let at = states.iter().position(|e| e.node == id);
    match (on, at) {
        // Taking a declaration back works for a frame that is gone too.
        (false, Some(i)) => {
            states.remove(i);
        }
        (false, None) => return Err(bad(cmd, format!("`id`: {} isn't declared as an end state", label(doc, id)))),
        (true, _) if !doc.contains(id) => return Err(bad(cmd, format!("`id`: no layer {id}"))),
        (true, _) if doc.top_level_frame(id) != Some(id) => {
            return Err(bad(cmd, format!("`id`: {} isn't a top-level frame (a flow ends on a screen)", label(doc, id))));
        }
        (true, Some(i)) => {
            if let (Some(e), Some(r)) = (states.get_mut(i), &reason) {
                e.reason.clone_from(r);
            }
        }
        (true, None) if states.len() >= MAX_ENTRIES => {
            return Err(bad(cmd, format!("the file holds {MAX_ENTRIES} end states already, the most it keeps")));
        }
        (true, None) => states.push(EndState { node: id, reason: reason.clone().unwrap_or_default() }),
    }
    let kept = states.iter().find(|e| e.node == id).map(|e| e.reason.clone());
    // Declaring what is declared already is no edit (and no undo step).
    if states != doc.design.end_states {
        Arc::make_mut(&mut s.doc_mut()?.design).end_states = states;
    }
    Ok(json!({"id": id, "endState": on, "reason": kept}))
}

pub(super) fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let doc = s.doc()?;
    let states: Vec<Value> = doc
        .design
        .end_states
        .iter()
        .map(|e| json!({"node": e.node, "name": doc.node(e.node).map(|n| n.name.clone()), "reason": e.reason, "exists": doc.contains(e.node)}))
        .collect();
    Ok(json!({"endStates": states}))
}
