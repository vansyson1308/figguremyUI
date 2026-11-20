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
    let e = evidence(&mut s);
    assert!(e.len() == 1 && e[0].starts_with("`Consent` is at 0% effective opacity"), "{e:?}");
    assert!(e[0].contains("below the 5% this product counts as shown (a product choice, not a standard)"), "{e:?}");
    set(&mut s, consent, json!({"opacity": 1}));
    let group = super::nid(&ex(&mut s, "object.group", json!({"ids": [box_]})));
    set(&mut s, group, json!({"visible": false}));
    assert_eq!(evidence(&mut s), [format!("`Box` is inside `{}`, which is hidden", s.doc().unwrap().node(group).unwrap().name)]);
    set(&mut s, group, json!({"visible": true}));
    // Near miss and miss by the layers around it: 50% × 50% shows; an ancestor at 0%, or 20% × 20%
    // (4% in all), doesn't.
    set(&mut s, group, json!({"opacity": 0.5}));
    set(&mut s, box_, json!({"opacity": 0.5}));
    assert!(found(&mut s).is_empty(), "25% effective opacity shows");
    set(&mut s, group, json!({"opacity": 0}));
    let e = evidence(&mut s);
    assert!(
        e.len() == 1
            && e[0].starts_with("`Box` is at 0% effective opacity")
            && e[0].contains(&format!("{} is the faintest", "`Group 1`").replace("Group 1", &s.doc().unwrap().node(group).unwrap().name)),
        "{e:?}"
    );
    set(&mut s, group, json!({"opacity": 0.2}));
    set(&mut s, box_, json!({"opacity": 0.2}));
    let e = evidence(&mut s);
    assert!(e.len() == 1 && e[0].starts_with("`Box` is at 4% effective opacity"), "{e:?}");
    set(&mut s, group, json!({"opacity": 1}));
    set(&mut s, box_, json!({"opacity": 1}));
    // Partly cut by the screen is still seen; wholly outside it isn't.
    set(&mut s, consent, json!({"y": 470}));
    assert!(found(&mut s).is_empty(), "partly clipped: part of it shows");
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
fn a_layer_hidden_in_the_file_but_shown_by_a_declared_state_counts_as_shown_in_that_state() {
    let (mut s, form, _, _) = form();
    // An error banner, hidden until the form is sent with something missing, holding the message
    // that is required.
    let banner =
        super::nid(&ex(&mut s, "frame.create", json!({"x": 20, "y": 20, "width": 280, "height": 60, "parent": form, "name": "Error banner"})));
    let message = label(&mut s, banner, "Your phone number is missing", [8.0, 8.0], json!({"name": "Message", "fill": "#1B2430"}));
    ex(&mut s, "node.set", json!({"id": banner, "props": {"visible": false}}));
    ex(&mut s, "design.required.add", json!({"text": "What is missing, in words", "screens": [form], "nodes": [message]}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "without a declared state it is hidden: {f:?}");
    // Declared as what shows the form's error state: shown in that state, a fact, not a finding.
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "error", "shownBy": {"node": banner}}));
    assert!(found(&mut s).is_empty());
    let got = ex(&mut s, "design.get", json!({}));
    assert_eq!(got["requiredFacts"], json!([{"id": "R1", "node": message, "shownIn": "the error state of `Form`"}]));
    // Near miss: what shows the state isn't what hides it.
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "error", "shownBy": {"node": message}}));
    assert_eq!(found(&mut s).len(), 1, "the banner around it still hides it, and no state shows the banner");
}

#[test]
fn on_a_screen_means_inside_it_and_a_screen_that_moved_is_reported_as_such() {
    let (mut s, form, done, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form, done], "nodes": [consent]}));
    // The consent is on Form only: Done misses it.
    assert_eq!(found(&mut s).len(), 1);
    // Done goes inside Form: no longer a screen, said once as that; Form still holds the consent.
    ex(&mut s, "node.reparent", json!({"ids": [done], "parent": form}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0]["subject"], format!("required:R1:screen:{done}"));
    assert!(
        f[0]["evidence"].as_str().unwrap().contains("`Done`")
            && f[0]["evidence"].as_str().unwrap().contains("is no longer a screen: it is inside `Form`"),
        "{f:?}"
    );
    assert!(f[0]["suggestion"].as_str().unwrap().contains("`Form`"), "{f:?}");
}

#[test]
fn findings_are_bounded_per_element_and_gone_layers_are_reported_once() {
    let (mut s, form, _, _) = form();
    let many: Vec<NodeId> = (0..30).map(|k| rect(&mut s, form, [k as f64, 0.0, 4.0, 4.0], json!({}))).collect();
    ex(&mut s, "design.required.add", json!({"text": "Thirty marks", "nodes": many}));
    ex(&mut s, "edit.delete", json!({"ids": many}));
    let f = found(&mut s);
    assert_eq!(f.len(), 21, "20 for the element, then one note: {}", f.len());
    let last = f.iter().find(|x| x["subject"] == "required:more").expect("the note that counts the rest");
    assert!(last["message"].as_str().unwrap().starts_with("10 more problem(s)"), "{last}");
    // The gone layers are this rule's: context.missingLink doesn't report them again.
    let links = ex(&mut s, "design.check", json!({"rules": ["context.missingLink"]}));
    assert_eq!(links["findings"], json!([]), "{links}");
    assert_eq!(ex(&mut s, "design.get", json!({}))["missingLinks"].as_array().unwrap().len(), 30, "design.get still lists them");
}

#[test]
fn an_agent_adds_links_to_a_persons_element_but_doesnt_take_any_away_or_change_its_screens() {
    let (mut s, form, done, consent) = form();
    let box_ = rect(&mut s, form, [20.0, 360.0, 24.0, 24.0], json!({"name": "Box"}));
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form], "nodes": [consent]}));
    as_agent(&mut s, "design.required.set", json!({"id": "R1", "nodes": [consent, box_]}));
    let e = agent_err(&mut s, "design.required.set", json!({"id": "R1", "nodes": [box_]}));
    assert!(e.contains("may add links to it but not remove or replace them") && e.contains(&consent.to_string()), "{e}");
    let e = agent_err(&mut s, "design.required.set", json!({"id": "R1", "screens": [form, done]}));
    assert!(e.contains("only its own proposals"), "{e}");
    // Its own pending proposal it relinks freely, until a person edits it.
    as_agent(&mut s, "design.required.add", json!({"text": "A price", "nodes": [consent]}));
    as_agent(&mut s, "design.required.set", json!({"id": "R2", "nodes": [box_]}));
    let r = ex(&mut s, "design.required.set", json!({"id": "R2", "text": "The price before paying"}));
    assert_eq!((&r["required"]["editedBy"], &r["required"]["status"]), (&json!("user"), &json!("proposed")), "{r}");
    let e = agent_err(&mut s, "design.required.set", json!({"id": "R2", "text": "A price, again"}));
    assert!(e.contains("edited by you") || e.contains("edited by user"), "{e}");
    let e = agent_err(&mut s, "design.required.set", json!({"id": "R2", "nodes": []}));
    assert!(e.contains("not remove or replace"), "{e}");
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
