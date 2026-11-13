//! Comparing the simulator's state with what a scenario expects. Every failure says what was
//! expected and what is there instead, naming layers by name and id.

use figcraft_doc::design::{Expect, TextExpect, VariantExpect};
use figcraft_doc::{NodeId, NodeKind, VariableType, VariableValue};
use serde_json::Value;

use super::state::is_end_state;
use super::steps::{Showing, showing};
use super::vars::{find_variable, prose_name, value_on, value_text};
use super::{Sim, label};

/// Numbers this close are the same value (expressions do floating-point arithmetic).
const TOLERANCE: f64 = 1e-9;

pub(super) fn check(sim: &Sim, e: &Expect) -> Vec<String> {
    let doc = &sim.shown;
    let mut out = Vec::new();
    if let Some(want) = e.screen
        && want != sim.screen
    {
        out.push(format!("expected screen {}, but {} is showing", label(doc, want), label(doc, sim.screen)));
    }
    if let Some(want) = &e.overlays {
        let open: Vec<NodeId> = sim.overlays.iter().map(|o| o.id).collect();
        if *want != open {
            let list = |ids: &[NodeId]| match ids {
                [] => "no overlay".to_string(),
                ids => ids.iter().map(|i| label(doc, *i)).collect::<Vec<_>>().join(", "),
            };
            out.push(format!("expected {} open, but {} is open (bottom first)", list(want), list(&open)));
        }
    }
    out.extend(e.variables.iter().filter_map(|(key, want)| variable(sim, key, want)));
    out.extend(e.variants.iter().filter_map(|v| variant(sim, v)));
    out.extend(e.visible.iter().filter_map(|id| visible(sim, *id)));
    out.extend(e.hidden.iter().filter_map(|id| hidden(sim, *id)));
    out.extend(e.text.iter().filter_map(|t| text(sim, t)));
    if let Some(want) = e.end_state
        && want != is_end_state(&sim.design, sim.screen)
    {
        let screen = label(doc, sim.screen);
        out.push(if want {
            format!("expected {screen} to be a declared end state, but it isn't one")
        } else {
            format!("expected {screen} not to be an end state, but it is declared as one")
        });
    }
    out
}

/// Whether two values of one variable are the same: numbers within [`TOLERANCE`], colours as the
/// hex they show as.
fn same(a: &VariableValue, b: &VariableValue) -> bool {
    match (a, b) {
        (VariableValue::Number(x), VariableValue::Number(y)) => (x - y).abs() <= TOLERANCE,
        (VariableValue::Color(x), VariableValue::Color(y)) => x.to_css_hex() == y.to_css_hex(),
        _ => a == b,
    }
}

/// An expected value as a value of a `kind` variable. Stricter than what an action may write
/// ([`crate::proto_run::literal`] reads any text but "true" as false): a boolean is expected as
/// `true` or `false` (or those words), so a mistyped expectation fails instead of passing as
/// `false`.
fn expected(kind: VariableType, want: &Value) -> Option<VariableValue> {
    match (kind, want) {
        (VariableType::Boolean, Value::Bool(b)) => Some(VariableValue::Boolean(*b)),
        (VariableType::Boolean, Value::String(s)) => match s.trim() {
            "true" => Some(VariableValue::Boolean(true)),
            "false" => Some(VariableValue::Boolean(false)),
            _ => None,
        },
        (VariableType::Boolean, _) => None,
        _ => crate::proto_run::literal(kind, want),
    }
}

fn variable(sim: &Sim, key: &str, want: &Value) -> Option<String> {
    let doc = &sim.design;
    let shown = figcraft_format::clip(&want.to_string(), 80);
    let v = match find_variable(doc, key) {
        Ok(v) => v,
        Err(why) => return Some(format!("expected `{key}` to be {shown}, but {why}")),
    };
    let name = prose_name(doc, v);
    let Some(want) = expected(v.kind, want) else {
        return Some(format!("expected `{name}` to be {shown}, but the expected value isn't a {} value, which the variable holds", v.kind.id()));
    };
    match value_on(doc, v, sim.screen) {
        Some(is) if same(&is, &want) => None,
        Some(is) => Some(format!("expected `{name}` to be {}, but it is {}", value_text(&want), value_text(&is))),
        None => Some(format!("expected `{name}` to be {}, but it has no value", value_text(&want))),
    }
}

/// Whether `want` names the variant called `name` with id `component`: the id, the whole name,
/// one `Property=Value` pair of it, or one of its property values.
fn names_variant(want: &str, component: NodeId, name: &str) -> bool {
    let want = want.trim();
    if want.parse::<NodeId>().is_ok_and(|id| id == component) || want == name.trim() {
        return true;
    }
    let props = crate::components::variant_props(name);
    match want.split_once('=') {
        Some((k, v)) => props.get(k.trim()).is_some_and(|value| value == v.trim()),
        None => props.values().any(|value| value == want),
    }
}

fn variant(sim: &Sim, e: &VariantExpect) -> Option<String> {
    let doc = &sim.shown;
    let (inst, want) = (label(doc, e.instance), figcraft_format::clip(&e.variant, 80));
    let n = match doc.node(e.instance) {
        Some(n) => n,
        None => return Some(format!("expected {inst} to show variant `{want}`, but it doesn't exist")),
    };
    let NodeKind::Instance { component, .. } = &n.kind else {
        return Some(format!("expected {inst} to show variant `{want}`, but it isn't an instance"));
    };
    let name = doc.node(*component).map(|c| c.name.clone()).unwrap_or_default();
    (!names_variant(&e.variant, *component, &name))
        .then(|| format!("expected {inst} to show variant `{want}`, but it shows `{}` ({component})", crate::error::prose(&name)))
}

/// Whether `id` shows on the screen or an open overlay; else why not.
fn shows(sim: &Sim, id: NodeId) -> Result<NodeId, String> {
    let doc = &sim.shown;
    if !doc.contains(id) {
        return Err("it doesn't exist".into());
    }
    let mut why = None;
    for root in std::iter::once(sim.screen).chain(sim.overlays.iter().map(|o| o.id)) {
        match showing(doc, root, id) {
            Showing::Yes => return Ok(root),
            Showing::Hidden(by) if by == id => why = Some("it is hidden".to_string()),
            Showing::Hidden(by) => why = Some(format!("{} around it is hidden", label(doc, by))),
            Showing::Elsewhere => {}
        }
    }
    Err(why.unwrap_or_else(|| format!("it isn't on {} or an open overlay", label(doc, sim.screen))))
}

fn visible(sim: &Sim, id: NodeId) -> Option<String> {
    shows(sim, id).err().map(|why| format!("expected {} to be showing, but {why}", label(&sim.shown, id)))
}

fn hidden(sim: &Sim, id: NodeId) -> Option<String> {
    let doc = &sim.shown;
    if !doc.contains(id) {
        return Some(format!("expected {} to be hidden, but it doesn't exist, so nothing was checked", label(doc, id)));
    }
    shows(sim, id).ok().map(|root| format!("expected {} to be hidden, but it is showing on {}", label(doc, id), label(doc, root)))
}

fn text(sim: &Sim, e: &TextExpect) -> Option<String> {
    let doc = &sim.shown;
    let layer = label(doc, e.node);
    let quoted = |s: &str| format!("{:?}", figcraft_format::clip(s, 200));
    let Some(n) = doc.node(e.node) else { return Some(format!("expected text on {layer}, but it doesn't exist")) };
    let Some(t) = n.kind.text() else { return Some(format!("expected text on {layer}, but it isn't a text layer")) };
    let reads = &t.characters;
    match (&e.equals, &e.contains) {
        (None, None) => Some(format!("expected text on {layer}, but neither `equals` nor `contains` says what")),
        (Some(want), _) if want != reads => Some(format!("expected {layer} to read {}, but it reads {}", quoted(want), quoted(reads))),
        (_, Some(part)) if !reads.contains(part.as_str()) => {
            Some(format!("expected {layer} to contain {}, but it reads {}", quoted(part), quoted(reads)))
        }
        _ => None,
    }
}
