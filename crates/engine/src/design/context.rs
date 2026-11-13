//! Reading the design context, and the commands for the brief, the stage, assumptions and
//! decisions.
//!
//! Two things are a person's to say and are refused to an AI agent: that an assumption was
//! supported or refuted (that is evidence about people, which an agent's own review isn't), and
//! that a decision is accepted. An agent proposes; its changes to an accepted decision put it
//! back to proposed, and the decision then says who edited it (`editedBy`). What a person
//! recorded about an assumption (that it was supported or refuted, as it reads, on what
//! evidence) an agent neither changes nor removes.

use figcraft_doc::design::{Assumption, AssumptionStatus, Brief, Decision, DecisionStatus, DesignContext, Stage};
use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use super::checks::missing_links;
use super::input::{
    actor, agent, choice, edit, entry_ids, layers, req_choice, req_entry_id, req_layer_id, req_text, room_for, shown, text, text_not_empty, texts,
};
use crate::cmd::CommandSpec;
use crate::{Result, Session, bad, spec};

const STATUSES: [(&str, AssumptionStatus); 3] =
    [("untested", AssumptionStatus::Untested), ("supported", AssumptionStatus::Supported), ("refuted", AssumptionStatus::Refuted)];

pub(super) const STAGES: [(&str, Stage); 3] = [("exploring", Stage::Exploring), ("converging", Stage::Converging), ("ready", Stage::Ready)];

/// One field of the brief, to write to.
type Field<T> = fn(&mut Brief) -> &mut T;

/// The brief's text fields by parameter name.
const BRIEF_TEXTS: [(&str, Field<String>); 7] = [
    ("audience", |b| &mut b.audience),
    ("sponsor", |b| &mut b.sponsor),
    ("job", |b| &mut b.job),
    ("context", |b| &mut b.context),
    ("content", |b| &mut b.content),
    ("format", |b| &mut b.format),
    ("tone", |b| &mut b.tone),
];

/// The brief's list fields by parameter name.
const BRIEF_LISTS: [(&str, Field<Vec<String>>); 2] = [("constraints", |b| &mut b.constraints), ("success", |b| &mut b.success)];

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "design.get",
            "Design context",
            "{}  → {brief, briefRevision, stage, assumptions, decisions, scenarios, endStates, findings, alternatives, missingLinks: [{node, from}] (links whose layer no longer exists), counts}",
            false,
            get_all
        ),
        spec!(
            "design.forNode",
            "Design context of a layer",
            "{id}  what links to exactly this layer (not to the layers around or inside it) → {assumptions: [ids], decisions: [ids], scenarios: [ids], findings: [ids], alternatives: [set ids], endState: bool}",
            false,
            for_node
        ),
        spec!(
            "design.brief.set",
            "Edit design brief",
            "{audience? (who uses it), sponsor? (who asks for it and what they want from it), job? (what the audience needs to get done), context? (the circumstances of use), content? (what it must say or offer; not the final copy), format? (the form it takes and where people meet it), tone?, constraints?: [text], success?: [text] (how success would be recognised)}  only the given fields change; \"\" or [] clears one; a call that changes the brief raises briefRevision → {brief, briefRevision}",
            true,
            brief_set
        ),
        spec!(
            "design.stage.set",
            "Set design stage",
            "{stage: exploring|converging|ready}  how far along the design is: design.check runs only the rules that are worth running at that stage and lists the rest as skipped → {stage}",
            true,
            stage_set
        ),
        spec!(
            "design.assumption.add",
            "Add assumption",
            "{text, links?: [layer ids], status?: untested|supported|refuted (default untested; supported and refuted need evidence, and only a person records them), evidence? (where the status comes from: a study, analytics, a conversation)} → {id}",
            true,
            assumption_add
        ),
        spec!(
            "design.assumption.set",
            "Edit assumption",
            "{id, text?, status?: untested|supported|refuted, evidence?, links?: [layer ids]}  only the given fields change; an AI agent can't change the text, status or evidence of an assumption a person recorded as supported or refuted (its links it may) → {assumption}",
            true,
            assumption_set
        ),
        spec!(
            "design.assumption.remove",
            "Remove assumption",
            "{id}  decisions that rested on it no longer list it; an AI agent removes only untested assumptions → {removed}",
            true,
            assumption_remove
        ),
        spec!(
            "design.decision.add",
            "Add decision",
            "{title, rationale, rejected? (what else was considered and why it lost), assumptions?: [assumption ids], links?: [layer ids]}  the user's is accepted; an AI agent's is proposed until a person accepts it → {id, status}",
            true,
            decision_add
        ),
        spec!(
            "design.decision.set",
            "Edit decision",
            "{id, title?, rationale?, rejected?, assumptions?: [assumption ids], links?: [layer ids]}  only the given fields change; an AI agent's change to an accepted decision makes it proposed again; a change by anyone but its author is recorded as editedBy → {decision}",
            true,
            decision_set
        ),
        spec!(
            "design.decision.remove",
            "Remove decision",
            "{id}  an AI agent removes only its own proposed decisions → {removed}",
            true,
            decision_remove
        ),
        spec!(
            "design.decision.accept",
            "Accept decision",
            "{id}  a person stands behind it as the brief is now (it takes the current briefRevision); refused to an AI agent. Accepting the decision of an AI agent's proposed choice among alternatives makes that choice (see design.alternative.choose) → {decision, chose?: [{set, node}]}",
            true,
            decision_accept
        ),
    ]
}

fn get_all(s: &mut Session, _: &Value) -> Result<Value> {
    let doc = s.doc()?;
    let ctx = &doc.design;
    let missing = missing_links(doc);
    let untested = ctx.assumptions.iter().filter(|a| a.status == AssumptionStatus::Untested).count();
    let proposed = ctx.decisions.iter().filter(|d| d.status == DecisionStatus::Proposed).count();
    let open = ctx.findings.iter().filter(|f| f.status == figcraft_doc::design::FindingStatus::Open).count();
    Ok(json!({
        "brief": ctx.brief,
        "briefRevision": ctx.brief_revision,
        "stage": ctx.stage,
        "assumptions": ctx.assumptions,
        "decisions": ctx.decisions,
        "scenarios": ctx.scenarios,
        "endStates": ctx.end_states,
        "findings": ctx.findings,
        "alternatives": ctx.alternatives,
        "missingLinks": missing.iter().map(|(node, from)| json!({"node": node, "from": from})).collect::<Vec<_>>(),
        "counts": {
            "assumptions": ctx.assumptions.len(),
            "untestedAssumptions": untested,
            "decisions": ctx.decisions.len(),
            "proposedDecisions": proposed,
            "scenarios": ctx.scenarios.len(),
            "endStates": ctx.end_states.len(),
            "findings": ctx.findings.len(),
            "openFindings": open,
            "alternatives": ctx.alternatives.len(),
            "missingLinks": missing.len(),
        },
    }))
}

fn for_node(s: &mut Session, p: &Value) -> Result<Value> {
    let id = req_layer_id("design.forNode", p, "id")?;
    let ctx = &s.doc()?.design;
    let scenario_names = |sc: &figcraft_doc::design::Scenario| {
        sc.start == id
            || sc.expect.nodes().contains(&id)
            || sc.steps.iter().any(|step| step.action.node() == Some(id) || step.expect.nodes().contains(&id))
    };
    let ids = |list: Vec<&String>| list.into_iter().cloned().collect::<Vec<String>>();
    Ok(json!({
        "assumptions": ids(ctx.assumptions.iter().filter(|a| a.links.contains(&id)).map(|a| &a.id).collect()),
        "decisions": ids(ctx.decisions.iter().filter(|d| d.links.contains(&id)).map(|d| &d.id).collect()),
        "scenarios": ids(ctx.scenarios.iter().filter(|sc| scenario_names(sc)).map(|sc| &sc.id).collect()),
        "findings": ids(ctx.findings.iter().filter(|f| f.nodes.contains(&id)).map(|f| &f.id).collect()),
        "alternatives": ids(ctx.alternatives.iter().filter(|v| v.source == id || v.members.iter().any(|m| m.node == id)).map(|v| &v.id).collect()),
        "endState": ctx.end_states.iter().any(|e| e.node == id),
    }))
}

fn brief_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.brief.set";
    let mut new_texts = Vec::new();
    for (key, field) in BRIEF_TEXTS {
        if let Some(value) = text(cmd, p, key)? {
            new_texts.push((field, value));
        }
    }
    let mut new_lists = Vec::new();
    for (key, field) in BRIEF_LISTS {
        if let Some(value) = texts(cmd, p, key)? {
            new_lists.push((field, value));
        }
    }
    edit(s, |ctx, _| {
        let before = ctx.brief.clone();
        for (field, value) in new_texts {
            *field(&mut ctx.brief) = value;
        }
        for (field, value) in new_lists {
            *field(&mut ctx.brief) = value;
        }
        if ctx.brief != before {
            ctx.brief_revision = ctx.brief_revision.saturating_add(1);
        }
        Ok(json!({"brief": ctx.brief, "briefRevision": ctx.brief_revision}))
    })
}

fn stage_set(s: &mut Session, p: &Value) -> Result<Value> {
    let stage = req_choice("design.stage.set", p, "stage", &STAGES)?;
    edit(s, |ctx, _| {
        ctx.stage = stage;
        Ok(json!({"stage": stage}))
    })
}

/// What an assumption must satisfy however it was edited.
fn check_assumption(cmd: &str, a: &Assumption) -> Result<()> {
    if a.status != AssumptionStatus::Untested && a.evidence.is_empty() {
        return Err(bad(cmd, "`evidence` is required when `status` is supported or refuted: say where that comes from"));
    }
    Ok(())
}

/// An agent's own review of the design is not evidence about people: it may write down what it
/// found, but not that an assumption holds or fails.
fn refuse_agent_status(cmd: &str, s: &Session, status: Option<AssumptionStatus>) -> Result<()> {
    match status {
        Some(st) if agent(s).is_some() && st != AssumptionStatus::Untested => {
            Err(bad(cmd, "`status`: only a person records that an assumption was supported or refuted; put what you found in `evidence`"))
        }
        _ => Ok(()),
    }
}

fn assumption_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.assumption.add";
    let statement = req_text(cmd, p, "text")?;
    let status = choice(cmd, p, "status", &STATUSES)?;
    refuse_agent_status(cmd, s, status)?;
    let evidence = text(cmd, p, "evidence")?.unwrap_or_default();
    edit(s, |ctx, doc| {
        let links = layers(cmd, doc, p, "links")?.unwrap_or_default();
        room_for(cmd, "assumptions", ctx.assumptions.len(), 1)?;
        let a = Assumption { id: ctx.next_id("A"), text: statement, status: status.unwrap_or_default(), evidence, links };
        check_assumption(cmd, &a)?;
        let id = a.id.clone();
        ctx.assumptions.push(a);
        Ok(json!({"id": id}))
    })
}

fn assumption_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.assumption.set";
    let id = req_entry_id(cmd, p, "id")?;
    let statement = text_not_empty(cmd, p, "text")?;
    let status = choice(cmd, p, "status", &STATUSES)?;
    refuse_agent_status(cmd, s, status)?;
    let evidence = text(cmd, p, "evidence")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, doc| {
        let links = layers(cmd, doc, p, "links")?;
        let a = ctx.assumptions.iter_mut().find(|a| a.id == id).ok_or_else(|| bad(cmd, format!("`id`: no assumption {id}")))?;
        // What a person found out is theirs: the statement as it reads (rewording it would carry
        // their finding over to something nobody tested), that it was supported or refuted, and
        // the evidence for it.
        if let Some(me) = &me
            && a.status != AssumptionStatus::Untested
        {
            let changes = [
                ("text", statement.as_ref().is_some_and(|t| *t != a.text)),
                ("status", status.is_some_and(|st| st != a.status)),
                ("evidence", evidence.as_ref().is_some_and(|e| *e != a.evidence)),
            ];
            if let Some((key, _)) = changes.iter().find(|(_, changed)| *changed) {
                return Err(bad(cmd, format!("`{key}`: {}", persons_assumption(a, me, "change"))));
            }
        }
        if let Some(t) = statement {
            a.text = t;
        }
        if let Some(st) = status {
            a.status = st;
        }
        if let Some(e) = evidence {
            a.evidence = e;
        }
        if let Some(l) = links {
            a.links = l;
        }
        check_assumption(cmd, a)?;
        Ok(json!({"assumption": a}))
    })
}

/// Why an AI agent (`me`) may not `verb` assumption `a`, which a person settled.
fn persons_assumption(a: &Assumption, me: &str, verb: &str) -> String {
    let status = if a.status == AssumptionStatus::Refuted { "refuted" } else { "supported" };
    format!(
        "a person recorded {} as {status} (evidence: {}); an AI agent ({me}) doesn't {verb} that. Add another assumption (design.assumption.add) or ask them to {verb} it",
        a.id,
        shown(&json!(a.evidence))
    )
}

fn assumption_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.assumption.remove";
    let id = req_entry_id(cmd, p, "id")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let Some(a) = ctx.assumptions.iter().find(|a| a.id == id) else {
            return Err(bad(cmd, format!("`id`: no assumption {id}")));
        };
        if let Some(me) = &me
            && a.status != AssumptionStatus::Untested
        {
            return Err(bad(cmd, format!("`id`: {}", persons_assumption(a, me, "remove"))));
        }
        ctx.assumptions.retain(|a| a.id != id);
        for d in &mut ctx.decisions {
            d.assumptions.retain(|a| *a != id);
        }
        Ok(json!({"removed": id}))
    })
}

/// The assumption ids in `key`, each naming an assumption of the file.
fn assumption_refs(cmd: &str, ctx: &DesignContext, p: &Value, key: &str) -> Result<Option<Vec<String>>> {
    let Some(ids) = entry_ids(cmd, p, key)? else { return Ok(None) };
    match ids.iter().find(|i| !ctx.assumptions.iter().any(|a| a.id == **i)) {
        Some(missing) => Err(bad(cmd, format!("`{key}`: no assumption {missing}"))),
        None => Ok(Some(ids)),
    }
}

/// A decision recorded by whoever runs the command: the user's stands, an agent's is a proposal.
pub(super) fn new_decision(ctx: &mut DesignContext, agent: Option<&str>, title: String, rationale: String, links: Vec<NodeId>) -> Decision {
    Decision {
        id: ctx.next_id("D"),
        title,
        rationale,
        author: agent.unwrap_or(super::input::USER).to_string(),
        status: if agent.is_some() { DecisionStatus::Proposed } else { DecisionStatus::Accepted },
        brief_revision: ctx.brief_revision,
        links,
        ..Decision::default()
    }
}

fn decision_add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.decision.add";
    let title = req_text(cmd, p, "title")?;
    let rationale = req_text(cmd, p, "rationale")?;
    let rejected = text(cmd, p, "rejected")?.unwrap_or_default();
    let agent = agent(s).map(str::to_string);
    edit(s, |ctx, doc: &Document| {
        let assumptions = assumption_refs(cmd, ctx, p, "assumptions")?.unwrap_or_default();
        let links = layers(cmd, doc, p, "links")?.unwrap_or_default();
        room_for(cmd, "decisions", ctx.decisions.len(), 1)?;
        let d = Decision { rejected, assumptions, ..new_decision(ctx, agent.as_deref(), title, rationale, links) };
        let out = json!({"id": d.id, "status": d.status});
        ctx.decisions.push(d);
        Ok(out)
    })
}

fn decision_set(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.decision.set";
    let id = req_entry_id(cmd, p, "id")?;
    let title = text_not_empty(cmd, p, "title")?;
    let rationale = text_not_empty(cmd, p, "rationale")?;
    let rejected = text(cmd, p, "rejected")?;
    let (who, by_agent) = (actor(s), agent(s).is_some());
    edit(s, |ctx, doc| {
        let assumptions = assumption_refs(cmd, ctx, p, "assumptions")?;
        let links = layers(cmd, doc, p, "links")?;
        let d = ctx.decisions.iter_mut().find(|d| d.id == id).ok_or_else(|| bad(cmd, format!("`id`: no decision {id}")))?;
        let before = d.clone();
        if let Some(t) = title {
            d.title = t;
        }
        if let Some(r) = rationale {
            d.rationale = r;
        }
        if let Some(r) = rejected {
            d.rejected = r;
        }
        if let Some(a) = assumptions {
            d.assumptions = a;
        }
        if let Some(l) = links {
            d.links = l;
        }
        if *d != before {
            // The text is no longer all its author's: say who changed it last.
            d.edited_by = if who == d.author { String::new() } else { who };
            // What the person accepted is the decision as it read then.
            if by_agent {
                d.status = DecisionStatus::Proposed;
            }
        }
        Ok(json!({"decision": d}))
    })
}

fn decision_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.decision.remove";
    let id = req_entry_id(cmd, p, "id")?;
    let agent = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let Some(d) = ctx.decisions.iter().find(|d| d.id == id) else {
            return Err(bad(cmd, format!("`id`: no decision {id}")));
        };
        // An agent takes back its own proposals; what a person accepted or wrote stays theirs.
        if let Some(me) = &agent
            && (d.status == DecisionStatus::Accepted || d.author != *me)
        {
            return Err(bad(
                cmd,
                format!("`id`: {id} is {} by {}; an AI agent removes only its own proposed decisions", status_word(d.status), d.author),
            ));
        }
        ctx.decisions.retain(|d| d.id != id);
        // A set of alternatives keeps what was chosen, without a record of why; a proposed choice
        // goes with the decision that proposed it.
        for v in ctx.alternatives.iter_mut() {
            if v.decision == id {
                v.decision.clear();
            }
            if v.proposal.as_ref().is_some_and(|p| p.decision == id) {
                v.proposal = None;
            }
        }
        Ok(json!({"removed": id}))
    })
}

fn status_word(status: DecisionStatus) -> &'static str {
    match status {
        DecisionStatus::Accepted => "accepted",
        DecisionStatus::Proposed => "proposed",
    }
}

fn decision_accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.decision.accept";
    let id = req_entry_id(cmd, p, "id")?;
    if agent(s).is_some() {
        return Err(bad(cmd, format!("a person accepts a decision: {id} stays proposed until the user accepts it ({} can't)", actor(s))));
    }
    edit(s, |ctx, _| {
        let revision = ctx.brief_revision;
        let d = ctx.decisions.iter_mut().find(|d| d.id == id).ok_or_else(|| bad(cmd, format!("`id`: no decision {id}")))?;
        d.status = DecisionStatus::Accepted;
        d.brief_revision = revision;
        let mut out = json!({"decision": d});
        // The decision of a choice an agent proposed: accepting it makes the choice.
        let mut chose = Vec::new();
        for v in &mut ctx.alternatives {
            let Some(p) = v.proposal.take_if(|p| p.decision == id) else { continue };
            if p.node == v.source || v.members.iter().any(|m| m.node == p.node) {
                v.chosen = Some(p.node);
                v.decision.clone_from(&id);
                chose.push(json!({"set": v.id, "node": p.node}));
            }
        }
        if let (Some(o), false) = (out.as_object_mut(), chose.is_empty()) {
            o.insert("chose".into(), json!(chose));
        }
        Ok(out)
    })
}
