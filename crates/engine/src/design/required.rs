//! The brief's required elements (`design.required.*`): what the design must show (consent text,
//! a price, a permission prompt), each with the screens it must be on and the layers that are it,
//! so that `context.requiredMissing` can find it.
//!
//! What the brief requires is the person's: an AI agent's entry is a proposal until a person
//! accepts it (`design.required.accept`), an agent changes the words or the screens of its own
//! proposals only and removes only those; the layers that are it (`nodes`) it may link on any
//! entry, since linking is the work of building it. Adding or removing an element, or changing
//! its words or screens, is a change of the brief and raises `briefRevision`; linking layers or
//! accepting isn't.

use figcraft_doc::design::{DesignContext, ProposalStatus, RequiredElement};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use super::input::{USER, actor, agent, edit, layers, req_entry_id, req_text, room_for, text_not_empty};
use crate::cmd::CommandSpec;
use crate::{Result, Session, bad, spec};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "design.required.add",
            "Add required element",
            "{text (what the design must show, in words: \"consent to be contacted, next to the button that sends the form\"), screens?: [top-level frame ids] (where it must be; default anywhere), nodes?: [layer ids] (the layers that are it; without them context.requiredMissing can't check it)}  part of the brief: raises briefRevision. The user's entry stands; an AI agent's is a proposal until a person accepts it (design.required.accept) → {id, status}",
            true,
            add
        ),
        spec!(
            "design.required.set",
            "Edit required element",
            "{id, text?, screens?: [top-level frame ids], nodes?: [layer ids]}  only the given fields change; a change of text or screens raises briefRevision, linking layers doesn't. An AI agent links layers on any entry, and changes the text and screens of its own proposals only → {required}",
            true,
            set
        ),
        spec!(
            "design.required.remove",
            "Remove required element",
            "{id}  raises briefRevision; an AI agent removes only its own proposals → {removed}",
            true,
            remove
        ),
        spec!(
            "design.required.accept",
            "Accept required element",
            "{id}  a person stands behind an element an AI agent listed; refused to an AI agent; the person is recorded as acceptedBy → {required}",
            true,
            accept
        ),
    ]
}

/// `key` as a list of top-level frames of `doc`, when given.
fn screens(cmd: &str, doc: &Document, p: &Value, key: &str) -> Result<Option<Vec<NodeId>>> {
    let Some(list) = layers(cmd, doc, p, key)? else { return Ok(None) };
    match list.iter().find(|id| doc.top_level_frame(**id) != Some(**id)) {
        Some(id) => Err(bad(cmd, format!("`{key}`: {} isn't a top-level frame (a screen)", crate::proto_sim::label(doc, *id)))),
        None => Ok(Some(list)),
    }
}

fn find<'a>(cmd: &str, ctx: &'a mut DesignContext, id: &str) -> Result<&'a mut RequiredElement> {
    ctx.brief.required.iter_mut().find(|r| r.id == id).ok_or_else(|| bad(cmd, format!("`id`: no required element {id}")))
}

/// Refuse an AI agent (`me`) a change to an element that isn't its own pending proposal.
fn own_proposal(cmd: &str, me: Option<&str>, r: &RequiredElement, doing: &str) -> Result<()> {
    match me {
        Some(me) if r.author != me || r.status.is_accepted() => Err(bad(
            cmd,
            format!(
                "`id`: {} was listed by {}{}; an AI agent ({me}) {doing} only its own proposals. Ask them to change it",
                r.id,
                super::author_name(&r.author),
                if r.status.is_accepted() { " and stands accepted" } else { "" }
            ),
        )),
        _ => Ok(()),
    }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.required.add";
    let words = req_text(cmd, p, "text")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, doc| {
        let on = screens(cmd, doc, p, "screens")?.unwrap_or_default();
        let nodes = layers(cmd, doc, p, "nodes")?.unwrap_or_default();
        room_for(cmd, "required elements", ctx.brief.required.len(), 1)?;
        let r = RequiredElement {
            id: ctx.next_id("R"),
            text: words,
            screens: on,
            nodes,
            author: me.clone().unwrap_or_else(|| USER.to_string()),
            status: if me.is_some() { ProposalStatus::Proposed } else { ProposalStatus::Accepted },
            accepted_by: String::new(),
        };
        let out = json!({"id": r.id, "status": r.status});
        ctx.brief.required.push(r);
        ctx.brief_revision = ctx.brief_revision.saturating_add(1);
        Ok(out)
    })
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.required.set";
    let id = req_entry_id(cmd, p, "id")?;
    let words = text_not_empty(cmd, p, "text")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, doc| {
        let on = screens(cmd, doc, p, "screens")?;
        let nodes = layers(cmd, doc, p, "nodes")?;
        let r = find(cmd, ctx, &id)?;
        let before = r.clone();
        if let Some(t) = words {
            r.text = t;
        }
        if let Some(on) = on {
            r.screens = on;
        }
        if let Some(n) = nodes {
            r.nodes = n;
        }
        let reworded = r.text != before.text || r.screens != before.screens;
        if reworded {
            own_proposal(cmd, me.as_deref(), &before, "changes the words and screens of")?;
        }
        let out = json!({"required": r});
        if reworded {
            ctx.brief_revision = ctx.brief_revision.saturating_add(1);
        }
        Ok(out)
    })
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.required.remove";
    let id = req_entry_id(cmd, p, "id")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let r = find(cmd, ctx, &id)?;
        own_proposal(cmd, me.as_deref(), r, "removes")?;
        ctx.brief.required.retain(|r| r.id != id);
        ctx.brief_revision = ctx.brief_revision.saturating_add(1);
        Ok(json!({"removed": id}))
    })
}

fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.required.accept";
    let id = req_entry_id(cmd, p, "id")?;
    if agent(s).is_some() {
        return Err(bad(cmd, format!("a person accepts a required element: {id} stays proposed until the user accepts it ({} can't)", actor(s))));
    }
    edit(s, |ctx, _| {
        let r = find(cmd, ctx, &id)?;
        if r.status == ProposalStatus::Proposed {
            r.status = ProposalStatus::Accepted;
            if super::author_name(&r.author) != USER {
                r.accepted_by = USER.into();
            }
        }
        Ok(json!({"required": r}))
    })
}
