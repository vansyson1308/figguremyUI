# Independent review R2 (MCP passes 1–2, UI pass 1; `8b477ef0..81338c17`) — read, not run. Lead's triage inline.

## R2-01 · P1 · The revision a result reports is read in a separate call (attached: a person's change in between counts as "seen")
- Where: `crates/mcp/src/tools.rs:1000-1031` (`about_document` → `open_documents` → `doc.list`, l.1011); `inspect_document`/`get_node` bisection; `check_design {record}`; `create_node` (create, then `node.get`).
- Fix (lead): the engine reply carries the revision atomically. `engine.execute` (headless `dispatch` and app `control::handle`, inside `in_document`) returns the target document's `revision` computed right after `execute` (e.g. result `{…}` stays as is and the reply envelope gains `revision`, or the backend exposes `take_revision()` like `take_error_detail()`; choose the least intrusive that works on both backends; for an app that doesn't send it, fall back to reading it **before** the tool's first call). Read tools take the revision before the first backend call. If before ≠ after, report the earlier one. Close: test where a user command is injected between the tool's command and its identity read (headless: a backend wrapper that runs a user command between calls; attached: the app harness once the app side lands) → a later `ifRevision` equal to the reported revision is refused.

## R2-02 · P1 · An agent's command during a person's gesture or text edit is not counted, not its own step, and can be silently discarded
- Where: `crates/engine/src/session/exec.rs:332` (`&& ds.interaction.is_none()`), `doc_state.rs` `unrecorded_change`, `session/undo.rs:95` (`preview` rebuilds from the start snapshot), `:117-137` (`commit_interaction`/`cancel_interaction`); text editing is an interaction (`canvas/textedit.rs:94`).
- Fix: in `execute_command`, refuse a command run for an agent (`self.agent.is_some()`) that would change the target document while that document has `interaction.is_some()` (check after run by `Arc::ptr_eq` and roll back), code `busy`: "a person is in the middle of a gesture or text edit on this file; nothing was changed, try again shortly". Same for an agent's `begin_interaction` (pointer, type_text) while one is active. Close: engine tests (user begin_interaction + preview, then execute_as(agent, node.set) → refused, document and revision unchanged; then user text edit → same); MCP-level code `busy`.

## R2-03 · P1 · The refusal invites the bypass of "don't undo other people's steps"
- Where: `crates/engine/src/error.rs:85`; `crates/mcp/src/server.rs:46`; `tools.rs:206` (`others` description), `tools.rs:457`; `docs/mcp.md:405,1573,1745`.
- Fix: message "…made by {author}, not by you: nothing was changed. Leave it, or ask them; pass `others: true` only if the person asked you to take back their step"; same in the `others` schema description, `undo`/`redo` descriptions and INSTRUCTIONS. Close: `guidance.rs` asserts the wording in all three.

## R2-04 · lead: P0 · An agent can throw away a person's unsaved work and the whole undo history
- Where: `crates/engine/src/cmd/file/open.rs:123-147` (`revert`), `cmd/file.rs:231-239` (`file.revert`), `cmd/file.rs:475-480` (`file.close {force: true}`), reachable through `run_command`.
- Fix: for an agent, refuse `file.revert` and `file.close {force}` (and any other command that discards unsaved changes or history: check `file.close` without force when the app would prompt, `file.new` replacing?, `version.restore`, `branch.*` switching, `doc.closeAll`-like commands — grep for every call of the function(s) that drop a `DocState` or replace its history) when the document has unsaved steps not authored by that agent; code `otherAuthor`, message names who made them. For everyone: keep a version (autosave/history version) before revert. Close: execute_as tests: user edit → agent file.revert → refused, document and history unchanged; same for file.close {force: true} and each other path found.

## R2-05 · P2 · `key` tool commands run as the user (headless); attached every `key` runs as the user today
- Where: `crates/mcp/src/headless.rs:104-106` (`Headless::execute` → `execute_as(None, …)`), called from `key` at 194, 213, 217, 221, 224, 240, 244 inside `as_agent` (384).
- Fix: Headless runs with the session's current agent. Close: user step, then agent `key` Cmd+Z → `otherAuthor`; `key Delete` → `history.steps[0].author` is the agent.

## R2-06 · P2 · `acting_as` / `execute_as` / `Headless::as_agent` don't restore the actor after a panic
- Where: `crates/engine/src/session/access.rs:24-29`; `session/exec.rs:94-104`; `headless.rs` `as_agent`.
- Fix (engine side): restore through a drop guard (or catch_unwind + resume_unwind). App side (UI pass 2): wrap FigApp-level handlers with an app-level guard that restores `app.session.agent` on error and on panic.
- Close: test where `f` panics (caught by the caller) and `session.agent` is `None` afterwards.

## R2-07 · P2 · The panel hides agent-proposed end states, and editing their reason silently accepts them
- Where: `crates/ui-egui/src/panels/design_context/flow.rs:260-310`, `:277-279`; engine `crates/engine/src/proto_verify/end_state.rs:100-102` (a user's `set` on a proposal sets Accepted). Same for alternatives' `proposal`.
- Fix: engine — a user's `proto.endState.set` that only changes `reason` on a proposal keeps it proposed (accept only via `proto.endState.accept` or explicit `on: true`... decide: only `accept` accepts; document). UI pass 2 shows "Proposed by <agent>" + Accept.
- Close: UI test: agent proposes; panel shows proposed; editing the reason leaves `status: proposed`; Accept → accepted.

## R2-08 · P2 · The simulator's stated limits are incomplete; module docs describe the player wrongly
- Where: `crates/engine/src/proto_sim.rs:41-45` (says the player drops On click after a Mouse up; after M11.11 it runs both); `proto_verify/run.rs:115-121` (`NOT_MODELLED`, 5 entries).
- Not stated anywhere an agent reads: mouse up + click; mouse down + while pressing; hover/press last until the next step; click on a hovered variant's own interaction fails in the simulator; a scroll-to that can't scroll counts as fired; change-to a non-component refused; a delay under a hover; restart and section memory; the player's own keys; widgets.
- Fix (after UI pass 2 lands, since it changes the simulator): correct `proto_sim.rs` docs; a `knownDifferences` list in simulate/scenario results naming each remaining difference; repeat it in the `simulate_prototype` and `run_scenarios` descriptions and the `verify_prototype` prompt. Close: a guidance test keeps the list in step with the `difference_*` parity test names.

## R2-09 · P3 · Document text goes raw into prose error messages
- Where: `crates/engine/src/proto_sim.rs:247-251` (`label`), `error.rs:66-80` (conflict: raw document name, step labels; batch `label` chosen by caller).
- Fix: clip to 60 chars and replace control characters/backticks (escape_debug) in `label()` and conflict/other-author text. Close: layer name with newlines → one-line clipped error.

## R2-10 · P3 · Tools that change the document without `ifRevision`: `pointer` (drag), `key` (Delete), `type_text`, `check_design {record}`, `save_file`.
- Fix: accept `ifRevision` on these (checked in the same request); list them in CONDITIONAL_TOOLS and the instructions.

## R2-11 · P3 · Identity edges
- `open_file` reads `document.inspect {depth:1}` of the front tab in a second request (attached may describe another file) → use the opened file's id.
- `run_command file.new`/`file.open`/`file.duplicate` don't move the working document, unlike `open_file` → make them move it (the agent asked for it) and say so in the result.
- `file.close {index}`/`doc.switch {index}` act on another tab while the result names the working document → name the document they acted on.
- Resources read before any tool call use the front tab and don't settle the working document → settle it.

## R2-12 · P3 · Docs and capability details
- `docs/mcp.md:1544-1547` says saving doesn't change the revision; the first save renames the document (+1).
- `get_capabilities` hard-codes `"product": "FigCraft"`, the `formats` lists and `player: attached` → product from one constant (the rebrand will change it), formats from the format crate's real lists.
- `view.designContext` silently accepts an unknown `section` → refuse with the list.

## Q7 (panel and person-only actions over the control channel)
- `ui.pointer`/`ui.key`/`ui.type` drive the engine canvas, not egui widgets; they can't press panel buttons.
- `ui.input` (raw egui events), `ui.menu.invoke`, `ui.set` act as the person; MCP exposes none. Recommendation (lead accepts): the app refuses those methods when the request carries `agent`.

## Unverified suspicions (for the fixer to check; fix if real)
- Panel `design.forNode` runs through `execute`, which calls `textpreview::end`: with the panel open and a layer selected, a hover style/font preview may flicker (UI pass 2).
- `multiline_field`: a field that loses visibility while focused keeps its draft and may commit it later over a newer value (UI pass 2).
- Snapshot undo: an agent's undo of its own top step restores the whole document, also wiping a person's later change that left no undo step (first-save rename, unrecorded direct edits) without `others` (fixer).
- `text.rescanFonts`/`loadFont`/`font.download` call `doc_mut()` with nothing to refit → empty undo step + revision bump (fixer).
- `lift_temp` doesn't restore `scroll`/`history` from the snapshot (UI pass 2).
- While a person previews a version, agent reads return the version's layers while `revision` is the real document's (fixer).
