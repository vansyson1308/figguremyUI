# Review record: design context, prototype verification and the agent workflow

> **Written:** 2026-11-20 · Covers the fork's work from the baseline `011f08e5` to the delivered
> commit. The full notes of each round (steps, evidence, suspected locations, fix status) were kept
> in `handoff/notes/` and removed before merging; they stay in the repository's history at commit
> `9f05ba8e` (`git show 9f05ba8e:handoff/notes/<file>`).

Severity scale: **P0** data loss or corruption, access outside the allowed folders, a severe
crash; **P1** blocks creating, editing, running or saving a prototype, or makes an agent act on the
wrong document; **P2** a significant problem with use, layout, performance or recovery; **P3**
improvements. Every round was done by someone who did not build what they checked.

| Round | What was checked | Findings | Outcome | Notes file at `9f05ba8e` |
|---|---|---|---|---|
| Baseline | The repository at `011f08e5`: build, tests, MCP, prototypes, automation roots, performance | 16 (F-01–F-16): 2 P1, 8 P2, 6 P3; no P0 | All P1 and P2 fixed with tests; the P3s fixed last (F-11 geometry range, F-13 missing-font warning, F-14 `component.addVariant` on a lone component, F-15 missing-folder error, F-16 a clear message instead of a panic when a system library is missing), except F-12 (no built-in font without craft-fonts: by design, fonts live in craft-fonts) | `baseline-011f08e5.md` |
| R1 engine | Design context, findings, alternatives, flow check, scenarios | 13 (R-01–R-13) | All fixed with tests (re-check identity, authority holes, lenient loading, flow-rule noise) | `review-R1-engine.md` |
| R2 MCP and UI | The MCP server, the control channel, the panel | 12: 1 rated P0 by the lead, 3 P1, 4 P2, 4 P3 | All fixed with tests (revisions read with the result, an agent's step during a person's gesture refused as `busy`, no discarding a person's work, `key` runs as the agent) | `review-R2-mcp-ui.md` |
| QC round 1 | The app by real clicks and keys; the three examples | 16 | 12 fixed and re-checked in the app; QC-07, QC-11, QC-12, QC-13 kept as documented then; QC-11 and QC-12 fixed later | `qc-round1.md` |
| QC round 2 | The renamed release build; round 1 re-checked; README screenshots retaken | 11, all P3; no P0–P2 | All 11 fixed with tests and shots (two batches) | `qc-round2.md` |
| Declared states review | Declared states, required elements, steps to the goal | 3 P2, 10 P3; no P0/P1 | All fixed (M12.21–M12.24) | (in this record) |
| Re-check of that review | The 13 fixes, by another reviewer, with mutation runs | 9 fixed, 3 fixed with an edge case, 1 partly; new: 2 P2, 8 P3 | All fixed (M12.25–M12.30): a declared state excuses only the hidden layers an accepted state shows; checks stop at their caps; repairs keep a person's ids and names | (in this record) |
| Fresh tester (G1) | README and the install runbook, followed literally from a clone at `eeee288e` | 6 documentation problems, no defect | Fixed in `395bcee6` | (in this record) |

## Still open, by decision

- **QC-07**: engine refusals and some rule suggestions shown in the panel name command ids; the
  texts are shared with agents, who need the ids.
- **QC-13**: reduce motion doesn't change prototype transitions (documented).

The rest of what is known to be missing is ranked in [`gaps.md`](gaps.md#figgure-my-ui-the-forks-own-work).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-20 | major | Created from the round notes before `handoff/` was removed. |
