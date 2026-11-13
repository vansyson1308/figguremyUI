//! Scenario steps and expectations as commands take them. Layer ids are numbers or strings, as in
//! other commands; every error names the place (`steps[2].expect.visible[0]`) and the value.

use std::collections::BTreeMap;

use figcraft_doc::design::{Expect, Gesture, MAX_ENTRIES, MAX_TEXT, Scenario, Step, StepAction, TextExpect, VariantExpect};
use figcraft_doc::{Document, NodeId};
use serde_json::{Map, Value};

use crate::params::{bool_of, id_of};
use crate::{Result, bad};

/// The gestures a step can name, also as shorthand keys (`{click: <layer>}`).
const GESTURES: [(&str, Gesture); 8] = [
    ("click", Gesture::Click),
    ("hover", Gesture::Hover),
    ("press", Gesture::Press),
    ("drag", Gesture::Drag),
    ("mouseEnter", Gesture::MouseEnter),
    ("mouseLeave", Gesture::MouseLeave),
    ("mouseDown", Gesture::MouseDown),
    ("mouseUp", Gesture::MouseUp),
];

const STEP_KEYS: [&str; 9] = ["type", "node", "gesture", "key", "variable", "value", "expect", "note", "action"];
const EXPECT_KEYS: [&str; 8] = ["screen", "overlays", "variables", "variants", "visible", "hidden", "text", "endState"];

/// A value in an error message, cut short.
fn shown(v: &Value) -> String {
    figcraft_format::clip(&v.to_string(), 60)
}

fn object<'a>(cmd: &str, v: &'a Value, at: &str) -> Result<&'a Map<String, Value>> {
    v.as_object().ok_or_else(|| bad(cmd, format!("`{at}` must be an object (got {})", shown(v))))
}

/// Refuse keys `o` has that aren't in `known`: a misspelt `expect` would otherwise check nothing.
fn only(cmd: &str, o: &Map<String, Value>, known: &[&str], at: &str) -> Result<()> {
    match o.keys().find(|k| !known.contains(&k.as_str())) {
        Some(k) => Err(bad(cmd, format!("`{at}`: unknown key `{}` (it takes {})", figcraft_format::clip(k, 40), known.join(", ")))),
        None => Ok(()),
    }
}

fn list<'a>(cmd: &str, v: &'a Value, at: &str) -> Result<&'a [Value]> {
    let a = v.as_array().ok_or_else(|| bad(cmd, format!("`{at}` must be a list (got {})", shown(v))))?;
    if a.len() > MAX_ENTRIES {
        return Err(bad(cmd, format!("`{at}`: {} entries, at most {MAX_ENTRIES}", a.len())));
    }
    Ok(a)
}

fn layer(cmd: &str, v: &Value, at: &str) -> Result<NodeId> {
    id_of(v).ok_or_else(|| bad(cmd, format!("`{at}` must be a layer id (got {})", shown(v))))
}

fn layers(cmd: &str, v: &Value, at: &str) -> Result<Vec<NodeId>> {
    list(cmd, v, at)?.iter().enumerate().map(|(i, x)| layer(cmd, x, &format!("{at}[{i}]"))).collect()
}

/// Text (a number or boolean reads as its text), at most [`MAX_TEXT`] characters.
pub(super) fn text(cmd: &str, v: &Value, at: &str) -> Result<String> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        _ => return Err(bad(cmd, format!("`{at}` must be text (got {})", shown(v)))),
    };
    if s.chars().count() > MAX_TEXT {
        return Err(bad(cmd, format!("`{at}`: longer than {MAX_TEXT} characters")));
    }
    Ok(s)
}

/// A variable's value: any JSON the variable's type may read, of bounded size.
fn value(cmd: &str, v: &Value, at: &str) -> Result<Value> {
    if v.to_string().chars().count() > MAX_TEXT {
        return Err(bad(cmd, format!("`{at}`: longer than {MAX_TEXT} characters")));
    }
    Ok(v.clone())
}

fn present<'a>(o: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    o.get(key).filter(|v| !v.is_null())
}

fn required<'a>(cmd: &str, o: &'a Map<String, Value>, key: &str, at: &str) -> Result<&'a Value> {
    present(o, key).ok_or_else(|| bad(cmd, format!("`{at}.{key}` is required")))
}

/// What must hold: `{screen?, overlays?, variables?, variants?, visible?, hidden?, text?, endState?}`.
pub(super) fn expect(cmd: &str, v: &Value, at: &str) -> Result<Expect> {
    let o = object(cmd, v, at)?;
    only(cmd, o, &EXPECT_KEYS, at)?;
    let path = |key: &str| format!("{at}.{key}");
    let ids = |key: &str| present(o, key).map(|v| layers(cmd, v, &path(key))).transpose();
    let mut e = Expect {
        screen: present(o, "screen").map(|v| layer(cmd, v, &path("screen"))).transpose()?,
        overlays: ids("overlays")?,
        visible: ids("visible")?.unwrap_or_default(),
        hidden: ids("hidden")?.unwrap_or_default(),
        ..Expect::default()
    };
    if let Some(v) = present(o, "variables") {
        let vars = object(cmd, v, &path("variables"))?;
        if vars.len() > MAX_ENTRIES {
            return Err(bad(cmd, format!("`{}`: {} entries, at most {MAX_ENTRIES}", path("variables"), vars.len())));
        }
        e.variables = vars
            .iter()
            .map(|(name, v)| {
                let at = format!("{}.{}", path("variables"), figcraft_format::clip(name, 40));
                if name.chars().count() > MAX_TEXT {
                    return Err(bad(cmd, format!("`{at}`: the name is longer than {MAX_TEXT} characters")));
                }
                Ok((name.clone(), value(cmd, v, &at)?))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
    }
    if let Some(v) = present(o, "variants") {
        e.variants = list(cmd, v, &path("variants"))?
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let at = format!("{}[{i}]", path("variants"));
                let x = object(cmd, x, &at)?;
                only(cmd, x, &["instance", "variant"], &at)?;
                let instance = layer(cmd, required(cmd, x, "instance", &at)?, &format!("{at}.instance"))?;
                Ok(VariantExpect { instance, variant: text(cmd, required(cmd, x, "variant", &at)?, &format!("{at}.variant"))? })
            })
            .collect::<Result<_>>()?;
    }
    if let Some(v) = present(o, "text") {
        e.text = list(cmd, v, &path("text"))?
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let at = format!("{}[{i}]", path("text"));
                let x = object(cmd, x, &at)?;
                only(cmd, x, &["node", "equals", "contains"], &at)?;
                let read = |key: &str| present(x, key).map(|v| text(cmd, v, &format!("{at}.{key}"))).transpose();
                let t = TextExpect {
                    node: layer(cmd, required(cmd, x, "node", &at)?, &format!("{at}.node"))?,
                    equals: read("equals")?,
                    contains: read("contains")?,
                };
                if t.equals.is_none() && t.contains.is_none() {
                    return Err(bad(cmd, format!("`{at}` needs `equals` or `contains`")));
                }
                Ok(t)
            })
            .collect::<Result<_>>()?;
    }
    if let Some(v) = present(o, "endState") {
        e.end_state = Some(bool_of(v).ok_or_else(|| bad(cmd, format!("`{}` must be true or false (got {})", path("endState"), shown(v))))?);
    }
    Ok(e)
}

/// The gesture named `name`.
fn gesture(cmd: &str, v: &Value, at: &str) -> Result<Gesture> {
    let found = v.as_str().and_then(|name| GESTURES.iter().find(|(id, _)| *id == name));
    found.map(|(_, g)| *g).ok_or_else(|| {
        let names: Vec<&str> = GESTURES.iter().map(|(id, _)| *id).collect();
        bad(cmd, format!("`{at}`: {} isn't a gesture ({})", shown(v), names.join("|")))
    })
}

/// What the viewer does in a step written with a `type`.
fn typed(cmd: &str, o: &Map<String, Value>, kind: &Value, at: &str) -> Result<StepAction> {
    let path = |key: &str| format!("{at}.{key}");
    Ok(match kind.as_str() {
        Some("gesture") => StepAction::Gesture {
            node: layer(cmd, required(cmd, o, "node", at)?, &path("node"))?,
            gesture: present(o, "gesture").map(|v| gesture(cmd, v, &path("gesture"))).transpose()?.unwrap_or_default(),
        },
        Some("key") => StepAction::Key { key: text(cmd, required(cmd, o, "key", at)?, &path("key"))? },
        Some("wait") => StepAction::Wait,
        Some("clickOutside") => StepAction::ClickOutside,
        Some("setVariable") => StepAction::SetVariable {
            variable: text(cmd, required(cmd, o, "variable", at)?, &path("variable"))?,
            value: value(cmd, required(cmd, o, "value", at)?, &path("value"))?,
        },
        _ => return Err(bad(cmd, format!("`{}`: {} isn't a step type (gesture|key|wait|clickOutside|setVariable)", path("type"), shown(kind)))),
    })
}

/// One step: `{type: gesture, node, gesture?} | {type: key, key} | {type: wait} |
/// {type: clickOutside} | {type: setVariable, variable, value}`, the shorthand `{click: <layer>}`
/// (any gesture's name) or `{key: "Enter"}`, or the stored form `{action: {type…}}`; each with an
/// optional `expect` and `note`.
pub(super) fn step(cmd: &str, v: &Value, at: &str) -> Result<Step> {
    let o = object(cmd, v, at)?;
    let shorthand: Vec<&(&str, Gesture)> = GESTURES.iter().filter(|(name, _)| o.contains_key(*name)).collect();
    let known: Vec<&str> = STEP_KEYS.iter().copied().chain(GESTURES.iter().map(|(name, _)| *name)).collect();
    only(cmd, o, &known, at)?;
    let action = match (present(o, "type"), present(o, "action"), shorthand.as_slice()) {
        (Some(kind), None, []) => typed(cmd, o, kind, at)?,
        (None, Some(inner), []) => {
            let at = format!("{at}.action");
            let inner = object(cmd, inner, &at)?;
            only(cmd, inner, &["type", "node", "gesture", "key", "variable", "value"], &at)?;
            typed(cmd, inner, required(cmd, inner, "type", &at)?, &at)?
        }
        (None, None, [(name, g)]) => {
            StepAction::Gesture { node: layer(cmd, o.get(*name).unwrap_or(&Value::Null), &format!("{at}.{name}"))?, gesture: *g }
        }
        (None, None, []) if present(o, "key").is_some() => typed(cmd, o, &Value::String("key".into()), at)?,
        (None, None, []) => {
            return Err(bad(
                cmd,
                format!("`{at}` needs a `type` (gesture|key|wait|clickOutside|setVariable) or a shorthand such as {{click: <layer id>}}"),
            ));
        }
        _ => return Err(bad(cmd, format!("`{at}` says what to do more than once: give one of `type`, `action` or a gesture shorthand"))),
    };
    Ok(Step {
        action,
        expect: present(o, "expect").map(|v| expect(cmd, v, &format!("{at}.expect"))).transpose()?.unwrap_or_default(),
        note: present(o, "note").map(|v| text(cmd, v, &format!("{at}.note"))).transpose()?.unwrap_or_default(),
    })
}

/// The steps in list `v` (`at` names it), at most [`MAX_ENTRIES`].
pub(super) fn steps(cmd: &str, v: &Value, at: &str) -> Result<Vec<Step>> {
    list(cmd, v, at)?.iter().enumerate().map(|(i, s)| step(cmd, s, &format!("{at}[{i}]"))).collect()
}

/// A stored scenario must point at layers that exist, and start on a top-level frame: the first
/// thing that doesn't is the error, named by where the scenario says it.
pub(super) fn validate(cmd: &str, doc: &Document, s: &Scenario) -> Result<()> {
    let exists = |id: NodeId, at: &str| if doc.contains(id) { Ok(()) } else { Err(bad(cmd, format!("`{at}`: no layer {id}"))) };
    exists(s.start, "start")?;
    if doc.top_level_frame(s.start) != Some(s.start) {
        return Err(bad(cmd, format!("`start`: {} isn't a top-level frame", crate::proto_sim::label(doc, s.start))));
    }
    let expected = |e: &Expect, at: &str| -> Result<()> {
        e.screen.map_or(Ok(()), |id| exists(id, &format!("{at}.screen")))?;
        let lists = [("overlays", e.overlays.clone().unwrap_or_default()), ("visible", e.visible.clone()), ("hidden", e.hidden.clone())];
        for (key, ids) in lists {
            ids.iter().enumerate().try_for_each(|(i, id)| exists(*id, &format!("{at}.{key}[{i}]")))?;
        }
        e.variants.iter().enumerate().try_for_each(|(i, v)| exists(v.instance, &format!("{at}.variants[{i}].instance")))?;
        e.text.iter().enumerate().try_for_each(|(i, t)| exists(t.node, &format!("{at}.text[{i}].node")))
    };
    for (i, step) in s.steps.iter().enumerate() {
        if let Some(node) = step.action.node() {
            exists(node, &format!("steps[{i}].node"))?;
        }
        expected(&step.expect, &format!("steps[{i}].expect"))?;
    }
    expected(&s.expect, "expect")
}
