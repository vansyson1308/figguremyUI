# Benchmarks: fork milestone FM1 before and after

> **Measured:** 2026-11-20 · **Baseline:** `011f08e5` · **Before:** `9f05ba8e` (FM1 as delivered)
> · **New:** `99e3ae65` (the performance work M12.40–M12.45 on top of it) · Release builds of the
> CLI (`figcraft-cli` at the baseline, `figguremyui-cli` since). **Status:** PF2–PF5 **met** for
> all three examples (PF2 `proto.check` p95 27 / 35 / 44 ms, was 47 / 63 / 68 ms). PF1 means are
> within 10% but for `inspect_document {outline}` (+27%, 0.09 ms), and p95 is **not shown within
> budget** for `inspect_document {outline}`, `undo` (+10.3%) and `save_file` (+29%) in the deciding
> run; none of these is a code cost (the server's instructions are equal to or fewer than the
> baseline's). PF6 saving is met; reopening is **missed** for `signup-flow` (+18% mean, +33% p95)
> and `pantry-dashboard` (+11% / +15%): the cost of reading the context's JSON (+7.6% and +4.2% of
> the instructions). Details under each table. **After review (M12.47–M12.50, `1bc2ca40`):** the
> caches' hash is keyed per process (a file could choose ids that all collide), and a stored context
> opens, and is refused, exactly as through a `Value`; this costs +0.5% / +0.5% / +0.9% of
> `proto.check`'s instructions and takes a context's reopening overhead to +8.2% / +4.7% / +5.4%;
> PF2–PF5 still met (n = 30). See [After the review fixes](#after-the-review-fixes-m1247m1250).

## How it was measured

- Machine: Intel Xeon @ 2.30 GHz, 4 vCPU, 15 GB RAM, Ubuntu 24.04. The deciding run started at
  load average 0.96 (1 minute) after the machine had been idle down to 1.14, and nothing else ran
  during it; loads are given with each table.
- Each operation is a `tools/call` round trip over MCP stdio, headless, timed in the client (JSON
  encode, pipe, server, JSON decode), with `handoff/tools/bench.py`: each round runs every
  operation 10 times in a fresh server process, the baseline and the measured binary alternating
  round by round so both see the same load; 10 rounds (n = 100), samples pooled. The new build was
  run first, then the same script with `9f05ba8e` in place of it, back to back (a minute apart), so
  before and after share the session; the new operations were also run with the two binaries
  interleaved round by round (`proto.check`, `proto.scenario.run`, `design.check` as `bench.py`
  runs them, 10 rounds × 10). PF6 with `handoff/tools/pf6.py` (10 rounds × 10), for both builds.
- Mean and 95th percentile (nearest rank) are reported separately; a mean is never used to claim a
  percentile.
- Causes were found with profiles of release builds: `perf` (software clock, DWARF call graphs)
  and callgrind (exact instruction counts and call counts). Instruction counts are given where
  wall-clock time can't separate a small difference from the machine's noise.
- Pre-existing operations ran on `examples/ridgeline-studio-dashboard.figcraft` (268 layers), the
  new operations on the three new examples.
- Results are unchanged: every answer of `proto.check`, `proto.scenario.run`, `design.check` (all
  rules and default), `design.get`, the state, end-state and scenario lists, `inspect_document`
  (outline and depth 64), a short edit session and the bytes of the documents saved is identical
  between `9f05ba8e` and the new build on all eight examples; `the_prototype_answers_of_each_example_stay_what_they_were`
  (`crates/mcp/tests/it/examples.rs`) pins digests of the prototype answers, worked out from
  `9f05ba8e`'s answers as well.

## What was slow, and why (profiles)

**The regression came with the example files, not the code.** Between `21e99ada` and `9f05ba8e`
the three examples gained declared states, required elements, text expectations, more variables
set by their interactions and scenarios (sign-up 5 → 7, bike booking 5 → 6; the dashboard keeps 5).
Running each binary on each version of the files separates the two (callgrind, instructions for
one `proto.check` call after a warm-up, mean of three):

| | signup-flow | pantry-dashboard | bike-repair-booking |
|---|---:|---:|---:|
| `21e99ada` binary, files of `21e99ada` | 75.2 M | 138.2 M | 233.6 M |
| `9f05ba8e` binary, files of `21e99ada` | 75.2 M | 138.1 M | 233.3 M |
| `21e99ada` binary, files of `9f05ba8e` | 136.7 M | 247.2 M | 282.4 M |
| `9f05ba8e` binary, files of `9f05ba8e` | 137.7 M | 248.0 M | 282.7 M |

The code between the two commits costs 0.0–0.7% (the new states facts, the `states.*` and
`context.requiredMissing` rules); the files cost +82% / +79% / +21%. Wall-clock (n = 30 each, load
about 2) agreed: `proto.scenario.run` on pantry-dashboard took 19.3 ms with the old files and
36.6 ms with the new ones on the `21e99ada` binary, 19.6 and 33.1 ms on `9f05ba8e`. (The
`21e99ada` binary is the release build kept from the run of the previous version of this document;
it has none of the `proto.state.*` commands.)

**The hypothesis that stored scenarios now run twice per call is refuted.** In one `proto.check`
on bike-repair-booking, `run_watched` runs 6 times (the 6 stored scenarios) and `Sim::step` 37
times (their steps): the scenario rules and the states facts share one watched run per scenario
through `PageFacts` (call counts from callgrind; this code is the same at `9f05ba8e`).
`proto.check` costs 0.1–1.8% more instructions than `proto.scenario.run` of the same file.

**Where the time went.** Each set-variable action of a simulated run settles the copy (normalise,
then copy the document to play the instances' interactions), and the examples' interactions now
set more variables (the dashboard's Refresh and filters set the four counts): per call, steps and
variable sets went from 29 / 45 to 35 / 76 (sign-up), 16 / 91 to 16 / 135 (dashboard) and 30 / 100
to 37 / 122 (bike booking), and each one got dearer (pantry-dashboard: 0.90 M to 1.13 M
instructions a normalisation). Where a call went (callgrind of `proto.scenario.run` on
pantry-dashboard with the delivered code; inclusive shares, so they overlap):

- `normalize` 63% of the call: its full pass over the variable bindings after each variable set
  32%, auto layout 21%;
- copying layers to write them (`Arc::make_mut` of a layer shared with the version before) 15%:
  auto layout took every layer it placed or sized for writing and only then found it already where
  it was, which is most layers of a layout that runs again;
- playing the copy after each settle 16%: `play_interactions` built each instance's played list by
  copying it, then compared (8%), and the document was copied and dropped (7%);
- copying the document's styles and every variable with each copy of it (in a later profile, on
  bike-repair-booking after M12.41: 92 K instructions a copy, 11% of the call), and setting one
  variable copied all of them again (8%);
- a binding pass copied each layer's bindings, hashed short ids with SipHash for its caches (resolving
  variables 6%) and worked out a component set's variant options for every bound instance.

**The first call in a server process** (the p95 of `bench.py`'s samples, 10 of every 100) cost
528 M instructions on bike-repair-booking against 283 M for later calls: walking the font folders
allocated a zeroed buffer the length of every font file (110 M of `calloc`'s `memset`), and every
face loaded, the built-in ones among them, made its whole character map (79 M); the rest was the
same simulation as later calls plus the first shaping of each text.

**PF6.** Reopening a saved example cost 11.4% / 6.5% / 8.0% more instructions with its design
context than without, before M12.45 (signup-flow 11.8 M against 10.6 M an open): `read_stored` built a
`serde_json::Value` of the context, deserialized the context from it, copying every string, and
dropped it (0.86 M an open). (Reopening the same path twice is answered without reading the file,
so these counts reopen distinct saved copies, as `pf6.py` does.)

## What was changed (M12.40–M12.45)

- **M12.40:** the font-folder walk reads every file through one buffer whose written ranges are
  zeroed again (each read sees the bytes a fresh zeroed buffer held); a face makes its character
  map the first time a character is looked up.
- **M12.41:** auto layout writes a layer only when the value changes, and records it as touched in
  the dirty set exactly as before; `play_interactions` compares before copying; a binding pass
  reads the bindings in place, reuses the last set of modes and works out a set's variant options
  only for a value the shown variant doesn't have.
- **M12.42 and M12.44:** a document's styles, and each of its variables, are a `Shared` value (an
  `Arc` copied on write) shared by its copies until one changes them; read, compared (by value), written
  to the file and read back as before.
- **M12.44:** the per-pass caches (binding resolution, `ModesCache`, the text layout cache) use
  `FastMap`, a cheaper hash than the standard one; they are only read by key, and a text-cache hit
  is confirmed in full. (As first written it was a multiply-rotate hash with a fixed constant, which
  a file could defeat: M12.47 keys it.)
- **M12.45:** a stored design context that reads whole is read straight from its JSON text into its
  types (serde_json's `raw_value`); one that doesn't goes through a `Value` as before, so what is
  kept, repaired and reported is the same.

Instructions now (callgrind as above): `proto.check` 78.7 M / 143.0 M / 172.8 M, 57% / 58% / 61%
of `9f05ba8e`'s and on bike-repair-booking below `21e99ada`'s on its own, smaller, files; the first
call 110 M / 176 M / 215 M (was 371 M / 483 M / 528 M); reopening with a context +7.6% / +4.2% /
+4.9% over the same file without one.

## After the review fixes (M12.47–M12.50)

A review of M12.40–M12.46 found three faults, fixed on top of `859be1d9` (each with a test that
fails at `859be1d9`):

- **M12.47, `FastMap` could be made to collide by a file.** It hashed a word as `id × K` with a
  known `K` and no final mix, and its keys come from the file (layer ids in `ModesCache`, variable
  ids in a binding pass, text in the text cache): 50,000 layer ids `(i << 24) · K⁻¹` share one slot
  (inserting them took 1.39 s against 2.0 ms), and a 16-byte variable id's second word can cancel its
  first. Now each word is folded in by a 64 × 64 → 128-bit multiply with a secret key, the result
  goes through MurmurHash3's 64-bit finaliser, and the keys are drawn once per process from the
  standard library's `RandomState` (no new dependency; it builds for wasm32). The tests bound slot
  collisions rather than time: the crafted ids, and 50,000 byte strings crafted to collide fully,
  put at most 32 keys in one slot of 2¹⁶.
- **M12.48, a key given twice in a state's `shownBy`** read as the other variant straight than
  through a `Value`; `ShownBy` now always picks its variant from a `Value` (the last of a key, as
  before).
- **M12.48 and M12.50, JSON a `Value` refuses opened.** A lone surrogate (`"\ud800"`) used to make
  the file fail to open as corrupt; read straight, the file opened with the whole context dropped,
  and saving deleted it. A number past `f64` (`1e400`) in a field the context doesn't know, and
  nesting past serde_json's limit (counted from the context rather than the document), also opened.
  A one-pass scan of the context's text flags these (numbers and nesting generously; 8 bytes at a
  time through strings, 5.6 µs for signup-flow's 12.6 KB context), and so does a context whose
  `Value` read fails; the document is then read again with the context through a `Value` from the
  document's own reader, which gives the old result or the old error, at its place, exactly. The
  old refusal of `1e400` is kept, since the scan is cheap. (A `Document` deserialized from a
  `serde_json::Value` rather than from text, as only tests do, gets an error naming
  `figcraft_doc::design::document_from_json` for a context nested 100 deep.)

Instructions (callgrind, release, `1bc2ca40` against `99e3ae65`):

| | signup-flow | pantry-dashboard | bike-repair-booking |
|---|---:|---:|---:|
| `proto.check`, a call after a warm-up (mean of 3) | 79.3 M (+0.5%) | 144.4 M (+0.5%) | 174.5 M (+0.9%) |
| `proto.scenario.run`, a call after a warm-up | 78.5 M (+0.7%) | 140.4 M (+0.5%) | 173.5 M (+1.0%) |
| `proto.check`, the first call | 110.0 M (+0.4%) | 177.1 M (+0.4%) | 216.8 M (+0.6%) |
| Reopening (10 copies): with a context / without | 115.0 M / 106.3 M (+8.2%) | 134.6 M / 128.6 M (+4.7%) | 161.3 M / 153.1 M (+5.4%) |

(Reopening was +7.6% / +4.2% / +4.9% at `99e3ae65`. The `proto.*` rows were counted at `9a2d3d4d`,
before M12.50, which changes only how a file is opened.) A whole `figguremyui-cli perf --layers 1000
--instances 0` run (fixed repeat counts) is 4.825 G instructions against 4.820 G (+0.09%).

Wall-clock, n = 30 (3 rounds × 10), load 0.95 → 1.26 → 1.15, same scripts as the deciding run:
`proto.check` p95 25.0 / 31.8 / 45.1 ms (mean 13.3 / 21.5 / 34.6), `proto.scenario.run` p95 13.6 /
32.4 / 42.7, `design.check` p95 16.2 / 38.3 / 37.1: PF2–PF5 still met. `99e3ae65` in the same
session: `proto.check` p95 26.5 / 37.7 / 51.5. PF1 against the baseline, mean / p95: outline −0% /
−11%, depth 64 −15% / −10%, `create_node` −42% / −33%, `set_properties` −14% / −25%, `batch` −37% /
−29%, screenshot −7% / −17%, `undo` +32% / +24%, `save_file` +8% / +41%, `open_file` +2% / −8%
(`99e3ae65`, same session: undo −5% / +6%, save +3% / +8%). PF6, mean / p95 with a context against
without: save −9.5% to +1.8% / +0.8% to +2.7%; reopen −0.3% / +1.4% (signup-flow), −1.7% / +5.1%
(pantry-dashboard), +7.0% / −3.0% (bike-repair-booking); `99e3ae65` in the same session: reopen
+18.9% / +6.1%, −3.9% / −2.9%, +39.5% / +86.1%. At n = 30 these sub-millisecond and per-file figures
move by tens of points between two runs of the same binary (compare the two builds' PF6 rows), so
they show the budgets weren't moved, not small differences; the instruction counts above are the
measurement of what the fixes cost. The `perf` table, median of eight interleaved runs each (load
0.67 → 0.76), moved within −7% to +4% in the rows that use these caches (build + normalize, edit
main component, `document.inspect`, select all + move) and by up to ±34% in rows that don't (a
re-render after one move at 10,000 layers: +20% / +34%), which is the run-to-run spread of this
machine.

## PF1: operations that existed at the baseline (budget: at most 10% slower, mean and p95)

Deciding run, n = 100 (10 rounds × 10), load 0.96 → 1.03, new build (`99e3ae65`) against the
baseline. Milliseconds.

| Operation | Baseline mean | New mean | Δ mean | Baseline p95 | New p95 | Δ p95 | `9f05ba8e`, same session | `1d5884f9`, 13 min earlier | `9f05ba8e`, then |
|---|---:|---:|---:|---:|---:|---:|---|---|---|
| `inspect_document {outline:true}` | 0.34 | 0.43 | **+27%** | 0.55 | 0.67 | **+22%** | +9% / +5% | +12% / +20% | -7% / -2% |
| `inspect_document {depth:64}` | 9.86 | 10.38 | +5% | 12.06 | 13.06 | +8% | +14% / +11% | -0% / +7% | -4% / -11% |
| `create_node` rectangle | 0.20 | 0.18 | -10% | 0.50 | 0.49 | -2% | +3% / -8% | +6% / -8% | +4% / +2% |
| `set_properties` | 0.12 | 0.10 | -14% | 0.23 | 0.16 | -33% | -2% / -5% | +23% / +1% | +18% / +15% |
| `batch` of 50 steps | 3.45 | 2.54 | -26% | 5.71 | 3.45 | -40% | +1% / +35% | -21% / -30% | +8% / +12% |
| `screenshot` of one frame | 164.28 | 167.47 | +2% | 191.56 | 194.00 | +1% | -1% / +2% | -3% / -5% | -0% / +2% |
| `undo` | 0.13 | 0.13 | +3% | 0.34 | 0.37 | **+10.3%** | -13% / -10% | +1% / +15% | -9% / -11% |
| `save_file` | 8.20 | 8.59 | +5% | 9.37 | 12.04 | **+29%** | -5% / -18% | +6% / +24% | -2% / -3% |
| `open_file` | 2.30 | 2.07 | -10% | 3.41 | 2.55 | -25% | -2% / -13% | -5% / -18% | +7% / +4% |

The last three columns are Δ mean / Δ p95 against the baseline in the other runs of the session:
`9f05ba8e` measured right after the deciding run (load 1.03 → 1.12), and the run 13 minutes
earlier of `1d5884f9` (M12.40–M12.44, without M12.45) and of `9f05ba8e` after it (load 0.99 →
1.16). The same binary moves by 15–30 points between two runs on these sub-millisecond and
p95-of-ten-per-process figures, so the server's own work was counted (callgrind, 10 calls each, after
the setup `bench.py` does before them):

| Operation | Baseline | `9f05ba8e` | New | New against baseline |
|---|---:|---:|---:|---:|
| `inspect_document {outline:true}` | 1.96 M | 1.97 M | 1.98 M | +0.9% |
| `inspect_document {depth:64}` | 54.0 M | 53.6 M | 53.1 M | −1.8% |
| `create_node` rectangle | 0.37 M | 0.32 M | 0.31 M | −16% |
| `set_properties` | 0.33 M | 0.28 M | 0.27 M | −16% |
| `batch` of 50 steps | 21.1 M | 20.8 M | 14.8 M | −30% |
| `undo` | 0.18 M | 0.13 M | 0.14 M | −21% |
| `save_file` | 96.3 M | 104.3 M | 84.1 M | −13% |
| `open_file` | 16.4 M | 16.5 M | 16.3 M | −1.1% |

(Instructions a call.) None of the three figures outside the budget is a cost of the code: the
outline's server work is within 1% of the baseline's, `undo` and `save_file` do less. They stay
open as "not shown within budget" because this machine can't show them within it.

### `figguremyui-cli perf --layers 1000,10000 --instances 0` (engine and render, in-process)

Median of eight runs each, interleaved (baseline, `9f05ba8e`, new; eight times), load 0.51 → 0.93.
Milliseconds unless noted.

| Operation | 1,000: base | `9f05ba8e` | new | Δ base | 10,000: base | `9f05ba8e` | new | Δ base |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| build + normalize (like open) | 12.80 | 11.93 | 12.09 | −6% | 65.19 | 65.58 | 64.44 | −1% |
| save .figcraft | 3.17 | 3.13 | 3.19 | +1% | 26.73 | 26.94 | 26.95 | +1% |
| load .figcraft | 4.77 | 4.77 | 4.49 | −6% | 48.53 | 50.34 | 47.66 | −2% |
| render 1440×900 @100% | 5.54 | 5.78 | 5.60 | +1% | 4.58 | 4.29 | 4.31 | −6% |
| render 1440×900 zoomed to fit | 7.42 | 7.70 | 7.18 | −3% | 9.29 | 9.52 | 9.55 | +3% |
| render zoomed to fit, exact (no LOD) | 7.52 | 7.08 | 7.20 | −4% | 27.70 | 30.14 | 30.72 | **+11%** |
| render zoomed out (2× fit) | 6.05 | 5.68 | 5.56 | −8% | 14.88 | 14.70 | 15.58 | +5% |
| pan 40 px @100% | 0.92 | 0.96 | 0.96 | +5% | 1.07 | 1.12 | 1.10 | +2% |
| pan 40 px zoomed out | 0.87 | 0.93 | 0.92 | +6% | 2.58 | 2.81 | 2.78 | +8% |
| move one layer, re-render @100% | 0.06 | 0.07 | 0.07 | **+17%** | 0.69 | 0.73 | 0.71 | +4% |
| move one layer, re-render zoomed out | 0.08 | 0.08 | 0.08 | +7% | 0.76 | 0.71 | 0.65 | −15% |
| hit test (µs each) | 19.70 | 19.77 | 19.65 | −0% | 40.23 | 41.15 | 38.64 | −4% |
| boolean of 24, top operand | 0.65 | 0.66 | 0.65 | +0% | 0.36 | 0.36 | 0.34 | −7% |
| boolean of 24, bottom operand | 6.34 | 6.23 | 6.18 | −3% | 5.89 | 5.97 | 5.62 | −4% |
| hover pointer move (µs each) | 19.93 | 19.58 | 19.57 | −2% | 40.87 | 40.86 | 39.15 | −4% |
| node.set fill on one layer | 0.06 | 0.06 | 0.06 | +0% | 0.02 | 0.02 | 0.02 | — |
| edit main component | 0.19 | 0.19 | 0.19 | +0% | 3.26 | 3.27 | 3.20 | −2% |
| document.inspect (depth 8) | 8.25 | 8.11 | 8.35 | +1% | 104.99 | 105.97 | 100.85 | −4% |
| select all + move every screen | 2.06 | 2.08 | 2.10 | +2% | 27.04 | 30.01 | 27.37 | +1% |

Rows under 0.05 ms (transform, text edit, undo, redo, drag preview, normalize after one edit) are
0.00–0.04 ms in all three. Within 10% but two rows: `move one layer, re-render @100%` at 1,000
layers is 0.06 against 0.07 ms, the tool's last digit; the exact render at 10,000 layers is +11%
(+9% for `9f05ba8e`, so it predates this work, which doesn't touch rendering). The tool reports no
percentiles. An earlier pair of runs (load 1.1–1.5) differed by up to ±30% a row in both directions,
which is why eight runs and their median are given here.

## PF2–PF5: new operations

Deciding run, n = 100, load 0.96 → 1.03 (new build) and 1.03 → 1.12 (`9f05ba8e`, right after).
Milliseconds.

| Operation | File | New mean | New p95 | Budget (p95) | `9f05ba8e` mean / p95 |
|---|---|---:|---:|---|---|
| `proto.check` | signup-flow | 14.05 | 26.61 | ≤ 50: met | 23.25 / 47.23 |
| `proto.check` | pantry-dashboard | 22.04 | 35.07 | ≤ 50: met | 38.53 / 63.29 |
| `proto.check` | bike-repair-booking | 29.91 | 43.92 | ≤ 50: met | 44.99 / 67.69 |
| `proto.scenario.run` (all) | signup-flow | 13.46 | 18.48 | ≤ 150: met | 20.11 / 24.31 |
| `proto.scenario.run` (all) | pantry-dashboard | 19.55 | 21.58 | ≤ 150: met | 35.94 / 50.30 |
| `proto.scenario.run` (all) | bike-repair-booking | 29.08 | 41.64 | ≤ 150: met | 43.50 / 51.19 |
| `design.check` (all 23 rules) | signup-flow | 15.14 | 21.13 | ≤ 500: met | 22.24 / 32.39 |
| `design.check` (all 23 rules) | pantry-dashboard | 20.51 | 26.99 | ≤ 500: met | 34.86 / 39.84 |
| `design.check` (all 23 rules) | bike-repair-booking | 28.87 | 40.97 | ≤ 500: met | 44.98 / 60.69 |
| `design.get` | signup-flow / pantry-dashboard / bike-repair-booking | 0.51 / 0.35 / 0.40 | 0.75 / 0.57 / 0.60 | ≤ 20: met | 0.48 / 0.40 / 0.50, p95 0.69 / 0.69 / 0.73 |

With the two binaries interleaved round by round (10 × 10, load 1.16 → 1.04), mean / p95,
`9f05ba8e` → new: `proto.check` 23.90 / 47.31 → 14.17 / 26.37 (signup-flow), 41.42 / 71.84 →
23.50 / 34.92 (pantry-dashboard), 48.00 / 75.39 → 29.69 / 42.89 (bike-repair-booking);
`proto.scenario.run` 20.29 / 25.48 → 13.71 / 20.14, 35.14 / 43.04 → 21.16 / 31.67, 43.67 / 66.74 →
27.48 / 36.20; `design.check` 21.15 / 23.58 → 14.27 / 19.99, 34.52 / 42.59 → 19.72 / 22.46, 42.62 /
52.07 → 28.01 / 32.73.

**`proto.check`'s p95 is still its first call in each server process** (10 of the 100 samples):
in the deciding run the first call took 25.9 / 36.8 / 45.8 ms on average (median 26.6 / 35.2 /
44.1, slowest 32.7 / 51.5 / 61.7) and the 90 later calls 12.7 / 20.4 / 28.1 ms mean, 17.0 / 25.0 /
34.9 p95 (`9f05ba8e`: first 45.8 / 62.6 / 69.4, later 20.7 / 35.9 / 42.3 mean). bike-repair-booking
meets the budget with 6 ms to spare; a run whose first calls come out slower (one of the ten was
61.7 ms) would be near the line. What is left of the first call is the simulation itself, the
first shaping of each text and the faces read for it.

## PF6: save and reopen with a design context against the same file without one

`handoff/tools/pf6.py` (`save_file` then `open_file` of what was saved, 10 each per round, a fresh
server per round, the two files alternating), the copies without a context made by deleting the
`design` key from `document.json` in the `.figcraft` zip. Budget: within PF1's limit (10%) of the
file without one. n = 100, milliseconds, mean / p95.

| Operation | File | New, load 1.12 → 1.18: without | with | Δ | `9f05ba8e`, load 1.18 → 1.16: Δ |
|---|---|---|---|---|---|
| `save_file` | signup-flow | 9.94 / 27.20 | 10.34 / 26.73 | +4.0% / −1.7% | +4.0% / +0.7% |
| `save_file` | pantry-dashboard | 6.21 / 20.38 | 6.56 / 20.17 | +5.6% / −1.0% | +0.9% / +0.7% |
| `save_file` | bike-repair-booking | 11.42 / 31.16 | 12.02 / 29.92 | +5.2% / −4.0% | +15.3% / +13.4% |
| `open_file` | signup-flow | 1.63 / 2.07 | 1.92 / 2.76 | **+17.9% / +33.2%** | +12.4% / +25.4% |
| `open_file` | pantry-dashboard | 1.91 / 2.35 | 2.12 / 2.69 | **+11.1% / +14.5%** | −0.9% / +1.5% |
| `open_file` | bike-repair-booking | 2.35 / 2.88 | 2.37 / 2.95 | +0.9% / +2.2% | +15.9% / +10.4% |

Saving: met. Reopening: met for bike-repair-booking, **missed for signup-flow and
pantry-dashboard** in this run. The build before M12.45 (`1d5884f9`) gave signup-flow +6.1% /
+5.3% and pantry-dashboard +12.9% / +4.5% thirteen minutes earlier, and +18.9% / +30.7% and +1.3% /
−7.6% with the two files of each pair run in the other order, so the differences aren't the order
the script runs them in, and they move by ten points from run to run. The server does +7.6% /
+4.2% / +4.9% more instructions with the context (+11.4% / +6.5% / +8.0% before M12.45): what is
left is reading and inflating the context's JSON, which is 8%
of signup-flow's `document.json` (10 KB of 131 KB: 7 scenarios with their steps and expectations,
4 states, assumptions and decisions). Making that cheaper would mean reading JSON faster than
serde_json does; the wall-clock difference above the instruction difference wasn't traced (page
faults per server of 100 saves and 100 reopens: +6.5% with the context).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-20 | minor | After review (M12.47–M12.50, `1bc2ca40`): `FastMap` keyed per process with a full-avalanche finish (a file could make its ids collide); a stored context with a key twice in `shownBy`, a lone surrogate, a number past `f64` or deep nesting reads, or is refused, exactly as through a `Value`. +0.5% / +0.5% / +0.9% instructions for `proto.check`, reopen overhead +8.2% / +4.7% / +5.4%; PF2–PF5 still met (n = 30). |
| 2026-11-20 | major | Rewritten for fork milestone FM1 as delivered (`9f05ba8e`) and after M12.40–M12.45 (`99e3ae65`): the regression came from the example files (more variable sets per action and dearer settles), not the code (0.0–0.7%), and scenarios run once per call; settles copy only what changes, documents share styles and variables, caches hash cheaply, the first font walk zeroes no buffers, a design context is read straight. PF2–PF5 met (PF2 p95 27 / 35 / 44 ms); PF1 p95 of outline, undo and save and the outline's mean not shown within budget with no code cost; PF6 reopen missed for signup-flow and pantry-dashboard (+7.6% / +4.2% instructions, the context's JSON). |
| 2026-11-19 | major | After the performance work (M11.41–M11.45, measured at `21e99ada`): causes found with profiles; PF1 `set_properties` and `undo` met; PF2 met for two examples, missed at p95 for `bike-repair-booking` (first call per process: font indexing and first shaping); PF1 `save_file` / `open_file` p95 and PF6 reopen of `signup-flow` not shown within budget; PF3–PF5 met; `figcraft-cli perf` added; n = 100 runs because n = 30 can't resolve these differences on the shared machine. |
| 2026-11-13 | major | Created: before/after MCP timings for FM1; PF1 missed for `set_properties` and `undo`, PF2 missed for `proto.check`; PF3–PF5 met; PF6 not measured yet. |
