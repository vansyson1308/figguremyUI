//! `figguremyui-cli doctor` end to end: exit codes, the JSON's keys, a failing check with its fix,
//! and the printed setup line starting a server that answers.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn doctor(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_figguremyui-cli")).arg("doctor").args(args).output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn doctor_json(args: &[&str]) -> (i32, Value) {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let (code, out, err) = doctor(&all);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}\n{err}"));
    (code, v)
}

fn example() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/signup-flow.figcraft").to_string_lossy().into_owned()
}

fn check<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["checks"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no check {id}: {v:#}"))
}

#[test]
fn a_working_install_exits_0_with_the_documented_keys() {
    let (code, v) = doctor_json(&[]);
    let failed: Vec<&Value> = v["checks"].as_array().unwrap().iter().filter(|c| c["ok"] != true).collect();
    // Without any font (no craft-fonts input, none installed) the fonts check fails, and only it.
    let no_fonts = v["fonts"]["families"] == 0;
    assert_eq!(code, if no_fonts { 1 } else { 0 }, "{failed:#?}");
    for key in ["schema", "ok", "product", "version", "binary", "mcp", "example", "fonts", "checks", "warnings", "setup", "wroteFiles", "usedNetwork"]
    {
        assert!(v.get(key).is_some(), "no `{key}` in {v:#}");
    }
    assert_eq!(v["schema"], 1);
    assert_eq!(v["product"], "Figgure My UI");
    assert_eq!((v["wroteFiles"].clone(), v["usedNetwork"].clone()), (json!(false), json!(false)));
    let exe = std::fs::canonicalize(env!("CARGO_BIN_EXE_figguremyui-cli")).unwrap();
    assert_eq!(v["binary"]["path"].as_str().map(std::path::PathBuf::from), Some(exe));
    assert_eq!(v["mcp"]["serverName"], "figguremyui");
    assert_eq!(v["mcp"]["backend"], "headless");
    assert!(v["mcp"]["tools"].as_u64().unwrap() > 0);
    for id in
        ["binary", "mcp.initialize", "mcp.tools", "mcp.capabilities", "mcp.document", "mcp.screenshot", "mcp.check_prototype", "mcp.run_scenarios"]
    {
        assert_eq!(check(&v, id)["ok"], true, "{id}: {:#}", check(&v, id));
    }
    for key in ["system", "families", "builtIn", "installed", "inter"] {
        assert!(v["fonts"].get(key).is_some(), "no fonts.{key}");
    }
    assert_eq!(v["example"], Value::Null);
    let s = &v["setup"];
    assert_eq!(s["headless"]["args"], json!(["mcp"]));
    assert_eq!(s["attached"]["args"], json!(["mcp", "--attach", "7979"]));
    assert_eq!(s["attached"]["port"], 7979);
    for (mode, name) in [("headless", "figguremyui"), ("attached", "figguremyui-app")] {
        assert!(s[mode]["claudeCode"].as_str().unwrap().starts_with(&format!("claude mcp add --transport stdio {name} -- ")));
        assert!(s[mode]["codex"].as_str().unwrap().starts_with(&format!("codex mcp add {name} -- ")));
        assert_eq!(s[mode]["mcpServers"]["mcpServers"][name]["command"], s["command"]);
    }
    assert!(s["attached"]["startApp"].as_str().unwrap().ends_with(" --control 7979"));
}

#[test]
fn the_example_runs_its_scenarios() {
    let (code, v) = doctor_json(&["--example", &example(), "--port", "8123"]);
    let e = &v["example"];
    assert_eq!(e["opened"], true, "{e:#}");
    let total = e["scenarios"]["total"].as_u64().unwrap();
    assert!(total > 0, "{e:#}");
    assert_eq!(e["scenarios"]["failed"], 0, "{e:#}");
    assert_eq!(e["scenarios"]["passed"], total);
    assert_eq!(e["prototypeOk"], true, "{e:#}");
    for id in ["example.open", "example.scenarios", "example.check_prototype"] {
        assert_eq!(check(&v, id)["ok"], true, "{id}");
    }
    assert_eq!(v["setup"]["attached"]["args"], json!(["mcp", "--attach", "8123"]));
    assert_eq!(code == 0, v["ok"] == true);
}

#[test]
fn a_missing_example_fails_with_exit_1_and_says_what_to_do() {
    let missing = std::env::temp_dir().join(format!("figguremyui-doctor-missing-{}.figcraft", std::process::id()));
    let (code, out, _) = doctor(&["--example", missing.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}");
    let line = out.lines().find(|l| l.contains("example.open")).unwrap_or_else(|| panic!("{out}"));
    assert!(line.trim_start().starts_with("FAIL"), "{line}");
    assert!(out.contains("fix: give --example the path of a .figcraft file that exists"), "{out}");
    assert!(out.contains("of") && out.contains("checks failed"), "{out}");
    // The human output still prints the setup lines.
    assert!(out.contains("claude mcp add --transport stdio figguremyui -- "), "{out}");

    let (code, v) = doctor_json(&["--example", missing.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(v["ok"], false);
    let c = check(&v, "example.open");
    assert_eq!(c["ok"], false);
    assert!(c["fix"].as_str().is_some_and(|f| !f.is_empty()));
}

#[test]
fn without_fonts_the_fonts_check_fails() {
    // Opted out of the installed fonts, only the built-in ones count: none without craft-fonts.
    let (code, v) = doctor_json(&["--no-system-fonts"]);
    assert_eq!(v["fonts"]["system"], "optedOut");
    let c = check(&v, "fonts");
    if figcraft_text::CRAFT_FONTS.is_empty() {
        assert_eq!(code, 1);
        assert_eq!(c["ok"], false, "{c:#}");
        assert!(c["fix"].as_str().unwrap().contains("install"), "{c:#}");
    } else {
        assert_eq!(c["ok"], true, "{c:#}");
    }
}

#[test]
fn bad_arguments_are_usage_errors() {
    for args in [&["--port", "0"][..], &["--port", "65536"], &["--example"], &["stray"]] {
        let (code, _, err) = doctor(args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
}

/// The headless line doctor prints starts a server that answers `initialize` on stdio.
#[test]
fn the_printed_setup_starts_a_server() {
    let (_, v) = doctor_json(&[]);
    let server = &v["setup"]["headless"]["mcpServers"]["mcpServers"]["figguremyui"];
    let args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let mut child = Command::new(server["command"].as_str().unwrap())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"test"}}}}}}"#).unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    drop(stdin);
    let _ = child.wait();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["result"]["serverInfo"]["name"], "figguremyui", "{reply}");
}
