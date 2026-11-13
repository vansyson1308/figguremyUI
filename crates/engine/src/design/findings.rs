//! Running the design checks and keeping findings: what was found, how it was arrived at, and
//! where it stands.
//!
//! The three kinds are settled in different ways and never mixed. A `check` was measured by a
//! rule, so only running the rule again settles it (`design.finding.recheck`): saying it is
//! fixed doesn't make it so. A `judgement` is somebody's opinion and `feedback` is what a person
//! said: only a person confirms either is met or accepts it as it is. An AI agent records what
//! it changed (`design.finding.address`) and leaves the verdict to the person, and doesn't
//! reopen or re-address what a person accepted or verified.
//!
//! A stored check is the same problem as a later result of its rule when both are about the
//! same thing ([`same_problem`]): the result's subject when it names one (a scenario, an
//! overlay, a page), else its layers. Running the rule again verifies the finding only when
//! nothing about that thing is reported any more: not when the failure moved to another step or
//! layer, and not when what was measured can no longer be measured.
//!
//! Who a finding comes from is `source`; who ran the command that stored it is `recordedBy` when
//! that is somebody else (an agent passing on what a person said). An agent's judgement is
//! always its own.

use std::collections::BTreeMap;

use figcraft_doc::NodeId;
use figcraft_doc::design::{DesignContext, Finding, FindingKind, FindingStatus, MAX_ENTRIES, Recheck, Severity};
use serde_json::{Value, json};

use super::checks::{self, Outcome, min_stage, run_rule, same_problem};
use super::input::{actor, agent, choice, edit, layers, req_choice, req_entry_id, req_layer_id, req_text, room_for, shown, text};
use crate::check::{Facts, Found, Scope};
use crate::cmd::CommandSpec;
use crate::params::get;
use crate::proto_verify::{Where, subject_where};
use crate::{Result, Session, bad, spec};

const KINDS: [(&str, FindingKind); 3] = [("check", FindingKind::Check), ("judgement", FindingKind::Judgement), ("feedback", FindingKind::Feedback)];

const SEVERITIES: [(&str, Severity); 4] =
    [("blocker", Severity::Blocker), ("major", Severity::Major), ("minor", Severity::Minor), ("note", Severity::Note)];

const STATUSES: [(&str, FindingStatus); 4] = [
    ("open", FindingStatus::Open),
    ("addressed", FindingStatus::Addressed),
    ("verified", FindingStatus::Verified),
    ("accepted", FindingStatus::Accepted),
];

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "design.check",
            "Check design",
            "{scope?: [layer ids] (each with everything inside it; default: the current page: its top-level frames for the rules that measure layers, and for the flow.* and scenario.* rules everything proto.check reports for the page, also what is about the page itself or a layer outside every frame), rules?: [rule ids] (run exactly these; default: the rules the file's stage calls for, see design.stage.set and design.check.rules)}  measures the file; hidden layers are skipped; nothing is stored. With `scope`, a flow or scenario finding is kept when a layer it names is in scope, and one about a page as a whole when the scope holds that page or one of its top-level frames → {findings: [{rule, severity, nodes, subject? (what it is about when that isn't the layers: scenario:S1, overlay:12, page:1…), message, evidence, suggestion, kind: \"check\"}] (by severity, rule, layer), summary: {total, bySeverity: {blocker, major, minor, note}, byRule}, rules: [{id, about}] (the rules that ran), skipped: [{id, reason}] (the rules that didn't), stage}",
            false,
            check
        ),
        spec!(
            "design.check.rules",
            "List design check rules",
            "{}  → [{id, about (what it measures, its threshold and the standard that comes from), stage (the stage it runs from by default)}]",
            false,
            |_, _| Ok(Value::Array(checks::rules().iter().map(|r| json!({"id": r.id, "about": r.about, "stage": min_stage(r.id)})).collect()))
        ),
        spec!(
            "design.check.record",
            "Record design check findings",
            "{scope?: [layer ids], rules?: [rule ids]}  as design.check, then stores each result as an open finding of kind check unless a stored finding about the same problem (same rule and subject; without a subject, same rule on the same layers) is still unverified. The file keeps at most 500 findings: the results are stored worst first until it is full, and the rest are counted in `truncated` (narrow `scope` or `rules`, or remove settled findings, and record again) → {added: [ids], existing: [ids], total, truncated (results not stored), note? (when truncated), skipped: [{id, reason}]}",
            true,
            record
        ),
        spec!(
            "design.finding.add",
            "Add finding",
            "{kind: judgement|feedback (judgement: a reviewer's or an AI agent's opinion; feedback: what a person said), rule (a short label), message, severity?: blocker|major|minor|note (default major), nodes?: [layer ids], evidence? (what was observed; for feedback the person's words, required), suggestion?, source? (who holds it; default: the AI agent the command runs for, else user; for feedback the person, required from an agent; an AI agent's judgement is always its own: naming anyone else is refused)}  measured findings are stored by design.check.record instead; whoever runs the command is kept as recordedBy when that isn't the source → {id}",
            true,
            add
        ),
        spec!(
            "design.finding.list",
            "List findings",
            "{status?: open|addressed|verified|accepted, kind?: check|judgement|feedback, node? (findings about exactly this layer)} → {findings}",
            false,
            list
        ),
        spec!(
            "design.finding.address",
            "Mark finding addressed",
            "{id, resolution (what was changed)}  a fix is claimed, not confirmed: a check is confirmed by design.finding.recheck, a judgement or feedback by a person (design.finding.verify); refused to an AI agent on a finding a person accepted or verified → {finding}",
            true,
            address
        ),
        spec!(
            "design.finding.recheck",
            "Re-run checks of findings",
            "{id? (a finding of kind check; default: every stored check that isn't accepted)}  runs the finding's rule again where the finding was found: on its layers, or for a finding with a subject (a scenario, an overlay, a page) on the page that subject is on. Nothing about it is reported any more → verified; still reported → open with what is measured now, also when it was marked addressed and when the failure moved (another step fails, another layer opens the overlay); what was measured can no longer be measured (contrast.text) → open; an accepted one stays accepted; its layers, page or scenario are gone → verified → {results: [{id, passed, detail, status}]}",
            true,
            recheck
        ),
        spec!(
            "design.finding.verify",
            "Confirm finding is met",
            "{id, evidence (what was looked at), by? (who confirms; default user)}  for judgements and feedback, and for a person: refused to an AI agent, and for checks (re-run those with design.finding.recheck) → {finding}",
            true,
            verify
        ),
        spec!(
            "design.finding.accept",
            "Accept finding as it is",
            "{id, reason}  left as it is on purpose; a person's call: refused to an AI agent → {finding}",
            true,
            accept
        ),
        spec!(
            "design.finding.reopen",
            "Reopen finding",
            "{id}  refused to an AI agent on a finding a person accepted or verified → {finding}",
            true,
            reopen
        ),
        spec!(
            "design.finding.remove",
            "Remove finding",
            "{id}  refused to an AI agent for feedback, for checks and for judgements it didn't raise itself → {removed}",
            true,
            remove
        ),
    ]
}

/// The layers asked for in `scope` (none: the page).
fn scope_layers(cmd: &str, s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    Ok(layers(cmd, s.doc()?, p, "scope")?.unwrap_or_default())
}

/// The rules named in `rules` (`None`: not given, the stage decides).
fn picked_rules(cmd: &str, p: &Value) -> Result<Option<Vec<String>>> {
    let list = match get(p, "rules") {
        None => return Ok(None),
        Some(Value::Array(a)) => a,
        Some(v) => return Err(bad(cmd, format!("`rules` must be a list of rule ids (got {})", shown(v)))),
    };
    let known: Vec<&str> = checks::rules().iter().map(|r| r.id).collect();
    let mut picked = Vec::new();
    for v in list {
        match v.as_str().filter(|id| known.contains(id)) {
            Some(id) => picked.push(id.to_string()),
            None => return Err(bad(cmd, format!("`rules`: no rule {}; the rules are {}", shown(v), known.join(", ")))),
        }
    }
    Ok((!picked.is_empty()).then_some(picked))
}

fn found_json(f: &Found) -> Value {
    let mut v = json!({"rule": f.rule, "severity": f.severity, "nodes": f.nodes, "message": f.message, "evidence": f.evidence, "suggestion": f.suggestion, "kind": FindingKind::Check});
    if let (Some(o), false) = (v.as_object_mut(), f.subject.is_empty()) {
        o.insert("subject".into(), json!(f.subject));
    }
    v
}

fn skipped_json(run: &Outcome) -> Vec<Value> {
    run.skipped.iter().map(|(id, reason)| json!({"id": id, "reason": reason})).collect()
}

fn check(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.check";
    let (doc, page) = (s.doc()?, s.page()?);
    let layers = scope_layers(cmd, s, p)?;
    let run = checks::run(&Facts::new(doc), Scope { page, layers: &layers }, picked_rules(cmd, p)?.as_deref());
    let count = |sev: Severity| run.found.iter().filter(|f| f.severity == sev).count();
    let mut by_rule: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &run.found {
        *by_rule.entry(f.rule).or_default() += 1;
    }
    Ok(json!({
        "findings": run.found.iter().map(found_json).collect::<Vec<_>>(),
        "summary": {
            "total": run.found.len(),
            "bySeverity": {"blocker": count(Severity::Blocker), "major": count(Severity::Major), "minor": count(Severity::Minor), "note": count(Severity::Note)},
            "byRule": by_rule,
        },
        "rules": run.ran.iter().map(|r| json!({"id": r.id, "about": r.about})).collect::<Vec<_>>(),
        "skipped": skipped_json(&run),
        "stage": doc.design.stage,
    }))
}

fn record(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.check.record";
    let page = s.page()?;
    let only = picked_rules(cmd, p)?;
    let layers = scope_layers(cmd, s, p)?;
    let who = actor(s);
    edit(s, |ctx, doc| {
        let run = checks::run(&Facts::new(doc), Scope { page, layers: &layers }, only.as_deref());
        let (mut added, mut existing): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        // Results the file has no room for: the list is worst first, so those are the least.
        let mut truncated = 0usize;
        for found in &run.found {
            let stored = ctx.findings.iter().find(|f| f.kind == FindingKind::Check && f.status != FindingStatus::Verified && same_problem(f, found));
            match stored {
                Some(f) if !existing.contains(&f.id) && !added.contains(&f.id) => existing.push(f.id.clone()),
                Some(_) => {}
                None if ctx.findings.len() >= MAX_ENTRIES => truncated += 1,
                None => {
                    let id = ctx.next_id("F");
                    ctx.findings.push(Finding {
                        id: id.clone(),
                        kind: FindingKind::Check,
                        rule: found.rule.to_string(),
                        severity: found.severity,
                        nodes: found.nodes.clone(),
                        subject: found.subject.clone(),
                        message: found.message.clone(),
                        evidence: found.evidence.clone(),
                        suggestion: found.suggestion.clone(),
                        source: "check".into(),
                        recorded_by: who.clone(),
                        ..Finding::default()
                    });
                    added.push(id);
                }
            }
        }
        let mut out = json!({"added": added, "existing": existing, "total": run.found.len(), "truncated": truncated, "skipped": skipped_json(&run)});
        if let (Some(o), true) = (out.as_object_mut(), truncated > 0) {
            let note = format!(
                "{truncated} of {} result(s) were not stored: the file holds {MAX_ENTRIES} findings, the most it keeps. The worst were stored first. Narrow `scope` or `rules`, or remove settled findings (design.finding.remove), and record again",
                run.found.len()
            );
            o.insert("note".into(), json!(note));
        }
        Ok(out)
    })
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.add";
    let kind = req_choice(cmd, p, "kind", &KINDS)?;
    if kind == FindingKind::Check {
        return Err(bad(cmd, "`kind`: checks are recorded by design.check.record; a finding added by hand is a judgement or feedback"));
    }
    let rule = req_text(cmd, p, "rule")?;
    let message = req_text(cmd, p, "message")?;
    let severity = choice(cmd, p, "severity", &SEVERITIES)?.unwrap_or_default();
    let evidence = text(cmd, p, "evidence")?.unwrap_or_default();
    let suggestion = text(cmd, p, "suggestion")?.unwrap_or_default();
    let feedback = kind == FindingKind::Feedback;
    let (who, me) = (actor(s), agent(s));
    let source = match text(cmd, p, "source")?.filter(|who| !who.is_empty()) {
        // An agent can't put its opinion in somebody else's mouth, nor its own under feedback.
        Some(named) if !feedback && me.is_some_and(|me| me != named) => {
            return Err(bad(
                cmd,
                format!(
                    "`source`: a judgement an AI agent records is its own ({who}), not {}'s; to pass on what a person said, add it with `kind`: feedback, their name in `source` and their words in `evidence`",
                    shown(&json!(named))
                ),
            ));
        }
        Some(named) if feedback && me == Some(named.as_str()) => {
            return Err(bad(cmd, format!("`source`: feedback is what a person said; {who}'s own view is a judgement (`kind`: judgement)")));
        }
        Some(named) => named,
        // An agent passing on what someone said isn't the one who said it.
        None if feedback && (me.is_some() || get(p, "source").is_some()) => {
            return Err(bad(cmd, "`source` is required for feedback: the person who said it"));
        }
        None => who.clone(),
    };
    // Who ran the command, when the finding is somebody else's: "relayed by".
    let recorded_by = if who == source { String::new() } else { who };
    if feedback && evidence.is_empty() {
        return Err(bad(cmd, "`evidence` is required for feedback: what the person said, in their words"));
    }
    edit(s, |ctx, doc| {
        let nodes = layers(cmd, doc, p, "nodes")?.unwrap_or_default();
        room_for(cmd, "findings", ctx.findings.len(), 1)?;
        let id = ctx.next_id("F");
        ctx.findings.push(Finding {
            id: id.clone(),
            kind,
            rule,
            severity,
            nodes,
            message,
            evidence,
            suggestion,
            source,
            recorded_by,
            ..Finding::default()
        });
        Ok(json!({"id": id}))
    })
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.list";
    let status = choice(cmd, p, "status", &STATUSES)?;
    let kind = choice(cmd, p, "kind", &KINDS)?;
    let node = get(p, "node").map(|_| req_layer_id(cmd, p, "node")).transpose()?;
    let keep = |f: &&Finding| status.is_none_or(|st| f.status == st) && kind.is_none_or(|k| f.kind == k) && node.is_none_or(|n| f.nodes.contains(&n));
    let found: Vec<&Finding> = s.doc()?.design.findings.iter().filter(keep).collect();
    Ok(json!({"findings": found}))
}

fn finding_mut<'a>(cmd: &str, ctx: &'a mut DesignContext, id: &str) -> Result<&'a mut Finding> {
    ctx.findings.iter_mut().find(|f| f.id == id).ok_or_else(|| bad(cmd, format!("`id`: no finding {id}")))
}

/// Refuse an AI agent a verdict on a judgement or on feedback: whether an opinion is settled, or
/// what a person said is met, is for a person to say. `on_feedback` and `on_judgement` say what
/// only a person does.
fn person_decides(cmd: &str, s: &Session, id: &str, on_feedback: &str, on_judgement: &str) -> Result<()> {
    if agent(s).is_none() {
        return Ok(());
    }
    let finding = s.doc()?.design.findings.iter().find(|f| f.id == id);
    let refused = match finding.map(|f| f.kind) {
        Some(FindingKind::Feedback) => format!("only a person {on_feedback}; record what changed with design.finding.address ({id} is feedback)"),
        Some(FindingKind::Judgement) => {
            format!("only a person {on_judgement}; record what changed with design.finding.address ({id} is a judgement)")
        }
        _ => return Ok(()),
    };
    Err(bad(cmd, refused))
}

/// Refuse an AI agent a change of status on a finding a person settled: accepting one as it is
/// is always a person's doing, and so is verifying a judgement or feedback. Addressing or
/// reopening it would drop their reason or their confirmation.
fn settled_by_a_person(cmd: &str, me: Option<&str>, f: &Finding) -> Result<()> {
    let Some(me) = me else { return Ok(()) };
    let settled = match f.status {
        FindingStatus::Accepted => "accepted as it is by the user".to_string(),
        FindingStatus::Verified if !f.verified_by.is_empty() => format!("verified by {}", f.verified_by),
        _ => return Ok(()),
    };
    Err(bad(
        cmd,
        format!(
            "`id`: {} was {settled}; an AI agent ({me}) doesn't undo that. If you think the problem is back, say so in a judgement of your own (design.finding.add) or ask them to reopen it",
            f.id
        ),
    ))
}

fn address(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.address";
    let id = req_entry_id(cmd, p, "id")?;
    let resolution = req_text(cmd, p, "resolution")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let f = finding_mut(cmd, ctx, &id)?;
        settled_by_a_person(cmd, me.as_deref(), f)?;
        f.status = FindingStatus::Addressed;
        f.resolution = resolution;
        f.verified_by.clear();
        f.verification.clear();
        Ok(json!({"finding": f}))
    })
}

fn verify(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.verify";
    let id = req_entry_id(cmd, p, "id")?;
    let evidence = req_text(cmd, p, "evidence")?;
    let by = text(cmd, p, "by")?.filter(|b| !b.is_empty()).unwrap_or_else(|| actor(s));
    person_decides(cmd, s, &id, "confirms that feedback is met", "confirms a judgement is settled")?;
    edit(s, |ctx, _| {
        let f = finding_mut(cmd, ctx, &id)?;
        if f.kind == FindingKind::Check {
            return Err(bad(cmd, format!("`id`: {id} is a check; re-run it with design.finding.recheck")));
        }
        f.status = FindingStatus::Verified;
        f.verified_by = by;
        f.verification = evidence;
        Ok(json!({"finding": f}))
    })
}

fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.accept";
    let id = req_entry_id(cmd, p, "id")?;
    let reason = req_text(cmd, p, "reason")?;
    person_decides(cmd, s, &id, "accepts feedback as it is", "accepts a judgement as it is")?;
    // Leaving a measured problem in place is a decision about the design, so a person's too.
    if agent(s).is_some() {
        return Err(bad(cmd, format!("only a person accepts a finding as it is; leave {id} open and say why you didn't fix it")));
    }
    edit(s, |ctx, _| {
        let f = finding_mut(cmd, ctx, &id)?;
        f.status = FindingStatus::Accepted;
        f.resolution = reason;
        f.verified_by.clear();
        f.verification.clear();
        Ok(json!({"finding": f}))
    })
}

fn reopen(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.reopen";
    let id = req_entry_id(cmd, p, "id")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let f = finding_mut(cmd, ctx, &id)?;
        settled_by_a_person(cmd, me.as_deref(), f)?;
        f.status = FindingStatus::Open;
        f.verified_by.clear();
        f.verification.clear();
        Ok(json!({"finding": f}))
    })
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.remove";
    let id = req_entry_id(cmd, p, "id")?;
    let me = agent(s).map(str::to_string);
    edit(s, |ctx, _| {
        let f = finding_mut(cmd, ctx, &id)?;
        match (&me, f.kind) {
            (Some(_), FindingKind::Feedback) => return Err(bad(cmd, format!("`id`: {id} is feedback from {}; only a person removes it", f.source))),
            (Some(me), FindingKind::Judgement) if f.source != *me => {
                return Err(bad(cmd, format!("`id`: {id} is a judgement by {}; only they or the user remove it", f.source)));
            }
            // Removing a measured finding would drop it without the rule passing.
            (Some(_), FindingKind::Check) => {
                return Err(bad(cmd, format!("`id`: {id} is a check; fix it and run design.finding.recheck, only a person removes it")));
            }
            _ => {}
        }
        ctx.findings.retain(|f| f.id != id);
        Ok(json!({"removed": id}))
    })
}

/// What running a stored check's rule again gave.
enum Rerun {
    Passed(String),
    /// Still there: what is measured now, and the subject the result names when the stored
    /// finding (from before subjects existed) has none yet.
    Failed(String, Option<String>),
    /// The rule isn't one this version runs.
    NoRule,
}

/// Run the rule of stored check `f` again where it would report the finding again: on the
/// finding's layers, or for a finding with a subject on the page that subject is on (see
/// [`subject_where`]; `page`, the session's, when the subject isn't a place).
fn rerun(facts: &Facts<'_>, page: NodeId, f: &Finding) -> Rerun {
    let doc = facts.doc;
    let live: Vec<NodeId> = f.nodes.iter().copied().filter(|n| doc.contains(*n)).collect();
    let legacy = f.subject.is_empty();
    let (pages, layers): (Vec<NodeId>, &[NodeId]) = if legacy {
        // The layers are what the finding is about.
        if !f.nodes.is_empty() && live.is_empty() {
            return Rerun::Passed("the layer no longer exists".into());
        }
        (vec![live.first().and_then(|n| doc.page_of(*n)).unwrap_or(page)], &live)
    } else {
        match subject_where(doc, &f.subject) {
            Where::Page(on) => (vec![on], &[]),
            Where::Gone(why) => return Rerun::Passed(why),
            Where::EveryPage => (doc.pages.clone(), &[]),
            Where::Anywhere => (vec![page], &[]),
        }
    };
    let same = |r: &Found| {
        same_problem(f, r)
            // Layers of the finding that are gone can't be reported again: what is left counts.
            || (legacy && r.subject.is_empty() && !live.is_empty() && r.nodes == live)
            // Stored before the rule named the page as its subject: the rule still reporting
            // the page is the finding still being there, whichever frame it points at now.
            || (legacy && r.subject.starts_with("page:"))
    };
    for on in pages {
        let Some((found, unsettled)) = run_rule(facts, &f.rule, Scope { page: on, layers }) else { return Rerun::NoRule };
        if let Some(r) = found.iter().chain(&unsettled).find(|r| same(r)) {
            return Rerun::Failed(r.evidence.clone(), (legacy && !r.subject.is_empty()).then(|| r.subject.clone()));
        }
    }
    Rerun::Passed(format!("`{}` no longer reports it", f.rule))
}

fn recheck(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "design.finding.recheck";
    let only = get(p, "id").map(|_| req_entry_id(cmd, p, "id")).transpose()?;
    let page = s.page()?;
    edit(s, |ctx, doc| {
        if let Some(id) = &only {
            let f = finding_mut(cmd, ctx, id)?;
            if f.kind != FindingKind::Check {
                return Err(bad(cmd, format!("`id`: only checks can be re-run; use design.finding.verify ({id} isn't a check)")));
            }
        }
        // One set of facts for the whole call: the played copy, each page's flow graph and the
        // runs of its scenarios are worked out once, however many findings read them.
        let facts = Facts::new(doc);
        let mut results = Vec::new();
        let wanted = |f: &Finding| match &only {
            Some(id) => f.id == *id,
            None => f.kind == FindingKind::Check && f.status != FindingStatus::Accepted,
        };
        for f in ctx.findings.iter_mut().filter(|f| wanted(f)) {
            let (passed, detail) = match rerun(&facts, page, f) {
                Rerun::Passed(detail) => (true, detail),
                Rerun::Failed(detail, subject) => {
                    if let Some(subject) = subject {
                        f.subject = subject;
                    }
                    (false, detail)
                }
                Rerun::NoRule if only.is_some() => return Err(bad(cmd, format!("`id`: {}'s rule `{}` isn't one this version checks", f.id, f.rule))),
                Rerun::NoRule => {
                    results.push(json!({"id": f.id, "passed": null, "detail": format!("rule `{}` isn't one this version checks: not re-run", f.rule), "status": f.status}));
                    continue;
                }
            };
            // What a person accepted as it is stays accepted while it is still there.
            f.status = match (passed, f.status) {
                (true, _) => FindingStatus::Verified,
                (false, FindingStatus::Accepted) => FindingStatus::Accepted,
                (false, _) => FindingStatus::Open,
            };
            f.verified_by.clear();
            f.verification.clear();
            f.recheck = Some(Recheck { passed, detail: detail.clone() });
            results.push(json!({"id": f.id, "passed": passed, "detail": detail, "status": f.status}));
        }
        Ok(json!({"results": results}))
    })
}
