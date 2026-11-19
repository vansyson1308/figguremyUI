//! `figguremyui-cli doctor`: checks that this install works and prints how to connect an AI agent.
//!
//! The MCP self-test drives a [`figcraft_mcp::Server`] over a headless session in this process,
//! one JSON-RPC line at a time through [`figcraft_mcp::Server::serve`], the loop `mcp` runs on
//! stdio. Nothing is written to disk (the probe's documents stay in memory) and nothing reaches
//! the network: the attached mode's lines are printed, never tried.
//!
//! `--json` prints one object. Its keys are stable within [`SCHEMA`] and documented in
//! `docs/cli.md`; a key is only ever added.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::args::DoctorArgs;

/// The version of the `--json` object's layout.
pub const SCHEMA: u64 = 1;
/// The name `initialize` must answer in `serverInfo.name`.
const SERVER_NAME: &str = "figguremyui";
/// The desktop app's binary, which `mcp --attach` drives.
const APP: &str = "figguremyui";
/// This binary's own name, for the setup lines when its path can't be found.
const CLI: &str = "figguremyui-cli";

/// One check: what was tried, whether it held, and what to do when it didn't.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub id: &'static str,
    pub ok: bool,
    pub detail: String,
    pub fix: Option<String>,
}

impl Check {
    fn pass(id: &'static str, detail: impl Into<String>) -> Self {
        Self { id, ok: true, detail: detail.into(), fix: None }
    }
    fn fail(id: &'static str, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self { id, ok: false, detail: detail.into(), fix: Some(fix.into()) }
    }
    fn to_json(&self) -> Value {
        json!({"id": self.id, "ok": self.ok, "detail": self.detail, "fix": self.fix})
    }
}

/// What the run found, before it is printed.
pub struct Report {
    pub checks: Vec<Check>,
    pub warnings: Vec<String>,
    pub binary: Option<PathBuf>,
    pub app: Option<PathBuf>,
    pub mcp: Value,
    pub fonts: Value,
    pub example: Value,
    pub port: u16,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

pub fn run(a: &DoctorArgs) -> ExitCode {
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| e.to_string());
    let report = examine(exe, a.example.as_deref(), a.port);
    let text = if a.json { serde_json::to_string_pretty(&to_json(&report)).unwrap_or_default() } else { human(&report) };
    println!("{text}");
    if report.ok() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

/// Runs every check. `exe` is this binary's canonical path (or why it couldn't be found).
pub fn examine(exe: Result<PathBuf, String>, example: Option<&str>, port: u16) -> Report {
    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let (binary, app) = match exe {
        Ok(path) => {
            checks.push(Check::pass("binary", display_path(&path)));
            let app = path.with_file_name(format!("{APP}{}", std::env::consts::EXE_SUFFIX));
            (Some(path), app.is_file().then_some(app))
        }
        Err(e) => {
            checks.push(Check::fail(
                "binary",
                format!("the path of this binary can't be read: {e}"),
                "run the binary by its full path (for example target/release/figguremyui-cli doctor)",
            ));
            (None, None)
        }
    };
    if binary.is_some() && app.is_none() {
        warnings.push(format!(
            "the desktop app ({APP}{}) is not next to this binary: headless MCP works without it; for attached mode build it with `cargo build --release -p figcraft`",
            std::env::consts::EXE_SUFFIX
        ));
    }
    let mut probe = Probe::new();
    let mcp = self_test(&mut probe, &mut checks);
    let example = match example {
        Some(path) => run_example(&mut probe, path, &mut checks),
        None => Value::Null,
    };
    let fonts = fonts(&mut checks, &mut warnings);
    Report { checks, warnings, binary, app, mcp, fonts, example, port }
}

/// An MCP client in this process: each request is one line through the server's stdio loop.
struct Probe {
    server: figcraft_mcp::Server,
    next: u64,
}

impl Probe {
    fn new() -> Self {
        Self { server: figcraft_mcp::Server::new(Box::new(figcraft_mcp::Headless::new())), next: 0 }
    }

    /// The server's reply lines to one message.
    fn send(&mut self, msg: &Value) -> Result<Vec<Value>, String> {
        let line = format!("{msg}\n");
        let mut out = Vec::new();
        self.server.serve(line.as_bytes(), &mut out).map_err(|e| format!("the server's stdio loop failed: {e}"))?;
        String::from_utf8_lossy(&out)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| format!("the server wrote a line that isn't JSON ({e}): {l}")))
            .collect()
    }

    /// A request's `result`, or its error message.
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let replies = self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let reply = replies.into_iter().find(|r| r.get("id") == Some(&json!(id))).ok_or(format!("no reply to `{method}`"))?;
        if let Some(e) = reply.get("error") {
            return Err(format!("`{method}` failed: {}", e.get("message").and_then(Value::as_str).unwrap_or("no message")));
        }
        reply.get("result").cloned().ok_or(format!("the reply to `{method}` has no result"))
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        let replies = self.send(&json!({"jsonrpc": "2.0", "method": method}))?;
        match replies.is_empty() {
            true => Ok(()),
            false => Err(format!("the server answered the notification `{method}`")),
        }
    }

    /// A tool call's result (`content`, `isError`); a tool error is an `Err` with its text.
    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let r = self.request("tools/call", json!({"name": name, "arguments": arguments}))?;
        if r.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(format!("{name}: {}", text_block(&r, 0).unwrap_or("failed without a message")));
        }
        Ok(r)
    }

    /// A tool's JSON answer (its first text block).
    fn tool_json(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let r = self.tool(name, arguments)?;
        let text = text_block(&r, 0).ok_or(format!("{name} answered no text"))?;
        serde_json::from_str(text).map_err(|e| format!("{name} answered text that isn't JSON: {e}"))
    }
}

/// The text of a tool result's content block `i`.
fn text_block(r: &Value, i: usize) -> Option<&str> {
    r.get("content")?.get(i)?.get("text")?.as_str()
}

/// What to tell someone whose MCP check failed: the server itself is broken, so rebuilding is all
/// they can do, and the failure is worth reporting.
const MCP_FIX: &str = "rebuild from a clean checkout (git pull, then cargo build --release -p figcraft-cli) and run doctor again; if it still fails, report it with this output";

/// The handshake and a small prototype checked end to end: the calls an agent makes first.
fn self_test(p: &mut Probe, checks: &mut Vec<Check>) -> Value {
    let mut mcp = json!({"serverName": null, "protocolVersion": null, "tools": null, "backend": null, "commands": null});
    let init = p.request(
        "initialize",
        json!({"protocolVersion": figcraft_mcp::PROTOCOL_VERSION, "capabilities": {}, "clientInfo": {"name": "figguremyui-cli doctor", "version": env!("CARGO_PKG_VERSION")}}),
    );
    match init {
        Ok(r) => {
            let name = r.pointer("/serverInfo/name").and_then(Value::as_str).unwrap_or_default().to_string();
            mcp["serverName"] = json!(name);
            mcp["protocolVersion"] = r.get("protocolVersion").cloned().unwrap_or(Value::Null);
            checks.push(match name == SERVER_NAME {
                true => {
                    Check::pass("mcp.initialize", format!("serverInfo.name \"{name}\", protocol {}", mcp["protocolVersion"].as_str().unwrap_or("?")))
                }
                false => Check::fail("mcp.initialize", format!("serverInfo.name is \"{name}\", not \"{SERVER_NAME}\""), MCP_FIX),
            });
        }
        Err(e) => checks.push(Check::fail("mcp.initialize", e, MCP_FIX)),
    }
    if let Err(e) = p.notify("notifications/initialized") {
        checks.push(Check::fail("mcp.initialized", e, MCP_FIX));
    }
    match p.request("tools/list", json!({})) {
        Ok(r) => {
            let n = r.get("tools").and_then(Value::as_array).map_or(0, Vec::len);
            mcp["tools"] = json!(n);
            checks.push(match n > 0 {
                true => Check::pass("mcp.tools", format!("{n} tools")),
                false => Check::fail("mcp.tools", "tools/list answered no tools", MCP_FIX),
            });
        }
        Err(e) => checks.push(Check::fail("mcp.tools", e, MCP_FIX)),
    }
    match p.tool_json("get_capabilities", json!({})) {
        Ok(c) => {
            mcp["backend"] = c.get("backend").cloned().unwrap_or(Value::Null);
            mcp["commands"] = c.get("commands").cloned().unwrap_or(Value::Null);
            let commands = c.get("commands").and_then(Value::as_u64).unwrap_or(0);
            let flow = c.pointer("/features/flowCheck").and_then(Value::as_bool) == Some(true);
            let scenarios = c.pointer("/features/scenarios").and_then(Value::as_bool) == Some(true);
            checks.push(match commands > 0 && flow && scenarios {
                true => Check::pass(
                    "mcp.capabilities",
                    format!("{} backend, {commands} commands, flow check and scenarios", mcp["backend"].as_str().unwrap_or("?")),
                ),
                false => Check::fail(
                    "mcp.capabilities",
                    format!("get_capabilities: {commands} commands, flowCheck {flow}, scenarios {scenarios}"),
                    MCP_FIX,
                ),
            });
        }
        Err(e) => checks.push(Check::fail("mcp.capabilities", e, MCP_FIX)),
    }
    prototype(p, checks);
    mcp
}

/// Builds a two-screen prototype in the untitled document (in memory), looks at it and verifies it
/// the way an agent would: `screenshot`, `check_prototype`, `run_scenarios`.
fn prototype(p: &mut Probe, checks: &mut Vec<Check>) {
    let step = |command: &str, name: Option<&str>, params: Value| {
        let mut s = json!({"command": command, "params": params});
        if let Some(n) = name {
            s["as"] = json!(n);
        }
        s
    };
    let r = |name: &str| json!({"$ref": name});
    let commands = json!([
        step(
            "node.create",
            Some("home"),
            json!({"type": "frame", "name": "Home", "x": 0, "y": 0, "width": 240, "height": 160, "props": {"fill": "#FFFFFF"}})
        ),
        step(
            "node.create",
            Some("done"),
            json!({"type": "frame", "name": "Done", "x": 300, "y": 0, "width": 240, "height": 160, "props": {"fill": "#FFFFFF"}})
        ),
        step(
            "node.create",
            Some("go"),
            json!({"type": "rectangle", "name": "Continue", "parent": r("home"), "x": 20, "y": 100, "width": 200, "height": 40, "props": {"fill": "#2F6FEB", "cornerRadius": 8}}),
        ),
        step(
            "node.create",
            Some("back"),
            json!({"type": "rectangle", "name": "Back", "parent": r("done"), "x": 20, "y": 100, "width": 200, "height": 40, "props": {"fill": "#5A5A5A"}})
        ),
        step("proto.interaction.add", None, json!({"id": r("go"), "dest": r("done")})),
        step("proto.interaction.add", None, json!({"id": r("back"), "action": {"type": "back"}})),
        step(
            "proto.scenario.add",
            None,
            json!({"name": "Continue opens Done, Back returns", "start": r("home"), "steps": [{"click": r("go"), "expect": {"screen": r("done")}}, {"click": r("back")}], "expect": {"screen": r("home")}, "purpose": "doctor self-test"}),
        ),
    ]);
    let built = p.tool_json("batch", json!({"label": "doctor self-test", "commands": commands}));
    let home = match built {
        Ok(v) => match v.get(0).and_then(|s| s.get("id")).and_then(Value::as_u64) {
            Some(home) => {
                checks.push(Check::pass("mcp.document", "built a two-screen prototype with batch (in memory, not saved)"));
                home
            }
            None => {
                checks.push(Check::fail("mcp.document", format!("batch answered no frame id: {v}"), MCP_FIX));
                return;
            }
        },
        Err(e) => {
            checks.push(Check::fail("mcp.document", e, MCP_FIX));
            return;
        }
    };
    checks.push(match p.tool("screenshot", json!({"ids": [home]})) {
        Ok(r) => {
            let image = r.pointer("/content/0/type").and_then(Value::as_str) == Some("image")
                && r.pointer("/content/0/data").and_then(Value::as_str).is_some_and(|d| !d.is_empty());
            let size: Option<Value> = text_block(&r, 1).and_then(|t| serde_json::from_str(t).ok());
            let (w, h) = size.as_ref().map_or((0, 0), |s| (s["width"].as_u64().unwrap_or(0), s["height"].as_u64().unwrap_or(0)));
            match image && w > 0 && h > 0 {
                true => Check::pass("mcp.screenshot", format!("rendered a {w}×{h} PNG")),
                false => Check::fail("mcp.screenshot", "screenshot returned no image", MCP_FIX),
            }
        }
        Err(e) => Check::fail("mcp.screenshot", e, MCP_FIX),
    });
    checks.push(match p.tool_json("check_prototype", json!({})) {
        Ok(c) if c.get("ok").and_then(Value::as_bool) == Some(true) => {
            let screens = c.get("screens").and_then(Value::as_array).map_or(0, Vec::len);
            Check::pass("mcp.check_prototype", format!("{screens} screens, no blocker or major problem"))
        }
        Ok(c) => Check::fail("mcp.check_prototype", format!("the self-test prototype has problems: {}", problems(&c)), MCP_FIX),
        Err(e) => Check::fail("mcp.check_prototype", e, MCP_FIX),
    });
    checks.push(match p.tool_json("run_scenarios", json!({})) {
        Ok(r) => {
            let (total, failed) = counts(&r);
            match total > 0 && failed == 0 {
                true => Check::pass("mcp.run_scenarios", format!("{} of {total} scenarios passed", total - failed)),
                false => Check::fail("mcp.run_scenarios", format!("{failed} of {total} scenarios failed: {}", failures(&r)), MCP_FIX),
            }
        }
        Err(e) => Check::fail("mcp.run_scenarios", e, MCP_FIX),
    });
}

/// `run_scenarios`' totals: (total, failed).
fn counts(r: &Value) -> (u64, u64) {
    (r.get("total").and_then(Value::as_u64).unwrap_or(0), r.get("failed").and_then(Value::as_u64).unwrap_or(0))
}

/// The failed scenarios, by id and name.
fn failures(r: &Value) -> String {
    let failed: Vec<String> = r
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| s.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|s| format!("{} \"{}\"", s["id"].as_str().unwrap_or("?"), s["name"].as_str().unwrap_or("?")))
        .collect();
    if failed.is_empty() { "none named".into() } else { failed.join(", ") }
}

/// The first few problems `check_prototype` found.
fn problems(c: &Value) -> String {
    let all = c.get("problems").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut shown: Vec<String> = all
        .iter()
        .take(3)
        .map(|p| {
            format!("{} ({}): {}", p["rule"].as_str().unwrap_or("?"), p["severity"].as_str().unwrap_or("?"), p["message"].as_str().unwrap_or(""))
        })
        .collect();
    if all.len() > shown.len() {
        shown.push(format!("and {} more", all.len() - shown.len()));
    }
    if shown.is_empty() { "none listed".into() } else { shown.join("; ") }
}

/// Opens `path` and runs its stored scenarios and the flow check.
fn run_example(p: &mut Probe, path: &str, checks: &mut Vec<Check>) -> Value {
    let full = std::fs::canonicalize(path).map(|f| display_path(&f)).unwrap_or_else(|_| path.to_string());
    let mut out = json!({"path": full, "opened": false, "scenarios": null, "prototypeOk": null, "problems": null});
    if let Err(e) = p.tool_json("open_file", json!({"path": full})) {
        checks.push(Check::fail(
            "example.open",
            e,
            "give --example the path of a .figcraft file that exists, for example examples/signup-flow.figcraft from the repository's root",
        ));
        return out;
    }
    out["opened"] = json!(true);
    checks.push(Check::pass("example.open", full.clone()));
    let damaged = "the file is not as shipped: restore it (git checkout -- <file>) or check that it is the example you meant";
    match p.tool_json("run_scenarios", json!({})) {
        Ok(r) => {
            let (total, failed) = counts(&r);
            out["scenarios"] = json!({"total": total, "passed": total - failed.min(total), "failed": failed});
            checks.push(match (total, failed) {
                (0, _) => Check::fail("example.scenarios", "the file stores no scenarios, so nothing was run (an empty run is not a pass)", damaged),
                (t, 0) => Check::pass("example.scenarios", format!("{t} of {t} scenarios passed")),
                (t, f) => Check::fail("example.scenarios", format!("{f} of {t} scenarios failed: {}", failures(&r)), damaged),
            });
        }
        Err(e) => checks.push(Check::fail("example.scenarios", e, damaged)),
    }
    match p.tool_json("check_prototype", json!({})) {
        Ok(c) => {
            let ok = c.get("ok").and_then(Value::as_bool) == Some(true);
            let n = c.get("problems").and_then(Value::as_array).map_or(0, Vec::len);
            out["prototypeOk"] = json!(ok);
            out["problems"] = json!(n);
            checks.push(match ok {
                true => Check::pass("example.check_prototype", format!("no blocker or major problem ({n} minor)")),
                false => Check::fail("example.check_prototype", problems(&c), damaged),
            });
        }
        Err(e) => checks.push(Check::fail("example.check_prototype", e, damaged)),
    }
    out
}

/// The fonts text can lay out with: built in (craft-fonts at build time) and installed.
fn fonts(checks: &mut Vec<Check>, warnings: &mut Vec<String>) -> Value {
    // Indexes the installed fonts now unless this run opted out (`--no-system-fonts`).
    figcraft_text::scan_system_fonts();
    let families = figcraft_text::family_names();
    let scanned = figcraft_text::system_fonts_scanned();
    let mut embedded: Vec<String> = figcraft_text::CRAFT_FONTS.iter().map(|f| f.family.to_string()).collect();
    embedded.sort_by_key(|f| f.to_lowercase());
    embedded.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let installed = families.iter().filter(|f| !embedded.iter().any(|e| e.eq_ignore_ascii_case(f))).count();
    let inter = figcraft_text::has_family("Inter");
    let system = if scanned { "scanned" } else { "optedOut" };
    checks.push(match families.is_empty() {
        false => Check::pass(
            "fonts",
            format!(
                "{} families: {} built in, {installed} installed{}",
                families.len(),
                embedded.len(),
                if scanned { "" } else { " (installed fonts not read: --no-system-fonts or FIGGUREMYUI_NO_SYSTEM_FONTS)" }
            ),
        ),
        true => Check::fail(
            "fonts",
            match scanned {
                true => "no fonts: none were built in and none are installed, so text is laid out 1 px wide and draws nothing",
                false => "no fonts: none were built in and the installed fonts were not read (--no-system-fonts or FIGGUREMYUI_NO_SYSTEM_FONTS)",
            },
            "install any font (on Debian/Ubuntu: sudo apt-get install fonts-dejavu-core), or clone https://github.com/storytold/craft-fonts next to the repository and rebuild to build fonts in",
        ),
    });
    if !inter && !families.is_empty() {
        warnings.push("Inter is not installed: the examples are set in Inter and fall back to another font (https://rsms.me/inter/)".into());
    }
    json!({"system": system, "families": families.len(), "builtIn": embedded.len(), "installed": installed, "inter": inter})
}

/// `p` as people write it: Windows' `\\?\` prefix from canonicalizing is taken off.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => s.into_owned(),
    }
}

/// `word` quoted for a shell command line: POSIX shells, or PowerShell and cmd on Windows.
pub fn quote(word: &str, windows: bool) -> String {
    if windows {
        let plain = !word.is_empty() && !word.chars().any(|c| c.is_whitespace() || "\"&|<>^%;,()'`$@{}".contains(c));
        return if plain { word.to_string() } else { format!("\"{word}\"") };
    }
    let plain = !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%=,".contains(c));
    if plain { word.to_string() } else { format!("'{}'", word.replace('\'', r"'\''")) }
}

/// The client setup for both modes, with this binary's path.
fn setup(r: &Report) -> Value {
    let windows = cfg!(windows);
    let command = r.binary.as_deref().map_or_else(|| CLI.to_string(), display_path);
    let app = r.app.as_deref().map_or_else(|| format!("{APP}{}", std::env::consts::EXE_SUFFIX), display_path);
    let port = r.port.to_string();
    let mode = |name: &str, args: &[&str]| {
        let line: Vec<String> = std::iter::once(quote(&command, windows)).chain(args.iter().map(|a| quote(a, windows))).collect();
        let line = line.join(" ");
        json!({
            "name": name,
            "args": args,
            "claudeCode": format!("claude mcp add --transport stdio {name} -- {line}"),
            "codex": format!("codex mcp add {name} -- {line}"),
            "mcpServers": {"mcpServers": {name: {"command": command, "args": args}}},
        })
    };
    let mut attached = mode("figguremyui-app", &["mcp", "--attach", &port]);
    attached["port"] = json!(r.port);
    attached["startApp"] = json!(format!("{} --control {port}", quote(&app, windows)));
    json!({"command": command, "headless": mode("figguremyui", &["mcp"]), "attached": attached})
}

pub fn to_json(r: &Report) -> Value {
    json!({
        "schema": SCHEMA,
        "ok": r.ok(),
        "product": figcraft_mcp::PRODUCT_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "binary": {
            "path": r.binary.as_deref().map(display_path),
            "app": r.app.as_deref().map(display_path),
            "appFound": r.app.is_some(),
        },
        "mcp": r.mcp,
        "example": r.example,
        "fonts": r.fonts,
        "checks": r.checks.iter().map(Check::to_json).collect::<Vec<_>>(),
        "warnings": r.warnings,
        "setup": setup(r),
        "wroteFiles": false,
        "usedNetwork": false,
    })
}

/// The report for people (and agents reading it as text).
pub fn human(r: &Report) -> String {
    let s = setup(r);
    let get = |p: &str| s.pointer(p).and_then(Value::as_str).unwrap_or_default().to_string();
    // One line, `command` before `args` as people write it (the object form sorts its keys).
    let snippet = |mode: &str| {
        let text = |p: &str| serde_json::to_string(s.pointer(&format!("/{mode}/{p}")).unwrap_or(&Value::Null)).unwrap_or_default();
        format!(
            r#"{{"mcpServers": {{{}: {{"command": {}, "args": {}}}}}}}"#,
            text("name"),
            serde_json::to_string(&s["command"]).unwrap_or_default(),
            text("args")
        )
    };
    let mut o = String::new();
    let mut line = |t: &str| {
        o.push_str(t);
        o.push('\n');
    };
    line(&format!("{} {} doctor", figcraft_mcp::PRODUCT_NAME, env!("CARGO_PKG_VERSION")));
    line("");
    line(&format!("binary       {}", r.binary.as_deref().map_or_else(|| "(unknown)".to_string(), display_path)));
    line(&format!(
        "desktop app  {}",
        r.app.as_deref().map_or_else(|| format!("not found next to it ({APP}{})", std::env::consts::EXE_SUFFIX), display_path)
    ));
    line("");
    line("Checks");
    let width = r.checks.iter().map(|c| c.id.len()).max().unwrap_or(0);
    for c in &r.checks {
        line(&format!("  {}  {:width$}  {}", if c.ok { "ok  " } else { "FAIL" }, c.id, c.detail));
        if let Some(fix) = &c.fix {
            line(&format!("        {:width$}  fix: {fix}", ""));
        }
    }
    if !r.warnings.is_empty() {
        line("");
        line("Warnings");
        for w in &r.warnings {
            line(&format!("  - {w}"));
        }
    }
    line("");
    line("Connect a client, headless (an engine session of the agent's own, no window):");
    line("  Claude Code:");
    line(&format!("    {}", get("/headless/claudeCode")));
    line("  Codex:");
    line(&format!("    {}", get("/headless/codex")));
    line("  JSON (a project's .mcp.json, claude_desktop_config.json; merge into an existing \"mcpServers\"):");
    line(&format!("    {}", snippet("headless")));
    line("");
    line("Or attached to the running app, so a person sees every change (start the app first):");
    line(&format!("    {}", get("/attached/startApp")));
    line("  Claude Code:");
    line(&format!("    {}", get("/attached/claudeCode")));
    line("  Codex:");
    line(&format!("    {}", get("/attached/codex")));
    line("  JSON:");
    line(&format!("    {}", snippet("attached")));
    line("");
    line("Confirm with `claude mcp list` or `codex mcp list`. doctor wrote no files and used no network.");
    line("");
    let failed = r.checks.iter().filter(|c| !c.ok).count();
    match failed {
        0 => line(&format!("Result: all {} checks passed.", r.checks.len())),
        n => line(&format!("Result: {n} of {} checks failed; see each fix above.", r.checks.len())),
    }
    o.trim_end().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn quoting_keeps_plain_paths_and_quotes_the_rest() {
        assert_eq!(quote("/home/a/target/release/figguremyui-cli", false), "/home/a/target/release/figguremyui-cli");
        assert_eq!(quote("/Users/a b/figguremyui-cli", false), "'/Users/a b/figguremyui-cli'");
        assert_eq!(quote("/tmp/it's", false), r"'/tmp/it'\''s'");
        assert_eq!(quote("", false), "''");
        assert_eq!(quote(r"C:\src\figguremyUI\target\release\figguremyui-cli.exe", true), r"C:\src\figguremyUI\target\release\figguremyui-cli.exe");
        assert_eq!(quote(r"C:\Users\A B\figguremyui-cli.exe", true), r#""C:\Users\A B\figguremyui-cli.exe""#);
        assert_eq!(quote("7979", true), "7979");
    }

    #[test]
    fn windows_verbatim_prefixes_are_taken_off() {
        assert_eq!(display_path(Path::new(r"\\?\C:\src\figguremyui-cli.exe")), r"C:\src\figguremyui-cli.exe");
        assert_eq!(display_path(Path::new(r"\\?\UNC\server\share\figguremyui-cli.exe")), r"\\server\share\figguremyui-cli.exe");
        assert_eq!(display_path(Path::new("/usr/local/bin/figguremyui-cli")), "/usr/local/bin/figguremyui-cli");
    }

    fn exe() -> PathBuf {
        std::env::temp_dir().join("doctor-test").join("figguremyui-cli")
    }

    #[test]
    fn a_working_build_passes_and_prints_its_own_path() {
        let r = examine(Ok(exe()), None, 7979);
        assert!(r.ok(), "{:?}", r.checks.iter().filter(|c| !c.ok).collect::<Vec<_>>());
        let ids: Vec<&str> = r.checks.iter().map(|c| c.id).collect();
        for id in [
            "binary",
            "mcp.initialize",
            "mcp.tools",
            "mcp.capabilities",
            "mcp.document",
            "mcp.screenshot",
            "mcp.check_prototype",
            "mcp.run_scenarios",
        ] {
            assert!(ids.contains(&id), "{id} missing from {ids:?}");
        }
        let v = to_json(&r);
        let path = display_path(&exe());
        assert_eq!(
            v["setup"]["headless"]["claudeCode"],
            format!("claude mcp add --transport stdio figguremyui -- {} mcp", quote(&path, cfg!(windows)))
        );
        assert_eq!(v["setup"]["headless"]["mcpServers"]["mcpServers"]["figguremyui"]["command"], path);
        assert_eq!(v["setup"]["attached"]["args"], json!(["mcp", "--attach", "7979"]));
        assert_eq!(v["mcp"]["serverName"], "figguremyui");
        assert_eq!(v["binary"]["appFound"], false);
        assert!(r.warnings.iter().any(|w| w.contains("desktop app")), "{:?}", r.warnings);
        let text = human(&r);
        assert!(text.contains("Result: all") && text.contains(&format!("codex mcp add figguremyui -- {}", quote(&path, cfg!(windows)))), "{text}");
        // The pasteable JSON lines are the setup's `mcpServers` objects.
        let snippets: Vec<Value> =
            text.lines().map(str::trim).filter(|l| l.starts_with(r#"{"mcpServers""#)).map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(snippets, [v["setup"]["headless"]["mcpServers"].clone(), v["setup"]["attached"]["mcpServers"].clone()]);
    }

    #[test]
    fn a_missing_binary_path_and_a_missing_example_fail_with_a_fix() {
        let missing = std::env::temp_dir().join("figguremyui-doctor-no-such-file.figcraft");
        let r = examine(Err("gone".into()), Some(missing.to_str().unwrap()), 1);
        assert!(!r.ok());
        let failed: Vec<&Check> = r.checks.iter().filter(|c| !c.ok).collect();
        assert_eq!(failed.iter().map(|c| c.id).collect::<Vec<_>>(), ["binary", "example.open"]);
        assert!(failed.iter().all(|c| c.fix.as_deref().is_some_and(|f| !f.is_empty())));
        let v = to_json(&r);
        assert_eq!(v["ok"], false);
        assert_eq!(v["setup"]["command"], "figguremyui-cli");
        assert!(human(&r).contains("Result: 2 of"), "{}", human(&r));
    }
}
