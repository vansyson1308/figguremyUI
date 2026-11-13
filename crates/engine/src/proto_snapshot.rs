//! `proto.snapshot`: a picture of a simulated prototype state.
//!
//! [`crate::proto_sim`] says which screen shows and which overlays are open after a sequence of
//! triggers. This command plays such a sequence on a copy of the file and answers what that state
//! looks like, as a PNG: the screen with the open overlays over it where the player puts them. So
//! a state reached by clicking through a prototype can be looked at without a window, by a
//! person reading a report or by an AI agent that has no player.
//!
//! The picture is the simulator's ([`crate::proto_sim::render`]) and has its limits: the screen's
//! own box as it is in the file (no device around it, not resized, nothing scrolled), and no
//! animation in progress. It shows what the state rules produce, not what a viewer saw.
//!
//! The command reads only: the file is not changed and no undo step is made. The steps are the
//! ones `proto.simulate` takes; what a step expects (`expect`) is that command's to check and is
//! not looked at here, and every step runs whether or not the one before it did anything, since a
//! picture of where the prototype ended up is still a picture.

use base64::Engine as _;
use figcraft_doc::design::{Gesture, MAX_ENTRIES, MAX_TEXT, StepAction};
use figcraft_doc::{ExportFormat, NodeId};
use serde_json::{Map, Value, json};

use crate::cmd::CommandSpec;
use crate::error::bad_param;
use crate::params::{f64_of, get, id_of, opt_id};
use crate::proto_sim::Sim;
use crate::{EngineError, Result, Session, bad, spec};

#[cfg(test)]
mod tests;

const CMD: &str = "proto.snapshot";

/// The largest side of a picture, in pixels (what a headless page render is held to as well).
pub const MAX_SIDE: f64 = 4096.0;
/// The scales a picture may be asked for (pixels per page unit).
pub const SCALES: std::ops::RangeInclusive<f64> = 0.1..=4.0;

/// The keys a step may have beside a gesture's name as shorthand.
const STEP_KEYS: [&str; 9] = ["type", "node", "gesture", "key", "variable", "value", "expect", "note", "action"];
/// The keys that say what the viewer does.
const ACTION_KEYS: [&str; 6] = ["type", "node", "gesture", "key", "variable", "value"];

pub fn specs() -> Vec<CommandSpec> {
    vec![spec!(
        CMD,
        "Picture of a prototype state",
        "{start? (a top-level frame id; default as proto.simulate: the selected frame, else the first flow, else the first frame), steps? [the steps proto.simulate takes: {type: gesture, node, gesture?} | {type: key, key} | {type: wait} | {type: clickOutside} | {type: setVariable, variable, value}, the shorthand {click: <layer id>} (any gesture's name) or {key: \"Enter\"}; a step's expect and note are accepted and not looked at; default: none, the start screen as it opens], scale? (pixels per page unit, 0.1 to 4; default 1; lowered so that the larger side is at most 4096 pixels), path? (write the PNG there instead of returning it)}  plays the steps headless on a copy of the file, which never changes, and draws the state they lead to: the screen with the open overlays over it where the player puts them, each over its dimming colour. Every step runs, also after one that set nothing off. The picture is the screen's own box: no device, nothing scrolled, no animation in progress → {format: png, width, height, scale (the one used), start, state (as proto.simulate), steps: [{index, action, fired, effects, problem?, state: {screen, screenName, overlays}}], stepsTaken, simulated: true, notModelled, knownDifferences (as proto.simulate), data (base64) | path + bytes}",
        false,
        snapshot
    )]
}

/// A value in an error message, cut short.
fn shown(v: &Value) -> String {
    figcraft_format::clip(&v.to_string(), 60)
}

fn present<'a>(o: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    o.get(key).filter(|v| !v.is_null())
}

/// The gesture scenarios call `name` (`click`, `mouseEnter`…), as the file format reads it.
fn gesture_named(name: &str) -> Option<Gesture> {
    serde_json::from_value(Value::String(name.to_string())).ok()
}

/// Text of at most [`MAX_TEXT`] characters; a number or a boolean reads as its text.
fn text(v: &Value, at: &str) -> Result<Value> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        _ => return Err(bad(CMD, format!("`{at}` must be text (got {})", shown(v)))),
    };
    if s.chars().count() > MAX_TEXT {
        return Err(bad(CMD, format!("`{at}`: longer than {MAX_TEXT} characters")));
    }
    Ok(Value::String(s))
}

/// What the viewer does, from the keys of `o` that say it (`type` and what that type takes).
/// Layer ids are numbers or text as in other commands, and the file format's own reading of a
/// step decides the rest, so a step type or gesture it knows is one this takes.
fn typed(o: &Map<String, Value>, at: &str) -> Result<StepAction> {
    let mut action = Map::new();
    for key in ACTION_KEYS {
        let Some(v) = present(o, key) else { continue };
        let at = format!("{at}.{key}");
        let v = match key {
            "node" => json!(id_of(v).ok_or_else(|| bad(CMD, format!("`{at}` must be a layer id (got {})", shown(v))))?),
            "key" | "variable" => text(v, &at)?,
            "value" if v.to_string().chars().count() > MAX_TEXT => return Err(bad(CMD, format!("`{at}`: longer than {MAX_TEXT} characters"))),
            _ => v.clone(),
        };
        action.insert(key.to_string(), v);
    }
    serde_json::from_value(Value::Object(action)).map_err(|e| bad(CMD, format!("`{at}`: {e} (the forms are those of proto.simulate's steps)")))
}

/// What the viewer does in step `v`, in every form `proto.simulate` takes one: with a `type`,
/// as the shorthand `{click: <layer>}` (any gesture's name) or `{key: "Enter"}`, or as a stored
/// scenario writes it, `{action: {type…}}`.
fn action(v: &Value, at: &str) -> Result<StepAction> {
    let o = v.as_object().ok_or_else(|| bad(CMD, format!("`{at}` must be an object (got {})", shown(v))))?;
    let shorthand: Vec<(&String, Gesture)> = o.keys().filter_map(|k| gesture_named(k).map(|g| (k, g))).collect();
    if let Some(k) = o.keys().find(|k| !STEP_KEYS.contains(&k.as_str()) && gesture_named(k).is_none()) {
        return Err(bad(
            CMD,
            format!("`{at}`: unknown key `{}` (it takes {} or a gesture's name)", figcraft_format::clip(k, 40), STEP_KEYS.join(", ")),
        ));
    }
    if let Some(e) = present(o, "expect").filter(|e| !e.is_object()) {
        return Err(bad(CMD, format!("`{at}.expect` must be an object (got {})", shown(e))));
    }
    match (present(o, "type"), present(o, "action"), shorthand.as_slice()) {
        (Some(_), None, []) => typed(o, at),
        (None, Some(inner), []) => {
            let at = format!("{at}.action");
            let inner = inner.as_object().ok_or_else(|| bad(CMD, format!("`{at}` must be an object (got {})", shown(inner))))?;
            if let Some(k) = inner.keys().find(|k| !ACTION_KEYS.contains(&k.as_str())) {
                return Err(bad(CMD, format!("`{at}`: unknown key `{}` (it takes {})", figcraft_format::clip(k, 40), ACTION_KEYS.join(", "))));
            }
            typed(inner, &at)
        }
        (None, None, [(name, gesture)]) => {
            let layer = o.get(*name).unwrap_or(&Value::Null);
            let node: NodeId = id_of(layer).ok_or_else(|| bad(CMD, format!("`{at}.{name}` must be a layer id (got {})", shown(layer))))?;
            Ok(StepAction::Gesture { node, gesture: *gesture })
        }
        (None, None, []) if present(o, "key").is_some() => {
            let mut keyed = o.clone();
            keyed.insert("type".into(), json!("key"));
            typed(&keyed, at)
        }
        (None, None, []) => {
            Err(bad(CMD, format!("`{at}` needs a `type` (gesture|key|wait|clickOutside|setVariable) or a shorthand such as {{click: <layer id>}}")))
        }
        _ => Err(bad(CMD, format!("`{at}` says what to do more than once: give one of `type`, `action` or a gesture shorthand"))),
    }
}

/// The steps in `steps`, at most [`MAX_ENTRIES`].
fn actions(v: &Value) -> Result<Vec<StepAction>> {
    let list = v.as_array().ok_or_else(|| bad_param(CMD, "steps", format!("`steps` must be a list (got {})", shown(v))))?;
    if list.len() > MAX_ENTRIES {
        return Err(bad_param(CMD, "steps", format!("`steps`: {} entries, at most {MAX_ENTRIES}", list.len())));
    }
    list.iter().enumerate().map(|(i, s)| action(s, &format!("steps[{i}]"))).collect()
}

/// What shows in `sim`: the screen and the open overlays, as each step of `proto.simulate` says it.
fn showing(sim: &Sim) -> Value {
    let state = serde_json::to_value(sim.state()).unwrap_or(Value::Null);
    json!({"screen": state.get("screen"), "screenName": state.get("screenName"), "overlays": state.get("overlays")})
}

fn snapshot(s: &mut Session, p: &Value) -> Result<Value> {
    let steps = get(p, "steps").map(actions).transpose()?.unwrap_or_default();
    let asked = match get(p, "scale") {
        None => 1.0,
        Some(v) => match f64_of(v).filter(|x| *x > 0.0) {
            Some(x) => x.clamp(*SCALES.start(), *SCALES.end()),
            None => return Err(bad_param(CMD, "scale", format!("`scale` must be a number above 0 (got {})", shown(v)))),
        },
    };
    let path = match get(p, "path") {
        None => None,
        Some(Value::String(path)) if !path.trim().is_empty() => Some(path.clone()),
        Some(v) => return Err(bad_param(CMD, "path", format!("`path` must be a file path (got {})", shown(v)))),
    };
    let ds = s.ds()?;
    let start = match opt_id(CMD, p, "start")? {
        Some(id) => id,
        None => crate::cmd::proto::default_start(ds).ok_or_else(|| bad(CMD, "no frame to start at: the page has no top-level frame"))?,
    };
    let mut sim = Sim::new(&ds.doc, start).map_err(|why| bad_param(CMD, "start", format!("`start`: {why}")))?;
    let mut reports = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let outcome = sim.step(step);
        let mut report = json!({"index": index, "action": step, "fired": outcome.fired, "effects": outcome.effects, "state": showing(&sim)});
        if let (Some(o), Some(problem)) = (report.as_object_mut(), outcome.problem) {
            o.insert("problem".into(), json!(problem));
        }
        reports.push(report);
    }
    // The picture: the screen's box, held to a size a client can take.
    let doc = sim.document();
    let roots: Vec<NodeId> = std::iter::once(sim.screen()).chain(sim.overlays().iter().map(|o| o.id)).collect();
    figcraft_render::check_export_depth(doc, &roots).map_err(|e| EngineError::Other(e.to_string()))?;
    let screen = crate::proto_sim::label(doc, sim.screen());
    let bounds = doc.world_bounds(sim.screen()).ok_or_else(|| EngineError::Other(format!("the screen {screen} has no size to draw")))?;
    let side = bounds.width().max(bounds.height());
    if !(side.is_finite() && side > 0.0) {
        return Err(EngineError::Other(format!("the screen {screen} has no size to draw")));
    }
    let scale = asked.min(MAX_SIDE / side);
    let pix =
        crate::proto_sim::render(&sim, scale).ok_or_else(|| EngineError::Other(format!("the screen {screen} couldn't be drawn at scale {scale}")))?;
    let bytes = figcraft_render::encode_export(&pix, ExportFormat::Png, doc.color_profile).map_err(EngineError::Other)?;
    let mut out = json!({
        "format": "png",
        "width": pix.width(),
        "height": pix.height(),
        "scale": scale,
        "start": start,
        "state": sim.state(),
        "steps": reports,
        "stepsTaken": steps.len(),
        "simulated": true,
        "notModelled": crate::proto_verify::NOT_MODELLED,
        "knownDifferences": crate::proto_verify::KNOWN_DIFFERENCES.iter().map(|(_, says)| *says).collect::<Vec<_>>(),
    });
    let Some(o) = out.as_object_mut() else { return Ok(out) };
    match path {
        Some(path) => {
            crate::file_access::write(&path, &bytes)?;
            o.insert("path".into(), json!(path));
            o.insert("bytes".into(), json!(bytes.len()));
        }
        None => {
            o.insert("data".into(), json!(base64::engine::general_purpose::STANDARD.encode(&bytes)));
        }
    }
    Ok(out)
}
