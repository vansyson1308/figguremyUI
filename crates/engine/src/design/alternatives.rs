//! Alternatives: copies of a screen or component reworked side by side, each under a stated
//! constraint, compared by what can be counted and chosen from by a recorded decision.
//!
//! The source is never changed and nothing is deleted unless asked. A set says what it is
//! trying to find out and the limits every member has to respect, both stated before the
//! members are made; each member says what it was made under and what it gives up.

use std::sync::Arc;

use figcraft_doc::design::{Alternative, AlternativeSet, DesignContext, MAX_TEXT};
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::{Value, json};

use super::compare::{Difference, Facts};
use super::context::new_decision;
use super::input::{edit, flag, req_entry_id, req_layer, req_layer_id, req_text, room_for, text, text_not_empty, texts};
use crate::cmd::CommandSpec;
use crate::params::get;
use crate::{Result, Session, bad, spec};

/// The space left between the source, or the last alternative, and a new one (as Duplicate
/// leaves between top-level frames).
const GAP: f64 = 100.0;

/// Up to this many members are also compared with each other (each is always compared with the
/// source): the pairs grow with the square of the members.
const MAX_PAIRED: usize = 12;

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "design.alternative.create",
            "Create alternative",
            "{source (a top-level frame, or a component that isn't a variant), name, constraint (the limit this one is made under: \"one thumb, no scrolling\"), intent? (who or what situation it serves best), set? (the set to add it to; default: the source's set, made when there is none), question? (what the set is trying to find out; stored when the set is made), constraints?: [text] (limits every member respects; stored when the set is made)}  copies the source next to it (to the right of the source and the alternatives already there, 100 px apart, same parent), names the copy \"<source name> / <name>\" and records it; instances in the copy keep their overrides and interactions inside it point into it; the source isn't changed; a component's copy is a component of its own → {set, node}",
            true,
            create
        ),
        spec!(
            "design.alternative.update",
            "Edit alternative",
            "{set, node, name?, constraint?, intent?, tradeoffs? (what it gives up)}  only the given fields change; the layer keeps its name → {alternative}",
            true,
            update
        ),
        spec!(
            "design.alternative.question",
            "Edit alternatives' question",
            "{set, question?, constraints?: [text]}  what the set is trying to find out and the limits every member respects → {set, question, constraints}",
            true,
            question
        ),
        spec!(
            "design.alternative.compare",
            "Compare alternatives",
            "{set}  facts only, no ranking → {set, question, constraints, chosen, decision, members: [{node, name, isSource, constraint, intent, tradeoffs, exists, metrics: {width, height, layers (inside it), depth, textLayers, words, characters, interactive (layers with interactions), instances, components (distinct main components), images, density (area of the layers that hold no others, inside the frame, over the frame's area; 0–1, overlaps count twice)}, readingOrder: [up to 40 texts, top to bottom then left to right, each cut to 80 chars], actions: [names of layers with a click interaction]}] (the source first; hidden layers aren't counted), differences: [{a, b, structure (kinds, nesting or number of layers), order (the texts in reading order, ignoring case and spacing), content (the set of texts), actions (what reacts and what it does), size, layout (places and sizes of layers) and paint (fills, strokes, effects, fonts): null when the structure differs (layers can't be matched one to one), paintOnly, identical}] (each member with the source; members with each other up to 12 members), warnings: [text] (a member that differs from the source only in paint, or not at all; layers that are gone)}",
            false,
            compare
        ),
        spec!(
            "design.alternative.choose",
            "Choose alternative",
            "{set, node (a member, or the source), rationale}  marks it as chosen and records a decision (accepted when the user chooses, proposed when an AI agent does) linked to it; nothing is deleted → {decision, status}",
            true,
            choose
        ),
        spec!(
            "design.alternative.remove",
            "Remove alternative",
            "{set, node? (the member to forget; without it the whole set), deleteLayer? (also delete the forgotten members' layers; default false; the source is never deleted)} → {removed: [layer ids], deleted: [layer ids]}",
            true,
            remove
        ),
    ]
}

fn set_index(cmd: &str, ctx: &DesignContext, id: &str) -> Result<usize> {
    ctx.alternatives.iter().position(|v| v.id == id).ok_or_else(|| bad(cmd, format!("`set`: no set of alternatives {id}")))
}

fn set_mut<'a>(cmd: &str, ctx: &'a mut DesignContext, id: &str) -> Result<&'a mut AlternativeSet> {
    ctx.alternatives.iter_mut().find(|v| v.id == id).ok_or_else(|| bad(cmd, format!("`set`: no set of alternatives {id}")))
}

/// What alternatives can be made of: a screen (a top-level frame) or a component. A variant only
/// makes sense among the others of its set, so the set is what to copy.
fn check_source(cmd: &str, doc: &Document, id: NodeId) -> Result<()> {
    let n = doc.get(id)?;
    let component = matches!(n.kind, NodeKind::Component { .. });
    if component && crate::components::variant_set(doc, id).is_some() {
        return Err(bad(cmd, format!("`source`: {id} (`{}`) is a variant; make alternatives of its component set", n.name)));
    }
    if component || doc.top_level_frame(id) == Some(id) {
        return Ok(());
    }
    Err(bad(cmd, format!("`source`: {id} (`{}`) isn't a top-level frame or a component", n.name)))
}

/// Copy `source` under its parent, after and to the right of `beside` (the source and the
/// alternatives already made of it), level with the source. The copy is a deep copy: unlike
/// Duplicate, a component's is a component of its own, since an instance couldn't be reworked.
fn copy_beside(doc: &mut Document, source: NodeId, beside: &[NodeId]) -> Result<NodeId> {
    let parent = doc.get(source)?.parent.ok_or(crate::EngineError::NoNode(source))?;
    let row: Vec<NodeId> = beside.iter().copied().filter(|i| doc.node(*i).is_some_and(|n| n.parent == Some(parent))).collect();
    let index = row.last().and_then(|last| doc.index_in_parent(*last)).map(|i| i + 1);
    let copy = doc.duplicate_into(source, parent, index)?;
    let right = row.iter().filter_map(|i| doc.world_bounds(*i)).map(|b| b.x1).fold(f64::NEG_INFINITY, f64::max);
    if let (Some(from), Some(level)) = (doc.world_bounds(copy), doc.world_bounds(source))
        && right.is_finite()
    {
        crate::cmd::arrange::move_world(doc, copy, right + GAP - from.x0, level.y0 - from.y0)?;
    }
    crate::library::count_inserted(doc, &[copy]);
    Ok(copy)
}

/// `given` for a set that exists already: it may repeat what the set says, not change it here.
fn same_as_set<T: PartialEq>(cmd: &str, key: &str, given: Option<T>, stored: &T, set: &str) -> Result<()> {
    match given {
        Some(g) if g != *stored => Err(bad(cmd, format!("`{key}`: set {set} exists already; change it with design.alternative.question"))),
        _ => Ok(()),
    }
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.create";
    let name = req_text(cmd, p, "name")?;
    let constraint = req_text(cmd, p, "constraint")?;
    let intent = text(cmd, p, "intent")?.unwrap_or_default();
    let question = text(cmd, p, "question")?;
    let constraints = texts(cmd, p, "constraints")?;
    let wanted = get(p, "set").map(|_| req_entry_id(cmd, p, "set")).transpose()?;
    let doc = s.doc()?;
    let source = req_layer(cmd, doc, p, "source")?;
    check_source(cmd, doc, source)?;
    let source_name = doc.get(source)?.name.clone();
    let mut ctx = DesignContext::clone(&doc.design);
    let found = match &wanted {
        Some(id) => Some(set_index(cmd, &ctx, id)?),
        None => ctx.alternatives.iter().position(|v| v.source == source),
    };
    let at = match found {
        Some(at) => {
            let set = ctx.alternatives.get(at).ok_or_else(|| bad(cmd, "`set`: no such set"))?;
            if set.source != source {
                return Err(bad(cmd, format!("`set`: {} holds alternatives of layer {}, not of `source` {source}", set.id, set.source)));
            }
            same_as_set(cmd, "question", question, &set.question, &set.id)?;
            same_as_set(cmd, "constraints", constraints, &set.constraints, &set.id)?;
            at
        }
        None => {
            room_for(cmd, "sets of alternatives", ctx.alternatives.len(), 1)?;
            let id = ctx.next_id("V");
            ctx.alternatives.push(AlternativeSet {
                id,
                source,
                question: question.unwrap_or_default(),
                constraints: constraints.unwrap_or_default(),
                members: Vec::new(),
                chosen: None,
                decision: String::new(),
            });
            ctx.alternatives.len() - 1
        }
    };
    let set = ctx.alternatives.get_mut(at).ok_or_else(|| bad(cmd, "`set`: no such set"))?;
    room_for(cmd, "alternatives in this set", set.members.len(), 1)?;
    let beside: Vec<NodeId> = std::iter::once(source).chain(set.members.iter().map(|m| m.node)).collect();
    let doc = s.doc_mut()?;
    let copy = copy_beside(doc, source, &beside)?;
    doc.get_mut(copy)?.name = format!("{source_name} / {name}");
    set.members.push(Alternative { node: copy, name, constraint, intent, tradeoffs: String::new() });
    let out = json!({"set": set.id, "node": copy});
    doc.design = Arc::new(ctx);
    Ok(out)
}

fn update(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.update";
    let id = req_entry_id(cmd, p, "set")?;
    let node = req_layer_id(cmd, p, "node")?;
    let name = text_not_empty(cmd, p, "name")?;
    let constraint = text_not_empty(cmd, p, "constraint")?;
    let intent = text(cmd, p, "intent")?;
    let tradeoffs = text(cmd, p, "tradeoffs")?;
    edit(s, |ctx, _| {
        let set = set_mut(cmd, ctx, &id)?;
        let m = set
            .members
            .iter_mut()
            .find(|m| m.node == node)
            .ok_or_else(|| bad(cmd, format!("`node`: layer {node} isn't an alternative in set {id}")))?;
        if let Some(n) = name {
            m.name = n;
        }
        if let Some(c) = constraint {
            m.constraint = c;
        }
        if let Some(i) = intent {
            m.intent = i;
        }
        if let Some(t) = tradeoffs {
            m.tradeoffs = t;
        }
        Ok(json!({"alternative": m}))
    })
}

fn question(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.question";
    let id = req_entry_id(cmd, p, "set")?;
    let asked = text(cmd, p, "question")?;
    let constraints = texts(cmd, p, "constraints")?;
    edit(s, |ctx, _| {
        let set = set_mut(cmd, ctx, &id)?;
        if let Some(q) = asked {
            set.question = q;
        }
        if let Some(c) = constraints {
            set.constraints = c;
        }
        Ok(json!({"set": set.id, "question": set.question, "constraints": set.constraints}))
    })
}

/// One frame of a comparison: the source or a member, with its facts when its layer exists.
struct Row<'a> {
    node: NodeId,
    name: String,
    member: Option<&'a Alternative>,
    facts: Option<Facts>,
}

impl Row<'_> {
    fn json(&self) -> Value {
        let said = |pick: fn(&Alternative) -> &String| self.member.map(pick).cloned().unwrap_or_default();
        json!({
            "node": self.node,
            "name": self.name,
            "isSource": self.member.is_none(),
            "constraint": said(|m| &m.constraint),
            "intent": said(|m| &m.intent),
            "tradeoffs": said(|m| &m.tradeoffs),
            "exists": self.facts.is_some(),
            "metrics": self.facts.as_ref().map(Facts::metrics),
            "readingOrder": self.facts.as_ref().map(Facts::reading_order).unwrap_or_default(),
            "actions": self.facts.as_ref().map(Facts::actions).unwrap_or_default(),
        })
    }
}

fn compare(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.compare";
    let id = req_entry_id(cmd, p, "set")?;
    let doc = s.doc()?;
    let set = doc.design.alternatives.iter().find(|v| v.id == id).ok_or_else(|| bad(cmd, format!("`set`: no set of alternatives {id}")))?;
    // Instances play their main component's interactions, as the prototype player has them.
    let mut played = doc.clone();
    crate::proto_run::play_interactions(&mut played);
    let source_name = doc.node(set.source).map(|n| n.name.clone()).unwrap_or_default();
    let mut rows = vec![Row { node: set.source, name: source_name, member: None, facts: Facts::of(&played, set.source) }];
    rows.extend(set.members.iter().map(|m| Row { node: m.node, name: m.name.clone(), member: Some(m), facts: Facts::of(&played, m.node) }));

    let mut warnings = Vec::new();
    for r in &rows {
        match (&r.facts, r.member) {
            (None, None) => warnings.push(format!("the source (layer {}) no longer exists: nothing can be compared with it", r.node)),
            (None, Some(_)) => warnings.push(format!("`{}` (layer {}) no longer exists", r.name, r.node)),
            (Some(f), _) if f.cut_short => warnings.push(format!("`{}` holds more layers than are compared: only the first were", r.name)),
            _ => {}
        }
    }
    let mut differences = Vec::new();
    let (source, members) = match rows.split_first() {
        Some((source, members)) => (source, members),
        None => return Err(bad(cmd, "`set`: nothing to compare")),
    };
    for m in members {
        let (Some(a), Some(b)) = (&source.facts, &m.facts) else { continue };
        let d = Difference::between(a, b);
        if d.identical {
            warnings.push(format!("`{}` is identical to the source (same layers, text, layout and paint): nothing was changed yet.", m.name));
        } else if d.paint_only {
            warnings.push(format!(
                "`{}` differs from the source only in paint (fills, strokes, effects, fonts): it is a restyle, not a structural alternative.",
                m.name
            ));
        }
        differences.push(d.json(source.node, m.node));
    }
    if members.len() > MAX_PAIRED {
        warnings.push(format!("the set has more than {MAX_PAIRED} members: each is compared with the source, not with the others"));
    } else {
        for (i, a) in members.iter().enumerate() {
            for b in members.iter().skip(i + 1) {
                if let (Some(fa), Some(fb)) = (&a.facts, &b.facts) {
                    differences.push(Difference::between(fa, fb).json(a.node, b.node));
                }
            }
        }
    }
    Ok(json!({
        "set": set.id,
        "question": set.question,
        "constraints": set.constraints,
        "chosen": set.chosen,
        "decision": set.decision,
        "members": rows.iter().map(Row::json).collect::<Vec<_>>(),
        "differences": differences,
        "warnings": warnings,
    }))
}

fn choose(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.choose";
    let id = req_entry_id(cmd, p, "set")?;
    let rationale = req_text(cmd, p, "rationale")?;
    let agent = s.agent.clone();
    edit(s, |ctx, doc| {
        let node = req_layer(cmd, doc, p, "node")?;
        let set = set_mut(cmd, ctx, &id)?;
        let name = match set.members.iter().find(|m| m.node == node) {
            Some(m) => m.name.clone(),
            None if node == set.source => doc.get(node)?.name.clone(),
            None => return Err(bad(cmd, format!("`node`: layer {node} is neither the source of set {id} nor one of its alternatives"))),
        };
        let asked = if set.question.is_empty() { String::new() } else { format!(": {}", set.question) };
        let title: String = format!("Chose \"{name}\"{asked}").chars().take(MAX_TEXT).collect();
        room_for(cmd, "decisions", ctx.decisions.len(), 1)?;
        let decision = new_decision(ctx, agent.as_deref(), title, rationale, vec![node]);
        let out = json!({"decision": decision.id, "status": decision.status});
        let set = set_mut(cmd, ctx, &id)?;
        set.chosen = Some(node);
        set.decision = decision.id.clone();
        ctx.decisions.push(decision);
        Ok(out)
    })
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.alternative.remove";
    let id = req_entry_id(cmd, p, "set")?;
    let node = get(p, "node").map(|_| req_layer_id(cmd, p, "node")).transpose()?;
    let delete = flag(cmd, p, "deleteLayer")?.unwrap_or(false);
    let doc = s.doc()?;
    let mut ctx = DesignContext::clone(&doc.design);
    let (source, removed): (NodeId, Vec<NodeId>) = match node {
        Some(n) => {
            let set = set_mut(cmd, &mut ctx, &id)?;
            if n == set.source {
                return Err(bad(
                    cmd,
                    format!("`node`: layer {n} is the source of set {id}, not an alternative (leave `node` out to forget the whole set)"),
                ));
            }
            if !set.members.iter().any(|m| m.node == n) {
                return Err(bad(cmd, format!("`node`: layer {n} isn't an alternative in set {id}")));
            }
            set.members.retain(|m| m.node != n);
            if set.chosen == Some(n) {
                set.chosen = None;
                set.decision.clear();
            }
            (set.source, vec![n])
        }
        None => {
            let set = set_mut(cmd, &mut ctx, &id)?;
            let forgotten = (set.source, set.members.iter().map(|m| m.node).collect());
            ctx.alternatives.retain(|v| v.id != id);
            forgotten
        }
    };
    let doc = s.doc_mut()?;
    let mut deleted = Vec::new();
    if delete {
        for layer in removed.iter().copied() {
            // Deleting one member can take another with it (one nested in the other), and a
            // layer that holds the source goes nowhere: the source is never deleted.
            if doc.contains(layer) && !doc.is_ancestor(layer, source) {
                doc.remove(layer)?;
                deleted.push(layer);
            }
        }
    }
    doc.design = Arc::new(ctx);
    Ok(json!({"removed": removed, "deleted": deleted}))
}
