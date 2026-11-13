# Command spec: design context + prototype verification (lead's spec, v1)

Data model: `crates/doc/src/design.rs` (`Document.design: Arc<DesignContext>`; edit with
`Arc::make_mut(&mut doc.design)`). Shared check types: `crates/engine/src/check.rs` (`Found`, `Rule`).
All commands are engine commands (`spec!(id, label, params doc, undoable, run)`), so menus, CLI,
control channel and MCP share them. Mutating commands are one undo step and leave the document
unchanged on error. Read-only commands have `history = false`. Text fields are limited to
`design::MAX_TEXT` chars and lists to `design::MAX_ENTRIES`; over-limit input is an error that names
the field. Errors use `bad(cmd, msg)` and must name the offending parameter and value/id
(e.g. "`links`: no layer 4711"). Node ids in params are accepted as numbers or strings like other
commands do (`crate::params`). JSON shapes in results use camelCase like the model's serde.

## E1 — `design.*` (crates/engine/src/design.rs + crates/engine/src/design/*)

Read:
- `design.get {}` → `{brief, assumptions, decisions, scenarios, endStates, findings, alternatives,
  missingLinks: [{node, from}], counts: {…}}` (missingLinks: links whose layer no longer exists).
- `design.forNode {id}` → `{assumptions: [ids], decisions: [ids], scenarios: [ids], findings: [ids],
  alternatives: [set ids], endState: bool}`: everything that links to the layer **or any ancestor
  /descendant relation: exact node only** (keep it exact; say so in the params doc).

Brief:
- `design.brief.set {audience?, job?, context?, content?, tone?, constraints?: [string], success?: [string]}`
  patch semantics: only given keys change; `""` / `[]` clears; strings are trimmed; returns `{brief}`.

Assumptions:
- `design.assumption.add {text, links?: [ids], status?: untested|supported|refuted, evidence?}` → `{id}`
- `design.assumption.set {id, text?, status?, evidence?, links?}`; `design.assumption.remove {id}`
- Rules: `text` non-empty. `supported`/`refuted` require non-empty `evidence` (after the patch).
  Every link must be an existing layer when set. Removing an assumption removes its id from
  decisions' `assumptions`.

Decisions:
- `design.decision.add {title, rationale, rejected?, assumptions?: [assumption ids], links?: [ids]}` → `{id}`
- `design.decision.set {id, …same keys}`; `design.decision.remove {id}`
- Rules: `title` and `rationale` non-empty; assumption ids must exist; links must exist.

Checks (deterministic, re-runnable; see check.rs):
- `design.check {scope?: [ids] (default: the current page's top-level frames), rules?: [rule ids]}` (read-only)
  → `{findings: [{rule, severity, nodes, message, evidence, suggestion, kind: "check"}],
      summary: {total, bySeverity: {blocker, major, minor, note}, byRule: {…}},
      rules: [{id, about}] (the rules that ran)}`. Unknown rule id → error listing the rule ids.
  Results are ordered by severity, then rule id, then node id (stable for tests).
- `design.check.rules {}` → `[{id, about}]`.
- `design.check.record {scope?, rules?}` (undoable) → runs the checks and stores each result that
  has no stored finding with the same `rule` + same `nodes` whose status isn't `verified`
  → `{added: [ids], existing: [ids], total}`. Stored with kind `check`, source `check`, status `open`.
- Rule registry: `pub fn rules() -> Vec<crate::check::Rule>` in the design module, concatenated
  with `crate::proto_verify::rules()` (E2 provides flow/scenario rules; until merged it returns `[]`
  — E1: call `crate::proto_verify::rules()` if it exists at merge time; the lead wires it, so just
  leave a clearly marked `fn external_rules() -> Vec<Rule> { Vec::new() }` hook).
- E1 rules (each with an exact threshold stated in `about`, and evidence carrying the numbers):
  - `contrast.text` (major; blocker never): a visible text layer whose (first visible solid) fill
    against its effective background has contrast below 4.5:1, or below 3:1 for large text
    (font size ≥ 24 px, or ≥ 18.66 px and weight ≥ 700). Effective background: walk up ancestors to
    the first visible solid fill, compositing alpha (text alpha and layer opacities included); if
    the walk reaches the page, use the page background. If a gradient/image fill or an overlapping
    non-ancestor layer is in the way of a certain answer, do NOT guess: report rule
    `contrast.unmeasured` (note) instead ("text sits on a gradient/image; contrast wasn't measured").
    Evidence example: `2.85:1, needs 4.5:1; text #9AA0A6 on #FFFFFF, 14 px regular`.
    Suggestion should give a concrete passing colour direction (e.g. "darken the text to at least #767676 on #FFFFFF").
  - `text.clipped` (major): a text layer whose laid-out text is taller or wider than its box
    (fixed-size boxes; use the text layout the engine already uses for auto-size), unless
    truncation ("…"/max lines) is set on purpose; and any text layer whose bounds are cut by a
    clipping ancestor that doesn't scroll. Evidence: needed vs available size, which ancestor clips.
  - `layout.clippedContent` (minor): a visible non-text layer partly or fully outside a clipping,
    non-scrolling ancestor frame (content the viewer can't reach). Ignore layers entirely inside.
    Evidence: how many px are cut on which side.
  - `target.small` (major below 24×24; minor below 44×44 only when the page's prototype device is a
    phone or tablet): a layer that plays a click/press/drag interaction (own, or as an instance
    through `proto_run::play_interactions` semantics) and is smaller than the threshold in either
    dimension. Evidence: `18×18 px, minimum 24×24 (WCAG 2.2 target size)`.
  - `text.small` (minor): visible text with font size below 11 px. Evidence: size.
  - `type.manySizes` (note): a top-level frame using more than 6 distinct font sizes. Evidence: the sizes.
  - `context.missingLink` (minor): a design-context link to a layer that no longer exists
    (nodes: empty; evidence names the entry, e.g. `A2 → 418`).
  - `context.briefIncomplete` (note): the brief has no audience, job, context or success signal
    (evidence lists the empty fields). Only reported when the file has at least one top-level frame.
  - `context.untestedAssumption` (note): one finding listing how many assumptions are untested and
    their ids (so nobody mistakes an untested assumption for a fact). Not reported when there are none.
  All rules must skip hidden layers (invisible, or with an invisible ancestor) and must be
  deterministic (no time, randomness or fonts-dependent flakiness beyond what layout already has).

Findings (stored):
- `design.finding.add {kind: judgement|feedback, rule (short label), severity?: blocker|major|minor|note,
   nodes?: [ids], message, evidence?, suggestion?, source?}` → `{id}`.
  `kind: "check"` is refused here ("checks are recorded by design.check.record"). `source` defaults
  to the agent's name when the command runs for an agent (`s.agent`), else `"user"`.
  `feedback` requires a non-empty `source` (the person) **and** non-empty `evidence` (their words).
- `design.finding.list {status?, kind?, node?}` (read-only) → `{findings: [...]}`.
- `design.finding.address {id, resolution}` → status `addressed`; `resolution` required.
- `design.finding.recheck {id?}` (undoable; without `id`: every stored `check` finding not `accepted`)
  → re-runs the finding's rule scoped to its nodes (for rules without nodes, e.g. context.*, the
  whole page). No longer reported → `verified` with `recheck {passed: true, detail}`; still
  reported → `open` (also when it was `addressed`) with `recheck {passed: false, detail: <evidence now>}`.
  A finding whose nodes are all gone → `verified`, detail "the layer no longer exists".
  A `judgement`/`feedback` id → error: "only checks can be re-run; use design.finding.verify".
  Returns `{results: [{id, passed, detail, status}]}`.
- `design.finding.verify {id, evidence, by?}` → `verified`, `verified_by` = `by` or the agent
  name or `"user"`. `evidence` required (what was looked at). **Refused when the command runs
  for an agent (`s.agent.is_some()`) and the finding is `feedback`**: "only a person confirms that
  feedback is met; record what changed with design.finding.address". Refused for `check` kind
  ("re-run it with design.finding.recheck").
- `design.finding.accept {id, reason}` → `accepted`; reason required; refused for agents on `feedback`.
- `design.finding.reopen {id}` → `open`. `design.finding.remove {id}` (refused for agents on `feedback`).

Alternatives:
- `design.alternative.create {source, name, constraint, intent?, question?, set?}` → `{set, node}`.
  Duplicates `source` (a top-level frame or component; else error) with the engine's existing
  duplicate logic so instances keep their overrides and interactions inside the copy that pointed
  inside the source are remapped as duplicate already does; places the copy to the right of the
  source / last member with a 100 px gap on the same parent; names it `<source name> / <name>`;
  records it. `constraint` and `name` required non-empty. Without `set`, a set for this `source`
  is reused if one exists, else created (`question` stored on creation). The source is never modified.
- `design.alternative.update {set, node, name?, constraint?, intent?, tradeoffs?}`; also
  `design.alternative.question {set, question}`.
- `design.alternative.compare {set}` (read-only) → `{set, question, members: [{node, name, isSource,
  constraint, intent, tradeoffs, exists, metrics: {width, height, layers, depth, textLayers, words,
  characters, interactive (layers with interactions), instances, components (distinct mains),
  images, density (visible leaf-layer area / frame area, 0..1 rounded 2dp)},
  readingOrder: [up to 40 text strings in visual order: top-to-bottom, then left-to-right, by
  absolute bounds; each cut to 80 chars], actions: [names of layers with click interactions]}],
  differences: [{a, b, structure: bool, order: bool, content: bool, actions: bool, paintOnly: bool}],
  warnings: [string]}`.
  `structure` = the trees differ in layer kinds/nesting/count (names and paint ignored);
  `order` = readingOrder sequences differ after normalising whitespace/case as multisets equal but
  order differs, OR different sequence at all; `content` = the set of text strings differs;
  `paintOnly` = none of structure/order/content/actions/size differ but fills/strokes/effects/
  fonts do. For each member that is `paintOnly` against the source add the warning:
  "`<name>` differs from the source only in paint (fills, strokes, effects, fonts): it is a
  restyle, not a structural alternative." Members identical to the source get a warning too.
- `design.alternative.choose {set, node, rationale}` → sets `chosen`, adds a Decision (title
  `Chose "<member name>"` + (question if any), rationale, links: [node]) and stores its id in the
  set → `{decision}`. Nothing is deleted. `rationale` required. `node` may be the source.
- `design.alternative.remove {set, node?, deleteLayer?: bool=false}`: forget one member (or the whole
  set without `node`); layers are only deleted with `deleteLayer: true`, and never the source.

## E2 — prototype verification (crates/engine/src/proto_sim.rs + proto_verify.rs)

Simulator (`proto_sim`): a pure, deterministic state machine over the player's rules for *what
state comes next* (no animation, timing or scroll geometry). It works on its own copy of the
document (the file never changes). Follow the UI player's semantics exactly where they define
state — read `crates/ui-egui/src/present/{actions,navigate,input}.rs` and
`crates/engine/src/proto_run.rs` and reuse the `proto_run` helpers (`play_interactions`,
`navigate_target`, `remember_sections`, `change_to_target`, `set_variable`, `set_mode_by_id`, `branch`):
- State: `screen` (top-level frame), `overlays` (stack, bottom first, each with its
  close-on-outside flag), `history` (screens to go back to), section memory, the mutated document
  copy (variables, variable modes, instance variants), which after-delay interactions fired.
- `Sim::new(doc, start) -> Result<Sim>`: `start` must be a top-level frame (error names it);
  runs `play_interactions` on the copy like the player does.
- `Sim::step(&StepAction) -> StepOutcome { fired: bool, effects: Vec<String>, problem: Option<String> }`
  - `Gesture {node, gesture}`: the node must exist and be showing in the active root (the top
    overlay if any overlays are open, else the screen): inside its subtree, visible with visible
    ancestors; otherwise `problem` = "layer `<name>` (<id>) isn't on `<root name>`, which is what
    shows". Find the interaction: starting at the node, then each ancestor up to the active root,
    the first layer that has an interaction with the matching trigger (click→onClick, hover→onHover,
    press→onPress, drag→onDrag, mouseEnter/Leave/Down/Up likewise) — this is the player's "a layer
    without that trigger lets it through" rule approximated by ancestry; document this
    approximation in the module docs. No match → `fired: false`, `problem` = "nothing reacts to
    <gesture> on `<name>`". Run the interaction's actions in order with the player's rules
    (navigate replaces the screen, closes overlays, pushes history unless it's the same screen with
    no overlays; back closes the top overlay first, else pops history; open/swap/close overlay;
    changeTo via `change_to_target` + `components::sync_instance`; setVariable/setVariableMode;
    conditional via `branch` with MAX_ACTION_DEPTH; scrollTo and openUrl change no state but are
    recorded in `effects`, and a scrollTo whose dest isn't in what shows is a `problem`).
    An action that can't run (dest missing, variable missing…) adds a `problem` naming it.
  - `Hover`/`Press` gestures are temporary in the player (they revert when the pointer leaves /
    is released): model them as applied until the next step, which first reverts to the state
    before them (document this).
  - `Key {key}`: fire `keyPress` interactions matching the key on layers of the screen and the
    open overlays, as the player's input does (read input.rs for which layer wins).
  - `Wait`: fire the active root's pending `afterDelay` interactions once each per arrival, as
    `navigate.rs::next_delay` orders them; loop guard: at most 100 chained fires, then `problem`
    "after-delay loop".
  - `ClickOutside`: with an overlay open and close-on-outside → closes it (`fired: true`); with
    close-on-outside off → `fired: false`, no problem; without overlays → `fired: false`.
  - `SetVariable {variable, value}`: name or id; via `proto_run::literal` typing; unknown or
    mistyped → `problem`.
- `Sim::state() -> State` (serializable: `{screen, screenName, overlays: [{id, name}], history:
  [ids], variables: {"Collection/name": value …only those that differ from the file}, variants:
  [{instance, component, name}] only instances whose variant differs from the file, endState: bool}`).
- `Sim::check(&Expect) -> Vec<String>` human-readable failures, each naming expected vs actual
  with layer names and ids. `visible`/`hidden`: the layer is in the screen or an open overlay
  subtree and it and its ancestors are visible (after variable bindings applied). `text`: the
  text layer's current characters. `variants`: match the instance's current main component by
  id, exact name, or any `Prop=Value` value. `variables`: by id, `Collection/name` or unique name;
  compare numbers with 1e-9 tolerance, colours as hex.
- `Sim::document() -> &Document` and `pub fn render_png(sim: &Sim, scale: f64) -> Option<Vec<u8>>`
  (or returning a pixmap) that draws the screen and composites the open overlays where the player
  would put them (centre / the named positions / manual offset relative to the trigger layer is
  not available without geometry of the click: use the trigger layer's bounds when known, else
  centre) with the overlay background dim colour. Uses `figcraft_render` (allowed: render is below engine).

Commands (`proto_verify`):
- `proto.check {page?}` (read-only) → `{starts: [{frame, name, source: "flow"|"firstFrame"}],
  screens: [{id, name, reachable, endState, deadEnd, out: [{from, fromName, trigger, action, to?, toName?}]}],
  problems: [{rule, severity, nodes, message, evidence, suggestion}], ok: bool (no blocker/major)}`.
  The graph is static: every interaction in a screen's subtree (after `play_interactions` on a
  copy, so instances contribute their variants' interactions) is an edge; all conditional
  branches count as possible. Rules (also exposed as `pub fn rules() -> Vec<crate::check::Rule>`
  for `design.check`, plus `scenario.fails`):
  - `flow.brokenTarget` (blocker): an action whose target can't work: navigate/overlay/swap/
    scrollTo/changeTo `dest` missing; navigate dest not inside any top-level frame or an empty
    section; changeTo dest that isn't a variant in the set of any instance it could change;
    setVariable naming a missing variable or a value of the wrong type; setVariableMode naming a
    missing collection/mode; a conditional whose expression references a missing variable.
    Evidence names the layer, trigger, action and the missing id.
  - `flow.deadEnd` (major): a reachable screen with no way to leave it (no navigate/back/open
    overlay/openUrl interaction anywhere in it or its instances; `back` counts only when the screen
    isn't a flow start) that isn't a declared end state. Suggestion: add a way out or declare it
    with `proto.endState.set`.
  - `flow.trappedOverlay` (major): an overlay opened with close-on-outside off whose subtree has
    no closeOverlay/back/navigate/swap interaction.
  - `flow.unreachable` (minor when the frame has interactions of its own, else note): a top-level
    frame no flow start reaches, on a page that has at least one interaction; frames that are
    overlay targets, component sets/components, or recorded alternatives (members or sources in
    `design.alternatives`) are skipped.
  - `flow.noStart` (minor): the page has interactions but no flow start (the player falls back to
    the first frame; say which).
  - `scenario.fails` (major): a stored scenario that doesn't pass (evidence: the first failing
    step and its failures); `scenario.none` (note): the page has a flow start but no stored scenario.
- `proto.endState.set {id, reason?, on?: bool = true}` (undoable; a top-level frame, else error);
  `proto.endState.list {}`.
- `proto.simulate {start?, steps: [step…], expect?, stopOnFailure?: bool = true}` (read-only) →
  `{passed, state, steps: [{index, action, fired, effects, problem?, failures: [string], state:
  {screen, screenName, overlays}}], failures: [string] (final expect)}`. `start` defaults like
  `proto.present` (selected frame → first flow → first frame). A step is `{type: "gesture", node,
  gesture?} | {type: "key", key} | {type: "wait"} | {type: "clickOutside"} | {type: "setVariable",
  variable, value}` plus optional `expect` and `note`; shorthand `{click: <id>}` is accepted too.
  A step with a `problem`, or a gesture/key that fires nothing, fails the run.
- `proto.scenario.add {name, start, steps, expect?, purpose?}` → `{id}` (validates: start is a
  top-level frame; every node id exists; steps ≤ MAX_ENTRIES); `proto.scenario.set {id, …}`;
  `proto.scenario.remove {id}`; `proto.scenario.list {}` (read-only);
  `proto.scenario.run {id?}` (read-only; all when omitted) → `{passed, total, failed, results:
  [{id, name, passed, failedStep?: index, failures, steps: […as proto.simulate]}]}`.

## Shared rules for both builders
- Never crash: no unwrap/expect/panic/indexing-by-data; bound loops and recursion; treat every
  param as hostile. The junk-parameter sweep (`crates/engine/src/tests.rs`) runs every command
  with garbage; it must pass with the new commands.
- `cargo fmt`, `cargo clippy -p figcraft-engine --all-targets -- -D warnings` clean.
- Tests live inside your own module tree (`#[cfg(test)] mod …` declared from your module files),
  not in `lib.rs`. Each rule/command gets behaviour tests including: happy path, error names the
  param, undo restores exactly (compare `serde_json::to_value(doc)`), redo re-applies, save →
  reopen through `figcraft_format` keeps the data (see how existing tests round-trip, e.g. grep
  `figcraft_format::` in engine tests), and a file without design context serializes with no
  `design` key.
- If the repo has registries that every command must appear in (menu catalog, docs lists,
  feature-name tables, parity/doc checks — find them by running the engine tests and `cargo xtask
  docs`), satisfy them minimally and note what you touched.

---

# Amendment v1.1 (wins over v1 where they conflict)

Source: the book researcher's critique of the first hypothesis.

E1:
1. Brief gains `sponsor` (who asks for it, what they want from it) and `format` (what form, where
   people meet it). `DesignContext.briefRevision` increases by 1 on each `design.brief.set` that
   changes the brief.
2. `DesignContext.stage`: `exploring` (default) | `converging` | `ready`; `design.stage.set {stage}`.
   `design.check` filters rules by stage profile and returns `skipped: [{id, reason}]`.
   exploring: `flow.brokenTarget`, `context.missingLink`. converging: + contrast.text,
   contrast.unmeasured, text.clipped, layout.clippedContent, target.small, other flow.* and
   scenario.* rules. ready: + context.briefIncomplete, context.untestedAssumption,
   context.staleDecision. An explicit `rules` param runs exactly those.
3. Rules `text.small` and `type.manySizes` are removed. Thresholds cite their standard: WCAG 2.2
   SC 1.4.3 (contrast 4.5:1; 3:1 large text), SC 2.5.8 (target 24×24, major), SC 2.5.5 (44×44 on a
   phone/tablet device, note). New `context.staleDecision` (note): decision.briefRevision < current.
4. `Decision` gains `author`, `status` (`proposed` | `accepted`) and `briefRevision`. Agent-added →
   proposed; user-added → accepted. `design.decision.accept {id}` is refused for agents. An agent's
   `design.decision.set` on an accepted decision moves it back to proposed.
5. Agents: can't set an assumption's status to supported/refuted (may write `evidence`); can't
   `verify` or `accept` judgement or feedback findings (only `address`); can't remove feedback
   findings or judgement findings raised by someone else.
6. `AlternativeSet.constraints: [string]` (set-level, declared before generating). Compare never
   ranks or scores.

E2:
1. Rule `scenario.unvisited` (note): reachable screen no stored scenario shows (only when ≥1 scenario).
2. `stepsTaken` in simulate/scenario results.
3. `proto.check.ok` means only "no blocker/major found by these rules".

---

# Amendment v1.2 (review R1) — wins over v1 and v1.1 where they conflict

Source: the independent review of the engine side (`review/R1-engine.md`) and the lead's decisions on
it. Finding ids (R-01…, A1…) refer to that file and to the fixer's brief. "Agent" means a command run
through `Session::execute_as(Some(name), …)` (also inside `command.batch` run that way); its name in
records is `<name> (agent)`, e.g. `Claude (agent)`. Everything else acts as `user`.

These authority rules are a convention between cooperating parties, not a security boundary (stated
in the module docs of `crates/engine/src/design.rs`).

## Data model (`crates/doc/src/design.rs`) — all new fields are left out of the file while empty/default

| Where | Field (JSON) | Meaning |
|---|---|---|
| `EndState` | `author` | who declared it (`user` or the agent's name). Missing (old files) = the user's. |
| `EndState` | `status`: `proposed` \| `accepted` | missing = `accepted` (old files; written only when `proposed`). Only an accepted end state exempts its frame from `flow.deadEnd`, sets `endState: true` in `proto.check` / simulator state, and satisfies `expect.endState: true`. |
| `Scenario` | `author` | who wrote it. Missing (old files) = the user's. |
| `Finding` | `subject` | what a check is about when that isn't its layers (see "Identity"). |
| `Finding` | `recordedBy` | who ran the command that stored it, when that isn't `source`. |
| `Decision` | `editedBy` | who changed it last, when that isn't its `author`; cleared when the author edits it again. |
| `AlternativeSet` | `proposal`: `{node, decision}` | an agent's pending proposal to choose `node`, with its proposed decision's id. |
| `DesignContext` | (not serialized) `load_issues` | what a lenient read left out or cut; taken by the loader. |

Old shapes keep loading and are written back unchanged (tests:
`end_states_scenarios_and_findings_from_before_they_had_authors_load_as_the_users`,
`a_document_without_a_design_context_loads_and_writes_none`).

## Loading (R-07)

- `Document.design` is read leniently (`design::read_stored`). A file never fails to open because
  of its design context.
  - The value reads whole → kept.
  - It doesn't → each top-level part is read by itself and lists entry by entry; what reads is
    kept, the rest is left out. One load issue: `part of the design context couldn't be read and
    was left out (<`path`: serde error>; …up to 20, then "and N more"); the rest was kept`.
  - It isn't an object at all → empty context, issue `the design context couldn't be read and was
    left out: <serde error>`.
  - Lists longer than `MAX_ENTRIES` (500) are cut to 500 and texts longer than `MAX_TEXT` (4000
    chars) cut at a character boundary; variable values whose JSON is longer than `MAX_TEXT` become
    `null`. Issue: `the design context was larger than a file may hold and was cut: …`.
  - Id counters lower than an id in use are raised (issue: `…id counters were behind the ids in use
    and were raised…`), so `next_id` never repeats an id.
- The issues are appended to the loader's issues (`figcraft_format::from_json` / `load_bytes` →
  `Loaded.issues`, which `file.open` reports with the repairs). They are reported once; what
  couldn't be read is not written back on save.
- `design.get`'s `missingLinks` is computed in one pass (sets).

## Collaboration (R-06, documented limit)

- The design context still syncs as one whole value (`DocProp::Design`); of two concurrent edits
  the one the server takes last replaces the other (pinned by collab tests
  `a_design_context_change_syncs_to_a_second_client`,
  `of_two_concurrent_design_context_edits_the_last_writer_wins_and_the_other_is_lost`).
- A peer's context past `MAX_ENTRIES` / `MAX_TEXT` is refused: `Error::Invalid("design: larger than
  a file may hold (…)")` (`a_peers_design_context_past_the_limits_is_refused`).

## Identity of check findings and recheck (R-01, R-02, A2)

- `Found` and stored `Finding` carry `subject`. Results of `design.check` (`findings[]`) and
  `proto.check` (`problems[]`) include `subject` when it isn't empty.
- Subjects by rule:
  - `contrast.text`, `contrast.unmeasured`, `text.clipped`, `layout.clippedContent`, `target.small`,
    `flow.deadEnd`, `flow.unreachable`: none (the layer in `nodes` is the subject).
  - `flow.brokenTarget`: `action:<layer>:<interaction index>:<action index>` (`<layer>` = the layer
    that holds the interaction: the main component's layer when instances play it; action index =
    position among the interaction's actions, nested ones in order). Two broken actions on one
    layer are two findings.
  - `flow.trappedOverlay`: `overlay:<overlay id>` (`nodes`: the overlay, then the first layer that
    opens it that way).
  - `flow.noStart`, `scenario.none`, `scenario.unvisited`: `page:<page id>` (`nodes` only say where
    to look: the first frame / the first flow start / the screens not shown).
  - `scenario.fails`: `scenario:<scenario id>` (`nodes`: the failing step's layer, then the start).
  - `context.missingLink`: `link:<from>:<layer id>` (`from` as in the evidence: `A1`, `S2 step 3`,
    `end state`…); `context.briefIncomplete`: `brief`; `context.untestedAssumption`: `assumptions`;
    `context.staleDecision`: `decision:<id>`.
- Same problem = same rule and same subject; without subjects on both sides, same rule on the same
  `nodes` (node-less: same message). A finding stored before subjects existed matches a result with
  a subject when they share a layer (node-less: same message), and a recheck then stores the
  result's subject on it.
- `design.check.record` dedupes with that rule; stores `subject`; stores `recordedBy` = who ran it.
- `design.finding.recheck`:
  - a finding without a subject is re-run on its layers (all gone → verified, "the layer no longer
    exists");
  - with a subject it is re-run on the page the subject is on: `page:` that page (page gone →
    verified "the page no longer exists"); `scenario:` the page its start is on (scenario removed →
    verified "scenario S1 no longer exists"); `action:` the page of the layer holding the action;
    `overlay:` every page (any page may open it); context subjects: no place;
  - still reported under the same identity → `passed: false`, status `open` (accepted stays
    accepted), `detail` = the evidence now. This holds when the failing step, the opener, the
    first frame or the list of unvisited screens changed;
  - `contrast.text`: a text that can no longer be measured keeps its finding open, `detail`:
    `not measured, so not shown to pass: <why>` (`Rule.unsettled`);
  - one call builds the played copy, each page's flow graph and scenario runs once
    (`crate::check::Facts`), whatever the number of rules and findings.
- `crate::check::Rule` is now `{id, about, run: fn(&Facts, Scope) -> Vec<Found>, unsettled:
  Option<fn(&Facts, Scope) -> Vec<Found>>}`; `Scope {page, layers}`.

## `design.check` scope (A3) and page (R-13)

- No `scope` (or `[]`): the page the session shows (not `doc.current_page`). Rules that measure
  layers look at its top-level frames; `flow.*` and `scenario.*` give everything `proto.check`
  reports for that page, including page-level findings and findings on layers outside every frame.
- With `scope`: a flow/scenario finding is kept when one of its `nodes` is a scoped layer or inside
  one; a page-level finding (`page:` subject, or no nodes) is kept when the scope holds that page or
  one of its top-level frames.

## `design.check.record` at the cap (R-12)

- Stores results (worst first) until the file holds 500 findings; never fails for that.
- Result gains `truncated` (number of results not stored; 0 when none) and, when > 0, `note`
  (says so and to narrow `scope`/`rules` or remove settled findings).

## Flow rules (R-04, R-08, R-09, A1, R-13)

- `proto.check` result gains `prototyped` (bool); `starts[].source` may be `otherPage`;
  `screens[]` gain `component` (bool) and, for a frame an agent declared as an end state,
  `endStateProposedBy` (the agent); `problems[]` gain `subject` when set.
- A page is a prototype (`prototyped`) when it has a flow start, or a screen (top-level frame that
  isn't a component/component set) plays navigate / back / open, swap or close overlay / open
  link, or an interaction on another page navigates to one of its frames.
- Starts: the page's flows, plus frames that interactions on other pages navigate to (`otherPage`);
  only without either, the first top-level frame that isn't a component or component set
  (`firstFrame`).
- `flow.noStart`: only when `prototyped`, no flow start and no `otherPage` start.
- `flow.unreachable`: only when `prototyped`; components, component sets, overlays and alternatives
  are skipped.
- `flow.deadEnd`: a reachable, non-component screen that isn't an accepted end state and that
  nothing leaves, and only when (a) some screen of the page plays an action that moves between
  screens, or the screen is reached from another page, and (b) the page has another frame that
  counts as a screen (not a component/component set, not a frame only shown as an overlay, not a
  recorded alternative or its source), or the screen is reached from another page. "Back" doesn't
  count on a `flow`/`firstFrame` start; it does on an `otherPage` start. A frame an agent declared
  as an end state stays a dead end; evidence ends `; declared as an end state by <agent>, waiting
  for a person to accept (proto.endState.accept)`.
- `flow.trappedOverlay`: an overlay opened with close-on-outside off stays a "slot" through swaps.
  If no overlay the slot can come to show (by `swapOverlay`) has closeOverlay / back / navigate,
  the overlays where the swapping ends are reported (all of them when it goes round in circles).
  A swap alone is no way out.
- `flow.brokenTarget` reads the played file: what an instance plays from its main component is
  checked wherever the main lives (one finding per action: `nodes` = [main's layer, first instance
  layer on the page], evidence ends `; played by instance layer …`), and variants on other pages
  that change-to actions lead to are followed (`nodes` = [variant's layer, the layer on the page
  that leads there]; evidence ends `; <that layer> can be changed to the variant that holds it`). Change-to is broken only when the destination doesn't exist, isn't a
  component, or there is no instance at or around the layer and the layer isn't inside a main
  component (the player's `change_to_target` fallback is honoured: changing to a component of
  another set is not broken).

## Simulator results (R-13)

- `proto.simulate` and `proto.scenario.run` results carry `simulated: true` and `notModelled`:
  five strings (animation and timing; scroll position; drag direction; the device frame and
  responsive resizing; pointer hit-testing).
- An expected variable value that isn't of the variable's type fails: for a boolean only
  `true`/`false` (or those words) are accepted; message `…but the expected value isn't a boolean
  value, which the variable holds`.
- `contrast.unmeasured` also reports text with no fill that a visible stroke draws.
- `target.small`'s `about` says the SC 2.5.8 exceptions aren't evaluated.
- `design.alternative.compare`: `paintOnly` is true only when paint differs and nothing else does
  (no layout difference, fonts included).

## Authority (R-05, R-10, R-11)

Every refusal below is an error naming the entry, who owns or settled it and what the agent can do
instead; a refused command changes nothing and leaves no undo step, also as a step of a
`command.batch` an agent runs.

End states:
- `proto.endState.set {id, reason?, on?}` → `{id, endState, status, author, reason, note?}`.
  `endState` is now "declared **and accepted**"; `status` is `proposed` | `accepted` | `null` (taken
  back); `note` is present for a proposal.
  - User: declares → `accepted`, `author: "user"`, reason optional. On a frame an agent proposed:
    accepts it (the author stays the agent), `reason` replaces the reason when given.
  - Agent: `reason` is required and non-empty (error ``"`reason` is required when an AI agent
    declares an end state…"``); creates `status: proposed`, `author: <agent>`; `endState: false`.
    May change the reason of, or take back (`on: false`), only its own proposal. Any other end
    state (a user's, one from an old file, another agent's, or its own once accepted) → refused:
    ``"`id`: `B` (7) is an end state declared by user (accepted); an AI agent (Claude (agent))
    changes|takes back only its own proposals. Ask them to change it"``.
- New `proto.endState.accept {id}` (undoable) → `{id, endState: true, status: "accepted", author,
  reason}`. Refused to an agent (`"a person accepts an end state: … stays a proposal until the user
  accepts it (<agent> can't)…"`). Not declared → ``"`id`: … isn't declared as an end state
  (declare it with proto.endState.set)"``. Already accepted → no change, no undo step.
- `proto.endState.list` entries gain `author` (`user` for old files) and `status`.
- A proposed end state: `proto.check` `screens[].endState: false`, `deadEnd` unchanged,
  `endStateProposedBy: <agent>`; `flow.deadEnd` still reported (evidence ends `; declared as an end
  state by <agent>, waiting for a person to accept (proto.endState.accept)`); simulator
  `state.endState: false`; `expect.endState: true` fails.

Scenarios:
- `proto.scenario.add` records `author` (who ran it). `proto.scenario.list` shows it.
- `proto.scenario.set` / `.remove` by an agent on a scenario whose `author` isn't that agent (a
  user's, an old file's, another agent's) → refused: ``"`id`: S1 was written by user; an AI agent
  (<agent>) changes|removes only its own scenarios. Add your own scenario (proto.scenario.add) or
  ask them to change it"``. The user may edit or remove any; editing doesn't change the author.

Assumptions:
- Agent, on an assumption whose status is `supported` or `refuted`: `design.assumption.remove` is
  refused, and `design.assumption.set` is refused when it would change `text`, `status` (also to
  `untested`: the earlier "pass `status: untested` with the new text" path is gone) or `evidence`.
  Message: ``"`<key>`: a person recorded A1 as supported (evidence: "…"); an AI agent (<agent>)
  doesn't change|remove that. Add another assumption (design.assumption.add) or ask them to …"``.
  `links` may still be changed; repeating the current values is not a change. Untested assumptions:
  as before (an agent may edit text/evidence and remove).

Decisions:
- `design.decision.set`: when the call changes the decision and the caller isn't its `author`,
  `editedBy` = the caller; when the author changes it, `editedBy` is cleared. (An agent's change
  to an accepted decision still makes it `proposed`.)
- `design.decision.accept {id}`: when `id` is the decision of an alternative set's `proposal`, the
  set's `chosen` becomes the proposed layer, `decision` = `id`, `proposal` is cleared, and the
  result gains `chose: [{set, node}]`.
- `design.decision.remove`: also clears a set's `proposal` that pointed at the decision.

Alternatives:
- `design.alternative.choose {set, node, rationale}`:
  - User → as before, plus `proposal` cleared. Result `{decision, status: "accepted", chosen: node}`.
  - Agent → adds a `proposed` decision and sets the set's `proposal: {node, decision}`; `chosen`
    and `decision` are **not** changed. Result `{decision, status: "proposed", chosen: <the set's
    choice, unchanged, or null>, proposed: node}`. The agent's own earlier pending proposal for the
    set (its decision still `proposed`) is removed with its decision.
- `design.alternative.compare` result gains `proposal` (`{node, decision}` or `null`).
- `design.alternative.remove {set, node}` clears a `proposal` for that node.

Findings:
- `design.finding.add`:
  - stored `recordedBy` = who ran the command when that differs from `source`;
  - agent + `kind: judgement` + a `source` other than the agent's own name → refused (``"`source`:
    a judgement an AI agent records is its own (<agent>), not "<source>"'s; to pass on what a person
    said, add it with `kind`: feedback…"``);
  - agent + `kind: feedback` + `source` = the agent's own name → refused ("feedback is what a
    person said…"); feedback relayed by an agent: `source` = the person, `recordedBy` = the agent.
- `design.check.record`: stored findings carry `recordedBy` = who ran it (`source` stays `check`).
- `design.finding.address` and `design.finding.reopen` by an agent on a finding whose status is
  `accepted`, or `verified` with a `verifiedBy` (a person's confirmation) → refused: ``"`id`: F1
  was accepted as it is by the user | verified by <who>; an AI agent (<agent>) doesn't undo that.
  If you think the problem is back, say so in a judgement of your own (design.finding.add) or ask
  them to reopen it"``. A check verified by its rule (no `verifiedBy`) may be reopened by an agent.

## Tests to look at (by name)

doc: `design::tests::*` (lenient load, limits, old shapes). format: `design_load_tests::*`. collab:
`design_tests::*`. engine: `design::tests::checks::*` (identity, recheck, record cap, scope, page),
`design::tests::findings::an_agent_doesnt_undo_what_a_person_accepted_or_verified`,
`design::tests::context::only_a_person_records_that_an_assumption_was_supported_or_refuted`,
`design::tests::alternatives::choosing_records_a_decision_and_deletes_nothing`,
`proto_verify::tests::flow::*` (static pages, single screen, other pages, mains elsewhere, swap
traps, change-to), `proto_verify::tests::scenarios::*` (end state proposals, scenario authors, old
file shape), `proto_sim::tests::variables::an_expected_value_of_another_type_fails_instead_of_passing_as_false`.

## Not changed by this amendment

- The simulator's gesture semantics (R-03 items 1–3 are handled by the UI builder: parity test and
  player fix). `proto_sim.rs`'s module docs now also list the known differences 4–8 (mouse up vs
  click, a scroll-to that can't scroll, change-to a non-component, a delay firing during a hover,
  restart) and that layers which only exist after a change-to can't be named by a stored scenario.
- Per-entry merging of the design context in live sessions (backlog; see "Collaboration").
- `design.forNode`'s `endState` is true for any declared end state of the layer, proposed or accepted.
- A `setVariable` *step* still reads its value as an action would (`proto_run::literal`); only
  *expected* values are strict.

---

# Amendment v1.3 (review R2) — wins over v1, v1.1 and v1.2 where they conflict

Source: `review/R2-mcp-ui.md` (R2-01…R2-12, suspicions) and `review/R1-engine.md` X-01…X-04, as the
lead triaged them. "Agent" as in v1.2.

## A person's work and history (R2-04, P0; engine)
- `Session::refuse_discarding(cmd, uid)`: for an agent only, a command that would replace or close
  the tab `uid` (with its branch tabs) is refused with `otherAuthor` {command, label, author} when
  the tab holds an undo step not by that agent, or unsaved work not all its own (changes since the
  last save by someone else; unsaved with no change on record — a recovered file, an unrecorded
  edit, a forgotten change, a changed version history — counts as the user's). Live tabs: their own
  undo authors. Message: "`<cmd>` would discard "<label>", made by <author>, not by you, with the
  rest of the unsaved work or undo history that isn't yours: nothing was changed. Leave the file as
  it is, or ask them (they can save, revert or close it themselves)".
- Called by: `file.revert`, `file.open {reload: true}`, `file.close` (with or without `force`;
  before the unsaved-changes refusal), `branch.archive`, `branch.merge`, `branch.updateFromMain`
  (the branch's tab), `collab.host`, `collab.share` (local and online), `collab.publish`
  (`start_host` / `start_publish`), `collab.reconnect` when it joins again.
- `version.delete` is refused to agents (badParams: "a person deletes versions…").
- For everyone: a revert keeps the tab's unsaved work as an autosave version "Before revert" (when
  changed since the last checkpoint), and the versions the tab had beside the file's stay (newest
  first when over `MAX_VERSIONS`), sorted by time. The reverted tab is not dirty; the history is
  written with the next save.
- `DocState` gains a private `saved_rev`; `HistoryEntry` gains `revision` (the document's revision
  once the step was made, or on the redo stack undone); `Change` gains `stepped` (the change has an
  undo step: pushed one, or is an undo/redo).
- Snapshot undo (suspicion confirmed): an agent's `edit.undo`/`edit.redo` of its own step (without
  `others`) is refused (`otherAuthor`, label "<label> (made after your step, with no undo step of
  its own)") when somebody else made a change after the step's revision that has no undo step (a
  direct edit, the first save's rename, a change no longer remembered). Not for live tabs.

## Gestures in progress (R2-02; engine)
- New `EngineError::Busy {cmd, by}`, code `busy`, detail `{command, by: "user" | <agent>}`;
  message "`<cmd>`: a person | another AI agent (<name>) is in the middle of a gesture or text
  edit on this file; nothing was changed, try again shortly".
- `Interaction` gains `by` (who began it). An agent's command whose target document has another's
  interaction: textpreview commands refused up front; otherwise the live text preview isn't ended,
  the command runs and is refused and rolled back (document, selection, page, revision, the
  interaction and the text edit) when it changed the document, the interaction or the text edit.
- Refused for an agent the same way: `begin_interaction`, `preview`, `commit_interaction`,
  `cancel_interaction`, `Session::pointer`, `textedit::{edit (insert…), key, end}`.

## Who acts (R2-05, R2-06)
- `Session::acting_as` / `execute_as` restore the actor also after a panic (catch_unwind +
  resume_unwind); `Headless::as_agent` likewise. Headless `key` runs the commands it stands for as
  the agent in effect (Cmd+Z is the agent's undo; Delete the agent's step).

## Wording (R2-03, R2-09)
- Undo/redo refusal: "…a step made by <author>, not by you: nothing was changed. Leave it, or ask
  them; pass `others: true` only if the person asked you to undo|redo their step". Same idea in the
  `others` schema ("Only if the person asked you to take back their step: …"), the `undo`/`redo`
  descriptions and INSTRUCTIONS.
- Conflict, other-author and busy prose show document names, labels and authors through
  `error::prose`: ≤60 characters then "…", control characters escaped, backticks as `'`.
  (`proto_sim::label` is the UI agent's file: not changed.)

## End states (R2-07)
- A user's `proto.endState.set` on an agent's proposal accepts it only with `on: true` said, or
  with the frame named alone; a call that only gives a new `reason` keeps it `proposed`.

## Checks (X-01, X-02, X-04)
- `fonts::lays_out(text)`: every family it uses gives a face (its own or the fallback). Text that
  doesn't: `contrast.text` reads it as unmeasured, `contrast.unmeasured` evidence "no font to lay it
  out with"; `text.clipped` skips it; new rule `text.noFont` (note, runs from `exploring`; listed
  after `text.clipped`). `design.check` gains `warnings: [string]` (one entry when the session has
  no fonts at all; `[]` otherwise).
- `flow.brokenTarget` also flags a change-to whose instance has the variant property that tells
  the destination from what it shows bound to a string variable holding another valid value (the
  player re-applies the binding): evidence "`<prop>` of instance … is bound to variable `<name>`,
  which holds "<v>" here: the player puts the instance back …"; suggestion: set the variable in the
  same interaction, or unbind. Not when the same interaction sets that variable, nor when the
  variable names no variant.
- `text.missingFonts` doc: `layers` is a count.
- `text.refit` / `text.rescanFonts` / font loads with nothing to refit no longer copy the document
  (no empty undo step, no revision).

## Variables (X-03)
- `figcraft_doc::Variables::find(key)`: id, else `Collection/name`, else a name only one variable
  has; ambiguous or missing is an error naming the candidates. Used by `variable.set`, `.update`,
  `.duplicate`, `.resetOverride`, `.delete`, `.bind`, style variable bindings, aliases in
  `variable.create`, `selection.bindColorVariable`, `library.aliasVariable`, setVariable actions
  (`proto.interaction.*`) and node props (`resolve_strict`). `variable.bind` no longer takes the
  first of two same-named variables.

## Simulator results (R2-08)
- `NOT_MODELLED` (now `pub`, 8 entries): the five, plus "a gesture step is one trigger…", "the
  player's own keys (R restarts, Z, the arrow keys)", "widgets (…)".
- New `pub KNOWN_DIFFERENCES: [(key, sentence); 5]`, keys = the parity cases' names without
  `difference_`. `proto.simulate`, `proto.scenario.run` and `proto.snapshot` results gain
  `knownDifferences` (the sentences); `proto.snapshot` also gains `notModelled`. The
  `simulate_prototype` / `run_scenarios` descriptions and the `verify_prototype` prompt repeat them.

## Other engine changes
- `document.inspect` gains `previewing: {version, name, note}` while a version is shown in place
  of the document (suspicion confirmed).
- `view.designContext {section}` refuses an unknown section, listing the six.
- `cmd::file::{OPEN_FORMATS, IMAGE_FORMATS, SAVE_FORMATS, export_formats()}`.
- `Session::require_revision(what, expected)` (settles, then checks as `ifRevision` does).

## MCP (R2-01, R2-10, R2-11, R2-12)
- `Backend::take_revision() -> Option<(document id, revision)>`: Headless records the active
  document's revision right after `engine.execute`, `ui.pointer`, `ui.key`, `ui.type` (inside the
  `document` forward); Attached reads the reply envelope's `revision` (paired with the request's
  `document`, or `revisionDocument` when the app sends it).
- Every tool on a document reads `doc.list` before its first request. A result's
  `document.revision` is the revision the tool's change call left (from `take_revision`, or the
  command's own `{id, revision}` for files it opens/makes), else the revision from before the
  first request; never one read afterwards. Failed calls name the revision from before.
- `ifRevision` now also on `check_design` (only with `record: true`; otherwise refused), `save_file`,
  `pointer`, `key`, `type_text` (headless checks inside the request; MCP also pre-checks with
  `doc.list {ifRevision}` for apps that ignore it on canvas methods). CONDITIONAL_TOOLS lists 11.
- `open_file` reads the opened file by its id. `run_command` of `file.new`, `file.newBoard`,
  `file.open`, `file.openRecent`, `file.duplicate`, `file.revert` moves the working document to the
  file it answers with; the result gains `workingDocumentChanged {from, to}`. `file.close {index}` /
  `doc.switch {index}` on another tab: `document` names that tab (closed: `{id, name, closed}`),
  plus `workingDocument` (unchanged). A resource read settles the working document.
- `get_capabilities`: `product` = `figcraft_mcp::PRODUCT_NAME`; `formats` from the engine lists
  (open adds `jpeg`, `tif`; export adds `penpot`); `window` = attached and the app doesn't say
  `window: false`; `player` = window and the build has `proto.present`.
- INSTRUCTIONS: "pass that as `ifRevision` on every change", the `others` sentence; 448 words.
