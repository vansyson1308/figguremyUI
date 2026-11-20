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

use super::input::{USER, actor, agent, edit, layers, layers_keeping, req_entry_id, req_text, room_for, text_not_empty};
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
            "{id, text?, screens?: [top-level frame ids], nodes?: [layer ids]}  only the given fields change; screens and layers the element has are taken back as they are even when gone or moved (only those added must exist and be screens); a change of text or screens raises briefRevision, linking layers doesn't, and a change of text or screens by anyone but its author is recorded as editedBy. An AI agent adds links to any entry but removes or replaces links, and changes the text and screens, only of its own pending proposals that nobody else edited → {required}",
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

/// `key` as a list of top-level frames of `doc`, when given. The screens in `kept` (those the
/// element has) are taken as they are, even gone or moved, so re-sending them is never refused
/// (`context.requiredMissing` reports them); only a screen added must be one.
fn screens(cmd: &str, doc: &Document, p: &Value, key: &str, kept: &[NodeId]) -> Result<Option<Vec<NodeId>>> {
    let Some(list) = layers_keeping(cmd, doc, p, key, kept)? else { return Ok(None) };
    match list.iter().find(|id| !kept.contains(id) && doc.top_level_frame(**id) != Some(**id)) {
        Some(id) => Err(bad(cmd, format!("`{key}`: {} isn't a top-level frame (a screen)", crate::proto_sim::label(doc, *id)))),
        None => Ok(Some(list)),
    }
}

fn find<'a>(cmd: &str, ctx: &'a mut DesignContext, id: &str) -> Result<&'a mut RequiredElement> {
    ctx.brief.required.iter_mut().find(|r| r.id == id).ok_or_else(|| bad(cmd, format!("`id`: no required element {id}")))
}

/// Whether `r` is AI agent `me`'s own pending proposal, which nobody else has edited.
fn own(me: &str, r: &RequiredElement) -> bool {
    r.author == me && !r.status.is_accepted() && r.edited_by.is_empty()
}

/// Refuse an AI agent (`me`) a change to an element that isn't its own pending proposal.
fn own_proposal(cmd: &str, me: Option<&str>, r: &RequiredElement, doing: &str) -> Result<()> {
    match me {
        Some(me) if !own(me, r) => Err(bad(
            cmd,
            format!(
                "`id`: {} was listed by {}{}{}; an AI agent ({me}) {doing} only its own proposals. Ask them to change it",
                r.id,
                super::author_name(&r.author),
                if r.edited_by.is_empty() { String::new() } else { format!(" and edited by {}", super::author_name(&r.edited_by)) },
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
        let on = screens(cmd, doc, p, "screens", &[])?.unwrap_or_default();
        let nodes = layers(cmd, doc, p, "nodes")?.unwrap_or_default();
        room_for(cmd, "required elements", ctx.brief.required.len(), 1)?;
        let r = RequiredElement {
            id: ctx.next_id("R"),
            text: words,
            screens: on,
            nodes,
            author: me.clone().unwrap_or_else(|| USER.to_string()),
            edited_by: String::new(),
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
    let who = actor(s);
    edit(s, |ctx, doc| {
        // What the element has is taken back as it is, a gone layer or a moved screen included:
        // only what is added must exist (and be a screen).
        let (had_screens, had_nodes) = find(cmd, ctx, &id).map(|r| (r.screens.clone(), r.nodes.clone()))?;
        let on = screens(cmd, doc, p, "screens", &had_screens)?;
        let nodes = layers_keeping(cmd, doc, p, "nodes", &had_nodes)?;
        let r = find(cmd, ctx, &id)?;
        let before = r.clone();
        // An agent adds links to anybody's element; it takes links away only from its own proposal.
        if let (Some(me), Some(n)) = (&me, &nodes)
            && !own(me, &before)
            && let Some(kept) = before.nodes.iter().find(|old| !n.contains(old))
        {
            return Err(bad(
                cmd,
                format!(
                    "`nodes`: {id} isn't an AI agent's own pending proposal: {me} may add links to it but not remove or replace them (layer {kept} would go). Give the layers it has with yours added"
                ),
            ));
        }
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
            // The words are no longer all the author's: say who changed them last.
            if who != r.author {
                r.edited_by.clone_from(&who);
            }
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
        find(cmd, ctx, &id)?;
        // Every entry the call would take away (a file can hold an id twice).
        for r in ctx.brief.required.iter().filter(|r| r.id == id) {
            own_proposal(cmd, me.as_deref(), r, "removes")?;
        }
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
