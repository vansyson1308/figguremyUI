# Independent review R1: engine side of M11 (`011f08e5..7aca5d80`, crates/) — read, not run

Reviewer found no P0. Lead's triage is given per finding (severity may differ from the reviewer's).

## R-01 · P1 · `design.finding.recheck` marks unfixed findings `verified` when the reported node list shifts
- Where: `crates/engine/src/design/findings.rs:363-376` (`rerun`), `crates/engine/src/design/checks.rs:147-149` (`same_problem` = rule + exact `nodes`), `crates/engine/src/proto_verify/scenario.rs:172`, `:214-225`, `crates/engine/src/proto_verify/flow.rs:355-366`, `:386-398`.
- A stored finding is "still there" only if a new result has the identical node vector. Rules with incidental nodes:
  - `scenario.fails` uses `[failing step's layer, start]`: fix step 3, it now fails at step 5 on another layer → verified while S1 still fails.
  - `scenario.unvisited` uses the whole unvisited list: visit one of three → verified with two still unvisited.
  - `flow.trappedOverlay` uses `[overlay, first opener]`: two openers, fix the first → verified while the second still traps.
  - `flow.noStart` uses `[first frame in reading order]`: reorder frames → verified with no flow start added.
- Fix: give `Found` a stable subject key (scenario id, overlay id, page id) and match on rule + key; `nodes` for display.
- Close: tests for each of the four cases ending `open`, `passed: false`.

## R-02 · P1 · `contrast.text` recheck counts "can no longer be measured" as fixed
- Where: `crates/engine/src/design/checks/contrast.rs:344-357` (`failing` keeps only `Reading::Fails`), `findings.rs:369-375`.
- Record a contrast finding; change the card fill to a gradient / set a blend mode on a parent / put a layer in front → reading `Unmeasured`, recheck stores `verified`.
- Fix: for a `contrast.text` recheck, an `Unmeasured` reading of the same layer keeps the finding open with the reason as detail.
- Close: record → make backdrop a gradient → recheck gives `passed: false`, detail names the gradient.

## R-03 · P1 · Simulator vs player mismatches; no parity test (handled by the UI builder: parity test + player fix)
Cases (player files under `crates/ui-egui/src/present/`):
1. Pressed state swallows the click (player `input.rs:208-214`, `:341`): L with `onPress → setVariable v=1` (or variant `onPress → changeTo Pressed`) and `onClick → navigate B`. Player: stays on A. Sim `{click: L}`: B. (= baseline F-01.)
2. Hover then click that doesn't navigate is undone on leave (player `actions.rs:63,77`, `input.rs:209-212` restores snapshot incl. overlays/history): L `onHover → setVariable h=true`, `onClick → openOverlay O`. Player: hover, click, move onto O: O closes. Sim: O stays open.
3. Hover then click that navigates makes the hover permanent (player `navigate.rs:191` `go` drops `temp` without reverting). Player: on B with h=true. Sim reverts first (`steps.rs:70`): h=false.
4. `mouseUp` that does something suppresses `onClick` (player `input.rs:335-339`).
5. A click whose only action is a scroll-to that can't scroll: player returns false (`actions.rs:130-134`); sim reports `fired: true` (`proto_sim/actions.rs:161-170`).
6. `changeTo` a non-component: sim refuses (`proto_sim/actions.rs:258-265`), player repoints the instance (`actions.rs:206-221`).
7. After-delay firing during an active hover: player keeps the hover's changes; sim's `wait` reverts first.
8. Section memory after the player's Restart (`navigate.rs:204-219`) has no sim equivalent.
Found equal: history push/pop, navigate with overlay open, back closing top overlay first, swap, delay reset on overlay open, key lookup order, conditional evaluation, `play_interactions` after change-to, `modes_for`.

## R-04 · P1 · `flow.brokenTarget` misses broken actions that instances play from mains on another page
- Where: `crates/engine/src/proto_verify/flow.rs:315-334` walks `doc.subtree(page)` of the unplayed file (`check.rs:58`); only `graph()` plays a copy (`flow.rs:190-192`).
- Main component on page "Components" with root `onClick → navigate <deleted frame>`; instances on "Screens": check on Screens reports no blocker, `ok: true`; the edge appears in `out` with no `to`. Variants reached by change-to are skipped the same way.
- Fix: run `broken()` over the played copy's edges for each screen and overlay, naming both the instance and the main (dedupe so an instance-heavy page doesn't repeat one main's problem N times: one finding per main action, nodes = [main layer, first instance]).
- Close: test with the main on page 2 and the instance on page 1; checking page 1 reports `flow.brokenTarget`.

## R-05 · P1 · An agent can dismiss measured findings and erase a person's records through commands with no authority check
- Where: `crates/engine/src/proto_verify/end_state.rs:14-51`, `scenario.rs:54-112` (no `s.agent` anywhere in `proto_verify`), `crates/engine/src/design/context.rs:285-298`, `:247-283`.
  - `proto.endState.set` then recheck verifies `flow.deadEnd` (no author, no reason required).
  - `proto.scenario.set` / `.remove` then recheck verifies `scenario.fails`; scenarios have no author.
  - `design.assumption.remove` deletes an assumption a person marked supported, with its evidence.
  - `design.assumption.set {id, evidence}` replaces the person's evidence while the status stays `supported`.
- Lead's decision on the fix:
  - `EndState` gains `author` and `status` (`proposed` | `accepted`, same enum as decisions or its own); an agent-declared end state is `proposed`, requires a non-empty `reason`, and does NOT exempt its frame from `flow.deadEnd` until a person accepts it (`proto.endState.accept {id}`, refused to agents); the dead-end finding's evidence then says "declared as an end state by <agent>, waiting for a person to accept". A user-declared one is accepted at once (reason optional).
  - `Scenario` gains `author`. An agent may add scenarios, and edit or remove only scenarios it authored; a person's scenario is refused ("S1 was written by user; add your own scenario or ask them to change it").
  - Agents: refused `design.assumption.remove` and evidence/text edits on assumptions whose status isn't `untested`.
- Close: `execute_as` tests for each path, refused or leaving the finding open.

## R-06 · lead: P2, documented limitation · Concurrent edits of the design context in a live session lose one side
- Where: `crates/collab/src/docprops.rs:356` (`Design` whole-value), `crates/collab/src/merge.rs:175`; `set_scalar` (`docprops.rs:334-340`) applies a peer's context with no size check; peers from before this variant can't decode a change set carrying it; no collab tests for `Design`.
- Lead's decision: per-entry sync is out of this milestone (goes to the backlog with this evidence). In this pass: (a) add collab tests that pin today's behaviour (a design change syncs to a second client; two concurrent edits: last writer wins, stated in the test name), (b) clamp a peer's context on apply (`MAX_ENTRIES`, `MAX_TEXT`: refuse with `Error::Invalid`), (c) nothing else.

## R-07 · P2 · One bad value in `design` makes the whole file unopenable; nothing bounds a loaded context
- Where: `crates/doc/src/design.rs:255-270` (`Scenario.id/name/start` required), `:299-317` (tagged `StepAction`), enums; `crates/format/src/lib.rs:522`.
- `"design":{"scenarios":[{"id":"S1","name":"x"}]}`, `"stage":"shipping"`, or a step `{"type":"swipe"}` fails `Document` deserialization. `validate()`/`repair()` never look at `design`. `missing_links` (`crates/engine/src/design/checks/context.rs:22-32`) is quadratic.
- Fix: deserialize `Document.design` leniently: if the value doesn't parse, the document opens with an empty context and the loader reports it (find how load issues/`repairs`/`warnings` are surfaced by `figcraft_format` and `file.open`, and add one: "the design context couldn't be read and was left out: <serde error>"); clamp on load (truncate lists to MAX_ENTRIES and strings to MAX_TEXT at char boundaries, reported the same way); build missing links with a set.
- Close: load tests for the three malformed inputs (file opens, issue listed, layers intact); a 100k-link file answers `design.get` quickly.

## R-08 · P2 · Flow rules are noisy on ordinary files
- Where: `flow.rs:229-231` (`has_interactions` counts any layer on the page, component hover states included), `:238`, `:180-188`, `:405-409`.
  - A static page of screens plus one button component set with `onHover → changeTo` yields `flow.noStart`, a major `flow.deadEnd` on the first frame, `ok: false`, and an unreachable note per other frame.
  - If the first frame in reading order is a component set, the dead end is reported on the component set.
  - A single-screen interactive prototype is always a major dead end.
  - Frames reached only from another page's flow are "unreachable".
- Fix: a page counts as prototyped only when it has a declared flow start or a navigation/overlay action (navigate, back, open/swap/close overlay, openUrl) played by a top-level frame's subtree; components and component sets are never starts, dead ends or unreachable; a start screen with no navigation anywhere on the page isn't a dead end; a frame that is the target of an interaction on another page is reachable.
- Close: tests for the layouts above giving no major finding.

## R-09 · P2 · `flow.trappedOverlay` misses a trap reached by swap
- Where: `flow.rs:143`, `:171-177`, `:217-228`. Button opens A with close-on-outside off; A has `swapOverlay → B`; B has nothing: the swap keeps A's flag (`proto_sim/actions.rs:242-243`), viewer stuck in B, yet `ok: true`.
- Close: a test for that prototype reporting B.

## R-10 · P2 · `design.alternative.choose` lets an agent set and overwrite the set's `chosen`
- Where: `crates/engine/src/design/alternatives.rs:314-338` (`:333`).
- Fix: an agent's choose records a proposal only (a `proposed` decision linked from the set as `proposedChoice`/`proposal`); `chosen` changes when a person runs choose, or accepts that decision (`design.decision.accept` on the proposal's decision applies the choice).
- Close: `execute_as` test where the person's `chosen` survives an agent's choose.

## R-11 · P2 · Attribution can be forged or lost
- Where: `findings.rs:216-223`, `:268-280`, `:319-329`; `context.rs:340-373`.
  - An agent can add `{kind: judgement, source: "user"}`, or feedback under any person's name, with nothing marking it as relayed.
  - `design.finding.address` or `reopen` by an agent on an accepted or verified finding clears the person's reason, `verifiedBy` and `verification`.
  - An agent's `design.decision.set` rewrites a user decision's title and rationale; it goes back to proposed but still reads `author: "user"`.
- Lead's decision on the fix: findings gain `recordedBy` (who ran the command; left out when equal to `source`); an agent's judgement always has `source` = the agent (a `source` param naming anyone else is refused for judgements); feedback an agent records keeps the person in `source` and the agent in `recordedBy` ("relayed by"). An agent may not `address` or `reopen` a finding a person accepted or verified (refused, naming who settled it). Decisions gain `editedBy` (last editor when not the author; left out otherwise).
- Also state in the module docs of `design.rs` that these rules are a convention between cooperating parties, not a security boundary: the control channel without `agent`, the command line and replayed undo steps all act as the user.
- Close: tests for each path.

## R-12 · P2 · `design.check.record` is all-or-nothing at 500 findings
- Where: `findings.rs:182`, `crates/engine/src/design/input.rs:185-190`. 501 results can never be recorded; the message blames stored findings.
- Fix: store up to the cap, return `truncated: <how many were left out>`, and say to narrow `scope`/`rules`.

## R-13 · P3 · Smaller items (do the ones marked DO)
- DO: Empty scope uses the wrong page: `proto_verify/check.rs:93` uses `doc.current_page` (the page at last save), not the session's page; node-less flow findings recheck there.
- DO: `flow.brokenTarget` is stricter than the player for change-to (`flow.rs:299-313` ignores the `owning_instance` fallback in `proto_run.rs:163`): false blocker for a swap the player performs.
- DO: `target.small` `about` must say the SC 2.5.8 exceptions (spacing, inline, equivalent control, essential) aren't evaluated, so a result is "smaller than 24×24", to be judged against those exceptions.
- DO: Text with a stroke and no fill reads as `Passes` with nothing measured (`contrast.rs:313-315`) → `Unmeasured`.
- DO: Compare `paintOnly`: `fonts || !layout` (`compare.rs:278`) calls a font change plus rearranged non-text layers a restyle → paintOnly requires no layout change, full stop.
- DO: Expectations: a boolean variable expected as `"banana"` is coerced to `false` and can pass (`proto_run::literal`) → an expectation value that isn't of the variable's type is a failure ("expected value isn't a boolean").
- DO: Simulate/scenario results carry a constant `simulated: true` and a `notModelled` list (animation and timing, scroll position, drag direction, device frame and responsive resizing, pointer hit-testing) so nobody reads a pass as "the player was watched doing it".
- DO (cheap part): each of the 8 flow and scenario rules rebuilds the graph, and the 3 scenario rules each run every stored scenario (`check.rs:91-104`): compute once per `design.check` / `proto.check` call (pass a shared, lazily built context to the rules or cache per call) — keep `Rule.run`'s signature if you can, e.g. a thread-local or a per-call memo keyed by document revision is NOT acceptable; restructure the registry instead (the rule runner may take a prepared `FlowFacts`).
- Not now: clipping noise for off-canvas layers parked for animation; masks; layers that exist only after a change-to can't be named by a stored scenario (document it in the scenario params doc); `raise_fixed`/`relayout`/`settle` duplication (document in module docs; moving them into the engine for the player to call is a follow-up); `proto_sim::render_png` has no command yet (the MCP layer will use it).

## Checked and found sound (reviewer)
WCAG math (#767676 on white 4.54:1; #9AA0A6 2.64:1; floors, not rounds); non-finite floats handled; truncation by chars; no data indexing/unwrap in non-test code; bounds (conditional depth 16, delay chain 100, variants 256, compare 20,000 layers, ancestors 10,000); MAX_ENTRIES/MAX_TEXT on every command path; an error rolls the document back; no-op edits leave no undo step; `agent` survives `command.batch` nesting and can't equal `user`; ordered maps and total sorts; layering respected; `default_start` extraction preserves behaviour; library archive doesn't carry the context; undo/versions/file duplicate carry it whole; stage filtering never applies to recheck; an unknown rule is left unchanged, not verified.

## Acceptance coverage per reviewer
F1 partly (no collab test). F2 partly (R-05, R-10, R-11 paths; agent inside `command.batch`). F3 mostly. F4 partly (R-04, R-09, R-08 cases). F5 mostly (press/hover + click on one layer; scroll-only click; layers of a changed-to variant). F6 NOT proven (parity test pending). F7 partly (no recheck test for flow/scenario findings; unmeasured). D1 partly (malformed/forward-version design; an old file fixture compared byte for byte). D2 mostly (confirm `recheck`-all and `alternative.remove {deleteLayer}` undo).

# Defects found while building the examples (Fixture Builder, ran)

## X-01 · P2 · With no fonts available, every check passes silently
- A build whose font list is empty (craft-fonts missing at build time, run with `--no-system-fonts`): `text.fonts {}` → `[]`, every text lays out 1 px wide and draws nothing, yet `design.check` (contrast.text, text.clipped), `proto.check` and every scenario pass. Only `text.missingFonts` notices.
- Expected: text that has no font to lay out with is not measurable: `contrast.text`/`text.clipped` report it as unmeasured (a `text.noFont` note, or `contrast.unmeasured` with reason "no font to lay it out with"), and `design.check` says so once per run.
- Also docs/mcp.md "Fonts": `--no-system-fonts` "uses only the built-in fonts" is misleading when the build has none: say what happens.

## X-02 · P2 · A variant property bound to a variable silently undoes Change to; the simulator still reports the change
- A Button set with onPress → changeTo Pressed on Default; an instance whose `componentProperties/State` is bound to a string variable; simulate a press step → `fired: true`, effect "change … to State=Pressed", but the state shows Default (the binding is re-applied), and `expect.variants Pressed` fails.
- Expected: the simulator's effects describe what the state became; when a binding puts the variant back, the step reports it (problem or effect "State is bound to variable X, which keeps it at Default"); `flow.brokenTarget` (or a new note) flags a change-to on an instance whose property the change-to would set is bound. Check what the real player does (`apply_bindings` in the player's settle) and make simulator and player agree; add a parity case.

## X-03 · P3 · Variable names differ between commands
- `variable.set {variable: "Sign-up/code_filled"}` fails with "no such variable", while `proto.simulate` reports and expects variables as `Collection/name`.
- Expected: every command that takes a variable accepts an id, a unique name, or `Collection/name` (one resolver).

## X-04 · P3 · `text.missingFonts` params doc doesn't say `layers` is a count.
