//! The attached backend against a fake app: a loopback control server answering with a headless
//! session, speaking the desktop app's JSON-lines protocol.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use figcraft_mcp::{Attached, Backend, Headless, Server};
use serde_json::{Value, json};

fn fake_app() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut h = Headless::new();
        for stream in listener.incoming().flatten() {
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                let method = v["method"].as_str().unwrap_or("");
                let reply = match h.call(method, v["params"].clone()) {
                    Ok(r) => json!({"id": v["id"], "ok": true, "result": r}),
                    Err(e) => json!({"id": v["id"], "ok": false, "error": e}),
                };
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    });
    port
}

/// The next request on a connection that isn't the question a new connection starts with
/// (`app.info`). That one is answered as an app from before it answers, so the fakes below go on
/// speaking one request per connection.
fn next_request(reader: &mut BufReader<std::net::TcpStream>, out: &mut std::net::TcpStream) -> Value {
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        if request["method"] != "app.info" {
            return request;
        }
        writeln!(out, "{}", json!({"id": request["id"], "ok": false, "error": "unknown method `app.info`"})).unwrap();
    }
}

fn call(s: &mut Server, tool: &str, args: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}}).to_string();
    let r: Value = serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap();
    r["result"].clone()
}

#[test]
fn attached_backend_forwards_everything() {
    let port = fake_app();
    let backend = Attached::port(port).unwrap();
    assert!(backend.describe().contains(&port.to_string()));
    let mut s = Server::new(Box::new(backend));
    let r = call(&mut s, "create_node", json!({"type": "frame", "width": 64, "height": 32, "props": {"fill": "#FF0000"}}));
    assert_eq!(r["isError"], false, "{r}");
    let created: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    let id = created["id"].clone();
    // Render through ui.render + a temp file.
    let shot = call(&mut s, "screenshot", json!({}));
    assert_eq!(shot["content"][0]["type"], "image", "{shot}");
    let shot = call(&mut s, "screenshot", json!({"ids": [id], "scale": 2}));
    assert_eq!(shot["isError"], false, "{shot}");
    // Window capture through ui.screenshot.
    let win = call(&mut s, "screenshot", json!({"window": true}));
    assert_eq!(win["content"][0]["type"], "image", "{win}");
    let e = call(&mut s, "export", json!({"ids": [id]}));
    assert_eq!(e["content"][0]["mimeType"], "image/png");
    assert!(call(&mut s, "key", json!({"key": "Delete"}))["isError"] == false);
    let doc = call(&mut s, "inspect_document", json!({}));
    assert!(!doc["content"][0]["text"].as_str().unwrap().contains("\"Frame"));
    let err = call(&mut s, "run_command", json!({"command": "nope"}));
    assert_eq!(err["isError"], true);
}

#[test]
fn attached_inspection_forwards_compact_options_to_the_app() {
    let mut s = Server::new(Box::new(Attached::port(fake_app()).unwrap()));
    let created = call(&mut s, "create_node", json!({"type": "text", "text": "界".repeat(121), "name": "Title"}));
    let id = serde_json::from_str::<Value>(created["content"][0]["text"].as_str().unwrap()).unwrap()["id"].clone();
    let r = call(&mut s, "get_node", json!({"id": id, "outline": true}));
    assert_eq!(r["isError"], false, "{r}");
    let outline: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(outline["characters"], format!("{}...", "界".repeat(120)));
    assert_eq!(outline["visible"], true);
    assert!(outline.get("fontSize").is_none());
    let r = call(&mut s, "inspect_document", json!({"fields": [], "outline": false}));
    assert_eq!(r["isError"], false, "{r}");
    let compact: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(compact["pages"][0]["children"][0], json!({"id": id}));
    assert!(compact["pages"][0].get("name").is_none());
}

#[test]
fn an_attached_agent_comments_under_its_own_name() {
    let mut s = Server::new(Box::new(Attached::port(fake_app()).unwrap()));
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "Claude", "version": "1"}}});
    assert!(s.handle_line(&init.to_string()).is_some());
    let run = |s: &mut Server, command: &str, params: Value| {
        let r = call(s, "run_command", json!({"command": command, "params": params}));
        assert_eq!(r["isError"], false, "{r}");
        serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    run(&mut s, "comment.add", json!({"x": 10, "y": 10, "text": "Done"}));
    let t = run(&mut s, "comment.list", json!({}))["comments"][0].clone();
    assert_eq!((t["author"].clone(), t["agent"].clone()), (json!("Claude (agent)"), json!(true)), "{t}");
}

#[test]
fn attached_to_nothing_fails_cleanly() {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    assert!(Attached::port(port).is_err());
}

#[test]
fn lost_mutation_reply_is_not_replayed_and_next_call_reconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = std::thread::spawn(move || {
        let mut h = Headless::new();
        let mut methods = Vec::new();
        for lose_reply in [true, false] {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let request = next_request(&mut BufReader::new(stream.try_clone().unwrap()), &mut stream);
            let method = request["method"].as_str().unwrap();
            methods.push(method.to_string());
            let result = h.call(method, request["params"].clone()).unwrap();
            if !lose_reply {
                writeln!(stream, "{}", json!({"id": request["id"], "ok": true, "result": result})).unwrap();
            }
        }
        methods
    });
    let mut backend = Attached::port(port).unwrap();
    let result = backend.call("engine.execute", json!({"command": "shape.rectangle", "params": {"width": 64, "height": 32}}));
    assert!(result.is_err(), "a lost reply must not turn into a successful replay: {result:?}");
    let error = result.unwrap_err();
    assert!(error.contains("may have completed"), "{error}");
    assert!(error.contains("before retrying"), "{error}");
    let doc = backend.call("document.inspect", json!({"depth": 1})).unwrap();
    assert_eq!(doc["pages"][0]["children"].as_array().unwrap().len(), 1, "{doc}");
    assert_eq!(app.join().unwrap(), ["engine.execute", "document.inspect"]);
}

#[test]
fn a_connection_the_app_closed_while_idle_is_replaced_before_sending() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed_tx, closed_rx) = std::sync::mpsc::channel();
    let app = std::thread::spawn(move || {
        let mut h = Headless::new();
        let mut methods = Vec::new();
        // Each connection answers one request, then the app drops it as it does an idle one.
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let request = next_request(&mut BufReader::new(stream.try_clone().unwrap()), &mut stream);
            let method = request["method"].as_str().unwrap();
            methods.push(method.to_string());
            let result = h.call(method, request["params"].clone()).unwrap();
            writeln!(stream, "{}", json!({"id": request["id"], "ok": true, "result": result})).unwrap();
            drop(stream);
            closed_tx.send(()).unwrap();
        }
        methods
    });
    let mut backend = Attached::port(port).unwrap();
    backend.call("engine.execute", json!({"command": "shape.rectangle", "params": {"width": 64, "height": 32}})).unwrap();
    closed_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    // No error, nothing sent twice: the request goes out once, on a fresh connection.
    let doc = backend.call("document.inspect", json!({"depth": 1})).unwrap();
    assert_eq!(doc["pages"][0]["children"].as_array().unwrap().len(), 1, "{doc}");
    assert_eq!(app.join().unwrap(), ["engine.execute", "document.inspect"]);
}

#[test]
fn reconnect_failure_reports_request_not_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut backend = Attached::port(listener.local_addr().unwrap().port()).unwrap();
    drop(listener);
    assert!(backend.call("document.inspect", json!({})).is_err());
    let error = backend.call("document.inspect", json!({})).unwrap_err();
    assert!(error.contains("request was not sent"), "{error}");
}

#[test]
fn a_tool_call_naming_its_document_edits_that_file_and_not_the_one_in_front() {
    let mut s = Server::new(Box::new(Attached::port(fake_app()).unwrap()));
    let text = |r: &Value| -> Value { serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap() };
    let back = text(&call(&mut s, "inspect_document", json!({"depth": 0})))["id"].clone();
    assert!(back.is_u64(), "inspect_document names the file: {back}");
    assert_eq!(call(&mut s, "run_command", json!({"command": "file.new", "params": {"name": "Front"}}))["isError"], false);
    let r = call(&mut s, "create_node", json!({"type": "rectangle", "document": back}));
    assert_eq!(r["isError"], false, "{r}");
    let list = text(&call(&mut s, "run_command", json!({"command": "doc.list"})));
    assert_eq!((list[0]["id"].clone(), list[0]["active"].clone(), list[1]["active"].clone()), (back.clone(), json!(false), json!(true)));
    // A call without `document` stays on the working document (the one first read, then named),
    // whichever tab is in front; the tab in front is read by naming it.
    let working = text(&call(&mut s, "inspect_document", json!({"depth": 1})));
    assert_eq!((working["id"].clone(), working["pages"][0]["children"].as_array().map_or(0, Vec::len)), (back.clone(), 1), "{working}");
    let front = text(&call(&mut s, "inspect_document", json!({"depth": 1, "document": list[1]["id"]})));
    assert_eq!((front["name"].as_str(), front["pages"][0]["children"].as_array().map_or(0, Vec::len)), (Some("Front"), 0));
    let behind = text(&call(&mut s, "inspect_document", json!({"depth": 1, "document": back})));
    assert_eq!(behind["pages"][0]["children"].as_array().map_or(0, Vec::len), 1, "{behind}");
    let e = call(&mut s, "inspect_document", json!({"document": 999_999}));
    assert_eq!(e["isError"], true);
    // This fake app gives only the text of an error, as an app from before error codes does: the
    // server still knows a document that is not open, and lists the ones that are.
    let error = &e["structuredContent"]["error"];
    assert_eq!(error["code"], "noDocument", "{e}");
    assert!(error["message"].as_str().unwrap().contains("no open document 999999") && error["documents"].as_array().unwrap().len() == 2, "{e}");
    let e = call(&mut s, "run_command", json!({"command": "nope.nope"}));
    assert_eq!(e["structuredContent"]["error"]["code"], "error", "only the text is known: {e}");
    assert_eq!(e["structuredContent"]["error"]["message"], "unknown command `nope.nope`");
    assert_eq!(call(&mut s, "pointer", json!({"x": 0, "y": 0, "document": back}))["isError"], true, "pointer takes no `document`");
}

/// A fake app like [`fake_app`] that also says what its errors are (`errorDetail`) and which run
/// of the app it is (`app.info`), as the app is meant to. It has `runs` sessions, one per run of
/// the app: the method `test.restart` (the fake's own) ends the connection and the run, so the
/// next connection reaches the next session.
fn fake_app_runs(runs: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut apps: Vec<Headless> = (0..runs).map(|_| Headless::new()).collect();
        let mut run = 0;
        for stream in listener.incoming().flatten() {
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                let method = v["method"].as_str().unwrap_or("");
                if method == "test.restart" {
                    run = (run + 1).min(runs - 1);
                    let _ = writeln!(out, "{}", json!({"id": v["id"], "ok": true, "result": null}));
                    break;
                }
                let app = &mut apps[run];
                let mut reply = match app.call(method, v["params"].clone()) {
                    Ok(r) => json!({"id": v["id"], "ok": true, "result": r}),
                    Err(e) => json!({"id": v["id"], "ok": false, "error": e, "errorDetail": app.take_error_detail()}),
                };
                // As the app does: it has a window, and a command's reply carries the revision of
                // the document it acted on, read right after it, next to the result.
                if method == "app.info" {
                    reply["result"]["window"] = json!(true);
                }
                if let Some((_, revision)) = app.take_revision().filter(|_| method == "engine.execute") {
                    reply["revision"] = json!(revision);
                }
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    });
    port
}

#[test]
fn an_attached_app_that_says_what_its_errors_are_is_believed() {
    let mut s = Server::new(Box::new(Attached::port(fake_app_runs(1)).unwrap()));
    let text = |r: &Value| -> Value { serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap() };
    let made = call(&mut s, "create_node", json!({"type": "rectangle", "name": "Card"}));
    let doc = made["structuredContent"]["document"].clone();
    assert_eq!((doc["front"].clone(), doc["revision"].clone()), (json!(true), json!(2)), "{made}");
    let id = text(&made)["id"].clone();
    let error = |r: Value| r["structuredContent"]["error"].clone();
    let e = error(call(&mut s, "get_node", json!({"id": 424_242})));
    assert_eq!((e["code"].clone(), e["node"].clone(), e["document"]["id"].clone()), (json!("noSuchNode"), json!(424_242), doc["id"].clone()), "{e}");
    let e = error(call(&mut s, "run_command", json!({"command": "nope.nope"})));
    assert_eq!((e["code"].clone(), e["command"].clone()), (json!("unknownCommand"), json!("nope.nope")), "{e}");
    let e = error(call(&mut s, "set_properties", json!({"ids": [id], "props": {"x": 4}, "ifRevision": 1})));
    assert_eq!((e["code"].clone(), e["expected"].clone(), e["actual"].clone()), (json!("conflict"), json!(1), json!(2)), "{e}");
    assert_eq!(e["since"][0]["author"], "AI agent", "the client gave no name: {e}");
    // What the app says about itself reaches get_capabilities.
    let caps = text(&call(&mut s, "get_capabilities", json!({})));
    assert_eq!((caps["backend"].clone(), caps["window"].clone(), caps["player"].clone()), (json!("attached"), json!(true), json!(true)), "{caps}");
    assert!(caps["app"]["instance"].is_string(), "{caps}");
    assert_eq!((caps["features"]["engineErrorCodes"].clone(), caps["features"]["appIdentity"].clone()), (json!(true), json!(true)), "{caps}");
    assert_eq!(caps["workingDocument"]["id"], doc["id"]);
}

/// F-08 seen from a client: the app is restarted while the server runs. The ids the server
/// remembers belong to the run that is gone, so the next call fails, once, says so, and changes
/// nothing in the new run; after that the server starts over with the tab in front.
#[test]
fn a_restarted_app_fails_one_call_and_nothing_lands_in_the_new_run() {
    let mut s = Server::new(Box::new(Attached::port(fake_app_runs(2)).unwrap()));
    let text = |r: &Value| -> Value { serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap() };
    let made = call(&mut s, "create_node", json!({"type": "rectangle", "name": "Before the restart"}));
    assert_eq!(made["isError"], false, "{made}");
    let id = text(&made)["id"].clone();
    let first_run = text(&call(&mut s, "get_capabilities", json!({})))["app"]["instance"].clone();
    s.backend().call("test.restart", json!({})).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let r = call(&mut s, "set_properties", json!({"ids": [id], "props": {"name": "After"}}));
    assert_eq!(r["isError"], true, "{r}");
    let e = &r["structuredContent"]["error"];
    assert_eq!((e["code"].clone(), e["restarted"].clone()), (json!("transport"), json!(true)), "{r}");
    assert!(e["message"].as_str().unwrap().contains("was restarted") && e["message"].as_str().unwrap().contains("stale"), "{r}");
    // The new run was not touched, and the next call works there.
    let doc = call(&mut s, "inspect_document", json!({"depth": 1}));
    assert_eq!(doc["isError"], false, "{doc}");
    assert_eq!(text(&doc)["pages"][0]["children"].as_array().map_or(0, Vec::len), 0, "{doc}");
    assert_eq!(doc["structuredContent"]["document"]["revision"], 1);
    let second_run = text(&call(&mut s, "get_capabilities", json!({})))["app"]["instance"].clone();
    assert!(second_run.is_string() && second_run != first_run, "{first_run} {second_run}");
}
