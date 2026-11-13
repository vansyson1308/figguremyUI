# Acceptance matrix: milestone M11 (design context, prototype verification, agent workflow, rebrand)

> **Written:** 2026-11-13, before the milestone's implementation landed · **Baseline commit:**
> `011f08e5` · **Status column** is filled in from runs on the delivered commit; a row without
> evidence is not met.

"Done" for this milestone means every row below is met with evidence, or is listed as not met with
its real-world effect. It does not mean the product has no remaining defects.

Severity scale used for findings: **P0** data loss or corruption, access outside the allowed
folders, a severe crash; **P1** blocks creating, editing, running or saving a prototype, or makes
an agent act on the wrong document; **P2** a significant problem with use, layout, performance or
recovery; **P3** improvements.

## Criteria

| ID | Area | Condition to meet | How it is checked | Status / evidence |
|---|---|---|---|---|
| R1 | Book research | All 10 chapters analysed; each traced to a product decision with a status | `docs/design/book-to-product.md` sections 4 and 9; the "Implemented in" column names code | |
| R2 | Book research | Author's idea, team inference and product decision are kept apart; no long quotations; licence stated | Review of the note | |
| B1 | Defects | No confirmed P0 or P1 open in the delivered scope | Baseline report and review findings, each with its closing evidence | |
| F1 | Brief and rationale | Brief, assumptions and decisions are stored in the file, linked to layers by id, readable and editable by a person in the app and by an agent through commands | Engine tests; MCP end-to-end test; panel screenshot reviewed by someone who did not build it | |
| F2 | Brief and rationale | An agent cannot accept its own decision, mark an assumption supported or refuted, or close a person's feedback | Engine tests run as an agent (`execute_as`) | |
| F3 | Alternatives | Creating an alternative leaves the source unchanged, keeps instance overrides in the copy, is one undo step; the comparison reports facts and flags a restyle-only copy; nothing is ranked or scored | Engine tests; MCP end-to-end test | |
| F4 | Flow check | `proto.check` reports starts, reachable screens, broken targets, dead ends against declared end states and trapped overlays; a declared end state is not a dead end | Engine tests per rule with a near miss each | |
| F5 | Scenarios | A stored scenario runs headless and asserts the state after each step (screen, overlays, variables, variants, visible layers, text); a step nothing reacts to fails | Engine tests; fixture scenarios | |
| F6 | One set of rules | The headless simulator and the app's player reach the same state for the same steps | A parity test that drives both with the same documents and steps | |
| F7 | Critique loop | Checks are deterministic and cite their standard; findings carry layer ids, rule, evidence, severity and a suggestion; a check finding closes only when its rule passes again; opinions and people's feedback are separate kinds; no aggregate score exists anywhere | Engine tests (record, fix, re-check; record, claim fixed without fixing, re-check reopens) | |
| F8 | No placeholders | Every command and tool named in the documentation does what it says; nothing returns success without acting | Independent review; MCP end-to-end tests assert state, not return codes | |
| M1 | MCP | Capabilities, version, backend kind and the document acted on are discoverable; outline first, detail on demand | MCP tests, headless | |
| M2 | MCP | Errors are structured and name the command, parameter or layer at fault; a stale id is refused with a clear error | MCP tests | |
| M3 | MCP | A change can be made conditional on the document not having changed since the agent last read it | MCP test, headless and attached | |
| M4 | MCP | The full loop passes headless: brief, screens, layout and components, interactions, run scenarios, preview image, fix, save, reopen | Scripted MCP session asserting state at each stage | |
| M5 | MCP | The same loop passes attached to a running app (under a virtual display in this environment) | Scripted session against the app's control channel | |
| M6 | MCP | File access stays inside the read and write roots (traversal, links, absolute paths); document content never runs as a command | Baseline probes re-run on the delivered commit | |
| P1 | Prototypes | All scenarios defined for the three example files pass; no broken target; no dead end that is not a declared end state | `proto.scenario.run` and `proto.check` on each file, in a test | |
| D1 | Data | Save and reopen keeps structure, layout, components, variables, interactions and the design context | Round-trip tests comparing the whole document; old `.figcraft` files open unchanged and files without a design context serialize as before | |
| D2 | Undo and redo | Every changing command of this milestone undoes and redoes to the exact document | Engine tests comparing serialized documents | |
| L1 | Layout | In the three example files, no content is cut off or covered by accident at 360, 768 and 1440 px and at one in-between width not used while designing, including the long-text and Vietnamese cases | `design.check` (text.clipped, layout.clippedContent) at each width in a test, plus screenshots looked at by a reviewer | |
| Q1 | Visual and interaction review | Someone who did not build the features looked at the renders, used the main flows, and their findings were handled | Review notes with screenshots; each finding's closing evidence | |
| A1 | Accessibility, of the prototypes made | Text contrast and target size are measured by rule on the example files; reduced motion and keyboard paths are stated as checked or not checked | `design.check` output; written statement of what was not checked | |
| A2 | Accessibility, of the app itself | New panel text meets contrast in both themes; keyboard reachability of the new panel is stated as checked or not; no screen-reader claim is made without a test | Reviewer notes | |
| C1 | CI | The required checks pass on the delivered commit | `cargo xtask ci` parts run locally with output; hosted CI result for the commit, or the reason it could not run | |
| N1 | Rebrand | The product shows as "Figgure My UI" in the app, command line, MCP identity and documents; old-brand mentions remain only for attribution, migration and compatibility, enforced by an automated check | Brand check in `cargo xtask ci`; screenshots | |
| N2 | Compatibility | Existing `.figcraft` files open; old environment variable and resource names keep working or have a documented replacement; the upstream app's settings and recovery files are not overwritten | Tests; migration note | |
| N3 | Attribution | Licences, copyright notices, history and the credit to the upstream project and its authors are kept | Review of `NOTICE`, licences, About | |
| G1 | Documentation | A tester who did not write it can follow the README to start the app, connect an MCP client and make the sample prototype | A fresh agent follows the README from a clean build and reports each step | |

## Performance budgets

Set before optimising. Release build, same machine as the baseline, 30 runs each unless stated;
mean and 95th percentile are reported separately and a mean is never used to claim a percentile.
Machine, document sizes and commands are recorded with the numbers in
[`docs/benchmarks.md`](benchmarks.md).

| ID | Operation | Budget |
|---|---|---|
| PF1 | Operations that existed at the baseline (open, save, outline inspect, create, set properties, 50-step batch, frame screenshot; `figcraft-cli perf` engine and render timings at 1,000 and 10,000 layers) | No more than 10% slower than the baseline in mean and in p95; a larger difference is investigated and explained, not waved through |
| PF2 | `proto.check` on each example file | p95 ≤ 50 ms |
| PF3 | `proto.scenario.run` of all scenarios of one example file | p95 ≤ 150 ms |
| PF4 | `design.check` with the full rule set on each example file | p95 ≤ 500 ms |
| PF5 | `design.get` on each example file | p95 ≤ 20 ms |
| PF6 | Save then reopen of each example file with its design context | Within PF1's limit of the same file without one |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | major | Created before implementation: criteria, checks and performance budgets for M11. |
