use serde_json::{Value, json};

use super::NodeId;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("command `{0}` is not available right now: {1}")]
    Disabled(String, String),
    /// `param`: the parameter the message is about, where the check knows its name. `step`: the
    /// batch step it came from and the command that step runs ([`EngineError::at_step`]).
    #[error("{}", bad_text(.cmd, .msg, .step.as_deref()))]
    BadParams { cmd: String, msg: String, param: Option<String>, step: Option<Box<(usize, String)>> },
    #[error("no open document")]
    NoDocument,
    /// A `document` (an id or a path) that names no open file.
    #[error("no open document {0} (`document` takes an `id` from doc.list, or an open file's path)")]
    NoSuchDocument(String),
    #[error("no such node {0}")]
    NoNode(NodeId),
    #[error("{0}")]
    Other(String),
    /// A path outside the automation roots ([`crate::file_access`]).
    #[error("{0}")]
    PathRejected(String),
    /// The command can't do anything where it was called (the prototype player in a session
    /// without a window): refused instead of answering as if it had.
    #[error("`{cmd}` is not supported here: {msg}")]
    Unsupported { cmd: String, msg: String },
    /// `ifRevision` named another revision than the document is at ([`crate::Session::execute`]).
    #[error("{0}")]
    Conflict(Box<Conflict>),
    /// An AI agent's undo or redo would take back a step somebody else made.
    #[error("{}", other_author_text(.cmd, .label, .author))]
    OtherAuthor { cmd: String, label: String, author: String },
    /// Step `step` of a batch, which runs `command`, failed with `error`.
    #[error("step {step} ({command}): {error}")]
    Step { step: usize, command: String, error: Box<EngineError> },
    /// A command run in the background sent a request to a server and runs again with the answer
    /// ([`background`]); never returned to other callers.
    #[error("waiting for the server")]
    Waiting,
    /// A bug: the command panicked. The document was rolled back.
    #[error("internal error in `{cmd}`: {msg} (the document was left as it was before; please report this bug)")]
    Internal { cmd: String, msg: String },
}

/// What a refused conditional change (`ifRevision`) says: the revision the caller expected, the
/// one the document is at, and the changes made in between, as far as the document remembers them.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub cmd: String,
    /// The document's id (`doc.list`) and name.
    pub document: (u64, String),
    pub expected: u64,
    pub actual: u64,
    /// The changes after `expected`, oldest first.
    pub since: Vec<crate::Change>,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Conflict { cmd, document: (_, name), expected, actual, since } = self;
        write!(f, "conflict: `{cmd}` expected revision {expected} of \"{name}\", but it is at revision {actual}: nothing was changed.")?;
        if expected > actual {
            return write!(f, " Revision {expected} is not one this document has had (was another document meant, or was the app restarted?).");
        }
        let missing = (actual - expected).saturating_sub(since.len() as u64);
        if !since.is_empty() {
            let list: Vec<String> = since.iter().map(|c| format!("{} ({})", c.label, c.author_name())).collect();
            write!(f, " Changed since: {}", list.join(", "))?;
            if missing > 0 {
                write!(f, ", after {missing} earlier change(s) no longer remembered")?;
            }
            write!(f, ".")?;
        }
        write!(f, " Inspect the document again, then retry with ifRevision {actual}.")
    }
}

fn other_author_text(cmd: &str, label: &str, author: &str) -> String {
    let verb = if cmd == "edit.redo" { "redo" } else { "undo" };
    format!(
        "`{cmd}` would {verb} \"{label}\", a step made by {author}, not by you: nothing was changed. Pass {{\"others\": true}} to {verb} it anyway"
    )
}

/// A [`EngineError::BadParams`] message. In a batch it leads with the step, and names the command
/// the step runs when the complaint is about the batch itself (a reference that doesn't resolve).
fn bad_text(cmd: &str, msg: &str, step: Option<&(usize, String)>) -> String {
    match step {
        Some((n, command)) if command != cmd && !command.is_empty() => format!("invalid parameters for `{cmd}`: {msg} (step {n} runs `{command}`)"),
        _ => format!("invalid parameters for `{cmd}`: {msg}"),
    }
}

impl From<figcraft_doc::DocError> for EngineError {
    fn from(e: figcraft_doc::DocError) -> Self {
        match e {
            figcraft_doc::DocError::NoNode(id) => EngineError::NoNode(id),
            other => EngineError::Other(other.to_string()),
        }
    }
}

impl EngineError {
    /// This error from step `step` of a batch, which runs `command`: its message and its
    /// [`detail`](Self::detail) say so. An invalid-parameters error stays one (its message leads
    /// with the step); the others are wrapped in [`EngineError::Step`]. Errors that steer the
    /// caller rather than report to it ([`EngineError::Waiting`], [`EngineError::Internal`]) stay
    /// as they are.
    pub(crate) fn at_step(self, step: usize, command: &str) -> Self {
        match self {
            EngineError::BadParams { cmd, msg, param, step: None } => {
                EngineError::BadParams { cmd, msg: format!("step {step}: {msg}"), param, step: Some(Box::new((step, command.to_string()))) }
            }
            e @ (EngineError::BadParams { .. } | EngineError::Waiting | EngineError::Internal { .. } | EngineError::Step { .. }) => e,
            e => EngineError::Step { step, command: command.to_string(), error: Box::new(e) },
        }
    }

    /// Note that this invalid-parameters error, whose message names step `step` already, is about
    /// that step, which runs `command` (nothing for other errors, or one that has a step).
    pub(crate) fn located(self, step: usize, command: &str) -> Self {
        match self {
            EngineError::BadParams { cmd, msg, param, step: None } => {
                EngineError::BadParams { cmd, msg, param, step: Some(Box::new((step, command.to_string()))) }
            }
            e => e,
        }
    }

    /// The stable, machine-readable kind of this error (see [`detail`](Self::detail)).
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::UnknownCommand(_) => "unknownCommand",
            EngineError::Disabled(..) => "notAvailable",
            EngineError::BadParams { .. } => "badParams",
            EngineError::NoDocument | EngineError::NoSuchDocument(_) => "noDocument",
            EngineError::NoNode(_) => "noSuchNode",
            EngineError::Other(_) => "failed",
            EngineError::PathRejected(_) => "pathRejected",
            EngineError::Unsupported { .. } => "unsupportedHere",
            EngineError::Conflict(_) => "conflict",
            EngineError::OtherAuthor { .. } => "otherAuthor",
            EngineError::Step { error, .. } => error.code(),
            EngineError::Waiting => "waiting",
            EngineError::Internal { .. } => "internal",
        }
    }

    /// This error for a program: `{code, message}` and, where the error knows them, `command`,
    /// `param`, `node`, `step` (with `command` then the command that step runs), `expected` and
    /// `actual` (revisions), `since` (the changes in between: `[{revision, label, author}]`),
    /// `document` (`{id, name}` or the name asked for), `label` and `author` (the step an undo was
    /// refused for). `message` is the text [`Display`](std::fmt::Display) gives; the codes are
    /// stable and listed in `docs/mcp.md`.
    pub fn detail(&self) -> Value {
        let mut d = json!({"code": self.code(), "message": self.to_string()});
        self.fill_detail(&mut d);
        d
    }

    fn fill_detail(&self, d: &mut Value) {
        match self {
            EngineError::UnknownCommand(cmd)
            | EngineError::Disabled(cmd, _)
            | EngineError::Internal { cmd, .. }
            | EngineError::Unsupported { cmd, .. } => {
                d["command"] = json!(cmd);
            }
            EngineError::BadParams { cmd, param, step, .. } => {
                d["command"] = json!(cmd);
                if let Some(param) = param {
                    d["param"] = json!(param);
                }
                if let Some(at) = step {
                    let (n, command) = at.as_ref();
                    d["step"] = json!(n);
                    if !command.is_empty() {
                        d["command"] = json!(command);
                    }
                }
            }
            EngineError::NoSuchDocument(name) => d["document"] = json!(name),
            EngineError::NoNode(id) => d["node"] = json!(id),
            EngineError::Conflict(c) => {
                d["command"] = json!(c.cmd);
                d["document"] = json!({"id": c.document.0, "name": c.document.1});
                d["expected"] = json!(c.expected);
                d["actual"] = json!(c.actual);
                d["since"] = Value::Array(c.since.iter().map(crate::Change::to_json).collect());
            }
            EngineError::OtherAuthor { cmd, label, author } => {
                d["command"] = json!(cmd);
                d["label"] = json!(label);
                d["author"] = json!(author);
            }
            EngineError::Step { step, command, error } => {
                error.fill_detail(d);
                d["step"] = json!(step);
                d["command"] = json!(command);
            }
            EngineError::NoDocument | EngineError::Other(_) | EngineError::PathRejected(_) | EngineError::Waiting => {}
        }
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

pub fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.to_string(), msg: msg.into(), param: None, step: None }
}

/// [`bad`] for a complaint about parameter `param`: [`EngineError::detail`] names it.
pub fn bad_param(cmd: &str, param: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.to_string(), msg: msg.into(), param: Some(param.to_string()), step: None }
}
