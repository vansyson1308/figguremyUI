//! An in-process engine session that answers the control-channel methods itself, so agents can
//! design, render and export without a window.

use base64::Engine as _;
use figcraft_doc::{ExportFormat, NodeId};
use figcraft_engine::{PointerEvent, PointerKind, Session, textedit};
use serde_json::{Value, json};

use crate::backend::{Backend, error_detail};

/// Headless backend: a [`Session`] with one untitled document.
pub struct Headless {
    pub session: Session,
    /// The detail of the last failed call ([`Backend::take_error_detail`]).
    last_error: Option<Value>,
    /// What `app.info` answers: made once, so a client can tell this session from another.
    instance: String,
    /// The revision the last change left its document at ([`Backend::take_revision`]).
    last_revision: Option<(u64, u64)>,
}

/// A failed call: its text, and what kind of failure it is (`{code, message, …}`).
struct Fail {
    message: String,
    detail: Value,
}

impl Fail {
    fn coded(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self { detail: error_detail(code, &message), message }
    }
}

impl From<figcraft_engine::EngineError> for Fail {
    fn from(e: figcraft_engine::EngineError) -> Self {
        Self { message: e.to_string(), detail: e.detail() }
    }
}

/// A bare message is this backend's own check of a request's parameters.
impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self::coded("badParams", message)
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Self {
        Self::coded("badParams", message)
    }
}

/// The note a command's result gets when it asked for UI that was dropped (`kinds`: the requests).
fn ui_dropped(kinds: &[String]) -> String {
    format!("dropped ({}): no window in this backend", kinds.join(", "))
}

impl Default for Headless {
    fn default() -> Self {
        Self::new()
    }
}

/// Largest side of a headless page render (pixels), so screenshots stay a sensible size.
const MAX_SIDE: f64 = 4096.0;

fn b(p: &Value, k: &str) -> bool {
    figcraft_engine::params::boolean(p, k).unwrap_or(false)
}

/// Pen pressure `k` (0..1), if given.
fn pressure(p: &Value, k: &str) -> Option<f32> {
    f(p, k).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0) as f32)
}

fn f(p: &Value, k: &str) -> Option<f64> {
    figcraft_engine::params::num(p, k)
}

fn ids_of(p: &Value) -> Vec<NodeId> {
    p.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(figcraft_engine::params::id_of).collect()).unwrap_or_default()
}

impl Headless {
    /// A session with one new, empty document.
    pub fn new() -> Self {
        Self::with_session(Session::new())
    }

    /// Wrap an existing session (it can host and join live sessions: an agent takes part with
    /// its own cursor, `collab.cursor`).
    pub fn with_session(mut session: Session) -> Self {
        if !session.live.available() {
            session.live.install(Box::new(figcraft_collab_net::NetLink::new(std::sync::Arc::new(|| {}))));
        }
        // No window: the commands that only act on one (the prototype player) say so.
        session.windowless = true;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let instance = format!("headless-{}-{started:x}-{n}", std::process::id());
        Self { session, last_error: None, instance, last_revision: None }
    }

    /// A command for whoever acts now ([`Session::agent`]: the agent [`Self::as_agent`] put there
    /// for a key or gesture, else the user): a shortcut a key stands for is that agent's step.
    fn execute(&mut self, cmd: &str, params: &Value) -> Result<Value, Fail> {
        self.run(|s| s.execute(cmd, params))
    }

    /// A command for `agent` (an AI agent's name, `engine.execute {agent}`) or for the user.
    fn execute_as(&mut self, agent: Option<&str>, cmd: &str, params: &Value) -> Result<Value, Fail> {
        self.run(|s| s.execute_as(agent, cmd, params))
    }

    /// Run a command (`f`) with what a live session sent since, and the UI it asked for dropped.
    fn run(&mut self, f: impl FnOnce(&mut Session) -> figcraft_engine::Result<Value>) -> Result<Value, Fail> {
        // What happened in a live session since the last call (latest presence only).
        self.session.live_poll();
        let r = f(&mut self.session);
        self.session.live_sync();
        // No UI: dialogs and panels requested by commands are dropped, and a result that would
        // otherwise read as if they had opened says so.
        let dropped: Vec<String> = self.session.ui_requests.drain(..).map(|u| u.kind).collect();
        let mut v = r?;
        if let (false, Some(o)) = (dropped.is_empty(), v.as_object_mut()) {
            o.insert("ui".into(), json!(ui_dropped(&dropped)));
        }
        Ok(v)
    }

    fn pointer(&mut self, p: &Value) -> Result<Value, Fail> {
        let zoom = self.session.ds().map(|d| d.view.zoom).unwrap_or(1.0).max(1e-6);
        let (x, y) = match (f(p, "x"), f(p, "y")) {
            (Some(x), Some(y)) => (x, y),
            _ => match (f(p, "sx"), f(p, "sy"), self.session.ds()) {
                (Some(sx), Some(sy), Ok(ds)) => ds.view.screen_to_page(sx, sy),
                _ => return Err("pass x, y (page coordinates) or sx, sy (screen)".into()),
            },
        };
        let mk = |kind: PointerKind, x: f64, y: f64| PointerEvent {
            kind,
            x,
            y,
            sx: x * zoom,
            sy: y * zoom,
            shift: b(p, "shift"),
            alt: b(p, "alt"),
            cmd: b(p, "cmd"),
            ctrl: b(p, "ctrl"),
            zoom,
            pressure: pressure(p, "pressure"),
        };
        // A drag's pressure goes from `pressure` to `pressureTo`.
        let (pr0, pr1) = (pressure(p, "pressure"), pressure(p, "pressureTo").or(pressure(p, "pressure")));
        let kind = p.get("kind").and_then(Value::as_str).unwrap_or("click");
        let mut events = Vec::new();
        match kind {
            "down" => events.push(mk(PointerKind::Down, x, y)),
            "move" => events.push(mk(PointerKind::Move, x, y)),
            "up" => events.push(mk(PointerKind::Up, x, y)),
            "hover" => events.push(mk(PointerKind::Hover, x, y)),
            "double" | "doubleclick" | "dblclick" => events.push(mk(PointerKind::DoubleClick, x, y)),
            "click" => {
                events.push(mk(PointerKind::Down, x, y));
                events.push(mk(PointerKind::Up, x, y));
            }
            "drag" => {
                let to = p.get("to").and_then(Value::as_array).ok_or("drag needs `to: [x, y]`")?;
                let tx = to.first().and_then(figcraft_engine::params::f64_of).ok_or("bad `to`: expected [x, y]")?;
                let ty = to.get(1).and_then(figcraft_engine::params::f64_of).ok_or("bad `to`: expected [x, y]")?;
                let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 200);
                events.push(mk(PointerKind::Down, x, y));
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let pressure = pr0.zip(pr1).map(|(a, b)| a + (b - a) * t as f32);
                    events.push(PointerEvent { pressure, ..mk(PointerKind::Move, x + (tx - x) * t, y + (ty - y) * t) });
                }
                events.push(PointerEvent { pressure: pr1, ..mk(PointerKind::Up, tx, ty) });
            }
            other => return Err(format!("unknown pointer kind `{other}` (down|move|up|click|double|hover|drag)").into()),
        }
        for ev in &events {
            let r = figcraft_engine::guard::run("ui.pointer", || self.session.pointer(ev));
            self.session.ui_requests.clear();
            r?;
        }
        Ok(
            json!({"selection": self.session.selection(), "tool": self.session.tool.tool.id(), "textEdit": self.session.ds().ok().and_then(|d| d.text_edit.clone())}),
        )
    }

    fn key(&mut self, p: &Value) -> Result<Value, Fail> {
        use figcraft_engine::keymap::{Context, Mods, Platform, lookup, normalize};
        let key = p.get("key").and_then(Value::as_str).ok_or("`key` required")?;
        let m = Mods { shift: b(p, "shift"), alt: b(p, "alt"), cmd: b(p, "cmd"), ctrl: b(p, "ctrl") };
        let platform = Platform::current();
        // Text editing keys go to the editor, after the shortcuts live while editing; other
        // Cmd+letter shortcuts (undo…) still reach the canvas.
        if self.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false) {
            if let Some(bd) = lookup(platform, Context::Text, key, m).filter(|_| m.cmd || m.alt || m.ctrl) {
                self.execute(bd.id, &json!({}))?;
                return Ok(json!({"command": bd.id}));
            }
            if !(m.cmd && key.len() == 1) {
                let (word, line) = figcraft_engine::keymap::text_motion(platform, key, m);
                textedit::key(&mut self.session, key, m.shift, word, line)?;
                return Ok(json!({"ok": true, "textEdit": true}));
            }
        }
        // Vector edit mode and shape builder keys (tools, nudging points, delete, join, Enter), as
        // on the canvas.
        if self.session.tool_key(key, m.shift, m.alt, m.cmd)? {
            return Ok(json!({"ok": true}));
        }
        if let Some(bd) = lookup(platform, Context::Canvas, key, m) {
            match bd.id {
                "select.none" => self.session.escape()?,
                // Enter with a shape or frame tool chosen places one in the middle of the view.
                "select.children" if self.session.tool.tool.draws_shape() => {
                    self.execute("tool.place", &json!({}))?;
                }
                // Enter on a text layer starts editing it.
                "select.children" if self.single_text().is_some() => {
                    self.execute("text.edit.begin", &json!({"id": self.single_text()}))?;
                }
                // Enter on shapes edits them as vectors (several at once).
                "select.children" if self.vector_targets().is_some() => {
                    self.execute("vector.edit.begin", &json!({"ids": self.vector_targets()}))?;
                }
                id => {
                    self.execute(id, &json!({}))?;
                }
            }
            return Ok(json!({"command": bd.id, "tool": self.session.tool.tool.id()}));
        }
        let (name, _) = normalize(key).ok_or_else(|| Fail::from(format!("unknown key `{key}`")))?;
        match name {
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
                let (dx, dy) = match name {
                    "ArrowLeft" => (-1, 0),
                    "ArrowRight" => (1, 0),
                    "ArrowUp" => (0, -1),
                    _ => (0, 1),
                };
                // With nothing selected the arrows pan the view.
                let cmd = if self.session.selection().is_empty() { "view.pan" } else { "transform.nudge" };
                self.execute(cmd, &json!({"dx": dx, "dy": dy, "big": m.shift}))
            }
            // Number keys set the selection's opacity.
            d if m.is_none() && d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) && self.session.prefs.number_keys_opacity => {
                self.execute("object.opacityKeys", &json!({"digits": d}))
            }
            _ => Err(format!("no shortcut for `{key}` with these modifiers").into()),
        }
    }

    /// The selected layer when exactly one text layer is selected.
    /// The selected layers Enter edits as vectors (see `vectoredit::enter_targets`).
    fn vector_targets(&self) -> Option<Vec<NodeId>> {
        figcraft_engine::vectoredit::enter_targets(self.session.doc().ok()?, &self.session.selection())
    }

    fn single_text(&self) -> Option<NodeId> {
        let sel = self.session.selection();
        let doc = self.session.doc().ok()?;
        match sel.as_slice() {
            [id] if doc.node(*id).is_some_and(|n| matches!(n.kind, figcraft_doc::NodeKind::Text(_))) => Some(*id),
            _ => None,
        }
    }

    fn render_to(&mut self, p: &Value) -> Result<Value, Fail> {
        let scale = f(p, "scale").unwrap_or(1.0);
        let ids: Vec<u64> = ids_of(p).into_iter().map(|i| i.0).collect();
        let bytes = self.render(&ids, scale)?;
        let (w, h) = crate::png_size(&bytes).unwrap_or((0, 0));
        match p.get("path").and_then(Value::as_str) {
            Some(path) => {
                figcraft_engine::file_access::write(path, &bytes)?;
                Ok(json!({"path": path, "width": w, "height": h, "bytes": bytes.len()}))
            }
            None => Ok(json!({"format": "png", "width": w, "height": h, "data": base64::engine::general_purpose::STANDARD.encode(&bytes)})),
        }
    }

    fn inspect_ui(&self) -> Value {
        let view = self.session.ds().ok().map(|d| serde_json::to_value(&d.view).unwrap_or(Value::Null));
        json!({
            "backend": "headless",
            "status": self.session.status(),
            "tool": self.session.tool_info(),
            "view": view,
            "message": self.session.message,
        })
    }

    /// Run `f` as the request's `agent` (the AI agent a tool call is for), so that what a gesture
    /// or typed text changes is that agent's step in the undo history, as a command's is.
    fn as_agent<T>(&mut self, p: &Value, f: impl FnOnce(&mut Self) -> T) -> T {
        let Some(agent) = p.get("agent").and_then(Value::as_str) else { return f(self) };
        // The name as `engine.execute {agent}` forms it.
        let was = self.session.agent.replace(figcraft_engine::cmd::comment::agent_author(agent));
        // Who acted before comes back also when `f` panics (the server catches it): the next
        // request must not run as this agent.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)));
        self.session.agent = was;
        match r {
            Ok(v) => v,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// `ifRevision` on a gesture, key or typed text: refused, before anything happens, unless the
    /// document is at that revision (checked as a command's is).
    fn conditional(&mut self, method: &str, p: &Value) -> Result<(), Fail> {
        match p.get("ifRevision") {
            None | Some(Value::Null) => Ok(()),
            Some(v) => {
                let expected = v.as_u64().ok_or("`ifRevision` must be a revision number (a whole number, 0 or more)")?;
                Ok(self.session.require_revision(method, expected)?)
            }
        }
    }

    /// PNG bytes of `ids` (their union), or of the current page's content when there are none.
    fn render(&mut self, ids: &[u64], scale: f64) -> Result<Vec<u8>, Fail> {
        let scale = if scale.is_finite() && scale > 0.0 { scale.min(64.0) } else { 1.0 };
        let page = self.session.page()?;
        let doc = self.session.doc()?;
        if !ids.is_empty() {
            let ids: Vec<NodeId> = ids.iter().map(|i| NodeId(*i)).collect();
            if let Some(missing) = ids.iter().find(|i| !doc.contains(**i)) {
                return Err(figcraft_engine::EngineError::NoNode(*missing).into());
            }
            return Ok(figcraft_engine::cmd::file::export_bytes(doc, &ids, ExportFormat::Png, scale)?);
        }
        let bounds = doc.page_content_bounds(page).ok_or_else(|| Fail::coded("failed", "the current page is empty: nothing to render"))?;
        let side = bounds.width().max(bounds.height()).max(1.0);
        let scale = scale.min(MAX_SIDE / side);
        let view = figcraft_render::View {
            origin: (bounds.x0, bounds.y0),
            scale,
            width: (bounds.width() * scale).ceil().max(1.0) as u32,
            height: (bounds.height() * scale).ceil().max(1.0) as u32,
        };
        let pix = figcraft_render::render_view(doc, page, &view, &Default::default()).ok_or_else(|| Fail::coded("failed", "render failed"))?;
        figcraft_render::encode_png(&pix).map_err(|e| Fail::coded("failed", e))
    }

    /// PNG bytes of the current page with a margin, zoomed to fit 1600 px (there is no window).
    fn screenshot(&mut self) -> Result<Vec<u8>, Fail> {
        let page = self.session.page()?;
        let doc = self.session.doc()?;
        let b = doc.page_content_bounds(page).unwrap_or(figcraft_geom::Rect::new(0.0, 0.0, 800.0, 600.0));
        let b = b.inflate(32.0, 32.0);
        let scale = (1600.0 / b.width().max(b.height()).max(1.0)).min(2.0);
        let view = figcraft_render::View {
            origin: (b.x0, b.y0),
            scale,
            width: (b.width() * scale).ceil().max(1.0) as u32,
            height: (b.height() * scale).ceil().max(1.0) as u32,
        };
        let pix = figcraft_render::render_view(doc, page, &view, &Default::default()).ok_or_else(|| Fail::coded("failed", "render failed"))?;
        figcraft_render::encode_png(&pix).map_err(|e| Fail::coded("failed", e))
    }

    /// Keep the detail of a failure for [`Backend::take_error_detail`] and hand on its text.
    fn noted<T>(&mut self, r: Result<T, Fail>) -> Result<T, String> {
        self.last_error = None;
        r.map_err(|f| {
            self.last_error = Some(f.detail);
            f.message
        })
    }
}

impl Headless {
    fn dispatch(&mut self, method: &str, p: Value) -> Result<Value, Fail> {
        match method {
            "engine.execute" => {
                let cmd = p.get("command").and_then(Value::as_str).ok_or("`command` required")?.to_string();
                let params = match p.get("params") {
                    None | Some(Value::Null) => json!({}),
                    Some(v) => v.clone(),
                };
                self.execute_as(p.get("agent").and_then(Value::as_str), &cmd, &params)
            }
            "engine.commands" => Ok(Value::Array(
                figcraft_engine::all_commands().iter().map(|c| json!({"id": c.id, "label": c.label, "params": c.params, "menu": c.menu(), "shortcut": c.shortcut(), "undoable": c.history})).collect(),
            )),
            "document.inspect" => {
                let depth = p.get("depth").and_then(Value::as_u64).unwrap_or(8).min(64) as usize;
                let options = figcraft_engine::inspect::InspectOptions::parse(&p)?;
                Ok(figcraft_engine::inspect::document_with_options(&self.session, depth, b(&p, "allPages"), &options))
            }
            "node.get" => {
                let id = p.get("id").and_then(figcraft_engine::params::id_of).ok_or("`id` required (a node id)")?;
                let depth = p.get("depth").and_then(Value::as_u64).unwrap_or(1).min(64) as usize;
                let doc = self.session.doc()?;
                if !doc.contains(id) {
                    return Err(figcraft_engine::EngineError::NoNode(id).into());
                }
                let options = figcraft_engine::inspect::InspectOptions::parse(&p)?;
                Ok(figcraft_engine::inspect::node_json_with_options(doc, id, depth, &options))
            }
            "ui.inspect" => Ok(self.inspect_ui()),
            "ui.pointer" => {
                self.conditional(method, &p)?;
                self.as_agent(&p, |h| h.pointer(&p))
            }
            "ui.key" => {
                self.conditional(method, &p)?;
                self.as_agent(&p, |h| h.key(&p))
            }
            "ui.type" => {
                self.conditional(method, &p)?;
                let text = p.get("text").and_then(Value::as_str).ok_or("`text` required")?;
                if !self.session.ds().map(|d| d.text_edit.is_some()).unwrap_or(false) {
                    return Err(Fail::coded(
                        "notAvailable",
                        "not editing text: double-click a text layer, press Enter on it, or run text.edit.begin {id}",
                    ));
                }
                self.as_agent(&p, |h| textedit::insert(&mut h.session, text))?;
                Ok(json!({"ok": true}))
            }
            "ui.render" => self.render_to(&p),
            "ui.screenshot" => {
                let bytes = self.screenshot()?;
                let (w, h) = crate::png_size(&bytes).unwrap_or((0, 0));
                match p.get("path").and_then(Value::as_str) {
                    Some(path) => {
                        figcraft_engine::file_access::write(path, &bytes)?;
                        Ok(json!({"path": path, "width": w, "height": h}))
                    }
                    None => Ok(json!({"format": "png", "width": w, "height": h, "data": base64::engine::general_purpose::STANDARD.encode(&bytes)})),
                }
            }
            "events.poll" => {
                let after = p.get("after").and_then(Value::as_u64).unwrap_or(0);
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 1000) as usize;
                let topics = figcraft_engine::events::topics(p.get("topics"))?;
                // Headless has no other writer to wait for.
                let result = self.session.events.poll(after, &topics, limit, std::time::Duration::ZERO);
                serde_json::to_value(result).map_err(|e| Fail::coded("internal", e.to_string()))
            }
            // Which session this is: a client that reconnects can tell it was replaced.
            "app.info" => Ok(json!({
                "instance": self.instance,
                "version": env!("CARGO_PKG_VERSION"),
                "pid": std::process::id(),
                "window": false,
                "features": ["errorDetail", "revision"],
            })),
            "app.quit" => Ok(json!({"ok": true})),
            other => Err(Fail::coded("unknownMethod", format!("unknown method `{other}`"))),
        }
    }

    /// [`Backend::call`]: with `document` the request runs with that file in front, and the file
    /// that was in front is put back.
    fn call_in_document(&mut self, method: &str, p: Value) -> Result<Value, Fail> {
        self.last_revision = None;
        let Some(target) = p.get("document").filter(|d| !d.is_null()) else { return self.dispatched(method, p) };
        let forward = self.session.bring_forward(target)?;
        let r = self.dispatched(method, p);
        self.session.put_back(forward);
        r
    }

    /// [`Self::dispatch`], noting the revision of the document a change acted on right after it
    /// ([`Backend::take_revision`]), before anything else can change it.
    fn dispatched(&mut self, method: &str, p: Value) -> Result<Value, Fail> {
        let r = self.dispatch(method, p);
        if matches!(method, "engine.execute" | "ui.pointer" | "ui.key" | "ui.type") {
            self.last_revision = self.session.ds().ok().map(|d| (d.uid, d.doc_revision()));
        }
        r
    }
}

impl Backend for Headless {
    /// With `document` (an `id` from `doc.list`, or an open file's path) the call acts on that
    /// file, and the file in front stays in front.
    fn call(&mut self, method: &str, p: Value) -> Result<Value, String> {
        let r = self.call_in_document(method, p);
        self.noted(r)
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "headless (in-process engine session; no window)".into()
    }

    fn take_error_detail(&mut self) -> Option<Value> {
        self.last_error.take()
    }

    fn take_revision(&mut self) -> Option<(u64, u64)> {
        self.last_revision.take()
    }

    fn render_png(&mut self, ids: &[u64], scale: f64) -> Result<Vec<u8>, String> {
        let r = self.render(ids, scale);
        self.noted(r)
    }

    fn screenshot_png(&mut self) -> Result<Vec<u8>, String> {
        let r = self.screenshot();
        self.noted(r)
    }
}
