//! End states (`proto.endState.*`): the frames where a flow may end on purpose, so that a screen
//! with no way out is a finding only when nobody meant it.
//!
//! Whether a flow may end somewhere is a decision about the design, so a person's: an end state
//! the user declares stands at once; one an AI agent declares is a proposal (it needs a reason)
//! that exempts nothing until a person accepts it (`proto.endState.accept`). An agent changes and
//! takes back only its own proposals. As everywhere in the design commands this is a convention
//! between cooperating parties, not a security boundary (see [`crate::design`]).

use std::sync::Arc;

use figcraft_doc::NodeId;
use figcraft_doc::design::{EndState, EndStateStatus, MAX_ENTRIES};
use serde_json::{Value, json};

use super::parse;
use crate::design::{USER, agent, author_name};
use crate::params::{bool_of, get, req_id};
use crate::proto_sim::label;
use crate::{Result, Session, bad};

fn status_word(status: EndStateStatus) -> &'static str {
    match status {
        EndStateStatus::Proposed => "proposed",
        EndStateStatus::Accepted => "accepted",
    }
}

/// The answer of a command about frame `id`: whether it counts as an end state now (declared and
/// accepted), and the declaration as it stands.
fn answer(id: NodeId, state: Option<&EndState>) -> Value {
    match state {
        Some(e) => {
            let accepted = e.status == EndStateStatus::Accepted;
            let mut v = json!({"id": id, "endState": accepted, "status": e.status, "author": author_name(&e.author), "reason": e.reason});
            if let (Some(o), false) = (v.as_object_mut(), accepted) {
                o.insert("note".into(), json!("proposed: flow.deadEnd still reports the frame until a person accepts it with proto.endState.accept"));
            }
            v
        }
        None => json!({"id": id, "endState": false, "status": null, "reason": null}),
    }
}

/// Refuse an AI agent (`me`) a change to an end state that isn't its own pending proposal.
fn own_proposal(cmd: &str, me: Option<&str>, e: &EndState, doing: &str, at: &str) -> Result<()> {
    match me {
        Some(me) if e.author != me || e.status == EndStateStatus::Accepted => Err(bad(
            cmd,
            format!(
                "`id`: {at} is an end state declared by {} ({}); an AI agent ({me}) {doing} only its own proposals. Ask them to change it",
                author_name(&e.author),
                status_word(e.status)
            ),
        )),
        _ => Ok(()),
    }
}

pub(super) fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.endState.set";
    let id = req_id(cmd, p, "id")?;
    let on = match get(p, "on") {
        Some(v) => bool_of(v).ok_or_else(|| bad(cmd, format!("`on` must be true or false (got {})", figcraft_format::clip(&v.to_string(), 40))))?,
        None => true,
    };
    let reason = get(p, "reason").map(|v| parse::text(cmd, v, "reason").map(|r| r.trim().to_string())).transpose()?;
    let me = agent(s).map(str::to_string);
    let doc = s.doc()?;
    let mut states = doc.design.end_states.clone();
    let at = states.iter().position(|e| e.node == id);
    let needs_reason = || {
        bad(
            cmd,
            "`reason` is required when an AI agent declares an end state: say why the flow may end here. A person then accepts it (proto.endState.accept)",
        )
    };
    match (on, at) {
        // Taking a declaration back works for a frame that is gone too.
        (false, Some(i)) => {
            if let Some(e) = states.get(i) {
                own_proposal(cmd, me.as_deref(), e, "takes back", &label(doc, id))?;
            }
            states.remove(i);
        }
        (false, None) => return Err(bad(cmd, format!("`id`: {} isn't declared as an end state", label(doc, id)))),
        (true, _) if !doc.contains(id) => return Err(bad(cmd, format!("`id`: no layer {id}"))),
        (true, _) if doc.top_level_frame(id) != Some(id) => {
            return Err(bad(cmd, format!("`id`: {} isn't a top-level frame (a flow ends on a screen)", label(doc, id))));
        }
        (true, Some(i)) => {
            if let Some(e) = states.get_mut(i) {
                own_proposal(cmd, me.as_deref(), e, "changes", &label(doc, id))?;
                match (&me, &reason) {
                    (Some(_), Some(r)) if r.is_empty() => return Err(needs_reason()),
                    (_, Some(r)) => e.reason.clone_from(r),
                    (_, None) => {}
                }
                // A person declaring what an agent proposed stands behind it.
                if me.is_none() {
                    e.status = EndStateStatus::Accepted;
                }
            }
        }
        (true, None) if states.len() >= MAX_ENTRIES => {
            return Err(bad(cmd, format!("the file holds {MAX_ENTRIES} end states already, the most it keeps")));
        }
        (true, None) => {
            let reason = reason.clone().unwrap_or_default();
            states.push(match &me {
                Some(_) if reason.is_empty() => return Err(needs_reason()),
                Some(me) => EndState { node: id, reason, author: me.clone(), status: EndStateStatus::Proposed },
                None => EndState { node: id, reason, author: USER.into(), status: EndStateStatus::Accepted },
            });
        }
    }
    let out = answer(id, states.iter().find(|e| e.node == id));
    // Declaring what is declared already is no edit (and no undo step).
    if states != doc.design.end_states {
        Arc::make_mut(&mut s.doc_mut()?.design).end_states = states;
    }
    Ok(out)
}

pub(super) fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "proto.endState.accept";
    let id = req_id(cmd, p, "id")?;
    let doc = s.doc()?;
    if let Some(me) = agent(s) {
        return Err(bad(
            cmd,
            format!(
                "a person accepts an end state: {} stays a proposal until the user accepts it ({me} can't). Until then flow.deadEnd reports it",
                label(doc, id)
            ),
        ));
    }
    let mut states = doc.design.end_states.clone();
    let Some(e) = states.iter_mut().find(|e| e.node == id) else {
        return Err(bad(cmd, format!("`id`: {} isn't declared as an end state (declare it with proto.endState.set)", label(doc, id))));
    };
    e.status = EndStateStatus::Accepted;
    let out = answer(id, Some(e));
    // Accepting what is accepted already is no edit (and no undo step).
    if states != doc.design.end_states {
        Arc::make_mut(&mut s.doc_mut()?.design).end_states = states;
    }
    Ok(out)
}

pub(super) fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let doc = s.doc()?;
    let states: Vec<Value> = doc
        .design
        .end_states
        .iter()
        .map(|e| {
            json!({
                "node": e.node,
                "name": doc.node(e.node).map(|n| n.name.clone()),
                "reason": e.reason,
                "author": author_name(&e.author),
                "status": e.status,
                "exists": doc.contains(e.node),
            })
        })
        .collect();
    Ok(json!({"endStates": states}))
}
