//! The attached backend reads the app's replies with a bound (#1113).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use super::{Attached, Backend};

/// A fake app that takes one connection per reply, reads one request on it and answers with the
/// reply. It keeps the connections open until [`finish`] (so an unterminated reply never reaches
/// EOF), then returns how many requests it read.
fn fake_app(replies: Vec<Vec<u8>>) -> (u16, mpsc::Sender<()>, std::thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (done, wait) = mpsc::channel::<()>();
    let app = std::thread::spawn(move || {
        let mut open = Vec::new();
        for reply in &replies {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            out.write_all(reply).unwrap();
            out.flush().unwrap();
            open.push(out);
        }
        wait.recv_timeout(Duration::from_secs(30)).ok();
        open.len()
    });
    (port, done, app)
}

/// Let the fake app close its connections, and check it read `requests` requests.
fn finish(done: mpsc::Sender<()>, app: std::thread::JoinHandle<usize>, requests: usize) {
    drop(done);
    assert_eq!(app.join().unwrap(), requests);
}

fn ok_reply(text: &str) -> Vec<u8> {
    format!("{}\n", json!({"id": 1, "ok": true, "result": text})).into_bytes()
}

/// A backend that sends only what a test asks for: these fakes answer one request per
/// connection, so the question a new connection normally starts with (`app.info`) is left out.
fn wire_only(port: u16) -> Attached {
    let mut app = Attached::port(port).unwrap();
    app.identify = false;
    app
}

/// A fake app that answers by script: one list per connection, each entry the method it must be
/// asked next and the reply line it gives. A connection is closed when its list ends, as the app
/// closes an idle one (`closed` then gets a message), and the methods asked are returned.
fn scripted(connections: Vec<Vec<(&'static str, serde_json::Value)>>) -> (u16, mpsc::Receiver<()>, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed_tx, closed) = mpsc::channel();
    let app = std::thread::spawn(move || {
        let mut asked = Vec::new();
        for script in connections {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            for (method, reply) in script {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method, "{request}");
                asked.push(method.to_string());
                writeln!(out, "{reply}").unwrap();
            }
            drop((reader, out));
            closed_tx.send(()).unwrap();
        }
        asked
    });
    (port, closed, app)
}

fn says(instance: &str) -> serde_json::Value {
    json!({"id": 1, "ok": true, "result": {"instance": instance, "version": "9.9.9", "features": ["errorDetail"]}})
}

fn ok(result: serde_json::Value) -> serde_json::Value {
    json!({"id": 1, "ok": true, "result": result})
}

/// Wait until the fake app closed the connection in use, and the close has arrived.
fn until_closed(closed: &mpsc::Receiver<()>) {
    closed.recv_timeout(Duration::from_secs(10)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
}

/// F-08: the server used to carry on silently with whatever app answered on the port. Document
/// ids, node ids and revisions belong to one run of the app, so the first call that reaches
/// another run fails, once, and says why.
#[test]
fn reconnecting_to_another_run_of_the_app_fails_the_first_call_once() {
    let (port, closed, fake) = scripted(vec![
        vec![("app.info", says("run-1")), ("engine.commands", ok(json!([])))],
        vec![("app.info", says("run-2")), ("document.inspect", ok(json!({"id": 1})))],
    ]);
    let mut app = Attached::port(port).unwrap();
    app.call("engine.commands", json!({})).unwrap();
    assert_eq!(app.app_info().unwrap()["instance"], "run-1");
    until_closed(&closed);
    let err = app.call("engine.execute", json!({"command": "node.set", "params": {"ids": [7], "props": {"x": 1}}})).unwrap_err();
    assert!(err.contains("was restarted") && err.contains("stale") && err.contains("not sent"), "{err}");
    let detail = app.take_error_detail().unwrap();
    assert_eq!((detail["code"].clone(), detail["restarted"].clone(), detail["message"].clone()), (json!("transport"), json!(true), json!(err)));
    // Told once: the next call goes to the app that is there now.
    assert_eq!(app.call("document.inspect", json!({})).unwrap(), json!({"id": 1}));
    assert_eq!(app.app_info().unwrap()["instance"], "run-2");
    assert!(app.take_error_detail().is_none());
    // The edit was never sent to the other run.
    assert_eq!(fake.join().unwrap(), ["app.info", "engine.commands", "app.info", "document.inspect"]);
}

#[test]
fn reconnecting_to_the_same_run_of_the_app_goes_on() {
    let (port, closed, fake) = scripted(vec![
        vec![("app.info", says("run-1")), ("engine.commands", ok(json!([])))],
        vec![("app.info", says("run-1")), ("engine.commands", ok(json!(["again"])))],
    ]);
    let mut app = Attached::port(port).unwrap();
    app.call("engine.commands", json!({})).unwrap();
    until_closed(&closed);
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!(["again"]));
    assert_eq!(fake.join().unwrap().len(), 4);
}

/// An app from before `app.info` answers that it doesn't know the method: nothing is known about
/// it and nothing is checked, as before.
#[test]
fn an_app_that_does_not_say_which_app_it_is_is_used_as_before() {
    let unknown = json!({"id": 1, "ok": false, "error": "unknown method `app.info`"});
    let (port, closed, fake) = scripted(vec![
        vec![("app.info", unknown.clone()), ("engine.commands", ok(json!([])))],
        vec![("app.info", unknown), ("engine.commands", ok(json!(["again"])))],
    ]);
    let mut app = Attached::port(port).unwrap();
    app.call("engine.commands", json!({})).unwrap();
    assert!(app.app_info().is_none());
    until_closed(&closed);
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!(["again"]));
    assert_eq!(fake.join().unwrap().len(), 4);
}

/// The app's `errorDetail` is what `take_error_detail` gives; an app that sends only the text
/// gets code `error`; a failure of the connection is code `transport`.
#[test]
fn what_the_app_says_an_error_is_comes_with_it() {
    let detailed =
        json!({"id": 1, "ok": false, "error": "no such node 7", "errorDetail": {"code": "noSuchNode", "node": 7, "message": "stale text"}});
    let plain = json!({"id": 1, "ok": false, "error": "something else"});
    let (port, _closed, fake) =
        scripted(vec![vec![("node.get", detailed), ("node.get", plain.clone()), ("node.get", plain), ("node.get", ok(json!({"id": 7})))]]);
    let mut app = wire_only(port);
    assert_eq!(app.call("node.get", json!({"id": 7})).unwrap_err(), "no such node 7");
    assert_eq!(app.take_error_detail().unwrap(), json!({"code": "noSuchNode", "node": 7, "message": "no such node 7"}));
    assert!(app.take_error_detail().is_none(), "taken once");
    assert_eq!(app.call("node.get", json!({"id": 7})).unwrap_err(), "something else");
    assert_eq!(app.take_error_detail().unwrap(), json!({"code": "error", "message": "something else"}));
    // A call that works leaves no detail behind from the one before.
    app.call("node.get", json!({"id": 7})).unwrap_err();
    app.call("node.get", json!({"id": 7})).unwrap();
    assert!(app.take_error_detail().is_none());
    drop(app);
    fake.join().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut gone = wire_only(listener.local_addr().unwrap().port());
    drop(listener);
    gone.call("node.get", json!({})).unwrap_err();
    let err = gone.call("node.get", json!({})).unwrap_err();
    assert_eq!(gone.take_error_detail().unwrap(), json!({"code": "transport", "message": err}));
}

/// #1696: a raised timeout goes with every request; without one the line is as it always was.
#[test]
fn a_raised_timeout_is_asked_for_on_each_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Reads the requests of two calls; `with_timeout`'s dropped connection brings none.
    let app = std::thread::spawn(move || {
        let mut lines = Vec::new();
        while lines.len() < 2 {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).unwrap() > 0 {
                out.write_all(&ok_reply("fine")).unwrap();
                lines.push(serde_json::from_str::<serde_json::Value>(&line).unwrap());
            }
        }
        lines
    });
    wire_only(port).call("engine.commands", json!({})).unwrap();
    let mut long = wire_only(port).with_timeout(Duration::from_secs(300)).unwrap();
    long.call("engine.commands", json!({})).unwrap();
    let lines = app.join().unwrap();
    assert_eq!(lines[0].get("timeoutMs"), None, "{:?}", lines[0]);
    assert_eq!(lines[1]["timeoutMs"], 300_000, "{:?}", lines[1]);
}

#[test]
fn an_oversized_reply_is_refused_and_the_next_call_reconnects() {
    let (port, done, fake) = fake_app(vec![vec![b'x'; 200], ok_reply("fine")]);
    let mut app = wire_only(port);
    app.max_reply = 64;
    let err = app.call("engine.commands", json!({})).unwrap_err();
    assert!(err.contains("reply larger than"), "{err}");
    assert!(err.contains("may have completed"), "{err}");
    assert!(app.conn.is_none());
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!("fine"));
    finish(done, fake, 2);
}

#[test]
fn a_reply_at_the_limit_is_read() {
    let reply = ok_reply("at the limit");
    let (port, done, fake) = fake_app(vec![reply.clone()]);
    let mut app = wire_only(port);
    app.max_reply = reply.len() - 1; // the newline doesn't count
    assert_eq!(app.call("engine.commands", json!({})).unwrap(), json!("at the limit"));
    finish(done, fake, 1);
}

#[test]
fn a_reply_that_is_not_utf8_is_an_error() {
    let (port, done, fake) = fake_app(vec![b"{\"ok\": true, \"result\": \"\xff\"}\n".to_vec()]);
    let mut app = wire_only(port);
    let err = app.call("engine.commands", json!({})).unwrap_err();
    assert!(err.contains("failed"), "{err}");
    assert!(app.conn.is_none());
    finish(done, fake, 1);
}
