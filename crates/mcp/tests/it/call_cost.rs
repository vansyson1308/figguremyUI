//! What a tool call asks of its backend (performance budget PF1, `docs/acceptance.md`). A tool
//! call reads the open documents before its first request and after its last, to name the
//! document and revision its result belongs to (review R2-01). The headless backend answers those
//! reads from its own session (`Backend::documents`), so they run no command; a backend that
//! can't (an attached app) is asked `doc.list`. Both give the same results.

use std::cell::RefCell;
use std::rc::Rc;

use figcraft_mcp::{Backend, Headless, OpenDocument, Server};
use serde_json::{Value, json};

use crate::common::Client;

/// What a [`Counting`] backend was asked: the commands it ran, `doc.list` apart.
#[derive(Default)]
struct Asked {
    lists: usize,
    commands: Vec<String>,
}

/// A headless backend that counts the commands it runs. `documents: false` plays an app that
/// can't read the documents without a command.
struct Counting {
    inner: Headless,
    documents: bool,
    asked: Rc<RefCell<Asked>>,
}

impl Backend for Counting {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if method == "engine.execute" {
            let command = params["command"].as_str().unwrap_or_default().to_string();
            let mut asked = self.asked.borrow_mut();
            match command.as_str() {
                "doc.list" => asked.lists += 1,
                _ => asked.commands.push(command),
            }
        }
        self.inner.call(method, params)
    }

    fn has_ui(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        "headless, counting".into()
    }

    fn take_error_detail(&mut self) -> Option<Value> {
        self.inner.take_error_detail()
    }

    fn take_revision(&mut self) -> Option<(u64, u64)> {
        self.inner.take_revision()
    }

    fn documents(&mut self) -> Option<Vec<OpenDocument>> {
        if self.documents { self.inner.documents() } else { None }
    }
}

fn counting(documents: bool) -> (Client, Rc<RefCell<Asked>>) {
    let asked = Rc::new(RefCell::new(Asked::default()));
    let backend = Counting { inner: Headless::new(), documents, asked: asked.clone() };
    (Client::of(Server::new(Box::new(backend))), asked)
}

/// `v` with the document ids (numbered per process, so different in each session) as 0.
fn without_document_ids(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| match (k.as_str(), x) {
                    // A document's tree (`inspect_document`) has its id at the top.
                    ("id", _) if o.contains_key("currentPage") => (k.clone(), json!(0)),
                    ("document" | "workingDocument", Value::Object(d)) => {
                        let mut d = d.clone();
                        d.insert("id".into(), json!(0));
                        (k.clone(), Value::Object(d))
                    }
                    _ => (k.clone(), without_document_ids(x)),
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_document_ids).collect()),
        // The JSON text blocks say the same as `structuredContent`.
        Value::String(s) if s.starts_with('{') => {
            serde_json::from_str::<Value>(s).map(|j| Value::String(without_document_ids(&j).to_string())).unwrap_or_else(|_| v.clone())
        }
        _ => v.clone(),
    }
}

/// A short session of an agent: what each call answered.
fn session(c: &mut Client) -> Vec<Value> {
    let mut out = Vec::new();
    let r = c.call("create_node", json!({"type": "rectangle", "x": 10, "y": 10, "width": 40, "height": 20}));
    let id = serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap()["id"].clone();
    out.push(r);
    for i in 0..3 {
        out.push(c.call("set_properties", json!({"ids": [id], "props": {"width": 50 + i, "fill": "#3366FF"}})));
    }
    out.push(c.call("undo", json!({})));
    out.push(c.call("redo", json!({})));
    out.push(c.call("get_node", json!({"id": id})));
    out.push(c.call("inspect_document", json!({"outline": true})));
    // A conditional change on a stale revision is refused, naming the document as it was.
    out.push(c.call("set_properties", json!({"ids": [id], "props": {"x": 1}, "ifRevision": 0})));
    out.push(c.call("run_command", json!({"command": "proto.check", "params": {}})));
    out.push(c.call("batch", json!({"commands": [{"command": "node.set", "params": {"ids": [id], "props": {"y": 3}}}]})));
    out.push(c.call("undo", json!({})));
    out
}

#[test]
fn headless_tool_calls_read_the_documents_without_a_command_and_answer_as_with_one() {
    let (mut fast, fast_asked) = counting(true);
    let (mut listed, listed_asked) = counting(false);
    let a = session(&mut fast);
    let b = session(&mut listed);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(without_document_ids(x), without_document_ids(y));
    }
    // Every call still names its document and revision (a failed one in its error).
    let named =
        |r: &Value| [&r["structuredContent"]["document"], &r["structuredContent"]["error"]["document"]].iter().any(|d| d["revision"].is_u64());
    assert!(a.iter().all(named), "{a:?}");
    assert_eq!(a.iter().filter(|r| r["isError"] == true).count(), 1);
    let (fast_asked, listed_asked) = (fast_asked.borrow(), listed_asked.borrow());
    assert_eq!(fast_asked.lists, 0, "the headless backend read the documents with doc.list");
    // Without the backend's own reading: one before each call and one after.
    assert!(listed_asked.lists >= 2 * a.len(), "{}", listed_asked.lists);
    // And the same commands otherwise.
    assert_eq!(fast_asked.commands, listed_asked.commands);
}

#[test]
fn the_headless_documents_are_what_doc_list_answers_an_agent() {
    let mut h = Headless::new();
    h.call("engine.execute", json!({"command": "file.new", "params": {}})).unwrap();
    h.call("engine.execute", json!({"command": "shape.rectangle", "params": {"x": 0, "y": 0, "width": 5, "height": 5}})).unwrap();
    let listed = h.call("engine.execute", json!({"command": "doc.list", "params": {}, "agent": "Claude"})).unwrap();
    let listed: Vec<OpenDocument> = listed.as_array().unwrap().iter().filter_map(OpenDocument::from_listed).collect();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|d| d.revision.is_some() && d.index.is_some()));
    assert_eq!(listed.iter().filter(|d| d.front).count(), 1);
    assert_eq!(h.documents(), Some(listed));
}
