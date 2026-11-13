//! Stored findings: adding judgements and feedback, their life cycle, and who may settle what.

use figcraft_doc::design::{FindingKind, FindingStatus, Severity};
use serde_json::json;

use super::{AGENT_NAME, Session, Value, agent_err, as_agent, err, ex, ex_undoable, frame, rect, refused_unchanged};

/// A judgement by the user, one by the agent and feedback from a person: F1, F2, F3.
fn three_findings(s: &mut Session) {
    ex(s, "design.finding.add", json!({"kind": "judgement", "rule": "hierarchy", "message": "Two things shout"}));
    as_agent(s, "design.finding.add", json!({"kind": "judgement", "rule": "density", "message": "The list feels cramped", "severity": "minor"}));
    as_agent(
        s,
        "design.finding.add",
        json!({"kind": "feedback", "rule": "wording", "message": "The label is unclear", "evidence": "\"What does Sync mean here?\"", "source": "Ana"}),
    );
}

fn status(s: &Session, id: &str) -> FindingStatus {
    s.doc().unwrap().design.findings.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("no finding {id}")).status
}

#[test]
fn judgements_and_feedback_are_added_with_their_source() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    let r = ex_undoable(
        &mut s,
        "design.finding.add",
        json!({"kind": "judgement", "rule": " hierarchy ", "message": "Two things shout", "nodes": [home], "evidence": "Title and banner are both 32 px bold", "suggestion": "Quieten the banner"}),
    );
    assert_eq!(r["id"], "F1");
    let f = s.doc().unwrap().design.findings[0].clone();
    assert_eq!((f.kind, f.rule.as_str(), f.severity, f.status), (FindingKind::Judgement, "hierarchy", Severity::Major, FindingStatus::Open));
    assert_eq!((f.source.as_str(), f.nodes, f.suggestion.as_str()), ("user", vec![home], "Quieten the banner"));
    // Run for an agent, the finding is the agent's unless it says whose it is.
    as_agent(&mut s, "design.finding.add", json!({"kind": "judgement", "rule": "density", "message": "Cramped", "severity": "note"}));
    as_agent(&mut s, "design.finding.add", json!({"kind": "judgement", "rule": "tone", "message": "Stiff", "source": "Review with Bo"}));
    let found = &s.doc().unwrap().design.findings;
    assert_eq!((found[1].source.as_str(), found[1].severity), (AGENT_NAME, Severity::Note));
    assert_eq!(found[2].source, "Review with Bo");
    // Feedback is a person's words: both the person and the words are required.
    let r = ex_undoable(
        &mut s,
        "design.finding.add",
        json!({"kind": "feedback", "rule": "wording", "message": "Unclear label", "evidence": "\"What is Sync?\"", "source": "Ana"}),
    );
    assert_eq!(r["id"], "F4");
    ex(&mut s, "design.finding.add", json!({"kind": "feedback", "rule": "wording", "message": "Unclear", "evidence": "I don't get it"}));
    assert_eq!(s.doc().unwrap().design.findings[4].source, "user", "the user's own feedback is the user's");
}

#[test]
fn finding_add_errors_name_the_parameter() {
    let mut s = Session::new();
    for (params, needle) in [
        (json!({"rule": "x", "message": "m"}), "`kind` is required (one of check, judgement, feedback)"),
        (json!({"kind": "check", "rule": "contrast.text", "message": "m"}), "checks are recorded by design.check.record"),
        (json!({"kind": "opinion", "rule": "x", "message": "m"}), "`kind` must be one of"),
        (json!({"kind": "judgement", "message": "m"}), "`rule` is required"),
        (json!({"kind": "judgement", "rule": "x"}), "`message` is required"),
        (json!({"kind": "judgement", "rule": "x", "message": " "}), "`message` can't be empty"),
        (json!({"kind": "judgement", "rule": "x", "message": "m", "severity": "huge"}), "`severity` must be one of blocker, major, minor, note"),
        (json!({"kind": "judgement", "rule": "x", "message": "m", "nodes": [777]}), "`nodes`: no layer 777"),
        (json!({"kind": "feedback", "rule": "x", "message": "m", "source": "Ana"}), "`evidence` is required for feedback"),
        (json!({"kind": "feedback", "rule": "x", "message": "m", "source": "", "evidence": "words"}), "`source` is required for feedback"),
    ] {
        let e = refused_unchanged(&mut s, "design.finding.add", params.clone());
        assert!(e.contains(needle), "{params}: {e}");
    }
    // An agent passing feedback on has to say who said it: it isn't the agent's own.
    let e = agent_err(&mut s, "design.finding.add", json!({"kind": "feedback", "rule": "x", "message": "m", "evidence": "words"}));
    assert!(e.contains("`source` is required for feedback"), "{e}");
    assert!(s.doc().unwrap().design.findings.is_empty());
}

#[test]
fn findings_are_listed_by_status_kind_and_layer() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    three_findings(&mut s);
    ex(&mut s, "design.finding.add", json!({"kind": "judgement", "rule": "spacing", "message": "Uneven", "nodes": [home]}));
    ex(&mut s, "design.finding.address", json!({"id": "F1", "resolution": "Quietened the banner"}));
    let ids = |s: &mut Session, p: Value| -> Vec<String> {
        ex(s, "design.finding.list", p)["findings"].as_array().unwrap().iter().map(|f| f["id"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(ids(&mut s, json!({})), ["F1", "F2", "F3", "F4"]);
    assert_eq!(ids(&mut s, json!({"status": "open"})), ["F2", "F3", "F4"]);
    assert_eq!(ids(&mut s, json!({"status": "addressed"})), ["F1"]);
    assert_eq!(ids(&mut s, json!({"kind": "feedback"})), ["F3"]);
    assert_eq!(ids(&mut s, json!({"kind": "judgement", "status": "open"})), ["F2", "F4"]);
    assert_eq!(ids(&mut s, json!({"node": home})), ["F4"]);
    assert_eq!(ids(&mut s, json!({"node": 999_999})), Vec::<String>::new());
    assert!(err(&mut s, "design.finding.list", json!({"status": "closed"})).contains("`status` must be one of open, addressed, verified, accepted"));
    assert!(err(&mut s, "design.finding.list", json!({"kind": "bug"})).contains("`kind` must be one of"));
    assert!(err(&mut s, "design.finding.list", json!({"node": "it"})).contains("`node`"));
    assert_eq!(s.undo_depth().0, 6, "listing is no undo step");
}

#[test]
fn a_finding_moves_through_its_life_as_one_undo_step_each() {
    let mut s = Session::new();
    three_findings(&mut s);
    let r = ex_undoable(&mut s, "design.finding.address", json!({"id": "F1", "resolution": "Quietened the banner"}));
    assert_eq!((r["finding"]["status"].as_str(), r["finding"]["resolution"].as_str()), (Some("addressed"), Some("Quietened the banner")));
    let r = ex_undoable(&mut s, "design.finding.verify", json!({"id": "F1", "evidence": "Looked at Home at 100%", "by": "Ana"}));
    assert_eq!((r["finding"]["status"].as_str(), r["finding"]["verifiedBy"].as_str()), (Some("verified"), Some("Ana")));
    assert_eq!(r["finding"]["verification"], "Looked at Home at 100%");
    let r = ex_undoable(&mut s, "design.finding.reopen", json!({"id": "F1"}));
    assert_eq!(r["finding"]["status"], "open");
    assert!(r["finding"].get("verifiedBy").is_none(), "{r}");
    let r = ex_undoable(&mut s, "design.finding.accept", json!({"id": "F2", "reason": "The list is dense on purpose"}));
    assert_eq!((r["finding"]["status"].as_str(), r["finding"]["resolution"].as_str()), (Some("accepted"), Some("The list is dense on purpose")));
    // Without `by`, the user confirms.
    ex(&mut s, "design.finding.verify", json!({"id": "F3", "evidence": "Ana read the new label and nodded"}));
    assert_eq!(s.doc().unwrap().design.findings[2].verified_by, "user");
    assert_eq!(ex_undoable(&mut s, "design.finding.remove", json!({"id": "F3"}))["removed"], "F3");
    assert_eq!(s.doc().unwrap().design.findings.len(), 2);
    assert_eq!(ex(&mut s, "design.finding.add", json!({"kind": "judgement", "rule": "x", "message": "m"}))["id"], "F4", "ids aren't reused");
}

#[test]
fn life_cycle_errors_name_the_parameter() {
    let mut s = Session::new();
    three_findings(&mut s);
    for (cmd, params, needle) in [
        ("design.finding.address", json!({"id": "F1"}), "`resolution` is required"),
        ("design.finding.address", json!({"id": "F1", "resolution": " "}), "`resolution` can't be empty"),
        ("design.finding.address", json!({"id": "F9", "resolution": "x"}), "`id`: no finding F9"),
        ("design.finding.address", json!({"resolution": "x"}), "`id` is required"),
        ("design.finding.verify", json!({"id": "F1"}), "`evidence` is required"),
        ("design.finding.verify", json!({"id": "F9", "evidence": "x"}), "`id`: no finding F9"),
        ("design.finding.accept", json!({"id": "F1"}), "`reason` is required"),
        ("design.finding.accept", json!({"id": "F9", "reason": "x"}), "`id`: no finding F9"),
        ("design.finding.reopen", json!({"id": "F9"}), "`id`: no finding F9"),
        ("design.finding.remove", json!({"id": "F9"}), "`id`: no finding F9"),
        ("design.finding.recheck", json!({"id": "F9"}), "`id`: no finding F9"),
        ("design.finding.recheck", json!({"id": "F1"}), "only checks can be re-run; use design.finding.verify"),
    ] {
        let e = refused_unchanged(&mut s, cmd, params.clone());
        assert!(e.contains(needle), "{cmd} {params}: {e}");
    }
}

#[test]
fn an_agent_addresses_feedback_but_only_a_person_settles_it() {
    let mut s = Session::new();
    three_findings(&mut s);
    // F3 is feedback from Ana.
    let r = as_agent(&mut s, "design.finding.address", json!({"id": "F3", "resolution": "Renamed Sync to Save to cloud"}));
    assert_eq!(r["finding"]["status"], "addressed");
    let e = agent_err(&mut s, "design.finding.verify", json!({"id": "F3", "evidence": "I re-read the label"}));
    assert!(e.contains("only a person confirms that feedback is met; record what changed with design.finding.address"), "{e}");
    let e = agent_err(&mut s, "design.finding.accept", json!({"id": "F3", "reason": "Fine as it is"}));
    assert!(e.contains("only a person") && e.contains("feedback"), "{e}");
    let e = agent_err(&mut s, "design.finding.remove", json!({"id": "F3"}));
    assert!(e.contains("feedback") && e.contains("only a person removes it"), "{e}");
    assert_eq!(status(&s, "F3"), FindingStatus::Addressed, "the refused calls changed nothing");
    assert_eq!(s.doc().unwrap().design.findings.len(), 3);
    // The user can do each of them.
    ex(&mut s, "design.finding.verify", json!({"id": "F3", "evidence": "Ana read it: clear now"}));
    assert_eq!(status(&s, "F3"), FindingStatus::Verified);
    ex(&mut s, "design.finding.accept", json!({"id": "F3", "reason": "Agreed with Ana to keep it"}));
    assert_eq!(status(&s, "F3"), FindingStatus::Accepted);
    ex(&mut s, "design.finding.remove", json!({"id": "F3"}));
    assert_eq!(s.doc().unwrap().design.findings.len(), 2);
}

#[test]
fn an_agent_cant_close_opinions_either() {
    let mut s = Session::new();
    three_findings(&mut s);
    // F1 is the user's judgement, F2 the agent's own.
    for id in ["F1", "F2"] {
        as_agent(&mut s, "design.finding.address", json!({"id": id, "resolution": "Changed it"}));
        let e = agent_err(&mut s, "design.finding.verify", json!({"id": id, "evidence": "Looks right to me"}));
        assert!(e.contains("only a person confirms a judgement") && e.contains("design.finding.address"), "{e}");
        let e = agent_err(&mut s, "design.finding.accept", json!({"id": id, "reason": "Good enough"}));
        assert!(e.contains("only a person accepts a judgement"), "{e}");
        assert_eq!(status(&s, id), FindingStatus::Addressed);
    }
    // It removes its own judgement, not somebody else's.
    let e = agent_err(&mut s, "design.finding.remove", json!({"id": "F1"}));
    assert!(e.contains("judgement by user"), "{e}");
    as_agent(&mut s, "design.finding.remove", json!({"id": "F2"}));
    let left: Vec<String> = s.doc().unwrap().design.findings.iter().map(|f| f.id.clone()).collect();
    assert_eq!(left, ["F1", "F3"]);
    // Another agent's judgement isn't its own either.
    s.execute_as(Some("Other"), "design.finding.add", &json!({"kind": "judgement", "rule": "x", "message": "m"})).unwrap();
    let e = agent_err(&mut s, "design.finding.remove", json!({"id": "F4"}));
    assert!(e.contains("judgement by Other (agent)"), "{e}");
    // The user verifies and accepts judgements.
    ex(&mut s, "design.finding.verify", json!({"id": "F1", "evidence": "Compared before and after"}));
    assert_eq!(status(&s, "F1"), FindingStatus::Verified);
}

#[test]
fn checks_are_not_verified_by_saying_so() {
    let mut s = Session::new();
    let home = frame(&mut s, "Home", 0.0, 320.0, 480.0);
    rect(&mut s, home, [300.0, 10.0, 100.0, 40.0], json!({"name": "Banner"}));
    let added = ex(&mut s, "design.check.record", json!({"rules": ["layout.clippedContent"]}))["added"].clone();
    assert_eq!(added, json!(["F1"]));
    let claim = json!({"id": "F1", "evidence": "Looks fine"});
    for e in [err(&mut s, "design.finding.verify", claim.clone()), agent_err(&mut s, "design.finding.verify", claim)] {
        assert!(e.contains("re-run it with design.finding.recheck"), "{e}");
    }
    assert_eq!(status(&s, "F1"), FindingStatus::Open);
    // Anyone may claim a fix or accept a measured finding as it is; the claim doesn't verify it.
    as_agent(&mut s, "design.finding.address", json!({"id": "F1", "resolution": "Moved the banner"}));
    assert_eq!(status(&s, "F1"), FindingStatus::Addressed);
    as_agent(&mut s, "design.finding.accept", json!({"id": "F1", "reason": "The banner bleeds off the edge on purpose"}));
    assert_eq!(status(&s, "F1"), FindingStatus::Accepted);
}
