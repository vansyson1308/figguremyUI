//! MCP resources: the document, one layer, one command, the open comments, the design context
//! with its findings, and the prototype's flow and scenarios.

use serde_json::{Value, json};

use crate::backend::Backend;

pub const DOC_URI: &str = "figcraft://document";
pub const COMMANDS_URI: &str = "figcraft://commands";
pub const COMMENTS_URI: &str = "figcraft://comments";
pub const DESIGN_URI: &str = "figcraft://design";
pub const FINDINGS_URI: &str = "figcraft://design/findings";
pub const FLOW_URI: &str = "figcraft://prototype/flow";
pub const SCENARIOS_URI: &str = "figcraft://prototype/scenarios";
pub const NODE_TEMPLATE: &str = "figcraft://node/{id}";
pub const COMMAND_TEMPLATE: &str = "figcraft://command/{id}";

pub enum ReadError {
    NotFound(String),
    Backend(String),
}

pub fn list() -> Value {
    json!({"resources": [
        {"uri": DOC_URI, "name": "document", "title": "Working document", "description": "The working document as JSON (current page's layer tree, pages, styles, selection, history), like inspect_document.", "mimeType": "application/json"},
        {"uri": COMMANDS_URI, "name": "commands", "title": "Command catalog", "description": "Every FigCraft command with label, menu, shortcut and parameter docs.", "mimeType": "application/json"},
        {"uri": COMMENTS_URI, "name": "comments", "title": "Open comments", "description": "Every open comment thread on every page, as JSON (the same objects as comment.list).", "mimeType": "application/json"},
        {"uri": DESIGN_URI, "name": "design", "title": "Design context", "description": "Why the working document is the way it is: the brief, the stage, assumptions, decisions, scenarios, end states, findings, alternatives and links to layers that are gone (what the design_context tool returns).", "mimeType": "application/json"},
        {"uri": FINDINGS_URI, "name": "findings", "title": "Findings", "description": "The findings stored in the working document: measured checks, judgements and people's feedback, each with its status. They are what was recorded; check_design measures the file as it is now.", "mimeType": "application/json"},
        {"uri": FLOW_URI, "name": "flow", "title": "Prototype flow", "description": "The current page's prototype as facts: starts, screens with their reachability, dead ends and end states, and what the flow rules find (what the check_prototype tool returns).", "mimeType": "application/json"},
        {"uri": SCENARIOS_URI, "name": "scenarios", "title": "Scenarios", "description": "The scenarios stored in the working document: named walks through the prototype with what must hold along the way. Stored is not run: run_scenarios plays them.", "mimeType": "application/json"},
    ]})
}

pub fn templates() -> Value {
    json!({"resourceTemplates": [
        {"uriTemplate": NODE_TEMPLATE, "name": "node", "title": "Layer", "description": "One layer and its children (depth 2) as JSON.", "mimeType": "application/json"},
        {"uriTemplate": COMMAND_TEMPLATE, "name": "command", "title": "Command", "description": "One command's label, menu path, shortcut and parameter docs, e.g. figcraft://command/layout.add.", "mimeType": "application/json"},
    ]})
}

/// The read-only command a resource is, with its parameters, for the resources that are one.
fn command_of(uri: &str) -> Option<(&'static str, Value)> {
    Some(match uri {
        COMMENTS_URI => ("comment.list", json!({"page": "all", "filter": "open"})),
        DESIGN_URI => ("design.get", json!({})),
        FINDINGS_URI => ("design.finding.list", json!({})),
        FLOW_URI => ("proto.check", json!({})),
        SCENARIOS_URI => ("proto.scenario.list", json!({})),
        _ => return None,
    })
}

/// Read resource `uri`. `document`: the working document (its id), which the document, node,
/// comments, design and prototype resources are read from; without one they are the tab in
/// front's.
pub fn read(b: &mut dyn Backend, uri: &str, document: Option<&Value>) -> Result<Value, ReadError> {
    let in_document = |mut params: Value| {
        if let (Some(d), Some(o)) = (document, params.as_object_mut()) {
            o.insert("document".into(), d.clone());
        }
        params
    };
    if uri == DOC_URI {
        return b.call("document.inspect", in_document(json!({"depth": 8}))).map_err(ReadError::Backend);
    }
    if uri == COMMANDS_URI {
        return b.call("engine.commands", json!({})).map_err(ReadError::Backend);
    }
    if let Some((command, params)) = command_of(uri) {
        return b.call("engine.execute", in_document(json!({"command": command, "params": params}))).map_err(ReadError::Backend);
    }
    if let Some(id) = uri.strip_prefix("figcraft://node/") {
        let Some(id) = figcraft_engine::params::id_of(&Value::String(id.to_string())) else { return Err(ReadError::NotFound(uri.into())) };
        return match b.call("node.get", in_document(json!({"id": id.0, "depth": 2}))) {
            Ok(Value::Null) => Err(ReadError::NotFound(uri.into())),
            Ok(v) => Ok(v),
            Err(e) if e.contains("no such node") => Err(ReadError::NotFound(uri.into())),
            Err(e) => Err(ReadError::Backend(e)),
        };
    }
    if let Some(id) = uri.strip_prefix("figcraft://command/") {
        let all = b.call("engine.commands", json!({})).map_err(ReadError::Backend)?;
        return all
            .as_array()
            .and_then(|a| a.iter().find(|c| c.get("id").and_then(Value::as_str) == Some(id)).cloned())
            .ok_or_else(|| ReadError::NotFound(uri.into()));
    }
    Err(ReadError::NotFound(uri.into()))
}
