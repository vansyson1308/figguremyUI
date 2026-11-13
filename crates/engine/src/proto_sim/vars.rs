//! Variables as scenarios name them and as reports show them.

use figcraft_doc::{Document, NodeId, Variable, VariableValue};
use serde_json::{Value, json};

use crate::error::prose;

/// The variable a scenario names: its id, `Collection/name`, or its name when only one variable
/// has it. The error says which of these failed.
pub fn find_variable<'a>(doc: &'a Document, key: &str) -> Result<&'a Variable, String> {
    let vars = &doc.variables;
    if let Some(v) = vars.get_by_id(key) {
        return Ok(v);
    }
    if let Some(v) = vars.variables.iter().find(|v| full_name(doc, v) == key) {
        return Ok(v);
    }
    let mut named = vars.variables.iter().filter(|v| v.name == key);
    match (named.next(), named.count()) {
        (Some(v), 0) => Ok(v),
        (Some(_), more) => Err(format!("{} variables are named `{}`: say which as `Collection/name`", more + 1, prose(key))),
        (None, _) => Err(format!("there is no variable `{}`", prose(key))),
    }
}

/// `Collection/name`, as reports and expectations name a variable.
pub(super) fn full_name(doc: &Document, v: &Variable) -> String {
    match doc.variables.collection_by_id(&v.collection) {
        Some(c) => format!("{}/{}", c.name, v.name),
        None => v.name.clone(),
    }
}

/// [`full_name`] as a sentence shows it ([`prose`]: one line, clipped, no backtick of its own).
pub(super) fn prose_name(doc: &Document, v: &Variable) -> String {
    prose(&full_name(doc, v))
}

/// The value variable `v` has for the layers of `screen` (its collection's mode there).
pub(super) fn value_on(doc: &Document, v: &Variable, screen: NodeId) -> Option<VariableValue> {
    doc.variables.resolve(&v.id, &doc.modes_for(screen))
}

/// A value as JSON: numbers, text and booleans as themselves, colours as `#RRGGBB(AA)`.
pub(super) fn value_json(v: &VariableValue) -> Value {
    match v {
        VariableValue::Number(n) => json!(n),
        VariableValue::String(s) => json!(s),
        VariableValue::Boolean(b) => json!(b),
        VariableValue::Color(c) => json!(c.to_css_hex()),
        other => json!(other.display()),
    }
}

/// A value in a sentence: text in quotes, colours as `#RRGGBB(AA)`, the rest as the variables
/// panel shows it (`5`, not `5.0`).
pub(super) fn value_text(v: &VariableValue) -> String {
    match v {
        VariableValue::Color(c) => c.to_css_hex(),
        VariableValue::String(s) => format!("{s:?}"),
        other => other.display(),
    }
}
