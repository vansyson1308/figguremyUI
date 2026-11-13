//! The example prototypes in `examples/` (built by `examples/build/build_examples.py`): each one
//! opens, its flow check finds nothing serious, its stored scenarios pass, the design check at the
//! file's stage finds nothing, and all of it survives a save and a reopen. The dashboard holds its
//! layout at four widths in each of its states, a missing font is named without breaking layout,
//! and the end states the builder (an AI agent) declared stand only because the maintainers'
//! acceptance step accepted them.
//!
//! Expectations are pinned exactly: when an example changes on purpose, rebuild it with the
//! script and update the lists here in the same change.

use std::path::PathBuf;

use crate::common::{Client, png_dims};
use serde_json::{Value, json};

/// The builder's agent name, as the MCP server records it (`clientInfo.title` + " (agent)").
const BUILDER: &str = "Example builder (agent)";

/// The rules `check_design` skips at `converging` (they run from `ready`).
const SKIPPED_AT_CONVERGING: [&str; 5] =
    ["context.briefIncomplete", "context.untestedAssumption", "context.staleDecision", "scenario.none", "scenario.unvisited"];

struct Example {
    file: &'static str,
    /// Stored scenarios: (id, name), all of which pass.
    scenarios: &'static [(&'static str, &'static str)],
    /// Top-level frames the flow check lists as screens: (name, reachable, accepted end state).
    screens: &'static [(&'static str, bool, bool)],
    /// The frame names of the accepted end states.
    end_states: &'static [&'static str],
}

const EXAMPLES: [Example; 3] = [
    Example {
        file: "signup-flow",
        scenarios: &[
            ("S1", "Happy path"),
            ("S2", "Missing contact shows an error"),
            ("S3", "Back keeps what was typed"),
            ("S4", "Resend toast closes by itself"),
            ("S5", "Continue explains itself while consent is off"),
        ],
        screens: &[
            ("Welcome", true, false),
            ("Sign-up form", true, false),
            ("Verify code", true, false),
            ("Done", true, true),
            ("Toast: code sent again", false, false),
            ("Button", false, false),
            ("Text button", false, false),
            ("Field", false, false),
            ("Checkbox", false, false),
            ("Segment", false, false),
        ],
        end_states: &["Done"],
    },
    Example {
        file: "pantry-dashboard",
        scenarios: &[
            ("S1", "Load, then ready"),
            ("S2", "Filter to empty and clear"),
            ("S3", "Error, retry, ready"),
            ("S4", "Row detail closes on an outside click"),
            ("S5", "Row detail closes with its button"),
        ],
        screens: &[
            ("Pantry dashboard", true, false),
            ("Sync toast", false, false),
            ("Item detail", false, false),
            ("Button", false, false),
            ("FilterChip", false, false),
            ("Toggle", false, false),
            ("KpiCard", false, false),
            ("StockRow", false, false),
        ],
        end_states: &[],
    },
    Example {
        file: "bike-repair-booking",
        scenarios: &[
            ("S1", "Book a tune-up"),
            ("S2", "Change the service from Confirm"),
            ("S3", "Back from Details keeps the service highlighted"),
            ("S4", "Booked is where the flow ends"),
            ("S5", "Continue waits for a day and a time"),
        ],
        screens: &[
            ("Choose service", true, false),
            ("Details", true, false),
            ("Confirm", true, false),
            ("Booked", true, true),
            ("Button", false, false),
            ("Text button", false, false),
            ("Field", false, false),
            ("Chip", false, false),
            ("ServiceCard", false, false),
            ("Steps", false, false),
        ],
        end_states: &["Booked"],
    },
];

fn path_of(file: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(format!("{file}.figcraft")).to_string_lossy().into_owned()
}

/// A headless server with the example open.
fn open(file: &str) -> Client {
    let mut c = Client::new();
    c.ok("open_file", json!({"path": path_of(file)}));
    c
}

/// A folder of its own under the temp dir, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("figcraft-mcp-examples-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }

    fn p(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The id of the one layer on the current page called exactly `name`.
fn named(c: &mut Client, name: &str) -> Value {
    let found = c.run("node.find", json!({"name": name, "limit": 1000}));
    let exact: Vec<Value> = found["ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| c.ok("get_node", json!({"id": id, "depth": 0, "fields": ["name"]}))["name"] == name)
        .cloned()
        .collect();
    assert_eq!(exact.len(), 1, "expected one layer named {name:?}, found {exact:?}");
    exact[0].clone()
}

/// A command a person runs in the app: no agent's name goes with it.
fn as_user(c: &mut Client, command: &str, params: Value) -> Value {
    c.server.backend().call("engine.execute", json!({"command": command, "params": params})).unwrap()
}

/// What `run_scenarios` says: (id, name, passed) per scenario, with the failures of any that failed.
fn scenario_results(c: &mut Client) -> (bool, Vec<(String, String, bool)>, Vec<Value>) {
    let r = c.ok("run_scenarios", json!({}));
    let results = r["results"].as_array().unwrap();
    let list =
        results.iter().map(|s| (s["id"].as_str().unwrap().to_string(), s["name"].as_str().unwrap().to_string(), s["passed"] == true)).collect();
    let failed = results.iter().filter(|s| s["passed"] != true).cloned().collect();
    (r["passed"] == true, list, failed)
}

fn expected_scenarios(e: &Example) -> Vec<(String, String, bool)> {
    e.scenarios.iter().map(|(id, name)| (id.to_string(), name.to_string(), true)).collect()
}

fn check_example(c: &mut Client, e: &Example) {
    let flow = c.ok("check_prototype", json!({}));
    assert_eq!(flow["problems"], json!([]), "{}: flow problems", e.file);
    assert_eq!(flow["ok"], true, "{}", e.file);
    assert_eq!(flow["prototyped"], true, "{}", e.file);
    let screens: Vec<(String, bool, bool)> = flow["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap().to_string(), s["reachable"] == true, s["endState"] == true))
        .collect();
    let want: Vec<(String, bool, bool)> = e.screens.iter().map(|(n, r, s)| (n.to_string(), *r, *s)).collect();
    assert_eq!(screens, want, "{}: screens", e.file);
    assert!(flow["screens"].as_array().unwrap().iter().all(|s| s["deadEnd"] == false), "{}: a dead end: {flow}", e.file);

    let (passed, list, failed) = scenario_results(c);
    assert_eq!(list, expected_scenarios(e), "{}: scenarios; failures: {failed:?}", e.file);
    assert!(passed, "{}", e.file);

    let design = c.ok("check_design", json!({}));
    assert_eq!(design["findings"], json!([]), "{}: design findings", e.file);
    let skipped: Vec<&str> = design["skipped"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(skipped, SKIPPED_AT_CONVERGING, "{}", e.file);

    let ends = c.run("proto.endState.list", json!({}));
    let ends: Vec<(String, String, String)> = ends["endStates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap().to_string(), s["status"].as_str().unwrap().to_string(), s["author"].as_str().unwrap().to_string()))
        .collect();
    let want: Vec<(String, String, String)> = e.end_states.iter().map(|n| (n.to_string(), "accepted".to_string(), BUILDER.to_string())).collect();
    assert_eq!(ends, want, "{}: end states", e.file);
}

/// The document as inspected, without what names the session rather than the file.
fn inspected(c: &mut Client) -> Value {
    let mut d = c.ok("inspect_document", json!({"depth": 64, "allPages": true}));
    if let Some(o) = d.as_object_mut() {
        for k in ["id", "revision", "history", "path", "dirty", "document", "name", "title", "savedRevision", "documents"] {
            o.remove(k);
        }
    }
    d
}

#[test]
fn each_example_passes_its_flow_check_scenarios_and_design_check() {
    for e in &EXAMPLES {
        let mut c = open(e.file);
        check_example(&mut c, e);
        let ctx = c.ok("design_context", json!({}));
        assert_eq!(ctx["stage"], "converging", "{}", e.file);
        let decisions: Vec<(String, String, String)> = ctx["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| (d["id"].as_str().unwrap().to_string(), d["status"].as_str().unwrap().to_string(), d["author"].as_str().unwrap().to_string()))
            .collect();
        let want: Vec<(String, String, String)> =
            ["D1", "D2", "D3"].iter().map(|d| (d.to_string(), "accepted".to_string(), BUILDER.to_string())).collect();
        assert_eq!(decisions, want, "{}: decisions", e.file);
        let assumptions: Vec<(String, String)> = ctx["assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (a["id"].as_str().unwrap().to_string(), a["status"].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(assumptions, ["A1", "A2", "A3"].map(|a| (a.to_string(), "untested".to_string())).to_vec(), "{}", e.file);
        assert!(ctx["decisions"].as_array().unwrap().iter().all(|d| !d["rejected"].as_str().unwrap_or("").is_empty()), "{}", e.file);
    }
}

#[test]
fn every_example_brief_says_who_it_is_for_what_they_need_where_and_how_success_shows() {
    for e in &EXAMPLES {
        let mut c = open(e.file);
        let brief = c.ok("design_context", json!({}))["brief"].clone();
        for field in ["audience", "sponsor", "job", "context", "content", "format", "tone"] {
            assert!(brief[field].as_str().is_some_and(|s| s.len() > 20), "{}: brief.{field} is {}", e.file, brief[field]);
        }
        assert!(brief["success"].as_array().is_some_and(|s| s.len() >= 2), "{}: {brief}", e.file);
        assert!(brief["constraints"].as_array().is_some_and(|s| s.len() >= 2), "{}: {brief}", e.file);
        let format = brief["format"].as_str().unwrap();
        assert!(format.contains("simulated") && format.contains("no backend"), "{}: the brief says the data is simulated: {format}", e.file);
    }
}

#[test]
fn examples_come_back_the_same_after_a_save_and_a_reopen() {
    let dir = Dir::new("roundtrip");
    for e in &EXAMPLES {
        let mut c = open(e.file);
        let doc = inspected(&mut c);
        let ctx = c.ok("design_context", json!({}));
        let scenarios = c.run("proto.scenario.list", json!({}));
        let path = dir.p(&format!("{}.figcraft", e.file));
        c.ok("save_file", json!({"path": path}));

        let mut again = Client::new();
        again.ok("open_file", json!({"path": path}));
        assert_eq!(inspected(&mut again), doc, "{}: the document", e.file);
        assert_eq!(again.ok("design_context", json!({})), ctx, "{}: the design context", e.file);
        assert_eq!(again.run("proto.scenario.list", json!({})), scenarios, "{}: the scenarios", e.file);
        check_example(&mut again, e);
    }
}

/// The dashboard's state variables for one of its states.
fn dashboard_state(c: &mut Client, status: &str) {
    let on = |s: &str| json!(status == s);
    for (var, value) in [
        ("status", json!(status)),
        ("is_loading", on("loading")),
        ("is_ready", on("ready")),
        ("is_empty", on("empty")),
        ("is_error", on("error")),
        ("show_list", json!(status == "ready" || status == "error")),
    ] {
        c.run("variable.set", json!({"variable": var, "value": value}));
    }
}

fn bounds(c: &mut Client, id: &Value) -> (f64, f64, f64, f64) {
    let b = c.ok("get_node", json!({"id": id, "depth": 0, "fields": ["absoluteBounds"]}))["absoluteBounds"].clone();
    (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap(), b["width"].as_f64().unwrap(), b["height"].as_f64().unwrap())
}

/// Where the dashboard writes its pictures at each width: `FIGCRAFT_EXAMPLE_SHOTS` when set, else
/// a folder under the temp dir.
fn shots_dir() -> PathBuf {
    let d = std::env::var_os("FIGCRAFT_EXAMPLE_SHOTS").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("figcraft-example-shots"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_dashboard_holds_its_layout_at_four_widths_in_every_state() {
    let mut c = open("pantry-dashboard");
    let dash = named(&mut c, "Pantry dashboard");
    let list = named(&mut c, "Low stock");
    let side = named(&mut c, "Side panel");
    let kpis: Vec<Value> = ["Items in stock", "Low on stock", "Families served this week", "Volunteers on shift today"]
        .iter()
        .map(|k| named(&mut c, &format!("KPI {k}")))
        .collect();
    let shots = shots_dir();
    // (width, KPI cards per row, side panel under the list)
    for (width, per_row, stacked) in [(360.0, 1, true), (768.0, 2, false), (1440.0, 4, false), (1100.0, 4, false)] {
        for status in ["loading", "ready", "empty", "error"] {
            dashboard_state(&mut c, status);
            c.ok("set_properties", json!({"ids": [dash], "props": {"width": width}}));
            let found = c.ok("check_design", json!({"rules": ["text.clipped", "layout.clippedContent"]}));
            assert_eq!(found["findings"], json!([]), "{width} px, {status}");
            let (_, _, w, h) = bounds(&mut c, &dash);
            assert_eq!(w, width, "{status}");
            assert!(h >= 900.0, "{width} px, {status}: the frame is {h} px tall");
            let path = shots.join(format!("pantry-dashboard-{status}-{width}.png")).to_string_lossy().into_owned();
            c.ok("export", json!({"ids": [dash], "format": "png", "scale": 1, "path": path}));
            assert!(std::fs::metadata(&path).unwrap().len() > 1000, "{path}");
        }
        let rows: Vec<f64> = kpis.clone().iter().map(|k| bounds(&mut c, k).1).collect();
        let first_row = rows.iter().filter(|y| (**y - rows[0]).abs() < 0.5).count();
        assert_eq!(first_row, per_row, "{width} px: KPI cards on the first row ({rows:?})");
        let (lx, ly, lw, lh) = bounds(&mut c, &list);
        let (sx, sy, sw, _) = bounds(&mut c, &side);
        if stacked {
            assert!(sy >= ly + lh, "{width} px: the side panel is under the list");
            assert_eq!((sx, sw), (lx, lw), "{width} px: and as wide");
        } else {
            assert_eq!(sy, ly, "{width} px: the side panel is beside the list");
            assert!(sx >= lx + lw, "{width} px");
        }
    }
}

#[test]
fn the_simulated_states_of_every_scenario_can_be_drawn() {
    for e in &EXAMPLES {
        let mut c = open(e.file);
        for s in c.run("proto.scenario.list", json!({}))["scenarios"].as_array().unwrap().clone() {
            let r = c.call("simulate_prototype", json!({"start": s["start"], "steps": s["steps"], "image": true}));
            assert_eq!(r["isError"], false, "{} {}: {r}", e.file, s["id"]);
            let img = r["content"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["type"] == "image")
                .unwrap_or_else(|| panic!("{} {}: no picture", e.file, s["id"]));
            let (w, h) = png_dims(img["data"].as_str().unwrap());
            assert!(w >= 390 && h >= 844, "{} {}: {w}×{h}", e.file, s["id"]);
        }
    }
}

#[test]
fn a_missing_font_is_named_and_the_text_still_lays_out() {
    let mut c = open("signup-flow");
    let title = {
        let welcome = named(&mut c, "Welcome");
        let ids = c.run("node.find", json!({"root": welcome, "name": "Title", "type": "TEXT"}));
        ids["ids"][0].clone()
    };
    let before = bounds(&mut c, &title);
    c.ok("set_properties", json!({"ids": [title], "props": {"fontFamily": "Figgure Missing Sans"}}));
    let missing = c.run("text.missingFonts", json!({}));
    let families = missing["families"].as_array().unwrap();
    assert_eq!(families.len(), 1, "{missing}");
    assert_eq!(families[0]["family"], "Figgure Missing Sans");
    assert_eq!(families[0]["layers"], 1, "one layer names it: {missing}");
    let (_, _, w, h) = bounds(&mut c, &title);
    assert_eq!(w, before.2, "the title keeps its fill width");
    assert!(h > 20.0, "the title is laid out with a fallback font: {h} px tall");
    let welcome = named(&mut c, "Welcome");
    let shot = c.call("screenshot", json!({"ids": [welcome]}));
    let img = shot["content"].as_array().unwrap().iter().find(|b| b["type"] == "image").unwrap();
    assert_eq!(png_dims(img["data"].as_str().unwrap()), (390, 844));
    let (passed, _, failed) = scenario_results(&mut c);
    assert!(passed, "{failed:?}");
}

#[test]
fn without_the_maintainers_acceptance_an_agents_end_state_stays_a_proposal() {
    // What the build script leaves without its acceptance step: the end state the agent declared.
    let mut c = open("signup-flow");
    let done = named(&mut c, "Done");
    as_user(&mut c, "proto.endState.set", json!({"id": done, "on": false}));
    let r = c.run("proto.endState.set", json!({"id": done, "reason": "The sign-up is complete."}));
    assert_eq!(r["status"], "proposed");
    let flow = c.ok("check_prototype", json!({}));
    assert_eq!(flow["ok"], false);
    let rules: Vec<(&str, &str)> =
        flow["problems"].as_array().unwrap().iter().map(|p| (p["rule"].as_str().unwrap(), p["severity"].as_str().unwrap())).collect();
    assert_eq!(rules, [("flow.deadEnd", "major"), ("scenario.fails", "major")]);
    let dead = &flow["problems"][0];
    assert_eq!(dead["nodes"], json!([done]));
    assert!(
        dead["evidence"]
            .as_str()
            .unwrap()
            .ends_with("; declared as an end state by test (agent), waiting for a person to accept (proto.endState.accept)"),
        "{dead}"
    );
    let (passed, list, _) = scenario_results(&mut c);
    assert!(!passed);
    assert_eq!(list[0], ("S1".to_string(), "Happy path".to_string(), false), "the happy path expects an end state");
    as_user(&mut c, "proto.endState.accept", json!({"id": done}));
    assert_eq!(c.ok("check_prototype", json!({}))["ok"], true);
}
