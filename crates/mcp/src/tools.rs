//! MCP tool definitions (names, descriptions, JSON Schemas) and their implementations on top of a
//! [`Backend`]. Every tool reduces to control-channel methods, so headless and attached sessions
//! behave the same.

use base64::Engine as _;
use figcraft_engine::params::normalize_paint;
use serde_json::{Map, Value, json};

use crate::backend::{Backend, decode_data, error_detail, page_children};

/// The result of `tools/call`: MCP content blocks plus the `isError` flag.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
    /// MCP `structuredContent`: what a program reads beside the blocks — `document` (which
    /// document the call read or changed: `{id, name, revision, front}`) and, for a failed call,
    /// `error` (`{code, message, …}`). The same JSON is the last text block, for the model.
    pub structured: Option<Value>,
}

impl ToolResult {
    pub fn text(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: false, structured: None }
    }
    pub fn json(v: &Value) -> Self {
        Self::text(serde_json::to_string_pretty(v).unwrap_or_default())
    }
    pub fn error(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: true, structured: None }
    }
    /// A failed call: the text people read first, then `{"error": detail}` for the model, and
    /// the same as `structuredContent`.
    pub fn failed(message: &str, detail: Value) -> Self {
        let mut r = Self::error(message);
        r.attach(json!({"error": detail}));
        r
    }
    /// Add `extra` (an object) to `structuredContent` and, compact, as one more text block. The
    /// blocks before it stay as they were: callers that parse the first one still can.
    pub fn attach(&mut self, extra: Value) {
        self.content.push(json!({"type": "text", "text": extra.to_string()}));
        match (self.structured.as_mut().and_then(Value::as_object_mut), extra) {
            (Some(have), Value::Object(more)) => have.extend(more),
            (_, extra) => self.structured = Some(extra),
        }
    }
    /// An image block followed by a JSON text block describing it.
    pub fn image(bytes: &[u8], mime: &str, info: &Value) -> Self {
        Self {
            content: vec![
                json!({"type": "image", "data": base64::engine::general_purpose::STANDARD.encode(bytes), "mimeType": mime}),
                json!({"type": "text", "text": info.to_string()}),
            ],
            is_error: false,
            structured: None,
        }
    }
    pub fn to_value(&self) -> Value {
        let mut v = json!({"content": self.content, "isError": self.is_error});
        if let Some(s) = &self.structured {
            v["structuredContent"] = s.clone();
        }
        v
    }
    /// The first text block (tests and the CLI).
    pub fn first_text(&self) -> &str {
        self.content.iter().find_map(|c| c.get("text").and_then(Value::as_str)).unwrap_or("")
    }
}

// ---------- schemas ----------

fn num(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}
fn int(desc: &str) -> Value {
    json!({"type": "integer", "minimum": 0, "description": desc})
}
fn string(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
fn boolean(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}
fn id_schema(desc: &str) -> Value {
    json!({"description": desc, "anyOf": [{"type": "integer"}, {"type": "string"}]})
}
fn ids_schema(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {"anyOf": [{"type": "integer"}, {"type": "string"}]}})
}
fn fields_schema() -> Value {
    json!({"type": "array", "items": {"type": "string", "enum": figcraft_engine::inspect::NODE_KEYS},
        "description": "Exact, case-sensitive top-level node keys to include at every level, pages too (e.g. name, type, x, fills, characters, pluginData). Omit for all properties; [] keeps only ids and structure. id, children, childCount and the transport's truncated marker are always kept. Selecting visible emits an explicit boolean for every node. Unknown keys are an error."})
}
fn outline_schema() -> Value {
    boolean(
        "Compact tree: id, name, type, characters (first 120 characters, then ... only if longer), mainComponentName and explicit visible booleans, plus structure. Cannot combine true with fields; false permits fields.",
    )
}
fn obj(props: Value, required: &[&str]) -> Value {
    let mut o = json!({"type": "object", "properties": props});
    if !required.is_empty() {
        o["required"] = json!(required);
    }
    o
}

fn tool(name: &str, title: &str, desc: &str, schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": schema,
        "annotations": {"title": title, "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false},
    })
}

/// Tool names, in `tools/list` order.
pub const TOOL_NAMES: &[&str] = &[
    "get_capabilities",
    "use_document",
    "list_commands",
    "run_command",
    "batch",
    "create_node",
    "set_properties",
    "inspect_document",
    "get_node",
    "select",
    "export",
    "screenshot",
    "design_context",
    "check_design",
    "check_prototype",
    "simulate_prototype",
    "run_scenarios",
    "open_file",
    "save_file",
    "undo",
    "redo",
    "pointer",
    "key",
    "type_text",
    "inspect_ui",
    "wait_for_changes",
];

/// The tools that take `document`: they act on the file it names instead of the one in front.
const DOCUMENT_TOOLS: &[&str] = &[
    "run_command",
    "batch",
    "create_node",
    "set_properties",
    "inspect_document",
    "get_node",
    "select",
    "export",
    "screenshot",
    "design_context",
    "check_design",
    "check_prototype",
    "simulate_prototype",
    "run_scenarios",
    "save_file",
    "undo",
    "redo",
];

/// The tools that act on the working document without taking `document`: they drive the canvas,
/// which with an attached app is the tab in front (the app refuses them for a file behind it).
const CANVAS_TOOLS: &[&str] = &["pointer", "key", "type_text"];

/// The tools that change the document and take `ifRevision`.
const CONDITIONAL_TOOLS: &[&str] =
    &["run_command", "batch", "create_node", "set_properties", "check_design", "save_file", "undo", "redo", "pointer", "key", "type_text"];

/// The reserved engine parameter a conditional change carries.
const IF_REVISION: &str = "ifRevision";

/// All tools, in `tools/list` order.
pub fn tool_definitions() -> Vec<Value> {
    let mut tools = tools();
    let document = id_schema(
        "The open file to act on: its `id` (from use_document, get_capabilities, run_command doc.list, or the `document` every result names) or its path. Default: the working document — the one the last open_file, use_document or `document` chose, never just the tab in front. Naming a file here makes it the working document. The tab in front stays in front; with an attached app, a screenshot of the window needs the file in front",
    );
    let if_revision = int(
        "Make the change only if the document is still at this revision (the `revision` every result names): when somebody changed it since, nothing is changed and the error (code `conflict`) lists what was done in between",
    );
    for t in &mut tools {
        let name = t.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(props) = t.pointer_mut("/inputSchema/properties").and_then(Value::as_object_mut) else { continue };
        if DOCUMENT_TOOLS.contains(&name.as_str()) {
            props.insert("document".into(), document.clone());
        }
        if CONDITIONAL_TOOLS.contains(&name.as_str()) {
            props.insert(IF_REVISION.into(), if_revision.clone());
        }
    }
    tools
}

fn tools() -> Vec<Value> {
    let empty = || obj(json!({}), &[]);
    let others = || {
        obj(
            json!({"others": boolean("Only if the person asked you to take back their step: also take a step somebody else made (the user, or another agent). Without it such a step is refused (code `otherAuthor`), naming it and its author, and nothing changes. Never pass it to get past that refusal on your own")}),
            &[],
        )
    };
    vec![
        tool(
            "get_capabilities",
            "Get capabilities",
            "What this server and the app behind it can do, as JSON: `backend` (headless: an engine in this process, no window; attached: the running app), `window` and `player` (whether there is a window, and so a live prototype player), the automation `roots` files may be read from and written to, the `workingDocument` ({id, name, revision}) and the open `documents`, how many `commands` there are, the `features` that are really there (prototype simulator and pictures of its states, flow check, scenarios, design context, the design check rules, conflict detection, structured errors), `limits` and file `formats`. Headless, nobody watches: verify by running (check_prototype, run_scenarios, simulate_prototype) and by looking (screenshot). Attached, a person sees every change as it is made and may be editing the same file: read before you write and pass `ifRevision`. Call it first, and again after an error with code `transport`.",
            empty(),
            true,
        ),
        tool(
            "use_document",
            "Use document",
            "Choose the working document: the open file every later call without `document` reads and changes, whichever tab is in front. `document` is a file's `id`, its path, or \"front\" for the tab in front now. Returns the working document ({id, name, revision, front}) and the open documents. open_file and any call that names `document` choose it too; a new file made with run_command file.new, or a tab switched with doc.switch, becomes the working document only when you say so here.",
            obj(
                json!({"document": id_schema("A file's `id` (from get_capabilities or run_command doc.list), its path, or \"front\"")}),
                &["document"],
            ),
            false,
        ),
        tool(
            "list_commands",
            "List commands",
            "List FigCraft commands: id, label, menu path, shortcut, whether it is undoable, and a parameter description. Every menu item, panel action and shortcut is a command; run one with run_command (or several as one undo step with batch). Most commands act on `ids` when given, else on the selection. Use `filter` to narrow (e.g. \"align\", \"layout\", \"variable\", \"style\", \"component\", \"proto\").",
            obj(json!({"filter": string("Case-insensitive substring matched against id, label, menu path and parameter docs")}), &[]),
            true,
        ),
        tool(
            "run_command",
            "Run command",
            "Execute one FigCraft command by id with JSON params (ids and params from list_commands). Mutating commands are one undo step each; a failing command leaves the document unchanged. Examples: {\"command\":\"align.left\",\"params\":{\"ids\":[3,4]}}, {\"command\":\"layout.add\",\"params\":{\"ids\":[2],\"direction\":\"vertical\"}}, {\"command\":\"style.create\",\"params\":{\"type\":\"paint\",\"name\":\"Brand/Primary\",\"from\":5}}. Returns the command's result (creation commands return {\"id\"}). Programmatic calls never open dialogs.",
            obj(
                json!({"command": string("Command id, e.g. node.create, align.left, component.create, file.export"), "params": {"type": "object", "description": "Command parameters (see list_commands)"}}),
                &["command"],
            ),
            false,
        ),
        tool(
            "batch",
            "Run commands as one step",
            "Run several commands in order as ONE undo step (engine `command.batch`). By default (onError: \"abort\"), if any command fails, everything is rolled back and the error names the failing step; success returns an array of the results in order. With onError: \"continue\", a failed step is rolled back on its own, the later steps still run, the call is not an error, and the array instead holds one {step, command, ok, result} or {step, command, ok, error} object per step; a reference to a failed step fails that step too. A later step can use what an earlier step returned: name a step with \"as\": \"card\", then anywhere in a later step's params {\"$ref\": \"card\"} stands for the `id` in its result ({\"$step\": n} does the same by zero-based position). Add \"field\" for another part of the result: a key or a slash path, e.g. {\"$ref\": \"copy\", \"field\": \"ids/0\"} for the first id edit.duplicate returned (with \"map\": true it also returns `map`, each original id → its copy, so \"field\": \"map/3\" is the copy of layer 3). Any object with a `$ref` or `$step` key is a reference. Example: [{\"command\":\"node.create\",\"as\":\"card\",\"params\":{\"type\":\"frame\"}}, {\"command\":\"node.create\",\"params\":{\"type\":\"text\",\"text\":\"Hi\",\"parent\":{\"$ref\":\"card\"}}}] builds a frame and a text inside it in one call.",
            obj(
                json!({
                    "commands": {"type": "array", "minItems": 1, "description": "Steps to run", "items": obj(json!({"command": string("Command id"), "params": {"type": "object"}, "as": string("Name for this step, unique in the batch; later steps use its result with {\"$ref\": name}")}), &["command"])},
                    "label": string("Undo history label (default \"Batch\")"),
                    "onError": {"type": "string", "enum": ["abort", "continue"], "description": "abort (default) rolls back the whole batch on the first failure; continue rolls back only the failed step, runs the rest and returns one {step, command, ok, result|error} object per step"},
                }),
                &["commands"],
            ),
            false,
        ),
        tool(
            "create_node",
            "Create layer",
            "Create a layer (engine `node.create`) and return its id and inspected properties. Coordinates are page units, y down; x/y are relative to the parent frame (the page when there is none, so then they are page coordinates; groups don't count). Inside an auto-layout frame the position is decided by the layout. The new layer becomes the selection.",
            obj(
                json!({
                    "type": {"type": "string", "enum": ["frame", "rectangle", "ellipse", "polygon", "star", "line", "arrow", "text", "vector", "section", "component", "slice"], "description": "Layer type"},
                    "x": num("Left, relative to the parent frame (default 0). For a vector with `path` or `points`, x/y move their own origin instead: data whose box starts at (30, 0) makes a layer whose left is x + 30. Start the path at 0,0 to place its left edge at x, or set x afterwards with set_properties (which moves the left edge)"),
                    "y": num("Top, relative to the parent frame (default 0). For a vector with `path` or `points`, their own top is added, as for x"),
                    "width": num("Width (default 100; text hugs its content unless width is set, which makes it wrap; for a vector with path, width/height scale the path, one alone keeping its proportion, and without them the path's own box is used)"),
                    "height": num("Height (default 100; ignored for lines/arrows and auto-height text)"),
                    "parent": id_schema("Parent frame/group/component id (default: the current page)"),
                    "index": int("Z-index among the parent's children (default: on top)"),
                    "name": string("Layer name"),
                    "text": string("Characters, for type text"),
                    "path": string("SVG path data, for type vector (e.g. \"M0 0 L100 0 L50 80 Z\"); its coordinates are kept relative to x/y, so a path that doesn't start at 0,0 is offset by its own box"),
                    "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}, "description": "For type vector, instead of path: the points [[x, y], ...]"},
                    "closed": boolean("For type vector: close the path"),
                    "select": boolean("Select the new layer (default true)"),
                    "props": {"type": "object", "description": "Any set_properties keys applied at creation, e.g. {\"fill\":\"#3366FF\",\"cornerRadius\":12,\"fontSize\":24,\"layout\":\"vertical\",\"gap\":8,\"padding\":16}"},
                }),
                &["type"],
            ),
            false,
        ),
        tool(
            "set_properties",
            "Set properties",
            "Set properties on one or more layers as one undo step (engine `node.set`). Keys: name, visible, locked, opacity (0-1), blend, x, y (relative to the parent frame; groups don't count), width, height, rotation (degrees), absoluteRotation (degrees on the page), fill (colour) / fills (array of paints: \"#hex\" or {\"type\":\"solid\",\"color\":\"#hex\"} or {\"type\":\"linearGradient\",\"stops\":[\"#hex\",\"#hex\"]} or stops [{\"position\":0..1,\"color\":\"#hex\"}], optional from/to handles in the unit box; radialGradient too), stroke / strokes, strokeWeight, strokeAlign (inside|center|outside), cornerRadius (n or [tl,tr,br,bl]), effects (array, e.g. {\"type\":\"dropShadow\",\"color\":\"#00000040\",\"offset\":[0,4],\"radius\":8}), clip, layout (null|horizontal|vertical|grid), gap, padding (n or [t,r,b,l]), mainAlign (min|center|max|spaceBetween|spaceAround|spaceEvenly), crossAlign, wrap, contentAlign (auto|spaceBetween: wrapped lines), sizingH / sizingV (fixed|hug|fill), constraints {horizontal, vertical}, characters, fontFamily, fontStyle, fontSize, lineHeight, letterSpacing, textAlign, textAutoResize. Auto layout keys (gap, padding, mainAlign, crossAlign, wrap, …) apply to frames, components and instances only: other layers among the ids are skipped and named in `warnings`, and with none that can take them the call is an error. A fontStyle draws in the family's nearest face; `warnings` says when that face has another weight or slant than the name asks for. Run list_commands with filter \"node.set\" for the full list. Unknown keys are an error. Returns the updated layers.",
            obj(
                json!({"ids": ids_schema("Layer ids (default: the selection)"), "props": {"type": "object", "description": "Property values to set"}}),
                &["props"],
            ),
            false,
        ),
        tool(
            "inspect_document",
            "Inspect document",
            "The active document as JSON: name, path, dirty flag, current page, selection, the current page's layer tree (id, name, type, x/y relative to the parent frame (groups don't count), width, height, absoluteBounds in page coordinates, fills, strokes, effects, cornerRadius, layout, sizing, text properties, component links, overrides, bound variables), other pages summarised, styles, variable count and the undo history. Use it to verify every change. Each level of `depth` multiplies the response, and `allPages` multiplies it again: on a large document the tree is reported at a shallower depth than asked for, with a `truncated` object saying so. Use get_node to read one subtree deeply instead.",
            obj(
                json!({
                    "depth": int("Tree depth for the current page (default 4)"),
                    "fields": fields_schema(),
                    "outline": outline_schema(),
                    "allPages": boolean("Expand every page, not only the current one"),
                    "maxBytes": int(
                        "Largest response to send, in bytes of the whole JSON-RPC message (default 8388608, which fits the usual 10 MiB client buffer); 1024 of it is kept for the envelope and the request id"
                    ),
                }),
                &[],
            ),
            true,
        ),
        tool(
            "get_node",
            "Get layer",
            "One layer as JSON (same fields as inspect_document) with its children to `depth`. A subtree too large for one message is reported at a shallower depth, or with a window of the children, and a `truncated` object says so and gives the `offset` to ask for next.",
            obj(
                json!({
                    "id": id_schema("Layer id"),
                    "depth": int("Child depth (default 1)"),
                    "fields": fields_schema(),
                    "outline": outline_schema(),
                    "offset": int("First child to report (default 0), for reading a node with more children than fit in one message"),
                    "maxBytes": int(
                        "Largest response to send, in bytes of the whole JSON-RPC message (default 8388608, which fits the usual 10 MiB client buffer); 1024 of it is kept for the envelope and the request id"
                    ),
                }),
                &["id"],
            ),
            true,
        ),
        tool(
            "select",
            "Select layers",
            "Replace the selection with `ids` (switching to their page). An empty array deselects everything. Most commands act on the selection when no `ids` are passed.",
            obj(json!({"ids": ids_schema("Layer ids to select")}), &["ids"]),
            false,
        ),
        tool(
            "export",
            "Export",
            "Export layers (default: the selection, else every top-level layer of the current page) as png, jpg, svg, webp, pdf or xml (an Android Vector Drawable, with `warnings` for what it changed or left out, per `vdCompat`) at `scale`, or the whole file as `fig` (experimental: written from public format knowledge, with `warnings` for what was left out; other apps may not open every file) (PDF: with `bleed`, `cropMarks`, `bleedMarks` and `cmyk` for print; options `contentsOnly`, `includeBoundingBox`, and for SVG `svgOutlineText`, `svgIdAttribute`, `svgSimplifyStroke`; for XML `svgIdAttribute` and `vdCompat`; for png, jpg and webp `colorProfile`). With `path` the file is written there (absolute path recommended) and the result says where. Without `path`: png/jpg/webp come back as an image content block, svg as text, xml as text followed by `{format, warnings}`, other formats as base64 `data`.",
            obj(
                json!({
                    "ids": ids_schema("Layers to export (several are composited into one image)"),
                    "format": {"type": "string", "enum": ["png", "jpg", "svg", "webp", "pdf", "xml", "fig"], "description": "Format (default png, or from the path's extension)"},
                    "scale": num("Pixel scale for raster formats (default 1, max 64)"),
                    "size": string("Instead of `scale`: `2x`, or a width or height in pixels like `512w` / `512h`"),
                    "path": string("Output file path"),
                    "bleed": {"type": ["number", "string"], "description": "PDF: bleed around each page, in points or as a length like `3mm` or `0.125in`"},
                    "cropMarks": boolean("PDF: crop marks"),
                    "bleedMarks": boolean("PDF: bleed marks"),
                    "cmyk": boolean("PDF: CMYK colour instead of RGB"),
                    "contentsOnly": boolean("Only the layers themselves (default true); false also draws every other layer that shows in their area"),
                    "includeBoundingBox": boolean("Export the layers' own boxes, cutting off shadows and outside strokes (default false)"),
                    "svgOutlineText": boolean("SVG: text as outlines (default true); false keeps `<text>` elements"),
                    "svgIdAttribute": boolean("SVG: layer names as unique `id` attributes (default false)"),
                    "svgSimplifyStroke": boolean("SVG: strokes as `stroke` attributes where exact (default true); false writes outlines"),
                    "vdCompat": {"type": "string", "enum": ["bestEffort", "exact", "ignore"], "description": "XML: what a Vector Drawable can't draw exactly is approximated with a warning (bestEffort, default), left out with a warning (exact), or written as far as it maps without warnings (ignore: overlapping layer opacity dropped, even-odd fills kept as fillType=evenOdd, diamond gradients left out)"},
                    "colorProfile": {"type": "string", "enum": ["sameAsFile", "srgb", "displayP3"], "description": "PNG, JPG and WebP: the color profile of the file (default sameAsFile: the document's); colors are converted and out-of-gamut ones clip. SVG, XML and RGB PDF output are always sRGB; PDF with cmyk is DeviceCMYK"},
                    "quality": {"type": "integer", "minimum": 1, "maximum": 100, "description": "JPG: the encoder's quality, 1 (smallest file) to 100; default 90"},
                }),
                &[],
            ),
            true,
        ),
        tool(
            "screenshot",
            "Screenshot",
            "Look at the design: renders the given layers (or the whole current page, with its background) to PNG and returns it as image content, plus its size and the scale used. The image is scaled down to fit `maxSize` pixels. With `window: true` (or a window id from `inspect_ui`'s `windows`) and an attached app, captures that app window including the UI instead.",
            obj(
                json!({
                    "ids": ids_schema("Layers to render (default: the whole current page)"),
                    "scale": num("Requested scale (default 1; reduced to fit maxSize)"),
                    "maxSize": int("Largest side in pixels (default 1600)"),
                    "window": {"description": "Capture the app window: true for the one in front, or a window id (attached app only; headless renders the page)", "anyOf": [{"type": "boolean"}, {"type": "integer"}]},
                }),
                &[],
            ),
            true,
        ),
        tool(
            "design_context",
            "Design context",
            "Why the working document is the way it is, as JSON (engine `design.get`). `brief`: who it is for, who asks for it, what they need to get done, where, within which limits and how success would be noticed; every field is optional and none is ever final. `stage`: exploring | converging | ready, which decides the check rules that run. `assumptions`: what the design takes to be true without knowing; each stays `untested` until a person records evidence. `decisions`: a choice and its reasons; `proposed` when an agent recorded it, `accepted` only by a person. `scenarios`: stored walks through the prototype with what must hold along the way, each with its author (run them with run_scenarios). `endStates`: frames where a flow may end on purpose; one an agent declared is `proposed` and counts only once a person accepts it. `findings`: problems on record; kind `check` was measured by a rule and is settled by running the rule again, `judgement` is somebody's opinion and `feedback` is what a person said, and those two are settled only by a person; `source` says whose it is and `recordedBy` who wrote it down when that is somebody else. `alternatives`: sets of variants kept beside their source, their constraints stated first; a person chooses, and an agent's choice is a proposal. `missingLinks`: records that point at layers which no longer exist. Read it before designing and before reporting. Change it with run_command: design.brief.set, design.stage.set, design.assumption.add, design.decision.add, design.finding.add, design.finding.address, design.finding.recheck, design.alternative.create, design.alternative.update, design.alternative.compare, proto.scenario.add, proto.scenario.set, proto.endState.set (list_commands {filter: \"design.\"} and {filter: \"proto.\"} give the parameters). What an agent records is a proposal: the engine refuses an agent that accepts its own decision, end state or choice, marks an assumption supported or refuted, accepts, verifies or removes what a person must settle, or changes what a person wrote or settled (a person's scenario, an assumption a person gave evidence for, a finding a person accepted or verified).",
            empty(),
            true,
        ),
        tool(
            "check_design",
            "Check design",
            "Measure the design with deterministic rules (engine `design.check`): the contrast of text against what is behind it, text and content that is cut off, touch targets that are too small, prototype actions whose target is gone, screens with no way out, scenarios that fail, and design-context records that point at nothing or have gone stale. The same file always gives the same findings; each rule states its threshold and the standard that comes from (`rules`, each with `about`). Which rules run depends on the document's stage (exploring: only what breaks the file; converging: also the finish and the flow; ready: also the records themselves) unless `rules` names them, and `skipped` lists every rule that did not run with the reason: report those as not checked. Returns {findings: [{rule, severity, nodes, subject?, message, evidence, suggestion}], summary, rules, skipped, stage}; `subject` says what a finding is about when that is not its layers (a scenario, an action, a page). With `record: true` the findings are also stored in the document as open findings (engine `design.check.record`, one undo step) and the result gains `recorded` {added, existing, total, truncated: how many were left out because the file already holds 500 findings} and `stored` (those findings as stored, each with its `id`). To settle one: fix the cause, claim it with run_command design.finding.address {id, resolution}, then design.finding.recheck {id} runs its rule again, and only that marks it verified. This is not a quality score: it finds what these rules can measure and says nothing about whether the design serves its purpose, so a result without findings does not mean the design is good.",
            obj(
                json!({
                    "scope": ids_schema("Layers to check, each with everything inside it (default: the current page's top-level frames)"),
                    "rules": {"type": "array", "items": {"type": "string"}, "description": "Run exactly these rule ids, whatever the stage (the ids get_capabilities lists under `checks`; run_command design.check.rules says what each measures)"},
                    "record": boolean("Also store the findings in the document (default false: nothing is stored)"),
                }),
                &[],
            ),
            false,
        ),
        tool(
            "check_prototype",
            "Check prototype flow",
            "Facts about a page's prototype as a graph, without playing or changing anything (engine `proto.check`). `prototyped`: whether the page is a prototype at all (it has a flow start, or a screen that leads somewhere). `starts`: the flow starts and the frames that interactions on other pages navigate to (`source`: flow | otherPage), or the first frame when there is neither (firstFrame). `screens`: each top-level frame with `reachable`, `endState`, `deadEnd`, whether it is a `component`, and its outgoing interactions `out` (from which layer, on which trigger, doing what, to where). `problems` ([{rule, severity, nodes, subject?, message, evidence, suggestion}]): an action whose target is missing, a screen with no way out that is not an accepted end, an overlay that cannot be closed, frames no start reaches, stored scenarios that fail and screens no scenario visits. Every interaction counts as a possible step, so `reachable` means some interaction leads there, not that a person will find it. `ok` says only that these rules found no blocker and no major problem. Run it after wiring or changing interactions and after deleting frames, then run_scenarios. A screen a flow is meant to end on is declared with run_command proto.endState.set {id, reason}. From an agent that is a proposal and needs its reason: the screen stays a dead end (`endStateProposedBy` names the agent) until a person accepts it.",
            obj(json!({"page": id_schema("The page to check: its id (default: the current page)")}), &[]),
            true,
        ),
        tool(
            "simulate_prototype",
            "Simulate prototype",
            &with_known_differences(
                "Play steps on the prototype without a window and report the state they lead to (engine `proto.simulate`). It works headless and attached, on a copy: the document never changes. `steps` are what a viewer does, in order: {\"click\": <layer id>} (also hover, press, drag, mouseEnter, mouseLeave, mouseDown, mouseUp), {\"key\": \"Enter\"}, {\"type\": \"wait\"} (the pending after-delay interactions fire), {\"type\": \"clickOutside\"} (outside the top overlay) and {\"type\": \"setVariable\", \"variable\": <name>, \"value\": <value>} (stands in for input a prototype cannot take, such as typing); each may carry `expect` and a `note`. `expect`, on a step or at the top for the end, is {screen?, overlays? [ids, bottom first], variables? {name: value}, variants? [{instance, variant}], visible? [ids], hidden? [ids], text? [{node, equals | contains}], endState?}. Returns {passed, simulated: true, notModelled, knownDifferences, start, state: {screen, screenName, overlays, history, variables, modes, variants, endState}, steps: [{index, action, fired, effects, problem?, failures, state}], failures, failedStep?, stepsTaken}. A step fails when its layer is not on what shows (the top overlay if one is open, else the screen), when a gesture or key sets nothing off, when an action cannot run, or when its `expect` does not hold; the run stops there unless `stopOnFailure` is false. With `image: true` a PNG of the final state follows the JSON as image content (the screen with its open overlays; engine `proto.snapshot`), and the JSON gains `image` {width, height, scale}. What the simulator does not model, as every result lists under `notModelled`: {NOT_MODELLED}. Where the simulator and the app's player still differ, as every result lists under `knownDifferences`: {KNOWN} So `passed` means that these state rules produced this state. It does not mean a person used it, could find the control, or saw it on a device. Keep a walk worth keeping with run_command proto.scenario.add {name, start, steps, expect, purpose}.",
            ),
            obj(
                json!({
                    "start": id_schema("The top-level frame to start on (default: the selected frame, else the first flow, else the page's first frame)"),
                    "steps": {"type": "array", "description": "What the viewer does, in order (at most 500 steps); [] reports the start screen as it opens", "items": {"type": "object", "description": "One step: {\"click\": <layer id>} (or hover, press, drag, mouseEnter, mouseLeave, mouseDown, mouseUp), {\"key\": \"Enter\"}, {\"type\": \"wait\"}, {\"type\": \"clickOutside\"} or {\"type\": \"setVariable\", \"variable\": <name>, \"value\": <value>}; optionally with `expect` (what must hold right after it) and `note`"}},
                    "expect": {"type": "object", "description": "What must hold after the last step: {screen?, overlays?, variables?, variants?, visible?, hidden?, text?, endState?}"},
                    "stopOnFailure": boolean("Stop at the first step that fails (default true)"),
                    "image": boolean("Also return a PNG of the final state as image content (default false)"),
                    "scale": num("Pixels per page unit for the image, 0.1 to 4 (default 1; lowered so that the larger side is at most 4096 pixels)"),
                }),
                &["steps"],
            ),
            true,
        ),
        tool(
            "run_scenarios",
            "Run scenarios",
            &with_known_differences(
                "Run the scenarios stored in the document (engine `proto.scenario.run`): each is a named walk through the prototype with what must hold along the way, played on a copy by the same simulator as simulate_prototype and stopping at its first failing step. `id` runs one (\"S1\"); without it all of them run. Returns {passed, simulated: true, notModelled, knownDifferences, total, failed, results: [{id, name, passed, failedStep?, failures, steps, stepsTaken}]}. Report a failed scenario as failed, and when `total` is 0 say that nothing was run: an empty run is not a pass. Never rewrite a scenario so that it passes; fix the prototype, or leave it failing. Scenarios are stored with run_command proto.scenario.add {name, start, steps, expect, purpose}, changed with proto.scenario.set and listed with proto.scenario.list (or design_context); an agent changes and removes only the scenarios it wrote. The simulator's limits apply, and every result lists them (`notModelled`): no animation or timing, no scroll position, no drag direction, no device frame or responsive resizing, no pointer hit-testing, one trigger per gesture step, none of the player's own keys, no widgets. Where it and the app's player still differ (`knownDifferences`): {KNOWN}",
            ),
            obj(json!({"id": string("One scenario's id, e.g. \"S1\" (default: every stored scenario)")}), &[]),
            true,
        ),
        tool(
            "open_file",
            "Open file",
            "Open a .figcraft or .json document (FigCraft or REST API file JSON), a .fig or .sketch file (best effort, with `warnings` for what wasn't imported; .sketch from Sketch 48 or newer), an .svg (becomes vectors) or an image (becomes an image layer) as the active document. Returns the new document's summary.",
            obj(
                json!({
                    "path": string("File path"),
                    "variables": {"description": "REST API file JSON only: the local variables JSON (`GET /v1/files/:key/variables/local`) as a path or the JSON object. Default: `<name>.variables.json` or `variables.json` next to the file."},
                    "reload": boolean("A file that is already open: read it again from disk into its tab (refused while the tab has unsaved changes). Without it, the result says `stale: true` when the file changed on disk since the tab read or saved it"),
                }),
                &["path"],
            ),
            false,
        ),
        tool(
            "save_file",
            "Save file",
            "Save the active document as a .figcraft file. `path` is required the first time (an untitled document has no path); later saves reuse it. A `path` ending in .fig writes an experimental .fig export copy instead and leaves the document's own path and unsaved changes alone.",
            obj(json!({"path": string("Output .figcraft path, or a .fig path for an experimental export copy")}), &[]),
            false,
        ),
        tool(
            "undo",
            "Undo",
            "Undo the last change (one command, batch or gesture). Returns {undone: bool}. Your undo stays in your own work: when the last step was made by the user or another agent, or a change of theirs with no undo step came after yours, it is refused (code `otherAuthor`), naming the step and its author; inspect_document's `history.steps` shows who made each step. Leave it, or ask them; pass `others: true` only if the person asked you to take back their step.",
            others(),
            false,
        ),
        tool(
            "redo",
            "Redo",
            "Redo the last undone change. Returns {redone: bool}. Like undo, it takes only your own steps; pass `others: true` only if the person asked you to redo their step.",
            others(),
            false,
        ),
        tool(
            "pointer",
            "Pointer gesture",
            "Drive the canvas with the mouse in page coordinates, exactly as a user would with the current tool: click to select, drag to move/resize/marquee, or draw with a shape tool (set `tool` first, e.g. {\"tool\":\"rectangle\",\"kind\":\"drag\",\"x\":0,\"y\":0,\"to\":[200,100]}). `double` on a text layer starts editing it. Returns the selection and tool. These gestures act on the editor canvas, never on a prototype that is playing: a playing prototype is driven by the person watching it (or by the control channel's `ui.input`), and while one plays in the attached app these gestures are refused, because the presenter covers the canvas. To check what a prototype does, use simulate_prototype or run_scenarios, which work in both backends.",
            obj(
                json!({
                    "kind": {"type": "string", "enum": ["click", "double", "drag", "down", "move", "up", "hover"], "description": "Gesture (default click)"},
                    "x": num("Page x"),
                    "y": num("Page y"),
                    "to": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "Drag end [x, y] (page)"},
                    "steps": int("Intermediate move events for drag (default 8)"),
                    "tool": {"type": "string", "description": "Tool to choose first: move|scale|hand|frame|section|slice|rectangle|line|arrow|ellipse|polygon|star|pen|pencil|text|comment|annotation"},
                    "shift": boolean("Shift held"),
                    "alt": boolean("Alt/Option held"),
                    "cmd": boolean("Cmd (macOS) / Ctrl held"),
                    "ctrl": boolean("Ctrl held (the Control key on macOS)"),
                    "pressure": num("Pen pressure 0-1 (the Pencil records it as the stroke's width profile; a drag starts with it)"),
                    "pressureTo": num("Pen pressure 0-1 at a drag's end (interpolated along it)"),
                }),
                &["x", "y"],
            ),
            false,
        ),
        tool(
            "key",
            "Press key",
            "Press a key or shortcut as if typed on the canvas: a menu shortcut (e.g. {\"key\":\"G\",\"cmd\":true} groups, {\"key\":\"A\",\"alt\":true,\"shift\":false} …), a tool key (R, O, T, F, V…), Escape, Delete, Enter, arrows (nudge; shift = big nudge). While editing text, keys go to the text editor (Backspace, Delete, Left, Right, Home, End, Enter, Escape, SelectAll). Keys go to the editor canvas, never to a prototype that is playing (a person drives that, or the control channel's `ui.input`; while one plays in the attached app, keys sent here are refused): a prototype's key interactions are checked with simulate_prototype (a step {\"key\": \"Enter\"}) or run_scenarios.",
            obj(
                json!({"key": string("Key name: a letter/digit, Escape, Delete, Backspace, Enter, ArrowLeft…"), "shift": boolean("Shift"), "alt": boolean("Alt/Option"), "cmd": boolean("Cmd/Ctrl"), "ctrl": boolean("Ctrl (the Control key on macOS)")}),
                &["key"],
            ),
            false,
        ),
        tool(
            "type_text",
            "Type text",
            "Type into a text layer. With `id`, starts editing that layer with all of its text selected (so the typed text replaces it), types, then stops editing. Without `id`, types at the caret of the text currently being edited (after a double-click with pointer, or after drawing with the text tool).",
            obj(json!({"text": string("Text to type (\\n for a new line)"), "id": id_schema("Text layer to replace the contents of")}), &["text"]),
            false,
        ),
        tool(
            "inspect_ui",
            "Inspect UI",
            "UI state: current tool, view (zoom, centre), status, panels, timings and the app's windows (`windows`: id, title, files) when attached to the app; headless reports the session status.",
            empty(),
            true,
        ),
        tool(
            "wait_for_changes",
            "Wait for changes",
            "Wait for new comment events in the attached FigCraft app. Returns pending events immediately; otherwise waits up to `timeoutMs` (default 20000, max 25000). Call it again with the returned `cursor` to keep watching. Headless returns immediately (no other writer). Events carry a sequence number, kind, document and a small payload (for comments, the thread id).",
            obj(
                json!({
                    "after": int("Return events after this cursor (default 0)"),
                    "timeoutMs": int("Longest wait in milliseconds (default 20000, max 25000)"),
                    "topics": {"type": "array", "items": {"type": "string", "enum": ["comments", "document", "all"]}, "description": "Event topics (default [\"comments\"]; document events are not emitted yet)"},
                    "limit": int("Most events to return (default 100, max 1000)")
                }),
                &[],
            ),
            true,
        ),
    ]
}

/// The simulator's known differences from the app's player, as one sentence list for a tool
/// description (the same text every simulated result lists under `knownDifferences`).
pub fn known_differences_text() -> String {
    let all: Vec<&str> = figcraft_engine::proto_verify::KNOWN_DIFFERENCES.iter().map(|(_, says)| *says).collect();
    format!("{}.", all.join("; "))
}

/// `desc` with `{KNOWN}` replaced by [`known_differences_text`], and `{NOT_MODELLED}` by what
/// the simulator leaves out, in the words its results use.
fn with_known_differences(desc: &str) -> String {
    desc.replace("{KNOWN}", &known_differences_text()).replace("{NOT_MODELLED}", &figcraft_engine::proto_verify::NOT_MODELLED.join("; "))
}

// ---------- helpers ----------

/// A backend whose every call names `document` (a tool's `document` argument).
struct InDocument<'a> {
    inner: &'a mut dyn Backend,
    document: Value,
}

impl Backend for InDocument<'_> {
    fn call(&mut self, method: &str, mut params: Value) -> Result<Value, String> {
        if let Some(o) = params.as_object_mut() {
            o.insert("document".into(), self.document.clone());
        }
        self.inner.call(method, params)
    }

    fn has_ui(&self) -> bool {
        self.inner.has_ui()
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }

    fn take_error_detail(&mut self) -> Option<Value> {
        self.inner.take_error_detail()
    }

    fn take_revision(&mut self) -> Option<(u64, u64)> {
        self.inner.take_revision()
    }

    fn app_info(&mut self) -> Option<Value> {
        self.inner.app_info()
    }
}

/// What a tool's run says about the documents besides its result.
#[derive(Default)]
struct Ctx {
    /// The file the call leaves as the working document, when it is another one than its target
    /// (a file it opened, made or reverted).
    moved: Option<u64>,
    /// The revision the call's change left its document at, read in the same request as the
    /// change: `(document, revision)` ([`Backend::take_revision`]).
    changed: Option<(u64, u64)>,
    /// Another open file the call acted on than the working document (`file.close {index}`,
    /// `doc.switch {index}`): the result names it, and the working document stays.
    acted_on: Option<u64>,
}

impl Ctx {
    /// The revision the backend read right after the call just made (the call's change).
    fn note(&mut self, b: &mut dyn Backend) {
        if let Some(r) = b.take_revision() {
            self.changed = Some(r);
        }
    }
}

/// Why a tool call failed: the text, and what kind of failure it is when the tool itself knows
/// (`None`: the backend's detail for this text, else a check of the arguments, code `badParams`).
struct Failure {
    message: String,
    detail: Option<Value>,
    /// The code when neither the tool nor the backend says what kind of failure this is.
    fallback: &'static str,
}

impl Failure {
    fn coded(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self { detail: Some(error_detail(code, &message)), message, fallback: "failed" }
    }

    /// What a backend call said, when it isn't about the arguments (a render, an export): the
    /// backend's own detail when it gave one, else just "it failed".
    fn from_backend(message: String) -> Self {
        Self { message, detail: None, fallback: "failed" }
    }

    /// A complaint about argument `param`.
    fn param(param: &str, message: impl Into<String>) -> Self {
        let mut f = Self::coded("badParams", message);
        if let Some(d) = f.detail.as_mut() {
            d["param"] = json!(param);
        }
        f
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { message, detail: None, fallback: "badParams" }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}

type Outcome = Result<ToolResult, Failure>;

fn exec(b: &mut dyn Backend, cmd: &str, params: Value) -> Result<Value, String> {
    b.call("engine.execute", json!({"command": cmd, "params": params}))
}

/// `params` with the call's `ifRevision`, when it has one: the engine refuses the command unless
/// the document is still at that revision.
fn conditional(a: &Map<String, Value>, mut params: Value) -> Value {
    if let (Some(rev), Some(o)) = (a.get(IF_REVISION).filter(|v| !v.is_null()), params.as_object_mut()) {
        o.insert(IF_REVISION.into(), rev.clone());
    }
    params
}

fn wait_for_changes(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    Ok(b.call("events.poll", Value::Object(a.clone())).map(|v| ToolResult::json(&v))?)
}

fn args_obj(args: &Value) -> Result<Map<String, Value>, String> {
    match args {
        Value::Object(o) => Ok(o.clone()),
        Value::Null => Ok(Map::new()),
        _ => Err("`arguments` must be an object".into()),
    }
}

fn s_arg<'a>(a: &'a Map<String, Value>, k: &str) -> Result<Option<&'a str>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("`{k}` must be a string")),
    }
}

fn req_s<'a>(a: &'a Map<String, Value>, k: &str) -> Result<&'a str, String> {
    s_arg(a, k)?.ok_or_else(|| format!("`{k}` is required"))
}

fn n_arg(a: &Map<String, Value>, k: &str) -> Result<Option<f64>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => figcraft_engine::params::f64_of(v).map(Some).ok_or_else(|| format!("`{k}` must be a finite number")),
    }
}

fn id_arg(v: &Value) -> Result<u64, String> {
    figcraft_engine::params::id_of(v).map(|i| i.0).ok_or_else(|| format!("{v} is not a layer id"))
}

fn ids_arg(a: &Map<String, Value>, k: &str) -> Result<Option<Vec<u64>>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => arr.iter().map(id_arg).collect::<Result<Vec<_>, _>>().map(Some),
        Some(v) => id_arg(v).map(|i| Some(vec![i])),
    }
}

fn png_info(bytes: &[u8]) -> (u32, u32) {
    crate::png_size(bytes).unwrap_or((0, 0))
}

/// Union of the layers' absolute bounds (page coordinates).
fn bounds_of(b: &mut dyn Backend, ids: &[u64]) -> Option<(f64, f64, f64, f64)> {
    let mut acc: Option<(f64, f64, f64, f64)> = None;
    for id in ids.iter().take(500) {
        let Ok(n) = b.call("node.get", json!({"id": id, "depth": 0})) else { continue };
        let Some(ab) = n.get("absoluteBounds") else { continue };
        let g = |k: &str| ab.get(k).and_then(Value::as_f64);
        let (Some(x), Some(y), Some(w), Some(h)) = (g("x"), g("y"), g("width"), g("height")) else { continue };
        acc = Some(match acc {
            None => (x, y, x + w, y + h),
            Some((a, b2, c, d)) => (a.min(x), b2.min(y), c.max(x + w), d.max(y + h)),
        });
    }
    acc
}

/// Make paint/effect JSON friendlier than the engine's strict serde (see
/// [`figcraft_engine::params::normalize_paint`]). The engine reads paints this way itself; effects
/// still need it here.
pub fn normalize_props(props: &mut Map<String, Value>) {
    for k in ["fills", "strokes", "effects"] {
        if let Some(v) = props.get_mut(k) {
            normalize_paint(v);
        }
    }
}

/// [`normalize_props`] for command params: `props`, flat node.set keys, `paint`, `effect`.
fn friendly_params(p: &mut Value) {
    let Some(o) = p.as_object_mut() else { return };
    if let Some(Value::Object(props)) = o.get_mut("props") {
        normalize_props(props);
    }
    normalize_props(o);
    for k in ["paint", "effect"] {
        if let Some(v) = o.get_mut(k) {
            normalize_paint(v);
        }
    }
}

// ---------- dispatch ----------

/// Refuse argument names `schema` doesn't list, at the top and in the objects it describes inside
/// (a batch's steps); objects it leaves open (`params`) take any. `path` leads the names in the
/// error (`commands[0].`).
fn refuse_unknown_args(schema: &Value, args: &Value, path: &str) -> Result<(), String> {
    if let (Some(props), Some(given)) = (schema.get("properties").and_then(Value::as_object), args.as_object()) {
        let unknown: Vec<String> = given.keys().filter(|k| !props.contains_key(*k)).map(|k| format!("`{path}{k}`")).collect();
        if !unknown.is_empty() {
            let s = if unknown.len() == 1 { "" } else { "s" };
            let takes: Vec<&str> = props.keys().map(String::as_str).collect();
            let takes = if takes.is_empty() { "none".to_string() } else { takes.join(", ") };
            return Err(format!("unknown argument{s} {} (it takes {takes})", unknown.join(", ")));
        }
        for (k, v) in given {
            if let Some(inner) = props.get(k) {
                refuse_unknown_args(inner, v, &format!("{path}{k}."))?;
            }
        }
    }
    if let (Some(items), Some(list)) = (schema.get("items"), args.as_array()) {
        for (i, v) in list.iter().enumerate() {
            refuse_unknown_args(items, v, &format!("{}[{i}].", path.trim_end_matches('.')))?;
        }
    }
    Ok(())
}

/// What a server remembers between tool calls: the working document, the open file that calls
/// without `document` read and change. It is chosen by `open_file`, `use_document` and any call
/// that names `document`, and otherwise taken once from the tab in front; after that the tab in
/// front can change (the user clicks another one) without a call landing in another file.
#[derive(Clone, Debug, Default)]
pub struct ToolState {
    working: Option<Working>,
}

#[derive(Clone, Debug, PartialEq)]
struct Working {
    id: u64,
    name: String,
}

impl ToolState {
    /// The working document as a control call's `document` (its id), once there is one.
    pub fn document(&self) -> Option<Value> {
        self.working.as_ref().map(|w| json!(w.id))
    }

    /// The working document, settled the first time it is needed (the tab in front then, or
    /// when the working document was closed), as get_capabilities settles it: a resource read
    /// before any tool call reads the file later calls will act on (review R2-11).
    pub fn settle(&mut self, b: &mut dyn Backend) -> Option<Value> {
        let docs = open_documents(b).ok()?;
        if self.working.as_ref().is_none_or(|w| !docs.iter().any(|d| d.id == w.id))
            && let Some(front) = docs.iter().find(|d| d.front)
        {
            self.working = Some(Working { id: front.id, name: front.name.clone() });
        }
        self.document()
    }
}

/// One open document, as `doc.list` gives it.
#[derive(Clone, Debug)]
struct Open {
    id: u64,
    /// Its tab's place among the open documents (`doc.list`'s `index`).
    index: Option<u64>,
    name: String,
    /// `None` from an app that doesn't count revisions yet.
    revision: Option<u64>,
    front: bool,
}

/// A document's name as results carry it: one line, short enough that naming the document never
/// pushes a response that fitted its size limit over it.
fn short_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(48).collect()
}

impl Open {
    /// `{id, name, revision, front}`: what every result says about the document it is about.
    fn identity(&self) -> Value {
        self.identity_at(self.revision)
    }

    /// [`Self::identity`] at `revision` (the one the result belongs to).
    fn identity_at(&self, revision: Option<u64>) -> Value {
        let mut v = json!({"id": self.id, "name": short_name(&self.name), "front": self.front});
        if let Some(r) = revision {
            v["revision"] = json!(r);
        }
        v
    }
}

/// The revision a result about document `d` belongs to: the one its change left it at, read in
/// the same request (`changed`), else the one it had before the call's first request (`before`):
/// never one read afterwards, which could take in somebody else's change made in between and so
/// let a later `ifRevision` through over it. `d.revision` (read after) only for a document the
/// call itself made.
fn revision_for(d: &Open, changed: Option<(u64, u64)>, before: Option<&[Open]>) -> Option<u64> {
    if let Some((_, rev)) = changed.filter(|(id, _)| *id == d.id) {
        return Some(rev);
    }
    match before.and_then(|b| b.iter().find(|x| x.id == d.id)) {
        Some(was) => was.revision,
        None => d.revision,
    }
}

/// The open documents. Asked without `document`, so `front` is the tab the user sees in front.
fn open_documents(b: &mut dyn Backend) -> Result<Vec<Open>, String> {
    let list = exec(b, "doc.list", json!({}))?;
    Ok(list
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    Some(Open {
                        id: d.get("id")?.as_u64()?,
                        index: d.get("index").and_then(Value::as_u64),
                        name: d.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                        revision: d.get("revision").and_then(Value::as_u64),
                        front: d.get("active").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// "Name" (id 3), "Other" (id 5): the open documents in an error message.
fn listed(docs: &[Open]) -> String {
    match docs {
        [] => "none".into(),
        docs => docs.iter().map(|d| format!("\"{}\" (id {})", short_name(&d.name), d.id)).collect::<Vec<_>>().join(", "),
    }
}

/// The id of the document a call names with `document`: a number is the id itself; a path is
/// found by the engine, which is asked which document it is.
fn named_document(b: &mut dyn Backend, document: &Value) -> Result<u64, Failure> {
    let not_a_document = || Failure::param("document", format!("`document` must be an open file's id (a number) or its path, not {document}"));
    match document {
        Value::Number(n) => n.as_u64().ok_or_else(not_a_document),
        Value::String(path) if !path.is_empty() => {
            let doc = b.call("document.inspect", json!({"depth": 0, "fields": [], "document": path}))?;
            doc.get("id").and_then(Value::as_u64).ok_or_else(|| Failure::coded("failed", "the app did not say which document that path is"))
        }
        _ => Err(not_a_document()),
    }
}

/// The document a call acts on: the one it names, else the working document, else (the first
/// time) the tab in front. `None`: no document is open.
fn target_document(state: &ToolState, b: &mut dyn Backend, explicit: Option<&Value>) -> Result<Option<u64>, Failure> {
    match (explicit, &state.working) {
        (Some(document), _) => named_document(b, document).map(Some),
        (None, Some(w)) => Ok(Some(w.id)),
        (None, None) => Ok(open_documents(b)?.iter().find(|d| d.front).map(|d| d.id)),
    }
}

/// Run one tool for a caller that keeps nothing between calls: each call acts on the document it
/// names, else on the tab in front. A server uses [`call_tool_in`], which remembers the working
/// document. Every failure becomes an `isError` result (never a panic).
pub fn call_tool(b: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    call_tool_in(&mut ToolState::default(), b, name, args)
}

/// Run one tool. Every failure becomes an `isError` result (never a panic) that says what failed
/// in words and as `{code, message, …}`; every result of a tool that reads or changes a document
/// names that document (`{id, name, revision, front}`).
pub fn call_tool_in(state: &mut ToolState, b: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    if !TOOL_NAMES.contains(&name) {
        let mut f = Failure::coded("unknownTool", format!("unknown tool `{name}` (tools/list has: {})", TOOL_NAMES.join(", ")));
        if let Some(d) = f.detail.as_mut() {
            d["tool"] = json!(name);
        }
        return refused(b, f);
    }
    let mut a = match args_obj(args) {
        Ok(a) => a,
        Err(e) => return refused(b, Failure::param("arguments", e)),
    };
    // Built once: the catalogue is the same on every call.
    static TOOLS: std::sync::LazyLock<Vec<Value>> = std::sync::LazyLock::new(tool_definitions);
    if let Some(schema) = TOOLS.iter().find(|t| t.get("name").and_then(Value::as_str) == Some(name)).and_then(|t| t.get("inputSchema"))
        && let Err(e) = refuse_unknown_args(schema, &Value::Object(a.clone()), "")
    {
        return refused(b, Failure::coded("badParams", e));
    }
    if a.get(IF_REVISION).is_some_and(|v| !(v.is_null() || v.is_u64())) {
        return refused(b, Failure::param(IF_REVISION, "`ifRevision` must be a revision number (a whole number, 0 or more)"));
    }
    match name {
        "get_capabilities" => return get_capabilities(state, b).unwrap_or_else(|f| refused(b, f)),
        "use_document" => return use_document(state, b, &a).unwrap_or_else(|f| refused(b, f)),
        _ => {}
    }
    let explicit = a.remove("document").filter(|d| !d.is_null());
    let on_document = DOCUMENT_TOOLS.contains(&name) || CANVAS_TOOLS.contains(&name);
    let target = match on_document {
        true => match target_document(state, b, explicit.as_ref()) {
            Ok(t) => t,
            Err(f) => return not_open(state, b, f, None, explicit.as_ref()),
        },
        false => None,
    };
    // The documents as they are before the call's first request: what a result that only reads
    // reports its revision from (review R2-01).
    let before = match on_document || name == "open_file" {
        true => match open_documents(b) {
            Ok(docs) => Some(docs),
            Err(message) => {
                // Another run of the app: nothing may be sent to it under this run's ids.
                let detail = b.take_error_detail().unwrap_or_else(|| error_detail("failed", &message));
                if detail.get("restarted").and_then(Value::as_bool) == Some(true) {
                    return closed_or(state, b, message, detail, target, explicit.as_ref(), None, None);
                }
                None
            }
        },
        false => None,
    };
    let mut ctx = Ctx::default();
    let outcome = match target {
        Some(id) => run_tool(&mut InDocument { inner: b, document: json!(id) }, name, &a, &mut ctx, before.as_deref()),
        None => run_tool(b, name, &a, &mut ctx, before.as_deref()),
    };
    if !(on_document || name == "open_file") {
        return outcome.unwrap_or_else(|f| refused(b, f));
    }
    about_document(state, b, outcome, target, explicit.as_ref(), before.as_deref(), &ctx)
}

fn run_tool(b: &mut dyn Backend, name: &str, a: &Map<String, Value>, ctx: &mut Ctx, before: Option<&[Open]>) -> Outcome {
    match name {
        "list_commands" => list_commands(b, a),
        "run_command" => run_command(b, a, ctx, before),
        "batch" => batch(b, a, ctx),
        "create_node" => create_node(b, a, ctx),
        "set_properties" => set_properties(b, a, ctx),
        "inspect_document" => inspect_document(b, a),
        "get_node" => get_node(b, a),
        "select" => select(b, a, ctx),
        "export" => export(b, a),
        "screenshot" => screenshot(b, a),
        "design_context" => Ok(exec(b, "design.get", json!({})).map(|v| ToolResult::json(&v))?),
        "check_design" => check_design(b, a, ctx),
        "check_prototype" => Ok(exec(b, "proto.check", passed(a, &["page"])).map(|v| ToolResult::json(&v))?),
        "simulate_prototype" => simulate_prototype(b, a),
        "run_scenarios" => Ok(exec(b, "proto.scenario.run", passed(a, &["id"])).map(|v| ToolResult::json(&v))?),
        "open_file" => open_file(b, a, ctx),
        "save_file" => save_file(b, a, ctx),
        "undo" => travel(b, a, "edit.undo", ctx),
        "redo" => travel(b, a, "edit.redo", ctx),
        "pointer" => pointer(b, a, ctx),
        "key" => key(b, a, ctx),
        "type_text" => type_text(b, a, ctx),
        "inspect_ui" => Ok(b.call("ui.inspect", json!({})).map(|v| ToolResult::json(&v))?),
        "wait_for_changes" => wait_for_changes(b, a),
        other => Err(Failure::coded("unknownTool", format!("unknown tool `{other}` (tools/list has: {})", TOOL_NAMES.join(", ")))),
    }
}

/// What kind of failure `f` is: what the tool said, else what the backend said about this very
/// message, else a check of the call's arguments.
fn detail_of(b: &mut dyn Backend, f: &Failure) -> Value {
    let from_backend = b.take_error_detail().filter(|d| d.get("message").and_then(Value::as_str) == Some(f.message.as_str()));
    f.detail.clone().or(from_backend).unwrap_or_else(|| error_detail(f.fallback, &f.message))
}

/// The result of a call that failed without being about a document.
fn refused(b: &mut dyn Backend, f: Failure) -> ToolResult {
    let detail = detail_of(b, &f);
    ToolResult::failed(&f.message, detail)
}

/// A failure that may be "that document is not open": then say which documents are, and that
/// nothing was done anywhere. `target`: the document the call was sent to, when it got that far.
fn not_open(state: &mut ToolState, b: &mut dyn Backend, f: Failure, target: Option<u64>, explicit: Option<&Value>) -> ToolResult {
    let detail = detail_of(b, &f);
    closed_or(state, b, f.message, detail, target, explicit, None, None)
}

/// Finish a failed call about a document: when the document it was for is not open (any more),
/// the error says so, names it and lists the open ones; else it names the document it was for.
fn closed_or(
    state: &mut ToolState,
    b: &mut dyn Backend,
    message: String,
    mut detail: Value,
    target: Option<u64>,
    explicit: Option<&Value>,
    docs: Option<Vec<Open>>,
    before: Option<&[Open]>,
) -> ToolResult {
    let restarted = detail.get("restarted").and_then(Value::as_bool) == Some(true);
    if restarted {
        // Another run of the app: the ids this server remembers mean nothing there.
        state.working = None;
        return ToolResult::failed(&message, detail);
    }
    let no_document = detail.get("code").and_then(Value::as_str) == Some("noDocument") || message.starts_with("no open document ");
    let Some(docs) = docs.or_else(|| open_documents(b).ok()) else { return ToolResult::failed(&message, detail) };
    let open = target.and_then(|t| docs.iter().find(|d| d.id == t));
    if let Some(d) = open {
        // A failed call changed nothing: it is about the document as the call found it.
        detail["document"] = d.identity_at(revision_for(d, None, before));
        return ToolResult::failed(&message, detail);
    }
    if !no_document || docs.is_empty() && target.is_none() && explicit.is_none() {
        return ToolResult::failed(&message, detail);
    }
    // Which document was meant: the one named, else the working document.
    let working = state.working.clone().filter(|w| explicit.is_none() && target.is_none_or(|t| t == w.id));
    let (what, document) = match (&working, explicit) {
        (Some(w), _) => (
            format!("the working document \"{}\" (id {}) is no longer open", short_name(&w.name), w.id),
            json!({"id": w.id, "name": short_name(&w.name), "closed": true}),
        ),
        (None, Some(named)) => (format!("no open document {named}"), json!({"asked": named, "closed": true})),
        (None, None) => (message.clone(), Value::Null),
    };
    let message = format!(
        "{what}, so nothing was done. Open documents: {}. Choose one with use_document {{document}} (an id, or \"front\"), or open a file with open_file",
        listed(&docs)
    );
    let mut detail = error_detail("noDocument", &message);
    if !document.is_null() {
        detail["document"] = document;
    }
    detail["documents"] = Value::Array(docs.iter().map(Open::identity).collect());
    ToolResult::failed(&message, detail)
}

/// Finish a call that read or changed a document: remember which document it was (it is the
/// working document from here on) and say so in the result, with the revision the result belongs
/// to ([`revision_for`]). `target`: the document the call was sent to.
fn about_document(
    state: &mut ToolState,
    b: &mut dyn Backend,
    outcome: Outcome,
    target: Option<u64>,
    explicit: Option<&Value>,
    before: Option<&[Open]>,
    ctx: &Ctx,
) -> ToolResult {
    // The backend's word on a failure, before the next call to it replaces it.
    let outcome = outcome.map_err(|f| {
        let detail = detail_of(b, &f);
        (f.message, detail)
    });
    if let Err((message, detail)) = &outcome
        && detail.get("restarted").and_then(Value::as_bool) == Some(true)
    {
        return closed_or(state, b, message.clone(), detail.clone(), target, explicit, None, before);
    }
    let reported = ctx.moved.or(target);
    let docs = open_documents(b).ok();
    let open = reported.and_then(|t| docs.as_ref()?.iter().find(|d| d.id == t).cloned());
    if let Some(d) = &open {
        state.working = Some(Working { id: d.id, name: d.name.clone() });
    }
    let identity = |d: &Open| d.identity_at(revision_for(d, ctx.changed, before));
    match outcome {
        Ok(mut result) => {
            // Another tab the command acted on (`file.close {index}`, `doc.switch {index}`): the
            // result names that one; the working document stays what it was.
            let acted = ctx.acted_on.filter(|a| Some(*a) != reported);
            if let Some(a) = acted {
                let other = docs.as_ref().and_then(|d| d.iter().find(|d| d.id == a));
                let named = before.and_then(|b| b.iter().find(|d| d.id == a));
                let document = match (other, named) {
                    (Some(d), _) => identity(d),
                    (None, Some(was)) => json!({"id": a, "name": short_name(&was.name), "closed": true}),
                    (None, None) => json!({"id": a, "closed": true}),
                };
                let mut extra = json!({"document": document});
                if let Some(d) = &open {
                    extra["workingDocument"] = identity(d);
                }
                result.attach(extra);
                return result;
            }
            match (&open, reported, &docs) {
                (Some(d), ..) => {
                    let mut extra = json!({"document": identity(d)});
                    // The command opened, made or reverted a file: it is the working document now.
                    if let (Some(to), Some(from)) = (ctx.moved, target)
                        && to != from
                    {
                        extra["workingDocumentChanged"] = json!({"from": from, "to": to});
                    }
                    result.attach(extra);
                }
                // The call itself closed the document it ran on.
                (None, Some(t), Some(_)) => {
                    let name = state.working.as_ref().filter(|w| w.id == t).map(|w| short_name(&w.name));
                    result.attach(json!({"document": {"id": t, "name": name, "closed": true}}));
                }
                _ => {}
            }
            result
        }
        Err((message, detail)) => closed_or(state, b, message, detail, reported, explicit, docs, before),
    }
}

/// `use_document`: choose the working document.
fn use_document(state: &mut ToolState, b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let asked = a
        .get("document")
        .filter(|d| !d.is_null())
        .ok_or_else(|| Failure::param("document", "`document` is required: a file's id, its path, or \"front\""))?;
    let wanted = match asked {
        Value::String(s) if s == "front" => None,
        other => match named_document(b, other) {
            Ok(id) => Some(id),
            Err(f) => return Ok(not_open(state, b, f, None, Some(asked))),
        },
    };
    let docs = open_documents(b)?;
    let chosen = match wanted {
        Some(id) => docs.iter().find(|d| d.id == id),
        None => docs.iter().find(|d| d.front),
    };
    let Some(doc) = chosen else {
        let f = Failure::coded("noDocument", format!("no open document {asked}"));
        let detail = detail_of(b, &f);
        return Ok(closed_or(state, b, f.message, detail, wanted, Some(asked), Some(docs), None));
    };
    state.working = Some(Working { id: doc.id, name: doc.name.clone() });
    let mut result = ToolResult::json(&json!({"workingDocument": doc.identity(), "documents": docs.iter().map(Open::identity).collect::<Vec<_>>()}));
    result.attach(json!({"document": doc.identity()}));
    Ok(result)
}

/// `get_capabilities`: what this server and the app behind it can do. Everything in `features` is
/// read from what the backend really has (its command list, its own answers), not assumed.
fn get_capabilities(state: &mut ToolState, b: &mut dyn Backend) -> Outcome {
    let commands = b.call("engine.commands", json!({}))?;
    let commands = commands.as_array().cloned().unwrap_or_default();
    let doc_of = |id: &str| {
        commands.iter().find(|c| c.get("id").and_then(Value::as_str) == Some(id)).map(|c| c.get("params").and_then(Value::as_str).unwrap_or_default())
    };
    let has = |id: &str| doc_of(id).is_some();
    let attached = b.has_ui();
    let docs = open_documents(b)?;
    // The working document is settled here, so what this answer says is what later calls use.
    if state.working.as_ref().is_none_or(|w| !docs.iter().any(|d| d.id == w.id))
        && let Some(front) = docs.iter().find(|d| d.front)
    {
        state.working = Some(Working { id: front.id, name: front.name.clone() });
    }
    let working = state.working.as_ref().and_then(|w| docs.iter().find(|d| d.id == w.id)).map(Open::identity);
    // The design check rules this build runs (none without the design commands).
    let checks: Vec<Value> = match has("design.check.rules") {
        true => exec(b, "design.check.rules", json!({}))
            .ok()
            .and_then(|r| r.as_array().map(|a| a.iter().filter_map(|r| r.get("id").cloned()).collect()))
            .unwrap_or_default(),
        false => Vec::new(),
    };
    let app = b.app_info();
    let app_feature =
        |name: &str| app.as_ref().and_then(|i| i.get("features")).and_then(Value::as_array).is_some_and(|f| f.iter().any(|x| x == name));
    // An attached app from before revisions has no `revision` in doc.list, and takes `ifRevision`
    // as an unknown parameter: only an engine that documents it detects conflicts.
    let revisions = doc_of("doc.list").is_some_and(|d| d.contains("revision"));
    let roots = figcraft_engine::file_access::current().map(|r| r.to_json());
    // A live prototype player needs a window: an app that says it has none (`app.info`'s
    // `window`) has none, an app from before that question is taken to have its window.
    let window = attached && app.as_ref().and_then(|i| i.get("window")).and_then(Value::as_bool).unwrap_or(true);
    let out = json!({
        "product": crate::PRODUCT_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": crate::server::PROTOCOL_VERSION,
        "backend": if attached { "attached" } else { "headless" },
        "window": window,
        "player": window && has("proto.present"),
        "app": app,
        "roots": roots,
        "workingDocument": working,
        "documents": docs.iter().map(Open::identity).collect::<Vec<_>>(),
        "commands": commands.len(),
        "features": {
            "prototypeSimulator": has("proto.simulate"),
            "prototypeSnapshot": has("proto.snapshot"),
            "scenarios": has("proto.scenario.run"),
            "flowCheck": has("proto.check"),
            "designContext": has("design.get"),
            "checks": checks,
            "workingDocument": true,
            "conflictDetection": revisions,
            "historyAuthors": doc_of("edit.undo").is_some_and(|d| d.contains("others")),
            "structuredErrors": true,
            "engineErrorCodes": !attached || app_feature("errorDetail"),
            "appIdentity": !attached || app.as_ref().is_some_and(|i| i.get("instance").is_some()),
        },
        "limits": {
            "maxLineBytes": crate::server::MAX_LINE,
            "maxResponseBytes": MAX_RESPONSE,
            "maxScreenshotSide": 8192,
            "historySteps": 20,
        },
        // The lists the engine's own open dispatcher, save and export commands work from.
        "formats": {
            "open": figcraft_engine::cmd::file::OPEN_FORMATS,
            "save": figcraft_engine::cmd::file::SAVE_FORMATS,
            "export": figcraft_engine::cmd::file::export_formats(),
        },
    });
    let mut result = ToolResult::json(&out);
    if let Some(w) = working {
        result.attach(json!({"document": w}));
    }
    Ok(result)
}

/// `undo` / `redo`: one step back or forward, the caller's own unless it says `others`.
fn travel(b: &mut dyn Backend, a: &Map<String, Value>, cmd: &str, ctx: &mut Ctx) -> Outcome {
    let mut params = json!({});
    if let Some(others) = a.get("others").filter(|v| !v.is_null()) {
        params["others"] = others.clone();
    }
    let v = exec(b, cmd, conditional(a, params))?;
    ctx.note(b);
    Ok(ToolResult::json(&v))
}

fn list_commands(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let filter = s_arg(a, "filter")?.map(str::to_lowercase).unwrap_or_default();
    let all = b.call("engine.commands", json!({}))?;
    let list: Vec<Value> = all
        .as_array()
        .map(|v| {
            v.iter()
                .filter(|c| {
                    filter.is_empty()
                        || ["id", "label", "menu", "params"]
                            .iter()
                            .any(|k| c.get(*k).and_then(Value::as_str).is_some_and(|s| s.to_lowercase().contains(&filter)))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if list.is_empty() {
        return Ok(ToolResult::text(format!("no command matches `{filter}`")));
    }
    // One compact line per command keeps this readable for agents.
    let mut out = format!("{} commands (id — label [menu] (shortcut) params; * = undoable):\n", list.len());
    for c in &list {
        let g = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("");
        out.push_str(g("id"));
        if c.get("undoable").and_then(Value::as_bool) == Some(true) {
            out.push('*');
        }
        out.push_str(" — ");
        out.push_str(g("label"));
        if !g("menu").is_empty() {
            out.push_str(&format!(" [{}]", g("menu")));
        }
        if !g("shortcut").is_empty() {
            out.push_str(&format!(" ({})", g("shortcut")));
        }
        out.push(' ');
        out.push_str(g("params"));
        out.push('\n');
    }
    Ok(ToolResult::text(out))
}

/// The commands that open, make or read again a file and put it in front: the file they answer
/// with (`id`) is the working document from here on, as with open_file (review R2-11).
const OPENS: &[&str] = &["file.new", "file.newBoard", "file.open", "file.openRecent", "file.duplicate", "file.revert"];

fn run_command(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx, before: Option<&[Open]>) -> Outcome {
    let cmd = req_s(a, "command")?;
    let params = match a.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(_) => return Err(Failure::param("params", "`params` must be an object")),
    };
    let mut params = params;
    friendly_params(&mut params);
    // A command on another tab, named by its place: the result names that document.
    if matches!(cmd, "file.close" | "doc.switch")
        && let Some(i) = params.get("index").and_then(Value::as_u64)
    {
        ctx.acted_on = before.and_then(|docs| docs.iter().find(|d| d.index == Some(i))).map(|d| d.id);
    }
    let v = exec(b, cmd, conditional(a, params))?;
    ctx.note(b);
    // A file opened, made or read again is the working document from here on (a reverted file is
    // the same document to the caller, under the id the engine gave it anew); the command's own
    // answer says which, and at which revision.
    if OPENS.contains(&cmd)
        && let Some(id) = v.get("id").and_then(Value::as_u64)
    {
        ctx.moved = Some(id);
        ctx.changed = v.get("revision").and_then(Value::as_u64).map(|r| (id, r)).or(ctx.changed.filter(|(d, _)| *d == id));
    }
    Ok(ToolResult::json(&v))
}

/// A step of a batch that is wrong before anything runs: which step, and the command it names.
fn bad_step(step: usize, command: Option<&str>, message: String) -> Failure {
    let mut f = Failure::coded("badParams", message);
    if let Some(d) = f.detail.as_mut() {
        d["step"] = json!(step);
        if let Some(c) = command {
            d["command"] = json!(c);
        }
    }
    f
}

fn batch(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let Some(Value::Array(cmds)) = a.get("commands") else {
        return Err(Failure::param("commands", "`commands` must be an array of {command, params}"));
    };
    if cmds.is_empty() {
        return Err(Failure::param("commands", "`commands` is empty"));
    }
    let mut steps = Vec::with_capacity(cmds.len());
    for (i, c) in cmds.iter().enumerate() {
        let Some(cmd) = c.get("command").and_then(Value::as_str) else {
            return Err(bad_step(i, None, format!("step {i}: `command` must be a string")));
        };
        let params = match c.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(v @ Value::Object(_)) => v.clone(),
            Some(_) => return Err(bad_step(i, Some(cmd), format!("step {i} ({cmd}): `params` must be an object"))),
        };
        let mut params = params;
        friendly_params(&mut params);
        let mut step = json!({"command": cmd, "params": params});
        if let (Some(name), Some(o)) = (c.get("as"), step.as_object_mut()) {
            o.insert("as".into(), name.clone());
        }
        steps.push(step);
    }
    let label = s_arg(a, "label")?.unwrap_or("Batch");
    let mut params = json!({"label": label, "commands": steps});
    if let Some(mode) = a.get("onError") {
        params["onError"] = mode.clone();
    }
    let v = exec(b, "command.batch", conditional(a, params))?;
    ctx.note(b);
    Ok(ToolResult::json(&v))
}

fn create_node(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    req_s(a, "type")?;
    for k in ["x", "y", "width", "height"] {
        n_arg(a, k)?;
    }
    if let Some(p) = a.get("props")
        && !p.is_object()
        && !p.is_null()
    {
        return Err(Failure::param("props", "`props` must be an object"));
    }
    let mut params = Map::new();
    for k in ["type", "x", "y", "width", "height", "parent", "index", "name", "text", "path", "points", "closed", "props", "select"] {
        if let Some(v) = a.get(k).filter(|v| !v.is_null()) {
            params.insert(k.into(), v.clone());
        }
    }
    if let Some(Value::Object(p)) = params.get_mut("props") {
        normalize_props(p);
    }
    let r = exec(b, "node.create", conditional(a, Value::Object(params)))?;
    ctx.note(b);
    let id = r.get("id").cloned().ok_or_else(|| Failure::coded("internal", "node.create returned no id"))?;
    let node = b.call("node.get", json!({"id": id, "depth": 0}))?;
    Ok(ToolResult::json(&json!({"id": id, "node": node})))
}

fn set_properties(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let props = match a.get("props") {
        Some(Value::Object(p)) if !p.is_empty() => p.clone(),
        _ => return Err(Failure::param("props", "`props` must be a non-empty object")),
    };
    let mut props = props;
    normalize_props(&mut props);
    let mut params = json!({"props": props});
    let ids = ids_arg(a, "ids")?;
    if let Some(ids) = &ids {
        if ids.is_empty() {
            return Err(Failure::param("ids", "`ids` is empty"));
        }
        params["ids"] = json!(ids);
    }
    let warnings = exec(b, "node.set", conditional(a, params))?.get("warnings").cloned();
    ctx.note(b);
    let ids = match ids {
        Some(i) => i,
        None => b
            .call("document.inspect", json!({"depth": 0}))?
            .get("selection")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
    };
    let nodes: Vec<Value> = ids.iter().take(20).filter_map(|i| b.call("node.get", json!({"id": i, "depth": 0})).ok()).collect();
    let mut out = json!({"ok": true, "nodes": nodes});
    if let Some(w) = warnings {
        out["warnings"] = w;
    }
    Ok(ToolResult::json(&out))
}

/// What one tool result may take on the wire. MCP stdio is newline-delimited JSON, so a result is
/// a single message and has to fit the client's read buffer whole -- there is no streaming of part
/// of one. The TypeScript SDK's stdio transport defaults to 10 MiB
/// (`STDIO_DEFAULT_MAX_BUFFER_SIZE`) and hosts rarely raise it; a document whose tree went past
/// that killed the server, because the client stopped reading, the pipe broke, and every later
/// call failed (#912). The 2 MiB left over covers the JSON-RPC envelope.
const MAX_RESPONSE: usize = 8 << 20;

/// Room kept for the `truncated` object, which is added after the size is measured and would
/// otherwise be able to push a response that only just fitted back over the limit.
const MARKER_RESERVE: usize = 1 << 10;

/// Room kept for everything the text block is wrapped in: the content object and `isError`, the
/// JSON-RPC envelope (`jsonrpc`, `id`, `result`) and the newline the transport writes after it.
/// About ninety bytes of that is fixed and the rest is the request id, so `maxBytes` holds for
/// any id up to roughly 900 bytes -- which is every id in practice, a number or a UUID. An id
/// larger than that is the one case where the message can pass the caller's figure, and sizing
/// the real envelope here would mean handing the transport's request id to every tool.
const ENVELOPE_RESERVE: usize = 1 << 10;

/// What the caller says it can read, as the bytes its *tree* may take: the figure is for the whole
/// message, so the envelope's share comes off it here.
fn budget(a: &Map<String, Value>) -> Result<(usize, usize), String> {
    // A caller that knows its own buffer can say so; 1 KiB is below the shallowest tree, so the
    // floor only keeps the arithmetic sane.
    let max_bytes = match n_arg(a, "maxBytes")? {
        Some(v) if v < 1024.0 => return Err("`maxBytes` must be at least 1024".into()),
        Some(v) => v.min(usize::MAX as f64) as usize,
        None => MAX_RESPONSE,
    };
    Ok((max_bytes, max_bytes.saturating_sub(ENVELOPE_RESERVE)))
}

/// What a value's text block would take on the wire. It is sent as a text block, so it is measured
/// pretty-printed *and* escaped -- escaping adds about a tenth.
fn wire_size(value: &Value) -> usize {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    serde_json::to_string(&text).map_or(text.len(), |s| s.len())
}

/// The document at `depth`, and what its text block would take on the wire. The tree is sent as a
/// text block, so it is measured pretty-printed *and* escaped -- escaping adds about a tenth.
fn inspect_at(b: &mut dyn Backend, depth: u64, all: bool, options: &Value) -> Result<(Value, usize), String> {
    let mut params = options.clone();
    params["depth"] = json!(depth);
    params["allPages"] = json!(all);
    let value = b.call("document.inspect", params)?;
    let wire = wire_size(&value);
    Ok((value, wire))
}

/// One layer's subtree at `depth`, and what its text block would take on the wire.
fn node_at(b: &mut dyn Backend, id: u64, depth: u64, options: &Value) -> Result<(Value, usize), String> {
    let mut params = options.clone();
    params["id"] = json!(id);
    params["depth"] = json!(depth);
    let value = b.call("node.get", params)?;
    let wire = wire_size(&value);
    Ok((value, wire))
}

fn inspection_options(a: &Map<String, Value>) -> Result<Value, String> {
    let options = Value::Object(a.iter().filter(|(k, _)| matches!(k.as_str(), "fields" | "outline")).map(|(k, v)| (k.clone(), v.clone())).collect());
    figcraft_engine::inspect::InspectOptions::parse(&options)?;
    Ok(options)
}

fn inspect_document(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let depth = n_arg(a, "depth")?.unwrap_or(4.0).clamp(0.0, 64.0) as u64;
    let all = match a.get("allPages") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(x)) => *x,
        Some(_) => return Err("`allPages` must be a boolean".into()),
    };
    let options = inspection_options(a)?;
    let (max_bytes, budget) = budget(a)?;

    let (value, wire) = inspect_at(b, depth, all, &options)?;
    if wire <= budget {
        return Ok(ToolResult::json(&value));
    }

    let (used, mut value, used_wire) = if depth == 0 {
        // Depth 0 is the shallowest tree there is, so there is nothing smaller to send: it goes
        // out over the limit with the marker saying so, the same as a bisection that reaches
        // depth 0. An error would leave the caller with nothing at all.
        (0, value, wire)
    } else {
        // The deepest tree that fits, by bisection: a deeper tree holds every node of a shallower
        // one, so the size does not fall as the depth grows. Bisecting keeps this to about six
        // more walks of the document rather than one per level.
        let (mut lo, mut hi) = (0u64, depth - 1);
        let mut best: Option<(u64, Value, usize)> = None;
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let (v, w) = inspect_at(b, mid, all, &options)?;
            if w + MARKER_RESERVE <= budget {
                best = Some((mid, v, w));
                if mid == hi {
                    break;
                }
                lo = mid + 1;
            } else if mid == 0 {
                best = Some((0, v, w));
                break;
            } else {
                hi = mid - 1;
            }
        }
        match best {
            Some(found) => found,
            None => return Ok(ToolResult::json(&value)),
        }
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "truncated".into(),
            json!({
                "requestedDepth": depth,
                "depth": used,
                "requestedDepthBytes": wire,
                "bytes": used_wire,
                "maxBytes": max_bytes,
                "reason": format!(
                    "the tree at depth {depth} would be {wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; it is reported at depth {used} instead"
                ),
                "hint": "get_node({id, depth}) walks one subtree at a time, within the same limit: a node with more children than fit answers with a window of them and the offset to ask for next",
            }),
        );
    }
    Ok(ToolResult::json(&value))
}

/// One layer's subtree, kept inside what the caller can read.
///
/// `inspect_document` sends a shallower tree when the document's is too large and points here for
/// the rest, so this tool is the way out of that limit and must not be a way past it: a frame with
/// fifteen thousand children answers with 10.5 MiB at depth 1, over the buffer the report in #912
/// is about, and no depth makes that smaller because the width is at depth 1. So the subtree is
/// cut by depth first, as the document's is, and then by a window over the children.
fn get_node(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let id = id_arg(a.get("id").ok_or("`id` is required")?)?;
    let depth = n_arg(a, "depth")?.unwrap_or(1.0).clamp(0.0, 64.0) as u64;
    let offset = n_arg(a, "offset")?.unwrap_or(0.0).max(0.0).min(u32::MAX as f64) as usize;
    let options = inspection_options(a)?;
    let (max_bytes, budget) = budget(a)?;

    let (value, wire) = node_at(b, id, depth, &options)?;
    if value.is_null() {
        let mut f = Failure::coded("noSuchNode", format!("no such layer {id}"));
        if let Some(d) = f.detail.as_mut() {
            d["node"] = json!(id);
        }
        return Err(f);
    }
    if wire <= budget && offset == 0 {
        return Ok(ToolResult::json(&value));
    }

    // The deepest subtree that fits, by bisection, as the document's tree is cut. A node asked for
    // at a depth of its own children only (1) cannot be made shallower without losing them all,
    // so that is where the window takes over.
    let (mut used, mut value, mut used_wire) = (depth, value, wire);
    if depth > 1 && offset == 0 {
        let (mut lo, mut hi) = (1u64, depth - 1);
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let (v, w) = node_at(b, id, mid, &options)?;
            if w + MARKER_RESERVE <= budget {
                (used, value, used_wire) = (mid, v, w);
                if mid == hi {
                    break;
                }
                lo = mid + 1;
            } else if mid == 1 {
                (used, value, used_wire) = (1, v, w);
                break;
            } else {
                hi = mid - 1;
            }
        }
        if used_wire + MARKER_RESERVE <= budget {
            return Ok(ToolResult::json(&with_marker(value, Cut { asked: depth, used, asked_wire: wire, used_wire }, max_bytes, true, None)));
        }
    }

    // Still over, or a window was asked for: the children from `offset` that fit.
    let total = value.get("children").and_then(Value::as_array).map_or(0, Vec::len);
    let window = children_window(&mut value, offset, budget.saturating_sub(MARKER_RESERVE));
    let shown = value.get("children").and_then(Value::as_array).map_or(0, Vec::len);
    // A child too large to fit on its own is passed over (a page of just the marker), so paging
    // moves on and every answer stays within maxBytes; `get_node` on the child reads it.
    let after = offset + shown.max(1);
    let next = (after < total).then_some(after);
    let used_wire = wire_size(&value);
    if used_wire + MARKER_RESERVE > budget {
        // Even with every child left out, the layer's own fields (a very long name, say) pass the
        // limit: an answer would be larger than the caller can read, so there is none.
        return Err(Failure::param(
            "maxBytes",
            format!(
                "layer {id} takes {used_wire} bytes on the wire without its children, more than fit in maxBytes {max_bytes}; ask again with a larger `maxBytes`"
            ),
        ));
    }
    // Children left out after the window were cut by size; a window that only `offset` moved, of a
    // subtree within the limit, is paging.
    let over = wire > budget || next.is_some();
    let mut out = with_marker(
        value,
        Cut { asked: depth, used, asked_wire: wire, used_wire },
        max_bytes,
        over,
        Some(json!({
            "from": offset,
            "shown": shown,
            "total": total,
            "nextOffset": next,
        })),
    );
    if window && shown == total && offset == 0 && used == depth {
        // Nothing was left out after all (an offset of zero that fitted whole).
        out.as_object_mut().map(|o| o.remove("truncated"));
    }
    Ok(ToolResult::json(&out))
}

/// Keep the children from `offset` that fit in `budget`, and say whether any were left out. The
/// window is found by bisection: a node with thousands of children would otherwise be serialised
/// once per child.
fn children_window(value: &mut Value, offset: usize, budget: usize) -> bool {
    let Some(all) = value.get("children").and_then(Value::as_array).cloned() else { return false };
    let rest: Vec<Value> = all.iter().skip(offset).cloned().collect();
    let fits = |value: &mut Value, n: usize| {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("children".into(), Value::Array(rest.iter().take(n).cloned().collect()));
        }
        wire_size(value) <= budget
    };
    if fits(value, rest.len()) {
        return rest.len() != all.len();
    }
    // The largest window that fits, between none and all of the rest.
    let (mut lo, mut hi) = (0usize, rest.len());
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if fits(value, mid) { lo = mid } else { hi = mid - 1 }
    }
    fits(value, lo);
    true
}

/// How a subtree was cut: the depth asked for and the depth reported, with their sizes on the wire.
struct Cut {
    asked: u64,
    used: u64,
    asked_wire: usize,
    used_wire: usize,
}

/// `value` with the `truncated` object saying how it was cut. `over`: the subtree asked for passed
/// the limit (else the window is only the `offset` asked for). `children`: the window, when the
/// children were cut as well as (or instead of) the depth.
fn with_marker(mut value: Value, cut: Cut, max_bytes: usize, over: bool, children: Option<Value>) -> Value {
    let Cut { asked, used, asked_wire, used_wire } = cut;
    if let Some(obj) = value.as_object_mut() {
        let mut marker = json!({
            "requestedDepth": asked,
            "depth": used,
            "requestedDepthBytes": asked_wire,
            "bytes": used_wire,
            "maxBytes": max_bytes,
        });
        match &children {
            Some(window) => {
                marker["children"] = window.clone();
                let from = window.get("from").and_then(Value::as_u64).unwrap_or(0);
                marker["reason"] = json!(if over {
                    format!(
                        "the subtree at depth {asked} would be {asked_wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; its children are reported from {from} instead"
                    )
                } else {
                    format!(
                        "its children are reported from {from}, the `offset` asked for; the subtree at depth {asked} ({asked_wire} bytes) is within the {max_bytes}-byte limit"
                    )
                });
                marker["hint"] = json!("ask again with the `offset` in `children.nextOffset` for the rest, or get_node({id, depth}) on one child");
            }
            None => {
                marker["reason"] = json!(format!(
                    "the subtree at depth {asked} would be {asked_wire} bytes on the wire, over the {max_bytes}-byte limit a client can read in one message; it is reported at depth {used} instead"
                ));
                marker["hint"] = json!("get_node({id, depth}) on a child reads deeper, one subtree at a time");
            }
        }
        obj.insert("truncated".into(), marker);
    }
    value
}

fn select(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let ids = ids_arg(a, "ids")?.ok_or("`ids` is required (an empty array deselects)")?;
    if ids.is_empty() {
        exec(b, "select.none", json!({}))?;
    } else {
        exec(b, "select.set", json!({"ids": ids}))?;
    }
    ctx.note(b);
    let sel = b.call("document.inspect", json!({"depth": 0}))?.get("selection").cloned().unwrap_or(Value::Null);
    Ok(ToolResult::json(&json!({"selection": sel})))
}

fn export(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let mut params = Map::new();
    if let Some(ids) = ids_arg(a, "ids")? {
        params.insert("ids".into(), json!(ids));
    }
    let format = s_arg(a, "format")?.map(str::to_ascii_lowercase);
    if let Some(f) = &format {
        if !["png", "jpg", "jpeg", "svg", "webp", "pdf", "xml", "fig"].contains(&f.as_str()) {
            return Err(Failure::param("format", format!("unknown format `{f}` (png|jpg|svg|webp|pdf|xml|fig)")));
        }
        params.insert("format".into(), json!(f));
    }
    if let Some(s) = n_arg(a, "scale")? {
        if s <= 0.0 {
            return Err("`scale` must be positive".into());
        }
        params.insert("scale".into(), json!(s));
    }
    if let Some(s) = s_arg(a, "size")? {
        params.insert("size".into(), json!(s));
    }
    // PDF print options and the export options, checked by the command.
    for k in [
        "bleed",
        "cropMarks",
        "bleedMarks",
        "cmyk",
        "contentsOnly",
        "includeBoundingBox",
        "svgOutlineText",
        "svgIdAttribute",
        "svgSimplifyStroke",
        "vdCompat",
        "colorProfile",
        "quality",
    ] {
        if let Some(v) = a.get(k).filter(|v| !v.is_null()) {
            params.insert(k.into(), v.clone());
        }
    }
    if let Some(p) = s_arg(a, "path")? {
        params.insert("path".into(), json!(p));
        return Ok(exec(b, "file.export", Value::Object(params)).map(|v| ToolResult::json(&v))?);
    }
    let r = exec(b, "file.export", Value::Object(params))?;
    let fmt = r.get("format").and_then(Value::as_str).unwrap_or("png").to_string();
    let bytes = decode_data(&r).map_err(Failure::from_backend)?;
    match fmt.as_str() {
        "png" => {
            let (w, h) = png_info(&bytes);
            Ok(ToolResult::image(&bytes, "image/png", &json!({"format": "png", "width": w, "height": h, "bytes": bytes.len()})))
        }
        "jpg" | "jpeg" => Ok(ToolResult::image(&bytes, "image/jpeg", &json!({"format": "jpg", "bytes": bytes.len()}))),
        "webp" => Ok(ToolResult::image(&bytes, "image/webp", &json!({"format": "webp", "bytes": bytes.len()}))),
        "svg" => Ok(ToolResult::text(String::from_utf8_lossy(&bytes).into_owned())),
        "xml" => {
            let info = json!({"format": "xml", "bytes": bytes.len(), "warnings": r.get("warnings").cloned().unwrap_or_else(|| json!([]))});
            let mut out = ToolResult::text(String::from_utf8_lossy(&bytes).into_owned());
            out.content.extend(ToolResult::json(&info).content);
            Ok(out)
        }
        other => {
            let mut out = json!({"format": other, "bytes": bytes.len(), "data": base64::engine::general_purpose::STANDARD.encode(&bytes)});
            if let Some(w) = r.get("warnings") {
                out["warnings"] = w.clone();
            }
            Ok(ToolResult::json(&out))
        }
    }
}

fn screenshot(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let id = a.get("window").and_then(Value::as_u64);
    if (id.is_some() || matches!(a.get("window"), Some(Value::Bool(true)))) && b.has_ui() {
        let bytes = match id {
            Some(id) => b.window_png(id).map_err(Failure::from_backend)?,
            None => b.screenshot_png().map_err(Failure::from_backend)?,
        };
        let (w, h) = png_info(&bytes);
        return Ok(ToolResult::image(&bytes, "image/png", &json!({"source": "window", "width": w, "height": h})));
    }
    let max = n_arg(a, "maxSize")?.unwrap_or(1600.0).clamp(16.0, 8192.0);
    let req = n_arg(a, "scale")?.unwrap_or(1.0);
    if req <= 0.0 {
        return Err("`scale` must be positive".into());
    }
    let ids = ids_arg(a, "ids")?.unwrap_or_default();
    let measured = if ids.is_empty() { page_children(b).map_err(Failure::from_backend)? } else { ids.clone() };
    let mut scale = req;
    if let Some((x0, y0, x1, y1)) = bounds_of(b, &measured) {
        let side = (x1 - x0).max(y1 - y0).max(1e-6);
        scale = scale.min(max / side);
    }
    let mut bytes = b.render_png(&ids, scale).map_err(Failure::from_backend)?;
    let (mut w, mut h) = png_info(&bytes);
    // Layer bounds leave out effects (shadows) and overflow of unclipped frames, but the render
    // includes them: shrink to the measured image until it fits `maxSize` (rounding can add a pixel).
    for _ in 0..4 {
        let side = f64::from(w.max(h));
        if side <= max {
            break;
        }
        scale *= max / side;
        bytes = b.render_png(&ids, scale).map_err(Failure::from_backend)?;
        (w, h) = png_info(&bytes);
    }
    Ok(ToolResult::image(
        &bytes,
        "image/png",
        &json!({"source": if ids.is_empty() { "page" } else { "layers" }, "ids": ids, "scale": scale, "width": w, "height": h}),
    ))
}

/// The arguments `keys` of a call as the parameters of the command it stands for, passed on as
/// they were given: the command checks them, so its errors name them and what it takes can grow
/// without this layer knowing.
fn passed(a: &Map<String, Value>, keys: &[&str]) -> Value {
    Value::Object(keys.iter().filter_map(|k| a.get(*k).filter(|v| !v.is_null()).map(|v| ((*k).to_string(), v.clone()))).collect())
}

/// A true-or-false argument; not given is false.
fn flag(a: &Map<String, Value>, k: &str) -> Result<bool, Failure> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(x)) => Ok(*x),
        Some(_) => Err(Failure::param(k, format!("`{k}` must be true or false"))),
    }
}

/// `check_design`: the design measured by its rules (`design.check`, passed through). With
/// `record` the findings are stored too, and the result says which (`recorded`) and gives them as
/// stored (`stored`), each with the id the commands that settle a finding take.
fn check_design(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let params = passed(a, &["scope", "rules"]);
    if !flag(a, "record")? {
        if a.get(IF_REVISION).is_some_and(|v| !v.is_null()) {
            return Err(Failure::param(
                IF_REVISION,
                "`ifRevision` is for a check that records its findings (`record: true`): a check alone changes nothing",
            ));
        }
        return Ok(exec(b, "design.check", params).map(|v| ToolResult::json(&v))?);
    }
    // Stored first: when that is refused, nothing is reported as recorded.
    let recorded = exec(b, "design.check.record", conditional(a, params.clone()))?;
    ctx.note(b);
    let mut out = exec(b, "design.check", params)?;
    let ids: Vec<Value> = ["added", "existing"].iter().filter_map(|k| recorded.get(*k).and_then(Value::as_array)).flatten().cloned().collect();
    let all = exec(b, "design.finding.list", json!({"kind": "check"}))?;
    let stored: Vec<Value> = all
        .get("findings")
        .and_then(Value::as_array)
        .map(|found| found.iter().filter(|f| f.get("id").is_some_and(|id| ids.contains(id))).cloned().collect())
        .unwrap_or_default();
    if let Some(o) = out.as_object_mut() {
        o.insert("recorded".into(), recorded);
        o.insert("stored".into(), Value::Array(stored));
    }
    Ok(ToolResult::json(&out))
}

/// `simulate_prototype`: steps played on the simulator (`proto.simulate`, passed through). With
/// `image` the state the run ended in is drawn (`proto.snapshot`) and follows the JSON as image
/// content: the same start, and the steps that ran, so a run that stopped at a failing step is
/// drawn where it stopped.
fn simulate_prototype(b: &mut dyn Backend, a: &Map<String, Value>) -> Outcome {
    let image = flag(a, "image")?;
    let scale = n_arg(a, "scale").map_err(|e| Failure::param("scale", e))?;
    if scale.is_some_and(|s| s <= 0.0) {
        return Err(Failure::param("scale", "`scale` must be above 0"));
    }
    let mut run = exec(b, "proto.simulate", passed(a, &["start", "steps", "expect", "stopOnFailure"]))?;
    if !image {
        return Ok(ToolResult::json(&run));
    }
    let taken = run.get("stepsTaken").and_then(Value::as_u64).map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
    let steps: Vec<Value> = a.get("steps").and_then(Value::as_array).map(|s| s.iter().take(taken).cloned().collect()).unwrap_or_default();
    let mut params = json!({"steps": steps});
    for (key, value) in [("start", run.get("start").cloned()), ("scale", scale.map(|s| json!(s)))] {
        if let Some(v) = value.filter(|v| !v.is_null()) {
            params[key] = v;
        }
    }
    let drawn = exec(b, "proto.snapshot", params).and_then(|shot| decode_data(&shot).map(|bytes| (shot, bytes)));
    let (info, bytes) = match drawn {
        Ok((shot, bytes)) => {
            let (width, height) = png_info(&bytes);
            let mut info = json!({"format": "png", "width": width, "height": height, "scale": shot.get("scale")});
            // An attached app's document can change between the two calls.
            if shot.get("state") != run.get("state") {
                info["note"] = json!("the document changed between the run and the picture: the picture shows the state the same steps lead to now");
            }
            (info, Some(bytes))
        }
        // The run stands without its picture, and the result says why there is none.
        Err(why) => {
            b.take_error_detail();
            (json!({"error": why}), None)
        }
    };
    if let Some(o) = run.as_object_mut() {
        o.insert("image".into(), info);
    }
    let mut result = ToolResult::json(&run);
    if let Some(bytes) = bytes {
        result.content.push(json!({"type": "image", "data": base64::engine::general_purpose::STANDARD.encode(&bytes), "mimeType": "image/png"}));
    }
    Ok(result)
}

/// `open_file`: the file opens in front and is the working document from here on (`moved`).
fn open_file(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let path = req_s(a, "path")?;
    let mut params = json!({"path": path});
    match a.get("variables") {
        None | Some(Value::Null) => {}
        Some(v @ (Value::String(_) | Value::Object(_))) => params["variables"] = v.clone(),
        Some(_) => return Err(Failure::param("variables", "`variables` must be a path or the local variables JSON object")),
    }
    if let Some(reload) = a.get("reload").filter(|v| !v.is_null()) {
        params["reload"] = reload.clone();
    }
    let r = exec(b, "file.open", params)?;
    let opened = r.get("id").and_then(Value::as_u64);
    // The opened file, read by its id: with an attached app the tab in front may be another one
    // by now (review R2-11). An app whose answer has no id: the tab in front, as before.
    let mut inspect = json!({"depth": 1});
    if let Some(id) = opened {
        inspect["document"] = json!(id);
    }
    let doc = b.call("document.inspect", inspect)?;
    ctx.moved = opened.or_else(|| doc.get("id").and_then(Value::as_u64));
    // Its revision as the command answered it (the read after could take in another change).
    ctx.changed = opened.zip(r.get("revision").and_then(Value::as_u64));
    Ok(ToolResult::json(&json!({"opened": r, "document": doc})))
}

fn save_file(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let params = match s_arg(a, "path")? {
        Some(p) => json!({"path": p}),
        None => json!({}),
    };
    let v = exec(b, "file.save", conditional(a, params))?;
    ctx.note(b);
    Ok(ToolResult::json(&v))
}

fn mods(a: &Map<String, Value>, out: &mut Map<String, Value>) -> Result<(), String> {
    for k in ["shift", "alt", "cmd", "ctrl"] {
        match a.get(k) {
            None | Some(Value::Null) => {}
            Some(Value::Bool(x)) => {
                out.insert(k.into(), json!(x));
            }
            Some(_) => return Err(format!("`{k}` must be a boolean")),
        }
    }
    Ok(())
}

/// A gesture, key or typed text with `ifRevision`: checked in the request that acts (the backend
/// refuses it there), and first by the engine on its own (an app from before this takes no
/// `ifRevision` on the canvas methods, and would act without checking).
fn canvas_conditional(b: &mut dyn Backend, a: &Map<String, Value>, p: &mut Map<String, Value>) -> Result<(), Failure> {
    let Some(rev) = a.get(IF_REVISION).filter(|v| !v.is_null()) else { return Ok(()) };
    exec(b, "doc.list", json!({IF_REVISION: rev})).map_err(Failure::from_backend)?;
    p.insert(IF_REVISION.into(), rev.clone());
    Ok(())
}

fn pointer(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let x = n_arg(a, "x")?.ok_or("`x` is required")?;
    let y = n_arg(a, "y")?.ok_or("`y` is required")?;
    let kind = s_arg(a, "kind")?.unwrap_or("click");
    if let Some(t) = s_arg(a, "tool")? {
        exec(b, "tool.set", json!({"tool": t}))?;
    }
    let mut p = Map::new();
    p.insert("kind".into(), json!(kind));
    p.insert("x".into(), json!(x));
    p.insert("y".into(), json!(y));
    if let Some(to) = a.get("to").filter(|v| !v.is_null()) {
        let arr =
            to.as_array().filter(|t| t.len() == 2 && t.iter().all(|v| figcraft_engine::params::f64_of(v).is_some())).ok_or("`to` must be [x, y]")?;
        p.insert("to".into(), Value::Array(arr.clone()));
    }
    if let Some(s) = n_arg(a, "steps")? {
        p.insert("steps".into(), json!(s.clamp(1.0, 200.0) as u64));
    }
    for k in ["pressure", "pressureTo"] {
        if let Some(v) = n_arg(a, k)? {
            p.insert(k.into(), json!(v.clamp(0.0, 1.0)));
        }
    }
    mods(a, &mut p)?;
    canvas_conditional(b, a, &mut p)?;
    let v = b.call("ui.pointer", Value::Object(p))?;
    ctx.note(b);
    Ok(ToolResult::json(&v))
}

fn key(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let k = req_s(a, "key")?;
    let mut p = Map::new();
    p.insert("key".into(), json!(k));
    mods(a, &mut p)?;
    canvas_conditional(b, a, &mut p)?;
    let v = b.call("ui.key", Value::Object(p))?;
    ctx.note(b);
    Ok(ToolResult::json(&v))
}

fn type_text(b: &mut dyn Backend, a: &Map<String, Value>, ctx: &mut Ctx) -> Outcome {
    let text = req_s(a, "text")?;
    match a.get("id").filter(|v| !v.is_null()) {
        Some(id) => {
            let id = id_arg(id)?;
            // Checked where the edit starts: from there on it is this agent's text edit.
            exec(b, "text.edit.begin", conditional(a, json!({"id": id})))?;
            // What the backend says about a failure is kept now: the next call replaces it.
            let typed = b.call("ui.type", json!({"text": text})).map_err(|message| {
                let detail = b.take_error_detail();
                Failure { message, detail, fallback: "failed" }
            });
            // Always leave edit mode, even when typing failed.
            let ended = exec(b, "text.edit.end", json!({}));
            ctx.note(b);
            typed?;
            ended?;
            let node = b.call("node.get", json!({"id": id, "depth": 0}))?;
            Ok(ToolResult::json(&json!({"ok": true, "node": node})))
        }
        None => {
            let mut p = Map::new();
            p.insert("text".into(), json!(text));
            canvas_conditional(b, a, &mut p)?;
            let v = b.call("ui.type", Value::Object(p))?;
            ctx.note(b);
            Ok(ToolResult::json(&v))
        }
    }
}
