use super::{DocState, Document, EngineError, NodeId, Result, Session, Value, json};

/// The file [`Session::bring_forward`] put in front for one request, and the tab to bring back.
#[must_use = "pass it to `Session::put_back`"]
pub struct Forward {
    target: u64,
    shown: Option<u64>,
}

impl Session {
    /// The uid of open file `target`: its `id` (from `doc.list` or `document.inspect`) or its path.
    pub fn document_uid(&self, target: &Value) -> Result<u64> {
        let found = match target {
            Value::Number(n) => n.as_u64().and_then(|id| self.docs.iter().find(|d| d.uid == id)),
            Value::String(p) => self.docs.iter().find(|d| d.path.as_deref() == Some(p.as_str())),
            _ => None,
        };
        found.map(|d| d.uid).ok_or_else(|| EngineError::NoSuchDocument(target.to_string()))
    }

    /// Run `f` as the AI agent named `agent` (the client's name), or as the user (`None`): what it
    /// changes outside a command — a pointer gesture, typed text — is that author's in the undo
    /// history, as a command's is with [`Session::execute_as`].
    pub fn acting_as<T>(&mut self, agent: Option<&str>, f: impl FnOnce(&mut Self) -> T) -> T {
        let name = agent.map(crate::cmd::comment::agent_author);
        self.with_actor(name, f)
    }

    /// Run `f` with [`Session::agent`] set to `actor` (a name as it keeps it, `None` for the
    /// user), and put back who acted before afterwards, also when `f` panics (the panic then goes
    /// on to the caller): a caught panic must not leave the next request running as the agent.
    pub(crate) fn with_actor<T>(&mut self, actor: Option<String>, f: impl FnOnce(&mut Self) -> T) -> T {
        let was = std::mem::replace(&mut self.agent, actor);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)));
        self.agent = was;
        match r {
            Ok(v) => v,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Refuse an AI agent command `cmd` when it would replace or close the document in tab `uid`
    /// (with its branch tabs) and so throw away somebody else's unsaved work or undo history:
    /// nobody but the person who made them can tell what may go ([`EngineError::OtherAuthor`]).
    /// The user (no agent) is never refused here.
    pub(crate) fn refuse_discarding(&self, cmd: &str, uid: u64) -> Result<()> {
        let Some(me) = self.agent.as_deref() else { return Ok(()) };
        for ds in self.docs.iter().filter(|d| d.uid == uid || d.branch_of.as_ref().is_some_and(|(main, _)| *main == uid)) {
            let found = if self.live_doc(ds.uid) {
                ds.others_work(me, ds.live_undo_authors().map(|a| ("a step of the live session's undo history".to_string(), a.clone())))
            } else {
                ds.others_work(me, ds.history.undo.iter().rev().map(|h| (h.label.clone(), h.author.clone())))
            };
            if let Some((label, author)) = found {
                return Err(EngineError::OtherAuthor { cmd: cmd.to_string(), label, author });
            }
        }
        Ok(())
    }

    /// Put open file `target` in front for one request (a control call naming its `document`),
    /// without the user's tab changing on screen: run the request, then [`Session::put_back`].
    pub fn bring_forward(&mut self, target: &Value) -> Result<Forward> {
        let uid = self.document_uid(target)?;
        let shown = self.ds().ok().map(|d| d.uid);
        if let Some(i) = self.docs.iter().position(|d| d.uid == uid) {
            self.active = i;
        }
        self.forwarded = Some((uid, shown));
        Ok(Forward { target: uid, shown })
    }

    /// Bring back the tab that was in front before [`Session::bring_forward`], unless the request
    /// itself moved the front to another open file (`file.open`, `doc.switch`…), as it would have.
    pub fn put_back(&mut self, f: Forward) {
        // A request that chose its own file as the one in front (`doc.switch` to it) said so.
        let shown = self.forwarded.take().filter(|(target, _)| *target == f.target).map_or(f.shown, |(_, shown)| shown);
        let moved = self.docs.iter().any(|d| d.uid == f.target) && self.ds().ok().map(|d| d.uid) != Some(f.target);
        if let Some(i) = shown.and_then(|u| self.docs.iter().position(|d| d.uid == u)).filter(|_| !moved) {
            self.active = i;
        }
    }

    /// The file in front as the user sees it: while a request runs on another file
    /// ([`Session::bring_forward`]) that is still the tab it was, unless the request itself moved
    /// the front. `doc.list` marks this one `active`.
    pub fn front_uid(&self) -> Option<u64> {
        let active = self.ds().ok().map(|d| d.uid);
        match self.forwarded {
            Some((target, shown)) if active == Some(target) => shown,
            _ => active,
        }
    }

    /// A command put the active document in front on purpose (`doc.switch`): it stays in front
    /// after a request that runs on it through [`Session::bring_forward`].
    pub(crate) fn front_chosen(&mut self) {
        let active = self.ds().ok().map(|d| d.uid);
        if let Some((_, shown)) = self.forwarded.as_mut() {
            *shown = active;
        }
    }

    pub fn ds(&self) -> Result<&DocState> {
        self.docs.get(self.active).ok_or(EngineError::NoDocument)
    }

    pub fn ds_mut(&mut self) -> Result<&mut DocState> {
        self.docs.get_mut(self.active).ok_or(EngineError::NoDocument)
    }

    pub fn doc(&self) -> Result<&Document> {
        Ok(self.ds()?.doc.as_ref())
    }

    pub fn doc_mut(&mut self) -> Result<&mut Document> {
        Ok(self.ds_mut()?.doc_mut())
    }

    pub fn selection(&self) -> Vec<NodeId> {
        self.ds().map(|d| d.selection.clone()).unwrap_or_default()
    }

    pub fn set_selection(&mut self, ids: Vec<NodeId>) {
        if let Ok(ds) = self.ds_mut() {
            ds.selection = ids;
            ds.clean_selection();
            ds.revision += 1;
        }
    }

    pub fn page(&self) -> Result<NodeId> {
        Ok(self.ds()?.page)
    }

    /// A summary for status displays.
    pub fn status(&self) -> Value {
        match self.ds() {
            Ok(ds) => json!({
                "document": ds.doc.name,
                "path": ds.path,
                "dirty": ds.is_dirty(),
                "page": ds.page,
                "selection": ds.selection,
                "tool": self.tool.tool.id(),
                "zoom": ds.view.zoom,
                "undo": self.undo_depth().0,
                "redo": self.undo_depth().1,
                "documents": self.docs.len(),
            }),
            Err(_) => json!({"documents": 0}),
        }
    }
}
