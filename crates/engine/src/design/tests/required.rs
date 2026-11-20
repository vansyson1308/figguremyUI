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
    assert!(last["evidence"].as_str().unwrap().ends_with("not looked at: 10 linked layer(s) of the elements listed"), "{last}");
    // From converging, where this rule runs, the gone layers are its: context.missingLink
    // doesn't report them again.
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
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

/// P2-1: a declared state excuses a hidden layer only when it shows that very layer. A state of
/// another screen that the whole Form screen shows excuses nothing hidden inside Form.
#[test]
fn a_state_shown_by_a_whole_screen_doesnt_excuse_a_hidden_layer_inside_it() {
    let (mut s, form, done, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form], "nodes": [consent]}));
    ex(&mut s, "node.set", json!({"id": consent, "props": {"visible": false}}));
    assert_eq!(found(&mut s).len(), 1);
    ex(&mut s, "proto.state.set", json!({"frame": done, "kind": "success", "shownBy": {"node": form}}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "Done's state shows Form, not the hidden consent in it: {f:?}");
    assert_eq!(f[0]["evidence"], "`Consent` is hidden");
    assert_eq!(ex(&mut s, "design.get", json!({}))["requiredFacts"], json!([]));
    // Near miss: a state of Form that shows the consent itself does excuse it.
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "other", "name": "Asked", "shownBy": {"node": consent}}));
    assert!(found(&mut s).is_empty());
}

/// P3-1: every hidden layer around a required layer must be shown by a state; a hidden group
/// around an excused banner is still reported. Low opacity is never excused.
#[test]
fn every_hidden_layer_around_it_must_be_shown_by_a_state_and_opacity_is_never_excused() {
    let (mut s, form, _, _) = form();
    let banner = super::nid(&ex(&mut s, "frame.create", json!({"x": 20, "y": 20, "width": 280, "height": 60, "parent": form, "name": "Banner"})));
    let message = label(&mut s, banner, "Your phone number is missing", [8.0, 8.0], json!({"name": "Message", "fill": "#1B2430"}));
    let group = super::nid(&ex(&mut s, "object.group", json!({"ids": [banner]})));
    ex(&mut s, "node.set", json!({"id": group, "props": {"name": "Holder"}}));
    ex(&mut s, "node.set", json!({"id": banner, "props": {"visible": false}}));
    ex(&mut s, "node.set", json!({"id": group, "props": {"visible": false}}));
    ex(&mut s, "design.required.add", json!({"text": "What is missing", "screens": [form], "nodes": [message]}));
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "error", "shownBy": {"node": banner}}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "the hidden group around the banner shows in no state: {f:?}");
    assert_eq!(f[0]["evidence"], "`Message` is inside `Holder`, which is hidden");
    assert_eq!(ex(&mut s, "design.get", json!({}))["requiredFacts"], json!([]));
    // Each hidden layer shown by a state of the screen: a fact naming both.
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "loading", "shownBy": {"node": group}}));
    assert!(found(&mut s).is_empty());
    let facts = ex(&mut s, "design.get", json!({}))["requiredFacts"].clone();
    assert_eq!(facts, json!([{"id": "R1", "node": message, "shownIn": "the error state of `Form` and the loading state of `Form`"}]));
    // Shown by states, but at 0% opacity: that no state excuses.
    ex(&mut s, "node.set", json!({"id": banner, "props": {"opacity": 0}}));
    let f = found(&mut s);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0]["evidence"].as_str().unwrap().starts_with("`Message` is at 0% effective opacity"), "{f:?}");
    assert_eq!(ex(&mut s, "design.get", json!({}))["requiredFacts"], json!([]), "not a fact while it doesn't show");
}

/// P3-2: only an accepted declaration excuses a hidden layer, as only an accepted end state
/// exempts a dead end: an agent's proposal doesn't silence a person's finding.
#[test]
fn an_agents_proposed_state_doesnt_excuse_a_hidden_layer_until_a_person_accepts_it() {
    let (mut s, form, _, consent) = form();
    ex(&mut s, "design.required.add", json!({"text": "Consent", "screens": [form], "nodes": [consent]}));
    ex(&mut s, "node.set", json!({"id": consent, "props": {"visible": false}}));
    as_agent(&mut s, "proto.state.set", json!({"frame": form, "kind": "other", "name": "Asked", "shownBy": {"node": consent}}));
    assert_eq!(found(&mut s).len(), 1, "a proposal");
    assert_eq!(ex(&mut s, "design.get", json!({}))["requiredFacts"], json!([]));
    ex(&mut s, "proto.state.accept", json!({"frame": form, "kind": "other", "name": "Asked"}));
    assert!(found(&mut s).is_empty(), "accepted by a person");
}

/// Size of what a `design.check` result carries in its findings' evidence.
fn evidence_bytes(out: &Value) -> usize {
    out["findings"].as_array().unwrap().iter().map(|f| f["evidence"].as_str().map_or(0, str::len)).sum()
}

/// P2-2: a hostile file (500 elements, each on 500 screens and linked to 500 layers none of
/// which is on them) is bounded: the findings stop at the caps, evidence names a few screens
/// and counts the rest, and the rule stops looking once the caps are reached. Work is counted
/// (linked layers and screens looked at) rather than timed, so a slow machine can't make the
/// test flaky and a fast one can't hide work.
#[test]
fn a_hostile_brief_gives_bounded_findings_evidence_and_work() {
    use std::sync::Arc;

    use figcraft_doc::design::DesignContext;
    let mut s = Session::new();
    let n = figcraft_doc::design::MAX_ENTRIES;
    let screens: Vec<NodeId> = (0..n).map(|k| frame(&mut s, &format!("Screen number {k}"), k as f64 * 400.0, 320.0, 480.0)).collect();
    let holder = frame(&mut s, "Holder", -1000.0, 320.0, 480.0);
    let nodes: Vec<NodeId> = (0..n).map(|k| rect(&mut s, holder, [(k % 300) as f64, 0.0, 4.0, 4.0], json!({}))).collect();
    let required: Vec<Value> =
        (0..n).map(|k| json!({"id": format!("R{}", k + 1), "text": "x".repeat(200), "screens": screens, "nodes": nodes})).collect();
    let ctx = DesignContext::from_stored(json!({"brief": {"required": required}}));
    assert_eq!(ctx.brief.required.len(), n);
    Arc::make_mut(&mut s.doc_mut().unwrap().design).brief = ctx.brief.clone();
    let started = std::time::Instant::now();
    let out = ex(&mut s, "design.check", json!({"rules": [RULE]}));
    eprintln!("requiredMissing on the hostile brief: {:?}, {} bytes", started.elapsed(), out.to_string().len());
    let f = out["findings"].as_array().unwrap();
    assert_eq!(f.len(), 201, "200 findings, then the note");
    assert!(evidence_bytes(&out) < 40_000, "evidence: {} bytes", evidence_bytes(&out));
    assert!(out.to_string().len() < 400_000, "result: {} bytes", out.to_string().len());
    let one = f.iter().find(|x| x["evidence"].as_str().unwrap().contains("isn't on")).unwrap();
    assert!(one["evidence"].as_str().unwrap().ends_with("`Screen number 2` or 497 more, where it must be"), "{one}");
    let note = f.iter().find(|x| x["subject"] == "required:more").unwrap();
    assert!(note["evidence"].as_str().unwrap().contains("490 element(s)"), "{note}");
    let work = crate::design::checks::required_work(s.doc().unwrap());
    assert!(work <= 10 * (n + 20), "looked at {work} layers and screens");
}

/// P3-3: while a file is exploring, only `context.missingLink` of the rules that see a gone
/// layer runs, so it reports the gone layers of required elements and declared states too;
/// from converging it leaves them to the rules that report them there.
#[test]
fn at_exploring_missing_link_reports_the_gone_layers_of_states_and_required_elements() {
    let (mut s, form, _, _) = form();
    let a = rect(&mut s, form, [0.0, 0.0, 10.0, 10.0], json!({}));
    let b = rect(&mut s, form, [20.0, 0.0, 10.0, 10.0], json!({}));
    ex(&mut s, "design.required.add", json!({"text": "Box", "nodes": [a]}));
    ex(&mut s, "proto.state.set", json!({"frame": form, "kind": "error", "shownBy": {"node": b}}));
    ex(&mut s, "edit.delete", json!({"ids": [a, b]}));
    let subjects = |s: &mut Session| -> Vec<String> {
        let out = ex(s, "design.check", json!({}));
        out["findings"].as_array().unwrap().iter().map(|f| format!("{} {}", f["rule"].as_str().unwrap(), f["subject"].as_str().unwrap())).collect()
    };
    let got = subjects(&mut s);
    assert!(got.contains(&format!("context.missingLink link:R1:{a}")), "{got:?}");
    assert!(got.contains(&format!("context.missingLink link:state error of {form}:{b}")), "{got:?}");
    // Near miss: from converging the specific rules run and missingLink leaves those links to them.
    ex(&mut s, "design.stage.set", json!({"stage": "converging"}));
    let got = subjects(&mut s);
    assert!(!got.iter().any(|g| g.starts_with("context.missingLink")), "{got:?}");
    assert!(got.contains(&format!("{RULE} required:R1:{a}")), "{got:?}");
    assert!(got.iter().any(|g| g.starts_with("states.notDesigned")), "{got:?}");
}

/// P2-2: a hostile context (500 assumptions, each linked to the same 500 layers that are gone)
/// gives a bounded `context.missingLink`: at most 20 findings for an entry and 200 in all, then a
/// note that counts the rest, and a `design.get` that lists 200 gone links and counts them all.
/// What is left is one walk over the links the file stores, linear in what it holds (which the
/// loader caps). What grew past MCP's response limit was the findings (250 000 of them, 68 MB,
/// though built in little more than a second), so the work that matters is counted as what is
/// built: the findings and the bytes are asserted. A generous time limit (60 s in a debug build)
/// only guards against the walk becoming worse than linear; it can't tell the old code apart.
#[test]
fn a_hostile_context_gives_a_bounded_missing_link_check() {
    use std::sync::Arc;

    use figcraft_doc::design::DesignContext;
    let mut s = Session::new();
    let n = figcraft_doc::design::MAX_ENTRIES;
    let _ = frame(&mut s, "Screen", 0.0, 320.0, 480.0);
    let gone: Vec<u64> = (0..n as u64).map(|k| 900_000 + k).collect();
    let assumptions: Vec<Value> = (0..n).map(|k| json!({"id": format!("A{}", k + 1), "text": "t", "status": "untested", "links": gone})).collect();
    let ctx = DesignContext::from_stored(json!({"assumptions": assumptions}));
    assert_eq!(ctx.assumptions.len(), n);
    Arc::make_mut(&mut s.doc_mut().unwrap().design).assumptions = ctx.assumptions.clone();
    let started = std::time::Instant::now();
    let out = ex(&mut s, "design.check", json!({"rules": ["context.missingLink"]}));
    let took = started.elapsed();
    eprintln!("missingLink on the hostile context: {took:?}, {} bytes", out.to_string().len());
    let f = out["findings"].as_array().unwrap();
    assert_eq!(f.len(), 201, "200 findings, then the note");
    assert!(out.to_string().len() < 200_000, "result: {} bytes", out.to_string().len());
    assert_eq!(f.iter().filter(|x| x["subject"].as_str().unwrap().starts_with("link:A1:")).count(), 20, "20 for one entry");
    let note = f.iter().find(|x| x["subject"] == "link:more").unwrap();
    assert!(note["message"].as_str().unwrap().starts_with(&format!("{} more link(s)", n * n - 200)), "{note}");
    assert!(took < std::time::Duration::from_secs(60), "{took:?}");
    let got = ex(&mut s, "design.get", json!({}));
    assert_eq!((got["missingLinks"].as_array().unwrap().len(), &got["counts"]["missingLinks"]), (200, &json!(n * n)));
}
