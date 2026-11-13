# Baseline of commit 011f08e5 (condensed by the lead from the Repository Investigator's report)

Scripts, logs, PNGs, JSON: this folder. Baseline binaries: `bin/figcraft-cli-release-011f08e5`, `bin/figcraft-cli-debug-011f08e5`.
Machine: 4 vCPU Xeon 2.10 GHz, 15 GB RAM, Ubuntu 24.04, shared (load 5–9 during all measurements).

## Checks at 011f08e5
- build debug app+cli: pass (923 s). fmt: pass. xtask layers/assets/docs: pass. clippy -D warnings (workspace, all targets): pass (509 s).
- tests `--workspace --lib --bins --tests`: 5755 passed, 4 failed, 12 ignored. The 4 are stable and environment (no ../craft-fonts):
  engine board_mindmap_tests ×3, text tests::small_caps_native_or_synthesised. CI requires craft-fonts.
- xtask wasm: first step (cargo check wasm32 on 5 crates) passes; plugins half not run. release cli build: pass (771 s).
- Real desktop app does not start here: libxkbcommon-x11.so missing (panic, exit 101). A windowless harness around the real FigApp
  (`harness/control_harness.rs`) was used for attached-mode probes.

## Confirmed findings
P0: none (51 automation-root probes held; failed batch leaves doc identical; undo/redo exact; 5 examples round-trip with 0 diffs).
- F-01 P1 player: a layer with While pressing (or Mouse up) + On click never fires the click (ui-egui present/input.rs:207-213, :335-339).
- F-02 P1 MCP: edits land in the wrong document after the front file changes; results carry no document identity; `document` optional.
- F-03 P2 proto.present/preview/presentOptions return success headless and do nothing.
- F-04 P2 proto.present {} answers {start:N} for a deleted flow start (engine cmd/proto.rs:320-329).
- F-05 P2 deleting a frame/overlay/variable leaves dangling interactions/flows/variable refs; nothing reports them.
- F-06 P2 failed batch doesn't name the step for "no such node"/"unknown command" (engine error.rs:36-45).
- F-07 P2 one shared undo stack, no revision/change signal: an agent's undo reverts the human's edit.
- F-08 P2 attached server silently reconnects to a different app process (mcp backend.rs:183-187), ids reused.
- F-09 P2 proto.interaction.add/set accept a setVariable for an unknown variable / wrong-typed value (cmd/proto.rs:408-443).
- F-10 P2 while the presenter is open, MCP pointer/key act on the hidden editor canvas (control.rs:1559-1600).
- F-11 P3 geometry values not capped (width 1e308 stored). F-12 P3 no built-in font without craft-fonts.
- F-13 P3 missing font family gives no warning at the call site. F-14 P3 component.addVariant {property,value} fails on a lone component.
- F-15 P3 save_file into a new subfolder: bare OS error. F-16 P3 app panics on missing libxkbcommon-x11; `--help` unknown.

## Gaps
G-1 no headless prototype runtime; G-2 no prototype validation; G-3 no image of a prototype state; G-4 MCP cannot operate the player;
G-5 player state partly observable; G-6 no revision/conflict signal; G-7 results don't identify the document, errors unstructured;
G-8 no app identity in attached mode; G-9 no layout/overflow diagnostics; G-10 no responsive example, 1/5 examples has interactions;
G-11 instructions/prompts don't mention `document`, headless limits or how to verify a prototype.

## Doc discrepancies
D-1 batch "names the failing step" (not always). D-2 `--no-system-fonts` "built-in fonts" (none without craft-fonts).
D-3 "everything builds, tests and runs without craft-fonts" (4 tests fail). D-4 proto.evaluate "like the interaction commands".
D-5 prototyping-parity "all pass" (F-01 untested). D-6 mcp.md typo "not`nwhere"; agents do get absolute path. D-7 prototype_flow verifies data only.

## Performance baseline (release, N=30, ms; noisy shared machine)
MCP round trips mean / p95: inspect outline 0.34/0.45; inspect outline depth 64 1.78/1.95; inspect full depth 64 11.67/13.79;
create_node rect 0.13/0.38; create_node text 0.71/0.37; set_properties 0.11/0.14; batch 50 4.00/4.25; screenshot frame 1240×960 172.5/203.8;
get_node depth 2 0.69/0.87; undo 0.15/0.30; redo 0.14/0.20; save_file 23.53/29.12 (dashboard 268 layers); open_file 3.46/5.34; cold spawn+init+open+outline 7.97/11.65 (N=10).
`figcraft-cli perf --layers 1000,10000` (mean): build+normalize 17.44/65.01; save 5.11/28.14; load 9.43/50.36; render fit 12.22/15.02;
document.inspect depth 8 8.65/113.51; select all+move 1.87/33.64. Full table: perf/perf_cli_output.txt, raw: perf/mcp_timings.json.

## Layout observations
None of the shipped example screens responds to width changes (fixed-width designs; constraints do what they say). Nothing reports clipping.
Vietnamese text with Inter renders all diacritics correctly; auto-width text in a fixed narrow frame is clipped silently.
