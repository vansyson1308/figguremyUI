//! Variables: set by a value or an expression, read by conditionals, switched by mode, and shown
//! by the layers bound to them.

use std::collections::BTreeMap;

use figcraft_doc::design::{Expect, StepAction, TextExpect};
use figcraft_doc::proto::{Action, ConditionalBlock, Interaction, Trigger};
use serde_json::{Value, json};

use super::{button, click, ex, frame, navigate, problem, rect, sim, state_variables, text};
use crate::Session;

fn expect_variables(pairs: &[(&str, Value)]) -> Expect {
    Expect { variables: pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<BTreeMap<_, _>>(), ..Expect::default() }
}

#[test]
fn set_variable_writes_a_value_or_what_an_expression_gives() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    let five = button(&mut s, "Five", a, 20.0, json!({"type": "setVariable", "variable": "count", "value": 5}));
    let more = button(&mut s, "More", a, 80.0, json!({"type": "setVariable", "variable": "count", "expression": "count + 1"}));
    let mut p = sim(&s, a);
    assert!(p.state().variables.is_empty(), "nothing differs from the file yet");
    let o = click(&mut p, more);
    assert!(o.fired && o.effects.iter().any(|e| e == "set `State/count` to 1"), "{o:?}");
    click(&mut p, more);
    assert_eq!(p.state().variables, BTreeMap::from([("State/count".to_string(), json!(2.0))]), "count + 1, twice");
    click(&mut p, five);
    assert_eq!(p.check(&expect_variables(&[("count", json!(5))])), Vec::<String>::new());
    assert_eq!(p.check(&expect_variables(&[("State/count", json!("5"))])), Vec::<String>::new(), "numbers given as text read as numbers");
    // A failure names the variable, what was expected and what it is.
    let failures = p.check(&expect_variables(&[("count", json!(6)), ("label", json!("Hi")), ("nope", json!(1))]));
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].contains("expected `State/count` to be 6, but it is 5"), "{failures:?}");
    assert!(failures[1].contains("`nope`") && failures[1].contains("there is no variable"), "{failures:?}");
}

#[test]
fn a_conditional_reads_what_the_same_interaction_just_set() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let step = rect(&mut s, "Step", a, 20.0, 20.0);
    let actions = json!([
        {"type": "setVariable", "variable": "count", "expression": "count + 1"},
        {"type": "conditional", "blocks": [
            {"expression": "count >= 2", "actions": [navigate(b)]},
            {"actions": [{"type": "setVariable", "variable": "label", "value": "again"}]},
        ]},
    ]);
    ex(&mut s, "proto.interaction.add", json!({"id": step, "actions": actions}));
    let mut p = sim(&s, a);
    click(&mut p, step);
    assert_eq!(p.screen(), a, "count is 1: the else branch ran");
    assert_eq!(p.check(&expect_variables(&[("label", json!("again")), ("count", json!(1))])), Vec::<String>::new());
    click(&mut p, step);
    assert_eq!(p.screen(), b, "count is 2: the condition holds now");
}

#[test]
fn bound_text_and_visibility_follow_the_variable() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    let title = text(&mut s, "Title", a, "Hi");
    let badge = rect(&mut s, "Badge", a, 150.0, 20.0);
    ex(&mut s, "variable.bind", json!({"ids": [title], "field": "characters", "variable": "label"}));
    ex(&mut s, "variable.bind", json!({"ids": [badge], "field": "visible", "variable": "open"}));
    let show = rect(&mut s, "Show", a, 20.0, 100.0);
    let actions = json!([
        {"type": "setVariable", "variable": "label", "value": "Welcome back"},
        {"type": "setVariable", "variable": "open", "value": true},
    ]);
    ex(&mut s, "proto.interaction.add", json!({"id": show, "actions": actions}));
    let reads = |equals: &str| Expect { text: vec![TextExpect { node: title, equals: Some(equals.into()), contains: None }], ..Expect::default() };
    let mut p = sim(&s, a);
    assert_eq!(p.check(&Expect { hidden: vec![badge], visible: vec![title], ..reads("Hi") }), Vec::<String>::new(), "as the file has it");
    click(&mut p, show);
    assert_eq!(p.check(&Expect { visible: vec![badge, title], ..reads("Welcome back") }), Vec::<String>::new(), "the bound layers followed");
    // What doesn't hold is said with both sides.
    let failures = p.check(&Expect { hidden: vec![badge], ..reads("Hi") });
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].contains("expected `Badge`") && failures[0].contains("to be hidden, but it is showing on `A`"), "{failures:?}");
    assert!(failures[1].contains("to read \"Hi\", but it reads \"Welcome back\""), "{failures:?}");
    let part = Expect { text: vec![TextExpect { node: title, equals: None, contains: Some("back".into()) }], ..Expect::default() };
    assert!(p.check(&part).is_empty());
}

#[test]
fn set_variable_mode_switches_what_bound_layers_show() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    ex(&mut s, "variable.createCollection", json!({"name": "Theme", "modes": ["Light", "Dark"]}));
    ex(&mut s, "variable.create", json!({"collection": "Theme", "name": "bg", "type": "color", "value": "#FFFFFF"}));
    ex(&mut s, "variable.set", json!({"variable": "bg", "mode": "Dark", "value": "#000000"}));
    ex(&mut s, "variable.bind", json!({"ids": [a], "field": "fill", "variable": "bg"}));
    let dark = button(&mut s, "Dark", a, 20.0, json!({"type": "setVariableMode", "collection": "Theme", "mode": "Dark"}));
    let fill = |p: &crate::proto_sim::Sim| p.document().node(a).and_then(|n| n.fills.first()?.solid_color()).map(|c| c.to_hex());
    let mut p = sim(&s, a);
    assert_eq!(fill(&p).as_deref(), Some("FFFFFF"));
    let o = click(&mut p, dark);
    assert!(o.fired && o.effects.iter().any(|e| e == "set mode of `Theme` to `Dark`"), "{o:?}");
    assert_eq!(fill(&p).as_deref(), Some("000000"));
    let st = p.state();
    assert_eq!(st.modes, BTreeMap::from([("Theme".to_string(), "Dark".to_string())]));
    assert!(st.variables.is_empty(), "the variable's values are the file's; its mode changed");
    // An expectation reads the variable in the mode the screen is in.
    assert!(p.check(&expect_variables(&[("Theme/bg", json!("#000000"))])).is_empty());
    assert!(p.check(&expect_variables(&[("bg", json!("#FFFFFF"))]))[0].contains("to be #FFFFFF, but it is #000000"));
}

#[test]
fn the_set_variable_step_stands_in_for_input() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    ex(&mut s, "variable.createCollection", json!({"name": "Other"}));
    ex(&mut s, "variable.create", json!({"collection": "Other", "name": "count", "type": "number", "value": 7}));
    let id = s.doc().unwrap().variables.get("label").unwrap().id.clone();
    let set = |variable: &str, value: Value| StepAction::SetVariable { variable: variable.into(), value };
    let mut p = sim(&s, a);
    // By `Collection/name`, by id, and by a name only one variable has.
    for (variable, value) in [("State/count", json!(3)), (id.as_str(), json!("typed")), ("open", json!(true))] {
        let o = p.step(&set(variable, value));
        assert!(o.fired && o.problem.is_none(), "{variable}: {o:?}");
    }
    let want = [("State/count", json!(3)), ("State/label", json!("typed")), ("State/open", json!(true)), ("Other/count", json!(7))];
    assert_eq!(p.check(&expect_variables(&want)), Vec::<String>::new());
    // (variable, value, what the problem says)
    for (variable, value, why) in [
        ("count", json!(1), "2 variables are named `count`"),
        ("missing", json!(1), "there is no variable `missing`"),
        ("State/count", json!("many"), "isn't a number value"),
        ("State/open", json!([1]), "isn't a boolean value"),
    ] {
        let o = p.step(&set(variable, value));
        assert!(!o.fired && problem(&o).contains(why), "{variable}: {o:?}");
    }
    assert_eq!(p.check(&expect_variables(&want)), Vec::<String>::new(), "a refused step sets nothing");
}

#[test]
fn actions_that_cannot_run_are_problems_that_name_them() {
    let mut s = Session::new();
    let (a, b) = (frame(&mut s, "A", 0.0), frame(&mut s, "B", 400.0));
    state_variables(&mut s);
    let broken = rect(&mut s, "Broken", a, 20.0, 20.0);
    let deep = rect(&mut s, "Deep", a, 20.0, 100.0);
    let mut nested = Action::Navigate { dest: b, preserve_scroll: false };
    for _ in 0..figcraft_doc::proto::MAX_ACTION_DEPTH * 2 {
        nested = Action::Conditional { blocks: vec![ConditionalBlock { condition: None, actions: vec![nested] }] };
    }
    let click_runs = |actions| Interaction {
        trigger: Trigger::OnClick,
        actions,
        transition: Default::default(),
        duration_ms: 0,
        easing: Default::default(),
        delay_ms: 0,
    };
    let mut doc = s.doc().unwrap().clone();
    // Written straight into the file: the commands refuse to store most of these.
    doc.get_mut(broken).unwrap().interactions.push(click_runs(vec![
        Action::SetVariable { variable: "gone".into(), value: json!(1), expr: None },
        Action::SetVariableMode { collection: "gone".into(), mode: "dark".into() },
        Action::Navigate { dest: figcraft_doc::NodeId(987_654), preserve_scroll: false },
        Action::OpenOverlay {
            dest: figcraft_doc::NodeId(987_655),
            position: Default::default(),
            close_on_outside: true,
            background: None,
            offset: None,
            anchor: Default::default(),
        },
    ]));
    doc.get_mut(deep).unwrap().interactions.push(click_runs(vec![nested]));
    let mut p = crate::proto_sim::Sim::new(&doc, a).unwrap();
    let o = click(&mut p, broken);
    assert!(!o.fired, "{o:?}");
    for why in [
        "there is no variable `gone`",
        "there is no collection `gone`",
        "navigate: layer 987654 doesn't exist",
        "open overlay: layer 987655 doesn't exist",
    ] {
        assert!(problem(&o).contains(why), "{why}: {o:?}");
    }
    let o = click(&mut p, deep);
    assert!(!o.fired && problem(&o).contains("conditional: nested more than"), "{o:?}");
    assert_eq!(p.screen(), a, "the player stops following conditionals that deep, and so does this");
}

#[test]
fn an_expected_value_of_another_type_fails_instead_of_passing_as_false() {
    let mut s = Session::new();
    let a = frame(&mut s, "A", 0.0);
    state_variables(&mut s);
    let p = sim(&s, a);
    // `open` is false. Expected as it is: passes, as a boolean or as the word.
    assert_eq!(p.check(&expect_variables(&[("open", json!(false))])), Vec::<String>::new());
    assert_eq!(p.check(&expect_variables(&[("open", json!("false"))])), Vec::<String>::new());
    // Expected as something that isn't a boolean: a failure that says so, not `false == false`.
    for wrong in [json!("banana"), json!("no"), json!(0), json!(null), json!([false])] {
        let failures = p.check(&expect_variables(&[("open", wrong.clone())]));
        assert_eq!(failures.len(), 1, "{wrong}: {failures:?}");
        assert!(
            failures[0].starts_with("expected `State/open` to be ")
                && failures[0].ends_with("but the expected value isn't a boolean value, which the variable holds"),
            "{failures:?}"
        );
    }
    assert_eq!(p.check(&expect_variables(&[("open", json!(true))])), ["expected `State/open` to be true, but it is false"]);
    // A number expected as a word that isn't one fails the same way.
    let failures = p.check(&expect_variables(&[("count", json!("many"))]));
    assert!(failures.len() == 1 && failures[0].contains("the expected value isn't a number value"), "{failures:?}");
}
