# M11 handoff: what is done, what is left, and how to pick it up

> **Stopped:** 2026-11-13, at the owner's request, to continue next week. · **Branch:**
> `claude/clever-thompson-u2vkq9` · **Pull request:**
> [vansyson1308/figguremyUI#1](https://github.com/vansyson1308/figguremyUI/pull/1) (draft, not merged)
> · **Last code commit:** `040ed89e` (CI was running on it when work stopped; the commit before it,
> `4501b37b`, failed CI only because of the browser-build mistake that `040ed89e` fixes).
>
> This folder (`handoff/`) is a working notebook kept in git so nothing is lost when the cloud
> machine is recycled. **Delete it before the pull request is merged** (step G3).

Work through the sections in order. Tick a box only with evidence (a test, a command's output, a
screenshot someone looked at).

## Where things are

| What | Where |
|---|---|
| Unfinished code, **not built or tested** | `handoff/wip/perf-in-progress.patch` (performance work: engine, MCP, text crates), `handoff/wip/qc-fixes-in-progress.patch` (Design context panel fixes). Both apply cleanly to `040ed89e` (`git apply --check` passed). |
| The command contract the features were built to (v1 → v1.3) | `handoff/notes/command-contract.md` |
| Review findings and how each was decided | `handoff/notes/review-R1-engine.md` (engine; all fixed), `handoff/notes/review-R2-mcp-ui.md` (MCP/UI; all fixed), `handoff/notes/qc-round1.md` (visual QC; **open**) |
| The baseline audit of the starting commit `011f08e5` | `handoff/notes/baseline-011f08e5.md` |
| Specs for later work | `handoff/notes/rebrand-spec.md` (deferred rename), `handoff/notes/fixtures-spec.md` (examples), `handoff/notes/app-side-protocol.md` (done; for reference) |
| MCP client setup facts (sources and dates) | `handoff/notes/mcp-client-setup-research.md` |
| Test tools | `handoff/tools/`: `run_app.sh` (the app under a virtual display), `ctl.py` (control-channel client), `mcp_client.py` (MCP stdio client), `attached_e2e.py` (attached-mode end-to-end, 25 steps), `bench.py` (before/after benchmark). They contain absolute paths of the old cloud machine's scratch folder: fix the paths at the top before use. |
| Acceptance criteria and budgets | `docs/acceptance.md` (status column still empty: step F2) |
| Measured performance | `docs/benchmarks.md` |

## A. Set up a machine again (cloud session)

- [ ] Clone the fonts the CI uses, next to the repository: `git clone https://github.com/storytold/craft-fonts ../craft-fonts` then `git -C ../craft-fonts checkout 37827adc41fd165f9a9234ac40f94dbad353aa69`. (The build script looks for `fonts/manifest.txt`, which that checkout doesn't have, so locally no fonts are embedded and system fonts are used; CI passes anyway. Don't "fix" craft-fonts.)
- [ ] Install the wasm target: `rustup target add wasm32-unknown-unknown`.
- [ ] To run the real app headless: the machine lacked `libxkbcommon-x11-0`, `libxcb-xkb1`, `libegl1`, `libegl-mesa0`. Installing them was denied by the permission policy last time; they were downloaded with `apt-get download` and unpacked with `dpkg -x` into a scratch folder that `run_app.sh` points `LD_LIBRARY_PATH` and `__EGL_VENDOR_LIBRARY_DIRS` at, with `WGPU_BACKEND=gl` and `LIBGL_ALWAYS_SOFTWARE=1`. Redo that (or install them if allowed).
- [ ] Disk is the tightest resource (one debug build ≈ 10 GB; the linker crashes with "Bus error" when the disk fills). Use `CARGO_INCREMENTAL=0`, run one large build at a time, delete old test executables in `target/debug/deps`.
- [ ] Baseline release CLI for before/after benchmarks: rebuild it from `011f08e5` in a separate worktree (`git worktree add --detach ../baseline 011f08e5`, `cargo build --release -p figcraft-cli`), since the copy from last time is gone with the machine.

## B. Finish the in-flight work

### B1. Performance (two budgets missed; `docs/benchmarks.md`)
- [ ] `git apply handoff/wip/perf-in-progress.patch`, build, and see what state it is in. It was mid-change: replacing a private `Open` struct in `crates/mcp/src/tools.rs` with `OpenDocument`; it adds `crates/engine/src/modes_cache_tests.rs` and `crates/mcp/tests/it/call_cost.rs`. Treat it as a draft: keep what is sound, throw away what isn't.
- [ ] Profile and confirm the cause before optimising (the cause is suspected, not proven):
  - [ ] **PF1** `set_properties` +22% mean / +27% p95 and `undo` +34% / +39% (0.02–0.11 ms). Suspected: extra backend requests per tool call for document identity and revision (`about_document` → `doc.list`, the "revision before the first request" read), and author bookkeeping in history.
  - [ ] **PF2** `proto.check` p95 53 / 104 / 129 ms on the three examples, budget 50 ms. Suspected: the scenario rules re-run every stored scenario inside `proto.check`, and each simulated step re-applies bindings over the whole document.
- [ ] Fix without weakening the safety features (atomic revision, document identity in every result, conflict detection, authors) and without changing behaviour (all tests, the simulator/player parity suite and the examples' tests unchanged).
- [ ] **PF6** not measured yet: save/reopen of each new example against the same file with its design context removed.
- [ ] Re-measure on a quiet machine with `bench.py` (baseline and new interleaved), update `docs/benchmarks.md`. Do not lower a budget or change the workload to pass; if one still can't be met, write down why with the profile.

### B2. Visual QC round 1 fixes (`handoff/notes/qc-round1.md`)
- [ ] `git apply handoff/wip/qc-fixes-in-progress.patch` (panel files only, unfinished: it had started on QC-01) and continue.
- [ ] **QC-01 (P1)**: the Design context panel shows another file's check results, scenario results, flow check and comparison after a tab switch or opening a file (a never-run scenario showed "Passed"). Results must be kept per document. Test: run in file A, switch to B → nothing from A.
- [ ] QC-02 (P2, example): the sign-up form's error summary stays after the field is fixed. Fix in `examples/build/build_examples.py`, regenerate, pin with a scenario expectation.
- [ ] QC-03 (P2, example): in the bike booking, Back on step 2 goes to step 3 after "Đổi". Same way.
- [ ] QC-04 (P2): keyboard focus in the panel can move out of view; scroll the focused control into view.
- [ ] QC-05 (P2): dark theme frame names on a light page: 1.92:1; pick the label colour against the page colour (≥ 4.5:1).
- [ ] QC-06 (P2): Vietnamese panel partly English (the simulator caveat items; the rail label "Tài ngu…" cut). Rule messages from the engine may stay English if the panel says so.
- [ ] QC-08, QC-09, QC-10, QC-14 (subset), QC-15 (`acceptedBy` on decisions and end states), QC-16 (focus reported for the panel): as listed in the notes file.
- [ ] Look at every fix in the real app (screenshots read, not just taken).
- [ ] Commit, then run the full checks (section F1) and push.

## C. Visual QC round 2 and README screenshots
- [ ] An independent reviewer (not whoever made the fixes) repeats the QC walk on a fresh release build: the three examples played by real clicks, the panel in light/dark/Vietnamese, keyboard, contrast.
- [ ] Retake the README screenshots after the fixes (`docs/images/m11-*.png`; the current ones are real but from `4501b37b`: `m11-panel-findings.png` still shows the QC-08 "changed since" line and two decisions made stale by the reviewer's own brief edit, `m11-dashboard-360.png` shows the frame covering the component sheet on the canvas).

## D. Decisions for the owner (ask once each; don't act without an answer)
- [ ] **Remove `docs/brand/` (the ArtCraft logo files)?** Their licence (`docs/brand/LICENSE-brand.txt`) says a distributed or published modified version must remove the marks. Nothing in the app, README or docs uses them any more, but the files are still in the repository: deleting them was blocked by the session's permission policy. Recommended: delete them, and the `docs/brand/` row in `ATTRIBUTION.md`, before the fork is published.
- [ ] **Contributor credit**: AGENTS.md asks to offer the person preparing the PR a `[people.<github username>]` entry in `contributors/people.toml` (a real name, a display name, and/or `sync_github_name = true`). Ask the owner once; add only their own entry, only if they want it.
- [ ] **Rebrand go-ahead** (deferred by the owner on 2026-11-13). The full spec is `handoff/notes/rebrand-spec.md`.
- [ ] Confirm the example files may ship with their end states and decisions accepted by the generator's maintainer step (`accept_as_maintainer()` in `examples/build/build_examples.py`, documented in `docs/examples.md`): it stands in for the owner's review of those files.

## E. Rebrand (deferred; after B–D)
- [ ] Follow `handoff/notes/rebrand-spec.md`: display name "Figgure My UI", binaries `figguremyui` / `figguremyui-cli` / `figguremyui-server`, app id `io.github.vansyson1308.figguremyui`, a new original icon (draft in the spec), `figguremyui://` with `figcraft://` still accepted, `FIGGUREMYUI_*` env vars with the old names still read, settings migration that copies and never writes the old folder, the brand guard `cargo xtask brand`, `docs/migration.md`. Crate names and the `.figcraft` format stay.
- [ ] Already done before the rename (don't redo): About says the build is a fork based on FigCraft and not endorsed by its makers; help and the updater point at this fork (the updater never takes an upstream release); no default sharing server; Windows company name. (Commit `e2226b2d`, fix `040ed89e`.)
- [ ] After the rename: update README names and commands, rerun everything in F.

## F. Before merging
- [ ] **F1 checks** on the final commit, with output kept: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --lib --bins --tests` (6094 passed at `4501b37b`); `cargo test --workspace --doc`; **`cargo xtask wasm`** (the browser build broke once without anyone noticing locally); `cargo xtask layers`, `assets`, `docs`; the attached end-to-end script against the real app.
- [ ] **F2 acceptance matrix**: fill the Status / evidence column of every row in `docs/acceptance.md` (R1, R2, B1, F1–F8, M1–M6, P1, D1, D2, L1, Q1, A1, A2, C1, N1–N3, G1, PF1–PF6) with the test names, commands or screenshots that prove it, or "not met" with the effect. N1–N3 stay "not met: rename deferred by the owner".
- [ ] **F3 README tested by someone who didn't write it**: on a clean machine, follow `README.md` to build, open an example, connect an MCP client headless and attached, and make the sample prototype; fix what they trip over. (The README was written at the stop; not yet tested.)
- [ ] **F4 records**: `ROADMAP.md` and `docs/roadmap.md` (an M11 entry and revision rows), `docs/gaps.md` rows touched, `docs/prototyping-parity.md` already has rows from the player fixes; the "Implemented in" column of the traceability table in `docs/design/book-to-product.md` (still "TBD").
- [ ] **F5** CI green on the final commit (the shared gate, SonarCloud, house rules). Then the owner has already said: merge the pull request when the final CI is green, and report.

## G. Known limits and backlog (not blocking; keep them true in the docs)
- [ ] Live sessions sync the design context as one value: two people editing it at the same moment lose one side (review R-06). Per-entry sync is backlog.
- [ ] Scenario steps can't be written in the panel yet (agents and the command line write them).
- [ ] Simulator vs player: five documented differences remain (listed in every result as `knownDifferences`, pinned by the parity test); no animation timing, scroll, drag direction, devices, hit-testing.
- [ ] Engine refusals and some rule suggestions shown in the panel contain command ids (QC-07); the presenter's arrows walk into components (QC-11); hotspot hints on after-delay frames (QC-12); reduce motion doesn't change prototype transitions (QC-13, documented).
- [ ] Baseline P3 findings not addressed: F-11 (geometry values not capped), F-13 (no warning at the call site for a missing font family), F-14 (`component.addVariant {property, value}` on a lone component), F-15 (`save_file` into a missing folder gives a bare OS error), F-16 (`figcraft --help`, and a panic instead of a message when `libxkbcommon-x11` is missing).
- [ ] No screen-reader pass over the new panel; `ui.inspect` doesn't report its focus (QC-16).
- [ ] `.fig` export leaves out variable and conditional actions (documented).
- [ ] **G3** delete `handoff/` (move anything still needed into `docs/`) in the last commit before merging.

## Done in this milestone (for orientation; details in the commits and `docs/`)
Research note on *The Shape of Design* (10/10 chapters, `docs/design/book-to-product.md`); design
context in the file (brief, assumptions, decisions, findings, alternatives, stage) with authority rules;
18 measured checks with a re-check loop; headless prototype simulator, flow check, end states and
stored scenarios; MCP working document, revisions and `ifRevision`, structured errors, capabilities,
verification tools, resources, instructions and 11 prompts; the Design context panel; player fixes
(press/hover + click); a simulator/player parity test; app control channel: error details, identity,
agent gestures, `ifRevision` on canvas input; three example prototypes with 15 scenarios built by a
script; fork compliance (About, updater, default server); fixes for every confirmed P0/P1 from the
baseline and two review rounds. Still open: QC-01 (P1, section B2).
