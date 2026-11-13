use super::{
    Arc, CommandSpec, DocState, Document, ENDS_PREVIEW, EngineError, Node, NodeId, PREVIEW_READ_ONLY, Result, Session, Value, bad, board, branches,
    cmd, components, events, find_command, guard, library, normalize, params, textpreview,
};
use crate::error::bad_param;

/// Whether a command, or a batch wrapping one, imports a file passed as Base64 `data`.
fn command_carries_import_bytes(id: &str, params: &Value) -> bool {
    match id {
        "file.import" | "image.replace" => params::get(params, "data").is_some(),
        "command.batch" => params.get("commands").and_then(Value::as_array).is_some_and(|commands| {
            commands.iter().any(|c| {
                c.get("command").and_then(Value::as_str).zip(c.get("params")).is_some_and(|(id, params)| command_carries_import_bytes(id, params))
            })
        }),
        _ => false,
    }
}

/// The layers `check_finite_geometry` looks at: only those the command added or changed. The diff
/// skips what the two versions share, so the check costs about the size of the edit, not of the
/// document (#1672).
pub(crate) fn finite_check_candidates(doc: &Document, before: &Document) -> Vec<NodeId> {
    doc.changed_ids(before, usize::MAX).unwrap_or_default()
}

/// The reserved parameter every command takes: the revision the active document must be at.
pub const IF_REVISION: &str = "ifRevision";

/// A command's `ifRevision` (none, or `null`: unconditional) and, when it had one, its params
/// without it.
fn conditional(id: &str, params: &Value) -> Result<(Option<u64>, Option<Value>)> {
    let Some(given) = params.get(IF_REVISION) else { return Ok((None, None)) };
    let expected = match given {
        Value::Null => None,
        v => Some(v.as_u64().ok_or_else(|| bad_param(id, IF_REVISION, "`ifRevision` must be a revision number (a whole number, 0 or more)"))?),
    };
    let mut own = params.clone();
    if let Some(o) = own.as_object_mut() {
        o.remove(IF_REVISION);
    }
    Ok((expected, Some(own)))
}

impl Session {
    /// Refuse command `id` unless the active document is at revision `expected` (`ifRevision`).
    fn check_revision(&self, id: &str, expected: u64) -> Result<()> {
        let ds = self.ds()?;
        // Inside a group the steps before this one don't count: the group is one change, and each
        // step is checked against the document the group started from.
        let actual = ds.recorded_revision();
        if expected == actual {
            return Ok(());
        }
        let since = ds.changes_since(expected).into_iter().filter(|c| c.revision <= actual).collect();
        Err(EngineError::Conflict(Box::new(crate::error::Conflict {
            cmd: id.to_string(),
            document: (ds.uid, ds.doc.name.clone()),
            expected,
            actual,
            since,
        })))
    }

    /// Append a live event with the active document's identity (nothing without one). Inside a
    /// [`Session::grouped`] call the event is buffered and committed only when the group succeeds.
    fn record_event(&mut self, kind: &str, data: Value) {
        let (document, path) = match self.ds() {
            Ok(d) => (d.doc.name.clone(), d.path.clone()),
            Err(_) => (String::new(), None),
        };
        if self.group_depth > 0 {
            self.pending_events.push((kind.to_string(), document, path, data));
        } else {
            self.events.push(kind, &document, path.as_deref(), data);
        }
    }

    /// Restore the batch context an inner command had at entry when it panicked.
    pub(crate) fn restore_after_internal_error(&mut self, group_depth: usize, pending_events: usize) {
        self.group_depth = group_depth;
        self.pending_events.truncate(pending_events);
    }

    /// Commit events buffered by a successful group.
    pub(super) fn flush_pending_events(&mut self) {
        let pending = std::mem::take(&mut self.pending_events);
        for (kind, document, path, data) in pending {
            self.events.push(&kind, &document, path.as_deref(), data);
        }
    }

    /// [`Session::execute`] for an AI agent named `agent` (the client's name; empty when it gave
    /// none), or for the user (`None`): the comments it makes are signed with the agent's name and
    /// marked as an agent's, and the app shows where it works ([`Session::agent_presence_at`]).
    pub fn execute_as(&mut self, agent: Option<&str>, id: &str, params: &Value) -> Result<Value> {
        let was = std::mem::replace(&mut self.agent, agent.map(cmd::comment::agent_author));
        let before = agent.and_then(|_| self.agent_before());
        let r = self.execute(id, params);
        if agent.is_some() {
            self.note_agent(before);
        }
        self.agent = was;
        r
    }

    /// Run a command by id. Mutations become one undo step; a failure or panic leaves the document
    /// as it was.
    ///
    /// Every command takes the reserved parameter `ifRevision` (a number): the command runs only
    /// when the active document is at that revision ([`DocState::doc_revision`]), and is refused
    /// with [`EngineError::Conflict`] when somebody changed the document since the caller read it.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Value> {
        // A command that runs other commands is one change with them, counted when it ends.
        let outermost = self.executing == 0;
        self.executing += 1;
        let result = self.execute_command(id, params);
        self.executing = if outermost { 0 } else { self.executing.saturating_sub(1) };
        result
    }

    /// Whether what runs now is a change of its own for the document's revision: not a step of a
    /// group and not a command another command runs (those are counted with what they are part of).
    pub(super) fn counts_alone(&self) -> bool {
        self.group_depth == 0 && self.executing <= 1
    }

    fn execute_command(&mut self, id: &str, params: &Value) -> Result<Value> {
        let spec = find_command(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        // Taken out before the command's own parameters are checked: it is every command's.
        let (if_revision, own) = conditional(id, params)?;
        let params = own.as_ref().unwrap_or(params);
        crate::cmd::refuse_unknown_params(id, params)?;
        board::refuse_on_board(self, id)?;
        // Revert replaces the document's uid: refuse before ending previews or syncing branches.
        if id == "file.revert" && self.live().doc_uid.is_some() {
            return Err(EngineError::Disabled(id.into(), "leave the live session before reverting to the saved file".into()));
        }
        branches::sync(self);
        self.apply_view_prefs();
        // A change nothing recorded gets its revision before this command can make the next one.
        // Not inside a group or another command: those make one change, counted when they end.
        if self.counts_alone() {
            self.docs.iter_mut().for_each(DocState::settle_revision);
        }
        if let Some(expected) = if_revision {
            self.check_revision(id, expected)?;
        }
        if textpreview::COMMANDS.contains(&id) {
            // A live preview is no edit: no undo step, normalisation or rollback here.
            let r = guard::run(id, || (spec.run)(self, params));
            if r.is_err() {
                textpreview::end(self);
            }
            return r;
        }
        // Every other command acts on the document itself, not on a live text preview.
        textpreview::end(self);
        if ENDS_PREVIEW.contains(&id)
            && (!matches!(id, "file.save" | "file.saveAs") || cmd::file::save_ends_preview(self, id, params))
            && let Ok(ds) = self.ds_mut()
        {
            ds.end_preview();
        }
        let before = self.ds().ok().map(|d| (d.doc.clone(), d.selection.clone(), d.page, d.uid));
        // The revision this command starts from (a command of its own, not a step of a group):
        // whatever it does is one change, and none when it fails.
        let revision_before = self.ds().ok().filter(|_| self.counts_alone()).map(DocState::revision_mark);
        // Cut and copy write the clipboard before a later check can refuse the command.
        let clipboard_before = self.clipboard.clone();
        // An inner command of a batch can panic while the outer group is still open; remember the
        // context so the panic does not clear the group's buffered events.
        let group_depth_before = self.group_depth;
        let pending_events_before = self.pending_events.len();
        let result = guard::run(id, || (spec.run)(self, params))
            .and_then(|v| self.check_instance_structure(id, &before).map(|()| v))
            .and_then(|v| self.check_preview(id, &before).map(|()| v))
            .and_then(|v| self.check_live_access(id, &before).map(|()| v))
            .and_then(|v| self.check_finite_geometry(id, &before).map(|()| v))
            // Normalising is part of the command: a panic in it rolls the edit back too (#1085).
            .and_then(|v| {
                guard::run(id, || {
                    self.normalize_edit(spec, &before);
                    Ok(v)
                })
            });
        match &result {
            Ok(_) => {
                self.after_command(spec, before, revision_before.as_ref());
                self.apply_view_prefs();
                if spec.history && id != "edit.repeat" {
                    self.last_command = Some((id.to_string(), params.clone()));
                }
                // Imported bytes already live in the document: a Base64 copy in the log would stay
                // for the whole session.
                if !command_carries_import_bytes(id, params) {
                    self.journal.push((id.to_string(), params.clone()));
                    if self.journal.len() > 5000 {
                        self.journal.drain(..1000);
                    }
                }
            }
            Err(_) => {
                // Roll back any partial edit — also during an interaction, where `before` is the
                // interaction's last valid preview: restore it and leave the gesture itself (its
                // own snapshot) alone.
                if let Some((doc, sel, page, uid)) = before
                    && let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid)
                {
                    ds.doc = doc;
                    ds.selection = sel;
                    ds.page = page;
                    // What a group or an undo inside it counted is taken back with the edit.
                    if let Some(mark) = revision_before {
                        ds.restore_revision(mark);
                    }
                }
                self.clipboard = clipboard_before;
            }
        }
        self.after_comment_command(id, params, &result, group_depth_before, pending_events_before);
        result
    }

    /// Record the live event a successful mutating `comment.*` command makes. A panic caught by the
    /// guard can leave a group's event buffer open: restore the depth and buffer it had when the
    /// command started (`group_depth`, `pending_events`), not the whole session's.
    fn after_comment_command(&mut self, id: &str, params: &Value, result: &Result<Value>, group_depth: usize, pending_events: usize) {
        match result {
            Ok(v) => {
                if let Some(kind) = events::comment_event_kind(id) {
                    let comment_id = v.get("id").and_then(Value::as_u64).or_else(|| params.get("id").and_then(params::id_of).map(|n| n.0));
                    self.record_event(
                        kind,
                        serde_json::json!({"command": id, "commentId": comment_id, "reply": v.get("reply").cloned().unwrap_or(Value::Null)}),
                    );
                }
            }
            Err(EngineError::Internal { .. }) => self.restore_after_internal_error(group_depth, pending_events),
            Err(_) => {}
        }
    }

    /// A version shown in place of the document is read-only: refuse an edit (it is rolled back).
    fn check_preview(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        if ds.preview.is_none() || Arc::ptr_eq(&ds.doc, bdoc) || id == "version.preview" {
            return Ok(());
        }
        Err(EngineError::Disabled(id.to_string(), PREVIEW_READ_ONLY.into()))
    }

    /// A layer's position and size stay finite: infinity and NaN are written as `null`, so a file
    /// holding them saves but can't be reopened. Refuse such an edit (it is rolled back).
    fn check_finite_geometry(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        if Arc::ptr_eq(&ds.doc, bdoc) {
            return Ok(());
        }
        let broken = |n: &Node| !(n.transform.as_coeffs().iter().all(|c| c.is_finite()) && n.width.is_finite() && n.height.is_finite());
        let untouched = |n: &Node| bdoc.node_arc(n.id).zip(ds.doc.node_arc(n.id)).is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        match finite_check_candidates(&ds.doc, bdoc).iter().filter_map(|i| ds.doc.node(*i)).find(|n| broken(n) && !untouched(n)) {
            Some(n) => Err(bad(id, format!("\"{}\" would get a position or size beyond the number range", n.name))),
            None => Ok(()),
        }
    }

    /// A live session's document you may only view (or that is on its way to a server) is
    /// read-only: refuse an edit (it is rolled back). With a comment link, one that changes only
    /// comment threads goes through.
    fn check_live_access(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        match self.live_read_only(*uid) {
            Some(why) if !Arc::ptr_eq(&ds.doc, bdoc) && !self.live_comment_edit(*uid, &ds.doc) => {
                Err(EngineError::Disabled(id.to_string(), why.into()))
            }
            _ => Ok(()),
        }
    }

    /// Instances can't gain, lose or reorder layers, nor move, resize or re-constrain the layers in
    /// them, library copies are read-only and components can't contain their own instances: refuse
    /// such an edit (it is rolled back).
    pub(crate) fn check_instance_structure(&self, id: &str, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) -> Result<()> {
        let Some((bdoc, _, _, uid)) = before else { return Ok(()) };
        let Some(ds) = self.docs.iter().find(|d| d.uid == *uid) else { return Ok(()) };
        if Arc::ptr_eq(&ds.doc, bdoc) {
            return Ok(());
        }
        // "Repeat" runs the repeated command.
        let effective = match (&self.last_command, id) {
            (Some((last, _)), "edit.repeat") => last.as_str(),
            _ => id,
        };
        library::check_read_only(effective, bdoc, &ds.doc)
            .and_then(|()| components::check_structure_edit(bdoc, &ds.doc))
            .and_then(|()| components::check_override_edit(bdoc, &ds.doc))
            .and_then(|()| components::check_component_cycles(bdoc, &ds.doc))
            .map_err(|m| EngineError::Disabled(id.to_string(), m))
    }

    /// Normalise what a command changed in the document it ran on (text boxes, groups, instances,
    /// auto layout…).
    fn normalize_edit(&mut self, spec: &CommandSpec, before: &Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>) {
        let Some((bdoc, _, _, uid)) = before else { return };
        let Some(ds) = self.docs.iter_mut().find(|d| d.uid == *uid) else { return };
        // Saving may adopt the chosen filename as the document title. The encoded snapshot is
        // already final; normalising it again would detach it from the saved baseline.
        if !Arc::ptr_eq(&ds.doc, bdoc) && !matches!(spec.id, "file.save" | "file.saveAs") {
            normalize(ds.doc_mut(), bdoc);
        }
    }

    /// Bookkeeping after a successful command (normalised already): push undo, bump revision.
    /// `counted_from`: the revision the command started from, when it is a command of its own.
    fn after_command(
        &mut self,
        spec: &CommandSpec,
        before: Option<(Arc<Document>, Vec<NodeId>, NodeId, u64)>,
        counted_from: Option<&crate::doc_state::RevisionMark>,
    ) {
        let Some((bdoc, bsel, bpage, uid)) = before else {
            if let Ok(ds) = self.ds_mut() {
                ds.revision += 1;
            }
            return;
        };
        let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid) else { return };
        let edit = !Arc::ptr_eq(&ds.doc, &bdoc) && ds.interaction.is_none();
        ds.clean_selection();
        ds.revision += 1;
        // A batch, an undo or a redo inside the command counted its change already: what the
        // command did to the document after that (normalising it) is the same change.
        if edit && counted_from.is_some_and(|mark| mark.counted_since(ds)) {
            ds.same_revision(false);
        }
        if edit {
            self.record(uid, spec.history.then_some(spec.label), bdoc, bsel, bpage);
        }
    }

    /// Record an edit of document `uid` (it was `before`, shown with `selection` on `page`) as one
    /// undo step called `label` (`None`: not undoable). On a live session's document the edit is
    /// synced and the step is the user's own ([`live`]); otherwise it is a snapshot of `before`.
    ///
    /// This is where an edit is committed, so it is also where the document gets its next
    /// revision ([`DocState::doc_revision`]) and the step its author (the agent the command runs
    /// for, else the user). A group's edits are one change, counted when the group ends.
    pub(crate) fn record(&mut self, uid: u64, label: Option<&str>, before: Arc<Document>, selection: Vec<NodeId>, page: NodeId) {
        let grouped = self.group_depth > 0;
        let author = self.agent.clone();
        // Undo and redo count themselves: the command around them has nothing left to count.
        if self.counts_alone()
            && let Some(ds) = self.docs.iter_mut().find(|d| d.uid == uid && d.unrecorded_change())
        {
            ds.committed(label.unwrap_or("Edit"), author.as_deref());
        }
        if self.live_doc(uid) {
            return self.live_edit(uid, label);
        }
        if let (Some(label), Some(ds)) = (label, self.docs.iter_mut().find(|d| d.uid == uid)) {
            // A group becomes one step at its end: its edits add to the step it already pushed,
            // so a long batch keeps one snapshot, not one per edit (#1099).
            if grouped && ds.history.group_step == Some(ds.history.undo.len()) {
                ds.history.redo.clear();
                return;
            }
            ds.push_undo(label, author.as_deref(), before, selection, page);
            if grouped {
                ds.history.group_step = Some(ds.history.undo.len());
            }
        }
    }
}
