//! Reading `design.*` parameters. Every value is checked before anything is stored: text is
//! trimmed and limited to [`MAX_TEXT`] chars, lists to [`MAX_ENTRIES`] entries, layer links must
//! exist, and an error names the parameter and the value it refused.

use std::sync::Arc;

use figcraft_doc::design::{DesignContext, MAX_ENTRIES, MAX_TEXT};
use figcraft_doc::{Document, NodeId};
use serde_json::Value;

use crate::params::{get, id_of};
use crate::{Result, Session, bad};

/// Who a command runs for when it isn't an AI agent.
pub(crate) const USER: &str = "user";

/// The AI agent the running command is for, by the name its records are signed with
/// (`Claude (agent)`); `None`: the user. The one place the design and prototype commands ask.
pub(crate) fn agent(s: &Session) -> Option<&str> {
    s.agent.as_deref()
}

/// Who the running command is for: the agent's name (as its comments are signed), else [`USER`].
pub(crate) fn actor(s: &Session) -> String {
    agent(s).unwrap_or(USER).to_string()
}

/// The author of an end state or a scenario as messages name them: a file from before authors
/// were recorded says nothing, and what it holds is the user's.
pub(crate) fn author_name(author: &str) -> &str {
    if author.is_empty() { USER } else { author }
}

/// Change the design context with `f`, which also sees the document. The file changes (one undo
/// step) only when `f` succeeds and leaves the context different: an error or a call that
/// changes nothing leaves no trace.
pub(super) fn edit<R>(s: &mut Session, f: impl FnOnce(&mut DesignContext, &Document) -> Result<R>) -> Result<R> {
    let doc = s.doc()?;
    let mut ctx = DesignContext::clone(&doc.design);
    let out = f(&mut ctx, doc)?;
    if ctx != *doc.design {
        s.doc_mut()?.design = Arc::new(ctx);
    }
    Ok(out)
}

/// `v` for an error message, cut short so a huge value doesn't come back whole.
pub(super) fn shown(v: &Value) -> String {
    const MAX: usize = 40;
    let text = v.to_string();
    match text.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", text.get(..cut).unwrap_or_default()),
        None => text,
    }
}

fn checked_text(cmd: &str, what: &str, v: &Value) -> Result<String> {
    let Value::String(text) = v else { return Err(bad(cmd, format!("{what} must be text (got {})", shown(v)))) };
    let text = text.trim();
    let len = text.chars().count();
    if len > MAX_TEXT {
        return Err(bad(cmd, format!("{what} is {len} characters long; the most is {MAX_TEXT}")));
    }
    Ok(text.to_string())
}

/// `key` as trimmed text when it is given (`""` is given: it clears).
pub(super) fn text(cmd: &str, p: &Value, key: &str) -> Result<Option<String>> {
    get(p, key).map(|v| checked_text(cmd, &format!("`{key}`"), v)).transpose()
}

/// `key` as text that says something.
pub(super) fn req_text(cmd: &str, p: &Value, key: &str) -> Result<String> {
    match text(cmd, p, key)? {
        Some(t) if !t.is_empty() => Ok(t),
        Some(_) => Err(bad(cmd, format!("`{key}` can't be empty"))),
        None => Err(bad(cmd, format!("`{key}` is required"))),
    }
}

/// `key` as text that, when given, says something (a patch can't blank a required field).
pub(super) fn text_not_empty(cmd: &str, p: &Value, key: &str) -> Result<Option<String>> {
    match text(cmd, p, key)? {
        Some(t) if t.is_empty() => Err(bad(cmd, format!("`{key}` can't be empty"))),
        t => Ok(t),
    }
}

fn entries<'a>(cmd: &str, p: &'a Value, key: &str) -> Result<Option<&'a Vec<Value>>> {
    match get(p, key) {
        None => Ok(None),
        Some(Value::Array(a)) if a.len() <= MAX_ENTRIES => Ok(Some(a)),
        Some(Value::Array(a)) => Err(bad(cmd, format!("`{key}` has {} entries; the most is {MAX_ENTRIES}", a.len()))),
        Some(v) => Err(bad(cmd, format!("`{key}` must be a list (got {})", shown(v)))),
    }
}

/// `key` as a list of non-empty texts when it is given (`[]` clears).
pub(super) fn texts(cmd: &str, p: &Value, key: &str) -> Result<Option<Vec<String>>> {
    let Some(list) = entries(cmd, p, key)? else { return Ok(None) };
    let mut out = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        let entry = checked_text(cmd, &format!("`{key}`: entry {}", i + 1), v)?;
        if entry.is_empty() {
            return Err(bad(cmd, format!("`{key}`: entry {} is empty", i + 1)));
        }
        out.push(entry);
    }
    Ok(Some(out))
}

/// `key` as a list of layers of `doc` when it is given, each once, in the order given.
pub(super) fn layers(cmd: &str, doc: &Document, p: &Value, key: &str) -> Result<Option<Vec<NodeId>>> {
    let Some(list) = entries(cmd, p, key)? else { return Ok(None) };
    let mut out: Vec<NodeId> = Vec::with_capacity(list.len());
    for v in list {
        let id = id_of(v).ok_or_else(|| bad(cmd, format!("`{key}`: {} isn't a layer id", shown(v))))?;
        if !doc.contains(id) {
            return Err(bad(cmd, format!("`{key}`: no layer {id}")));
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(Some(out))
}

/// `key` as a layer id (not looked up: a link can outlive its layer).
pub(super) fn req_layer_id(cmd: &str, p: &Value, key: &str) -> Result<NodeId> {
    match get(p, key) {
        None => Err(bad(cmd, format!("`{key}` is required (a layer id)"))),
        Some(v) => id_of(v).ok_or_else(|| bad(cmd, format!("`{key}`: {} isn't a layer id", shown(v)))),
    }
}

/// `key` as a layer of `doc`.
pub(super) fn req_layer(cmd: &str, doc: &Document, p: &Value, key: &str) -> Result<NodeId> {
    let id = req_layer_id(cmd, p, key)?;
    if doc.contains(id) { Ok(id) } else { Err(bad(cmd, format!("`{key}`: no layer {id}"))) }
}

/// `key` as one of `options` (by name, any case) when it is given.
pub(super) fn choice<T: Copy>(cmd: &str, p: &Value, key: &str, options: &[(&str, T)]) -> Result<Option<T>> {
    let Some(v) = get(p, key) else { return Ok(None) };
    let name = v.as_str().map(str::trim).unwrap_or_default();
    match options.iter().find(|(o, _)| o.eq_ignore_ascii_case(name)) {
        Some((_, value)) => Ok(Some(*value)),
        None => {
            let names: Vec<&str> = options.iter().map(|(o, _)| *o).collect();
            Err(bad(cmd, format!("`{key}` must be one of {} (got {})", names.join(", "), shown(v))))
        }
    }
}

/// `key` as one of `options`, required.
pub(super) fn req_choice<T: Copy>(cmd: &str, p: &Value, key: &str, options: &[(&str, T)]) -> Result<T> {
    choice(cmd, p, key, options)?.ok_or_else(|| {
        let names: Vec<&str> = options.iter().map(|(o, _)| *o).collect();
        bad(cmd, format!("`{key}` is required (one of {})", names.join(", ")))
    })
}

/// `key` as the id of an entry of the context (`A1`, `D2`, `F3`, `V1`; any case).
pub(super) fn req_entry_id(cmd: &str, p: &Value, key: &str) -> Result<String> {
    match get(p, key) {
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(id.trim().to_ascii_uppercase()),
        Some(v) => Err(bad(cmd, format!("`{key}` must be an id like A1 (got {})", shown(v)))),
        None => Err(bad(cmd, format!("`{key}` is required"))),
    }
}

/// `key` as a list of entry ids when it is given, each once.
pub(super) fn entry_ids(cmd: &str, p: &Value, key: &str) -> Result<Option<Vec<String>>> {
    let Some(list) = entries(cmd, p, key)? else { return Ok(None) };
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for v in list {
        let Some(id) = v.as_str().map(str::trim).filter(|i| !i.is_empty()) else {
            return Err(bad(cmd, format!("`{key}`: {} isn't an id like A1", shown(v))));
        };
        let id = id.to_ascii_uppercase();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(Some(out))
}

/// `key` as a yes or no when it is given.
pub(super) fn flag(cmd: &str, p: &Value, key: &str) -> Result<Option<bool>> {
    match get(p, key) {
        None => Ok(None),
        Some(v) => crate::params::bool_of(v).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false (got {})", shown(v)))),
    }
}

/// Refuse a list that would grow past [`MAX_ENTRIES`].
pub(super) fn room_for(cmd: &str, what: &str, len: usize, more: usize) -> Result<()> {
    if len.saturating_add(more) > MAX_ENTRIES {
        return Err(bad(cmd, format!("the file holds {len} {what}; the most is {MAX_ENTRIES} (remove some first)")));
    }
    Ok(())
}
