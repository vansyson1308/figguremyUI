//! The brief, the stage, assumptions, decisions, and reading the context back.

use figcraft_doc::design::{AssumptionStatus, DecisionStatus, MAX_ENTRIES, MAX_TEXT, Stage};
use serde_json::json;

use super::{AGENT_NAME, Session, agent_err, as_agent, err, ex, ex_undoable, frame, rect, refused_unchanged, reopened};

#[test]
fn a_file_without_design_context_has_no_design_key() {
    let mut s = Session::new();
    frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let json = figcraft_format::to_json(s.doc().unwrap()).unwrap();
    assert!(json.get("design").is_none(), "an untouched file must serialize as before");
    assert!(serde_json::to_value(s.doc().unwrap()).unwrap().get("design").is_none());
    // Reading the context doesn't create it.
    ex(&mut s, "design.get", json!({}));
    ex(&mut s, "design.check", json!({}));
    assert!(figcraft_format::to_json(s.doc().unwrap()).unwrap().get("design").is_none());
    assert_eq!(s.undo_depth().0, 1, "only the frame is an undo step");
}

#[test]
fn the_brief_is_patched_trimmed_and_cleared() {
    let mut s = Session::new();
    let r = ex_undoable(
        &mut s,
        "design.brief.set",
        json!({"audience": "  Night-shift nurses ", "sponsor": "The ward manager, who wants fewer handover errors", "constraints": [" one hand ", "gloves"]}),
    );
    assert_eq!(r["brief"]["audience"], "Night-shift nurses");
    assert_eq!(r["brief"]["constraints"], json!(["one hand", "gloves"]));
    assert_eq!(r["briefRevision"], 1);
    // Only the given keys change.
    let r = ex_undoable(&mut s, "design.brief.set", json!({"job": "Hand over a patient", "format": "A phone app, at the bedside"}));
    assert_eq!(r["brief"]["audience"], "Night-shift nurses");
    assert_eq!(r["brief"]["sponsor"], "The ward manager, who wants fewer handover errors");
    assert_eq!(r["brief"]["job"], "Hand over a patient");
    assert_eq!(r["brief"]["format"], "A phone app, at the bedside");
    assert_eq!(r["briefRevision"], 2);
    // "" and [] clear.
    let r = ex_undoable(&mut s, "design.brief.set", json!({"audience": "", "constraints": []}));
    assert!(r["brief"].get("audience").is_none() && r["brief"].get("constraints").is_none(), "{r}");
    let b = &s.doc().unwrap().design.brief;
    assert!(b.audience.is_empty() && b.constraints.is_empty() && b.job == "Hand over a patient");
    for key in ["context", "content", "tone"] {
        ex(&mut s, "design.brief.set", json!({key: "x"}));
    }
    ex(&mut s, "design.brief.set", json!({"success": ["handover under a minute"]}));
    let b = &s.doc().unwrap().design.brief;
    assert_eq!((b.context.as_str(), b.content.as_str(), b.tone.as_str(), b.success.len()), ("x", "x", "x", 1));
}

#[test]
fn the_brief_revision_rises_only_when_the_brief_changes() {
    let mut s = Session::new();
    ex(&mut s, "design.brief.set", json!({"audience": "Cyclists"}));
    let (revision, depth) = (s.doc().unwrap().design.brief_revision, s.undo_depth());
    assert_eq!(revision, 1);
    // The same value again, with spaces around it, and a call with nothing: no change.
    for same in [json!({"audience": " Cyclists "}), json!({})] {
        let r = ex(&mut s, "design.brief.set", same);
        assert_eq!(r["briefRevision"], 1, "{r}");
    }
    assert_eq!(s.undo_depth(), depth, "a set that changes nothing is no undo step");
    ex(&mut s, "design.brief.set", json!({"audience": "Commuting cyclists"}));
    assert_eq!(s.doc().unwrap().design.brief_revision, 2);
    assert_eq!(ex(&mut s, "design.get", json!({}))["briefRevision"], 2);
}

#[test]
fn brief_errors_name_the_field_and_change_nothing() {
    let mut s = Session::new();
    ex(&mut s, "design.brief.set", json!({"audience": "Cyclists"}));
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"job": "ok", "audience": 5}));
    assert!(e.contains("`audience`") && e.contains('5'), "{e}");
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"constraints": "one hand"}));
    assert!(e.contains("`constraints`") && e.contains("list"), "{e}");
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"success": ["ok", ""]}));
    assert!(e.contains("`success`") && e.contains("entry 2"), "{e}");
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"tone": "x".repeat(MAX_TEXT + 1)}));
    assert!(e.contains("`tone`") && e.contains(&MAX_TEXT.to_string()), "{e}");
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"constraints": vec!["x"; MAX_ENTRIES + 1]}));
    assert!(e.contains("`constraints`") && e.contains(&MAX_ENTRIES.to_string()), "{e}");
    let e = refused_unchanged(&mut s, "design.brief.set", json!({"audiance": "typo"}));
    assert!(e.contains("audiance"), "{e}");
    // At the limit is fine.
    ex(&mut s, "design.brief.set", json!({"tone": "x".repeat(MAX_TEXT), "constraints": vec!["x"; MAX_ENTRIES]}));
}

#[test]
fn the_stage_is_set_and_refuses_unknown_names() {
    let mut s = Session::new();
    assert_eq!(ex(&mut s, "design.get", json!({}))["stage"], "exploring");
    assert_eq!(ex_undoable(&mut s, "design.stage.set", json!({"stage": "converging"}))["stage"], "converging");
    assert_eq!(s.doc().unwrap().design.stage, Stage::Converging);
    assert_eq!(ex_undoable(&mut s, "design.stage.set", json!({"stage": "Ready"}))["stage"], "ready");
    let e = refused_unchanged(&mut s, "design.stage.set", json!({"stage": "done"}));
    assert!(e.contains("`stage`") && e.contains("exploring, converging, ready") && e.contains("done"), "{e}");
    let e = refused_unchanged(&mut s, "design.stage.set", json!({}));
    assert!(e.contains("`stage` is required"), "{e}");
}

#[test]
fn assumptions_are_added_edited_and_removed() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let a =
        ex_undoable(&mut s, "design.assumption.add", json!({"text": " People arrive with a patient id in hand ", "links": [home, home.to_string()]}));
    assert_eq!(a["id"], "A1");
    let stored = s.doc().unwrap().design.assumptions[0].clone();
    assert_eq!(
        (stored.text.as_str(), stored.status, stored.links.as_slice()),
        ("People arrive with a patient id in hand", AssumptionStatus::Untested, &[home][..])
    );
    // The user records evidence and what it showed.
    let r = ex_undoable(&mut s, "design.assumption.set", json!({"id": "A1", "status": "supported", "evidence": "Ward visit, 12 of 14 handovers"}));
    assert_eq!(r["assumption"]["status"], "supported");
    assert_eq!(r["assumption"]["text"], "People arrive with a patient id in hand", "the patch leaves the text alone");
    let r = ex_undoable(&mut s, "design.assumption.set", json!({"id": "a1", "links": []}));
    assert!(r["assumption"].get("links").is_none(), "{r}");
    // A decision resting on it forgets it when it is removed.
    ex(&mut s, "design.decision.add", json!({"title": "Scan first", "rationale": "The id is at hand", "assumptions": ["A1"]}));
    assert_eq!(ex_undoable(&mut s, "design.assumption.remove", json!({"id": "A1"}))["removed"], "A1");
    let ctx = &s.doc().unwrap().design;
    assert!(ctx.assumptions.is_empty() && ctx.decisions[0].assumptions.is_empty());
    // Ids aren't reused.
    assert_eq!(ex(&mut s, "design.assumption.add", json!({"text": "Another"}))["id"], "A2");
}

#[test]
fn assumption_errors_name_the_parameter() {
    let mut s = Session::new();
    ex(&mut s, "design.assumption.add", json!({"text": "Wards have wifi"}));
    for (params, needle) in [
        (json!({}), "`text` is required"),
        (json!({"text": "  "}), "`text` can't be empty"),
        (json!({"text": "x", "links": [4711]}), "`links`: no layer 4711"),
        (json!({"text": "x", "links": ["soon"]}), "`links`: \"soon\" isn't a layer id"),
        (json!({"text": "x", "status": "proven"}), "`status` must be one of untested, supported, refuted"),
        (json!({"text": "x", "status": "supported"}), "`evidence` is required"),
        (json!({"text": "x", "status": "refuted", "evidence": " "}), "`evidence` is required"),
        (json!({"text": "x".repeat(MAX_TEXT + 1)}), "`text` is 4001 characters long"),
    ] {
        let e = refused_unchanged(&mut s, "design.assumption.add", params.clone());
        assert!(e.contains(needle), "{params}: {e}");
    }
    for (params, needle) in [
        (json!({"id": "A9", "text": "x"}), "`id`: no assumption A9"),
        (json!({"text": "x"}), "`id` is required"),
        (json!({"id": "A1", "text": ""}), "`text` can't be empty"),
        (json!({"id": "A1", "status": "supported"}), "`evidence` is required"),
        (json!({"id": "A1", "links": [99999]}), "`links`: no layer 99999"),
    ] {
        let e = refused_unchanged(&mut s, "design.assumption.set", params.clone());
        assert!(e.contains(needle), "{params}: {e}");
    }
    let e = refused_unchanged(&mut s, "design.assumption.remove", json!({"id": "A7"}));
    assert!(e.contains("`id`: no assumption A7"), "{e}");
    // Clearing the evidence of a supported assumption would leave the claim without a source.
    ex(&mut s, "design.assumption.set", json!({"id": "A1", "status": "supported", "evidence": "Site survey"}));
    let e = refused_unchanged(&mut s, "design.assumption.set", json!({"id": "A1", "evidence": ""}));
    assert!(e.contains("`evidence` is required"), "{e}");
}

#[test]
fn only_a_person_records_that_an_assumption_was_supported_or_refuted() {
    let mut s = Session::new();
    for status in ["supported", "refuted"] {
        let e = agent_err(&mut s, "design.assumption.add", json!({"text": "x", "status": status, "evidence": "I looked at the layout"}));
        assert!(e.contains("only a person records that an assumption was supported or refuted") && e.contains("`evidence`"), "{e}");
    }
    assert!(s.doc().unwrap().design.assumptions.is_empty());
    // An agent adds assumptions, untested, and may write down what it found.
    let id =
        as_agent(&mut s, "design.assumption.add", json!({"text": "Users know the ward code", "status": "untested", "evidence": "The brief says so"}))
            ["id"]
            .clone();
    as_agent(&mut s, "design.assumption.set", json!({"id": id, "evidence": "Asked in the thread; no answer yet"}));
    let e = agent_err(&mut s, "design.assumption.set", json!({"id": id, "status": "supported"}));
    assert!(e.contains("only a person records"), "{e}");
    assert_eq!(s.doc().unwrap().design.assumptions[0].status, AssumptionStatus::Untested);
    // The user can.
    ex(&mut s, "design.assumption.set", json!({"id": id, "status": "refuted", "evidence": "Interview 3: nobody knew it"}));
    assert_eq!(s.doc().unwrap().design.assumptions[0].status, AssumptionStatus::Refuted);
    // What was refuted is the statement as it reads: an agent can't reword it and keep the verdict.
    let e = agent_err(&mut s, "design.assumption.set", json!({"id": id, "text": "Users know the ward name"}));
    assert!(e.contains("`text`") && e.contains("add another assumption"), "{e}");
    as_agent(&mut s, "design.assumption.set", json!({"id": id, "text": "Users know the ward name", "status": "untested"}));
    let a = &s.doc().unwrap().design.assumptions[0];
    assert_eq!((a.text.as_str(), a.status), ("Users know the ward name", AssumptionStatus::Untested));
}

#[test]
fn decisions_record_who_made_them_and_whether_a_person_accepted() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    ex(&mut s, "design.brief.set", json!({"audience": "Nurses"}));
    ex(&mut s, "design.assumption.add", json!({"text": "One hand is free"}));
    let r = ex_undoable(
        &mut s,
        "design.decision.add",
        json!({"title": "Bottom actions", "rationale": "Thumb reach", "rejected": "A top bar: out of reach", "assumptions": ["A1"], "links": [home]}),
    );
    assert_eq!((r["id"].as_str(), r["status"].as_str()), (Some("D1"), Some("accepted")));
    let d = s.doc().unwrap().design.decisions[0].clone();
    assert_eq!((d.author.as_str(), d.status, d.brief_revision), ("user", DecisionStatus::Accepted, 1));
    assert_eq!((d.rejected.as_str(), d.assumptions, d.links), ("A top bar: out of reach", vec!["A1".to_string()], vec![home]));

    // An agent's decision is a proposal, and it can't accept it itself.
    let r = as_agent(&mut s, "design.decision.add", json!({"title": "Large type", "rationale": "Read at arm's length"}));
    assert_eq!((r["id"].as_str(), r["status"].as_str()), (Some("D2"), Some("proposed")));
    assert_eq!(s.doc().unwrap().design.decisions[1].author, AGENT_NAME);
    let e = agent_err(&mut s, "design.decision.accept", json!({"id": "D2"}));
    assert!(e.contains("a person accepts a decision"), "{e}");
    assert_eq!(s.doc().unwrap().design.decisions[1].status, DecisionStatus::Proposed);
    // The user accepts it, as the brief is now.
    ex(&mut s, "design.brief.set", json!({"job": "Hand over"}));
    let r = ex_undoable(&mut s, "design.decision.accept", json!({"id": "D2"}));
    assert_eq!((r["decision"]["status"].as_str(), r["decision"]["briefRevision"].as_u64()), (Some("accepted"), Some(2)));

    // An agent's change to an accepted decision makes it a proposal again; the author stays.
    let r = as_agent(&mut s, "design.decision.set", json!({"id": "D1", "rationale": "Thumb reach, and the keyboard covers the top"}));
    assert_eq!(r["decision"]["status"], "proposed");
    assert_eq!(r["decision"]["author"], "user");
    // The user's own edit of an accepted decision leaves it accepted, and a patch keeps the rest.
    let r = ex_undoable(&mut s, "design.decision.set", json!({"id": "D2", "title": "Large type everywhere", "links": [home], "assumptions": []}));
    assert_eq!((r["decision"]["status"].as_str(), r["decision"]["rationale"].as_str()), (Some("accepted"), Some("Read at arm's length")));
    // An agent's call that changes nothing leaves an accepted decision accepted.
    as_agent(&mut s, "design.decision.set", json!({"id": "D2", "title": "Large type everywhere"}));
    assert_eq!(s.doc().unwrap().design.decisions[1].status, DecisionStatus::Accepted);

    assert_eq!(ex_undoable(&mut s, "design.decision.remove", json!({"id": "D1"}))["removed"], "D1");
    assert_eq!(s.doc().unwrap().design.decisions.len(), 1);
}

#[test]
fn decision_errors_name_the_parameter() {
    let mut s = Session::new();
    ex(&mut s, "design.decision.add", json!({"title": "T", "rationale": "R"}));
    for (params, needle) in [
        (json!({"rationale": "R"}), "`title` is required"),
        (json!({"title": "T", "rationale": ""}), "`rationale` can't be empty"),
        (json!({"title": "T", "rationale": "R", "assumptions": ["A9"]}), "`assumptions`: no assumption A9"),
        (json!({"title": "T", "rationale": "R", "assumptions": "A1"}), "`assumptions` must be a list"),
        (json!({"title": "T", "rationale": "R", "links": [31337]}), "`links`: no layer 31337"),
        (json!({"title": ["T"], "rationale": "R"}), "`title` must be text"),
    ] {
        let e = refused_unchanged(&mut s, "design.decision.add", params.clone());
        assert!(e.contains(needle), "{params}: {e}");
    }
    for (cmd, params, needle) in [
        ("design.decision.set", json!({"id": "D5", "title": "x"}), "`id`: no decision D5"),
        ("design.decision.set", json!({"id": "D1", "title": " "}), "`title` can't be empty"),
        ("design.decision.set", json!({"id": "D1", "assumptions": ["A3"]}), "`assumptions`: no assumption A3"),
        ("design.decision.remove", json!({"id": "D5"}), "`id`: no decision D5"),
        ("design.decision.accept", json!({"id": "D5"}), "`id`: no decision D5"),
        ("design.decision.accept", json!({}), "`id` is required"),
    ] {
        let e = refused_unchanged(&mut s, cmd, params.clone());
        assert!(e.contains(needle), "{cmd} {params}: {e}");
    }
}

#[test]
fn get_reports_everything_with_counts_and_missing_links() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let button = rect(&mut s, home, [10.0, 10.0, 100.0, 40.0], json!({"name": "Button"}));
    ex(&mut s, "design.assumption.add", json!({"text": "Tapped with a thumb", "links": [button]}));
    ex(&mut s, "design.assumption.add", json!({"text": "Seen outdoors", "status": "supported", "evidence": "Field notes"}));
    as_agent(&mut s, "design.decision.add", json!({"title": "Big button", "rationale": "Thumb", "links": [button, home]}));
    ex(
        &mut s,
        "design.finding.add",
        json!({"kind": "judgement", "rule": "hierarchy", "message": "The button competes with the title", "nodes": [button]}),
    );
    let r = ex(&mut s, "design.get", json!({}));
    for key in ["brief", "assumptions", "decisions", "scenarios", "endStates", "findings", "alternatives", "missingLinks", "counts"] {
        assert!(r.get(key).is_some(), "design.get has no `{key}`: {r}");
    }
    assert_eq!(r["missingLinks"], json!([]));
    assert_eq!(
        r["counts"],
        json!({"assumptions": 2, "untestedAssumptions": 1, "decisions": 1, "proposedDecisions": 1, "scenarios": 0, "endStates": 0, "findings": 1, "openFindings": 1, "alternatives": 0, "missingLinks": 0})
    );
    assert_eq!(r["assumptions"][0]["links"], json!([button]));
    assert_eq!(r["decisions"][0]["author"], AGENT_NAME);

    // What links to exactly the button; the frame around it has its own links only.
    let on_button = ex(&mut s, "design.forNode", json!({"id": button}));
    assert_eq!(
        on_button,
        json!({"assumptions": ["A1"], "decisions": ["D1"], "scenarios": [], "findings": ["F1"], "alternatives": [], "endState": false})
    );
    let on_frame = ex(&mut s, "design.forNode", json!({"id": home.to_string()}));
    assert_eq!(on_frame["assumptions"], json!([]), "a link to a layer inside isn't a link to the frame");
    assert_eq!(on_frame["decisions"], json!(["D1"]));
    assert!(err(&mut s, "design.forNode", json!({})).contains("`id` is required"));
    assert!(err(&mut s, "design.forNode", json!({"id": "the button"})).contains("`id`"));

    // The layer goes: the links stay in the file and are reported, not dropped.
    ex(&mut s, "edit.delete", json!({"ids": [button]}));
    let r = ex(&mut s, "design.get", json!({}));
    assert_eq!(r["missingLinks"], json!([{"node": button, "from": "A1"}, {"node": button, "from": "D1"}, {"node": button, "from": "F1"}]));
    assert_eq!(r["counts"]["missingLinks"], 3);
    assert_eq!(s.doc().unwrap().design.assumptions[0].links, vec![button]);
    assert_eq!(ex(&mut s, "design.forNode", json!({"id": button}))["assumptions"], json!(["A1"]), "a gone layer's links can still be asked for");
}

#[test]
fn the_context_survives_save_and_reopen() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    ex(
        &mut s,
        "design.brief.set",
        json!({"audience": "Nurses", "sponsor": "Ward manager", "format": "Phone", "success": ["Handover under a minute"]}),
    );
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    ex(&mut s, "design.assumption.add", json!({"text": "One hand is free", "links": [home], "status": "supported", "evidence": "Ward visit"}));
    as_agent(&mut s, "design.decision.add", json!({"title": "Bottom actions", "rationale": "Thumb reach", "assumptions": ["A1"], "links": [home]}));
    ex(
        &mut s,
        "design.finding.add",
        json!({"kind": "feedback", "rule": "wording", "message": "Too formal", "evidence": "\"Reads like a form\"", "source": "Ana"}),
    );
    ex(
        &mut s,
        "design.alternative.create",
        json!({"source": home, "name": "Dense", "constraint": "No scrolling", "question": "Does it fit one screen?", "constraints": ["Same content"]}),
    );
    let back = reopened(&s);
    assert_eq!(back.design, s.doc().unwrap().design, "the design context changed on the way through the file");
    assert_eq!(back.design.brief_revision, 1);
    assert_eq!(back.design.stage, Stage::Converging);
    assert_eq!(back.design.decisions[0].status, DecisionStatus::Proposed);
    assert_eq!(back.design.alternatives[0].constraints, vec!["Same content".to_string()]);
    // The file names things in camelCase.
    let json = figcraft_format::to_json(s.doc().unwrap()).unwrap();
    assert_eq!(json["design"]["briefRevision"], 1);
    assert_eq!(json["design"]["stage"], "converging");
    assert_eq!(json["design"]["decisions"][0]["briefRevision"], 1);
}
