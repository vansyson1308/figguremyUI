//! Where MCP tool calls end up: a control-channel method call, answered either by an in-process
//! engine session ([`crate::Headless`]) or by a running FigCraft app ([`Attached`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};

/// Something that answers the FigCraft control-channel methods (`engine.execute`,
/// `engine.commands`, `document.inspect`, `node.get`, `ui.pointer`, `ui.key`, `ui.type`,
/// `ui.render`, `ui.screenshot`, `ui.inspect`). See `docs/control-protocol.md`.
pub trait Backend {
    /// Call one method. `Ok` carries the `result`, `Err` the error message.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;

    /// True when a real UI window is attached (`ui.screenshot` captures the window).
    fn has_ui(&self) -> bool;

    /// Short human description ("headless", "attached to 127.0.0.1:7979").
    fn describe(&self) -> String;

    /// The machine-readable form of the error the last failed call returned, taken once:
    /// `{code, message, …}` as [`figcraft_engine::EngineError::detail`] builds it, with `message`
    /// the text that call's `Err` carried. `None` when the backend has nothing more than the text.
    fn take_error_detail(&mut self) -> Option<Value> {
        None
    }

    /// The revision of the document the last call acted on, read right after it in the same
    /// request, taken once: `(document id, revision)`. A command's reply carries it (the engine's
    /// `engine.execute`; headless, also a gesture, key or typed text), so a result can name the
    /// revision it belongs to without a second read that could include somebody else's change.
    /// `None` when the backend didn't say (an app from before this).
    fn take_revision(&mut self) -> Option<(u64, u64)> {
        None
    }

    /// What the app at the other end says about itself (`app.info`: `{instance, version, …}`),
    /// when there is one and it answers the question.
    fn app_info(&mut self) -> Option<Value> {
        None
    }

    /// PNG bytes of the document: `ids` (in page coordinates, their union) or, when empty, the
    /// current page's content. The default goes through `ui.render`, the PNG coming back as data.
    fn render_png(&mut self, ids: &[u64], scale: f64) -> Result<Vec<u8>, String> {
        let mut params = json!({"scale": scale});
        if !ids.is_empty() {
            params["ids"] = json!(ids);
        } else {
            // `ui.render` without ids exports the selection; pass the page's children instead.
            let kids = page_children(self)?;
            if kids.is_empty() {
                return Err("the current page is empty: nothing to render".into());
            }
            params["ids"] = json!(kids);
        }
        decode_data(&self.call("ui.render", params)?)
    }

    /// PNG bytes of what the user sees: the app window when attached, the current viewport when
    /// headless.
    fn screenshot_png(&mut self) -> Result<Vec<u8>, String> {
        capture(self, json!({}))
    }

    /// PNG bytes of the attached app's window `window` (an id from `window.list`).
    fn window_png(&mut self, window: u64) -> Result<Vec<u8>, String> {
        capture(self, json!({"window": window}))
    }
}

/// A window capture, the PNG coming back as data (no file: an app confined to automation roots
/// writes none outside them).
fn capture<B: Backend + ?Sized>(b: &mut B, params: Value) -> Result<Vec<u8>, String> {
    decode_data(&b.call("ui.screenshot", params)?)
}

/// The ids of the current page's top-level layers.
pub fn page_children<B: Backend + ?Sized>(b: &mut B) -> Result<Vec<u64>, String> {
    let doc = b.call("document.inspect", json!({"depth": 1}))?;
    let current = doc.get("currentPage").cloned().unwrap_or(Value::Null);
    let pages = doc.get("pages").and_then(Value::as_array).cloned().unwrap_or_default();
    let page = pages.iter().find(|p| p.get("id") == Some(&current)).or_else(|| pages.first());
    Ok(page
        .and_then(|p| p.get("children"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|c| c.get("id").and_then(Value::as_u64)).collect())
        .unwrap_or_default())
}

/// Decode a `data` field (base64, optionally a data: URL) from an export result.
pub fn decode_data(v: &Value) -> Result<Vec<u8>, String> {
    let d = v.get("data").and_then(Value::as_str).ok_or("the export returned no data")?;
    let d = d.split_once(',').map(|(_, b)| b).unwrap_or(d);
    base64::engine::general_purpose::STANDARD.decode(d.trim()).map_err(|e| format!("bad base64 from the app: {e}"))
}

/// The longest reply line we read from the app (512 MiB). The app's window captures alone reach
/// 256 MiB as base64 (192 MiB PNGs); anything longer is refused without reading the rest.
const MAX_REPLY: usize = 512 * 1024 * 1024;

/// An error as a program reads it: `{code, message}` (see `docs/mcp.md`, "Errors").
pub fn error_detail(code: &str, message: &str) -> Value {
    json!({"code": code, "message": message})
}

/// A running FigCraft app, reached through its loopback control port (`figcraft --control PORT`).
pub struct Attached {
    addr: String,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u64,
    /// [`MAX_REPLY`]; smaller in tests.
    max_reply: usize,
    /// How long the app is asked to wait for each request; `None`: its own 60 s.
    timeout: Option<Duration>,
    /// Ask each new connection which app it reached (`app.info`); off in tests of the wire alone.
    identify: bool,
    /// The connection in use was asked.
    identified: bool,
    /// What the app said about itself the last time it was asked (`None`: it doesn't know the
    /// question, as apps from before `app.info` don't).
    info: Option<Value>,
    /// The `instance` of the app this backend first talked to.
    instance: Option<String>,
    /// The detail of the last failed call ([`Backend::take_error_detail`]).
    last_error: Option<Value>,
    /// What the last reply said about the revision ([`Backend::take_revision`]).
    last_revision: Option<(u64, u64)>,
}

impl Attached {
    /// Connect to `addr` (e.g. `127.0.0.1:7979`), failing fast when nothing is listening.
    pub fn connect(addr: &str) -> std::io::Result<Self> {
        let mut r = Self {
            addr: addr.to_string(),
            conn: None,
            next_id: 1,
            max_reply: MAX_REPLY,
            timeout: None,
            identify: true,
            identified: false,
            info: None,
            instance: None,
            last_error: None,
            last_revision: None,
        };
        r.reconnect()?;
        Ok(r)
    }

    /// Ask the app to wait up to `timeout` for each request instead of its 60 s (it caps this; an
    /// app from before this ignores it and answers within its 60 s), and read its answers as long.
    pub fn with_timeout(mut self, timeout: Duration) -> std::io::Result<Self> {
        self.timeout = Some(timeout);
        self.reconnect()?;
        Ok(self)
    }

    /// Connect to `127.0.0.1:port`.
    pub fn port(port: u16) -> std::io::Result<Self> {
        Self::connect(&format!("127.0.0.1:{port}"))
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        self.conn = None;
        self.identified = false;
        let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, format!("cannot resolve {}", self.addr));
        for sa in self.addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, Duration::from_millis(800)) {
                Ok(s) => {
                    s.set_nodelay(true).ok();
                    // The app answers within its timeout; leave headroom.
                    s.set_read_timeout(Some(self.timeout.unwrap_or(Duration::from_secs(60)).saturating_add(Duration::from_secs(30)))).ok();
                    let read = s.try_clone()?;
                    self.conn = Some((BufReader::new(read), s));
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The app already closed this connection (it drops ones idle past its socket timeout), or
    /// left bytes on it nobody asked for. Checked before a request is written, when replacing the
    /// connection can't repeat anything.
    fn stale(&mut self) -> bool {
        let Some((reader, writer)) = self.conn.as_mut() else { return false };
        if !reader.buffer().is_empty() || writer.set_nonblocking(true).is_err() {
            return true;
        }
        // Nothing to read on a healthy idle connection: the peek would block.
        let idle = matches!(writer.peek(&mut [0]), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock);
        writer.set_nonblocking(false).is_err() || !idle
    }

    fn roundtrip(&mut self, line: &str) -> std::io::Result<String> {
        let max_reply = self.max_reply;
        let Some((reader, writer)) = self.conn.as_mut() else {
            return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "not connected to the app"));
        };
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        let mut reply = Vec::new();
        if reader.take((max_reply + 1) as u64).read_until(b'\n', &mut reply)? == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "control channel closed"));
        }
        if reply.last() == Some(&b'\n') {
            reply.pop();
        }
        if reply.len() > max_reply {
            let mib = max_reply / (1024 * 1024);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("reply larger than {mib} MiB")));
        }
        String::from_utf8(reply).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// A failure of the connection itself, as text and as its detail (code `transport`).
fn transport(message: String) -> (String, Value) {
    let detail = error_detail("transport", &message);
    (message, detail)
}

impl Attached {
    /// Ask the app on this connection which app it is (`app.info` → `{instance, …}`, `instance`
    /// made when the app started). An app that doesn't know the method is taken as it always was:
    /// nothing is known about it, and nothing is checked. `Err`: the app is not the one this
    /// backend first talked to (it was restarted), or the question got no answer.
    fn identify(&mut self) -> Result<(), (String, Value)> {
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({"id": id, "method": "app.info", "params": {}}).to_string();
        let reply = self.roundtrip(&line).map_err(|e| {
            self.conn = None;
            transport(format!("FigCraft app at {} did not say which app it is: {e} (the request was not sent)", self.addr))
        })?;
        self.identified = true;
        let v: Value = serde_json::from_str(reply.trim()).unwrap_or(Value::Null);
        let answered = v.get("ok").and_then(Value::as_bool) == Some(true);
        self.info = v.get("result").filter(|r| answered && r.is_object()).cloned();
        let Some(now) = self.info.as_ref().and_then(|i| i.get("instance")).and_then(Value::as_str).map(str::to_string) else { return Ok(()) };
        match self.instance.replace(now.clone()) {
            Some(first) if first != now => {
                let (message, mut detail) = transport(format!(
                    "the FigCraft app at {} was restarted: this is another run of the app than the one this server talked to before, so the \
                     document ids, node ids and revisions from earlier calls are stale. The request was not sent. Look again (get_capabilities, \
                     inspect_document) before going on",
                    self.addr
                ));
                detail["restarted"] = json!(true);
                Err((message, detail))
            }
            _ => Ok(()),
        }
    }

    /// One request, its failure as text and as its detail.
    fn request(&mut self, method: &str, params: Value) -> Result<Value, (String, Value)> {
        self.last_revision = None;
        let params_document = params.get("document").and_then(Value::as_u64);
        let id = self.next_id;
        self.next_id += 1;
        let mut line = json!({"id": id, "method": method, "params": params});
        if let (Some(t), Some(fields)) = (self.timeout, line.as_object_mut()) {
            fields.insert("timeoutMs".into(), json!(u64::try_from(t.as_millis()).unwrap_or(u64::MAX)));
        }
        let line = line.to_string();
        if self.conn.is_none() || self.stale() {
            self.reconnect().map_err(|e| {
                transport(format!(
                    "FigCraft app at {} is not reachable: {e} (request was not sent; start it with `figcraft --control PORT`)",
                    self.addr
                ))
            })?;
        }
        // A new connection may be to another run of the app: ask before anything is sent to it.
        if self.identify && !self.identified {
            self.identify()?;
        }
        // A lost reply does not mean the app failed to apply the request.
        let reply = self.roundtrip(&line).map_err(|e| {
            self.conn = None;
            transport(format!(
                "FigCraft control call `{method}` at {} failed: {e}; the operation may have completed. Inspect the document before retrying.",
                self.addr
            ))
        })?;
        let v: Value = serde_json::from_str(reply.trim()).map_err(|e| transport(format!("bad reply from the app: {e}")))?;
        // The app writes the revision of the document the command acted on next to the result:
        // the document the request named (`revisionDocument` when the app says which one).
        let said = v.get("revisionDocument").and_then(Value::as_u64).or(params_document);
        self.last_revision = v.get("revision").and_then(Value::as_u64).zip(said).map(|(rev, doc)| (doc, rev));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
        let message = v.get("error").and_then(Value::as_str).unwrap_or("unknown error").to_string();
        // An app that says what the error is (`errorDetail`) is believed; one from before that
        // gives only the text.
        let known = v.get("errorDetail").filter(|d| d.get("code").is_some_and(Value::is_string));
        let mut detail = known.cloned().unwrap_or_else(|| error_detail("error", &message));
        detail["message"] = json!(message);
        Err((message, detail))
    }
}

impl Backend for Attached {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.last_error = None;
        self.request(method, params).map_err(|(message, detail)| {
            self.last_error = Some(detail);
            message
        })
    }

    fn take_error_detail(&mut self) -> Option<Value> {
        self.last_error.take()
    }

    fn take_revision(&mut self) -> Option<(u64, u64)> {
        self.last_revision.take()
    }

    fn app_info(&mut self) -> Option<Value> {
        self.info.clone()
    }

    fn has_ui(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        format!("attached to the FigCraft app at {}", self.addr)
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
