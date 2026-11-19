//! The brief's required elements (`design.required.*`) and the rule that looks for them
//! (`context.requiredMissing`).

use figcraft_doc::design::ProposalStatus;
use serde_json::json;

use super::{AGENT_NAME, NodeId, Session, Value, agent_err, as_agent, err, ex, ex_undoable, frame, label, rect, refused_unchanged, reopened};

const RULE: &str = "context.requiredMissing";

/// A white "Form" screen with a dark "Consent" text on it, and a second screen "Done".
fn form() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let form = frame(&mut s, "Form", 0.0, 320.0, 480.0);
    let done = frame(&mut s, "Done", 400.0, 320.0, 480.0);
    let consent = label(&mut s, form, "I agree to be contacted", [20.0, 400.0], json!({"name": "Consent", "fill": "#1B2430", "fontSize": 14}));
    (s, form, done, consent)
}

/// What the rule finds in the file.
fn found(s: &mut Session) -> Vec<Value> {
    ex(s, "design.check", json!({"rules": [RULE]}))["findings"].as_array().unwrap().clone()
}

fn revision(s: &Session) -> u64 {
    s.doc().unwrap().design.brief_revision
}

#[test]
fn a_required_element_is_listed_linked_and_removed_and_the_brief_revision_follows_its_words() {
    let (mut s, form, done, consent) = form();
    let r = ex_undoable(&mut s, "design.required.add", json!({"text": " Consent to be contacted ", "screens": [form], "nodes": [consent]}));
    assert_eq!(r, json!({"id": "R1", "status": "accepted"}));
    assert_eq!(revision(&s), 1, "a new element changes the brief");
    let got = ex(&mut s, "design.get", json!({}));
    assert_eq!(
        got["brief"]["required"],
        json!([{"id": "R1", "text": "Consent to be contacted", "screens": [form], "nodes": [consent], "author": "user"}])
    );
    assert_eq!((&got["counts"]["requiredElements"], &got["counts"]["unlinkedRequiredElements"]), (&json!(1), &json!(0)));
    assert_eq!(ex(&mut s, "design.forNode", json!({"id": consent}))["required"], json!(["R1"]));
    // Linking layers isn't a change of the brief; its words and screens are.
    ex_undoable(&mut s, "design.required.set", json!({"id": "r1", "nodes": []}));
    assert_eq!(revision(&s), 1);
    ex_undoable(&mut s, "design.required.set", json!({"id": "R1", "screens": [form, done]}));
    assert_eq!(revision(&s), 2);
    let depth = s.undo_depth();
    ex(&mut s, "design.required.set", json!({"id": "R1", "text": "Consent to be contacted"}));
    assert_eq!((revision(&s), s.undo_depth()), (2, depth), "the same words are no change");
    ex_undoable(&mut s, "design.required.remove", json!({"id": "R1"}));
    assert_eq!(revision(&s), 3);
    // Ids aren't given out twice.
    assert_eq!(ex(&mut s, "design.required.add", json!({"text": "A price"}))["id"], "R2");
    // Errors name the parameter and change nothing.
    for (params, says) in [
        (json!({"text": ""}), "`text` can't be empty"),
        (json!({}), "`text` is required"),
        (json!({"text": "x", "screens": [consent]}), "isn't a top-level frame"),
        (json!({"text": "x", "nodes": [4711]}), "`nodes`: no layer 4711"),
        (json!({"text": "x", "where": 1}), "`where`"),
    ] {
        let e = refused_unchanged(&mut s, "design.required.add", params.clone());
        assert!(e.contains(says), "{params}: {e}");
    }
    assert!(err(&mut s, "design.required.set", json!({"id": "R9"})).contains("no required element R9"));
    assert!(err(&mut s, "design.required.set", json!({"id": "R2", "text": ""})).contains("`text` can't be empty"));
}

#[test]
fn an_agents_element_is_a_proposal_and_a_persons_is_theirs_but_anyone_links_layers() {
    let (mut s, form, _, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form]}));
    let r = as_agent(&mut s, "design.required.add", json!({"text": "The pool's safety rules"}));
    assert_eq!(r, json!({"id": "R2", "status": "proposed"}));
    // The agent links the person's element to the layer it built; their words are theirs.
    as_agent(&mut s, "design.required.set", json!({"id": "R1", "nodes": [consent]}));
    assert_eq!(s.doc().unwrap().design.brief.required[0].nodes, vec![consent]);
    for (cmd, params) in [("design.required.set", json!({"id": "R1", "text": "Consent, small"})), ("design.required.remove", json!({"id": "R1"}))] {
        let e = agent_err(&mut s, cmd, params);
        assert!(e.contains("listed by user") && e.contains("only its own proposals"), "{e}");
    }
    assert!(agent_err(&mut s, "design.required.accept", json!({"id": "R2"})).contains("a person accepts a required element"));
    as_agent(&mut s, "design.required.set", json!({"id": "R2", "text": "The pool's safety rules, linked"}));
    let r = ex(&mut s, "design.required.accept", json!({"id": "R2"}));
    assert_eq!((&r["required"]["author"], &r["required"]["acceptedBy"]), (&json!(AGENT_NAME), &json!("user")));
    assert!(r["required"].get("status").is_none(), "accepted is the default and isn't written");
    assert!(agent_err(&mut s, "design.required.remove", json!({"id": "R2"})).contains("stands accepted"));
    let saved = reopened(&s);
    assert_eq!(saved.design.brief.required, s.doc().unwrap().design.brief.required);
    assert_eq!(saved.design.brief.required[1].status, ProposalStatus::Accepted);
}

#[test]
fn an_element_without_a_layer_cant_be_checked_and_says_so() {
    let (mut s, form, _, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent to be contacted", "screens": [form]}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!((&f[0]["severity"], &f[0]["subject"], &f[0]["nodes"]), (&json!("minor"), &json!("required:R1"), &json!([form])));
    assert!(f[0]["message"].as_str().unwrap().contains("can't be checked until a layer is linked"), "{f:?}");
    // Near miss: linked to a text that shows and reads well.
    ex(&mut s, "design.required.set", json!({"id": "R1", "nodes": [consent]}));
    assert_eq!(found(&mut s), Vec::<Value>::new());
}

#[test]
fn a_linked_layer_that_is_gone_hidden_transparent_elsewhere_or_cut_out_of_view_is_major() {
    let (mut s, form, done, consent) = form();
    let box_ = rect(&mut s, form, [20.0, 360.0, 24.0, 24.0], json!({"name": "Box"}));
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form], "nodes": [consent, box_]}));
    assert!(found(&mut s).is_empty());
    let set = |s: &mut Session, id: NodeId, props: Value| ex(s, "node.set", json!({"id": id, "props": props}));
    let evidence = |s: &mut Session| -> Vec<String> {
        let f = found(s);
        assert!(f.iter().all(|x| x["severity"] == "major"), "{f:?}");
        f.iter().map(|x| x["evidence"].as_str().unwrap().to_string()).collect()
    };
    set(&mut s, consent, json!({"visible": false}));
    assert_eq!(evidence(&mut s), ["`Consent` is hidden"]);
    set(&mut s, consent, json!({"visible": true, "opacity": 0}));
    assert_eq!(evidence(&mut s), ["`Consent` has 0% opacity"]);
    set(&mut s, consent, json!({"opacity": 1}));
    let group = super::nid(&ex(&mut s, "object.group", json!({"ids": [box_]})));
    set(&mut s, group, json!({"visible": false}));
    assert_eq!(evidence(&mut s), [format!("`Box` is inside `{}`, which is hidden", s.doc().unwrap().node(group).unwrap().name)]);
    set(&mut s, group, json!({"visible": true}));
    // Wholly outside the screen, which clips its content.
    set(&mut s, consent, json!({"y": 900}));
    let e = evidence(&mut s);
    assert!(e.len() == 1 && e[0].starts_with("`Consent` is out of view: entirely outside `Form`"), "{e:?}");
    set(&mut s, consent, json!({"y": 400}));
    // On another screen than the one it must be on: reported for the layer, and for the screen
    // none of its layers is on.
    ex(&mut s, "design.required.set", json!({"id": "R1", "screens": [done]}));
    let f = found(&mut s);
    assert_eq!(f.len(), 3, "{f:?}");
    assert!(
        f.iter().any(|x| x["subject"] == format!("required:R1:screen:{done}") && x["message"].as_str().unwrap().contains("isn't on `Done`")),
        "{f:?}"
    );
    assert!(f.iter().any(|x| x["evidence"] == "`Consent` isn't on `Done`, where it must be"), "{f:?}");
    // Deleted.
    ex(&mut s, "design.required.set", json!({"id": "R1", "screens": []}));
    ex(&mut s, "edit.delete", json!({"ids": [box_]}));
    assert_eq!(evidence(&mut s), [format!("its layer {box_} no longer exists")]);
}

#[test]
fn a_linked_text_below_the_contrast_it_needs_is_major_and_cites_wcag() {
    let (mut s, _, _, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "nodes": [consent]}));
    ex(&mut s, "node.set", json!({"id": consent, "props": {"fill": "#B0B4B8"}}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0]["severity"], "major");
    let evidence = f[0]["evidence"].as_str().unwrap();
    assert!(evidence.contains("needs 4.5:1") && evidence.contains("WCAG 2.2 SC 1.4.3"), "{evidence}");
    assert!(f[0]["message"].as_str().unwrap().contains("below the legibility threshold"));
    // Near miss: the threshold contrast.text suggests passes here too.
    let suggestion = ex(&mut s, "design.check", json!({"rules": ["contrast.text"]}))["findings"][0]["suggestion"].as_str().unwrap().to_string();
    let colour = suggestion.split_whitespace().find(|w| w.starts_with('#')).unwrap().to_string();
    ex(&mut s, "node.set", json!({"id": consent, "props": {"fill": colour}}));
    assert!(found(&mut s).is_empty());
}

#[test]
fn the_rule_runs_from_converging_and_a_fixed_element_is_verified_by_rerunning_it() {
    let (mut s, form, _, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form]}));
    let v = ex(&mut s, "design.check", json!({}));
    assert!(v["skipped"].as_array().unwrap().iter().any(|x| x["id"] == RULE), "exploring: not run");
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let v = ex(&mut s, "design.check", json!({}));
    assert_eq!(v["summary"]["byRule"][RULE], 1, "{v}");
    ex(&mut s, "design.check.record", json!({"rules": [RULE]}));
    ex(&mut s, "design.required.set", json!({"id": "R1", "nodes": [consent]}));
    let r = ex(&mut s, "design.finding.recheck", json!({}));
    assert_eq!(r["results"][0]["status"], "verified", "{r}");
}
