# Benchmarks: milestone M11 before and after

> **Measured:** 2026-11-19 · **Baseline:** `011f08e5` · **New:** `21e99ada` (after the performance
> work, M11.41–M11.45) · Both release builds of `figcraft-cli`. **Status:** PF1's two misses
> (`set_properties`, `undo`) are fixed; PF2 is met for two of the three examples and **missed for
> `bike-repair-booking` at p95** (the first call in each server process); PF1 `save_file` and
> `open_file` p95 and PF6 `open_file` on `signup-flow` are **not shown within budget**; PF3–PF5
> met. Details under each table.

## How it was measured

- Machine: Intel Xeon @ 2.30 GHz, 4 vCPU, 15 GB RAM, Ubuntu 24.04, shared with another engineer's
  builds. The deciding run started at load average 0.73 and ended at 1.42 (the lowest seen that
  afternoon); other runs and their loads are listed where they disagree.
- Each operation is a `tools/call` round trip over MCP stdio, headless, timed in the client (JSON
  encode, pipe, server, JSON decode), with `handoff/tools/bench.py`: each round runs every
  operation 10 times in a fresh server process, the two binaries alternating round by round
  (baseline, new, baseline, new…) so both see the same load; samples pooled.
- **Sample size.** The protocol's 3 rounds (n = 30) can't resolve these differences on this
  machine: whole rounds shift by up to ±50% (one round of `save_file` at 10–11 ms where the other
  rounds of the same binary took 7 ms), so two n = 30 runs ten minutes apart gave `set_properties`
  +49% and −28%. The deciding run therefore has **10 rounds (n = 100)**; the workload, the
  operations and the budgets are unchanged. The n = 30 runs are shown beside it.
- Mean and 95th percentile (nearest rank) are reported separately; a mean is never used to claim a
  percentile.
- Pre-existing operations ran on `examples/ridgeline-studio-dashboard.figcraft` (268 layers), the
  new operations on the three new examples.
- The baseline binary is 55 MB, the new ones 105 MB: they were built with different
  `CRAFT_FONTS_DIR` inputs, so the embedded fonts differ. Where that matters (the first text
  layout in a process, renders of text), the new build is also compared with the build before
  this work (`4501b37b`, same fonts).

## What was slow, and why (profiles)

Profiled with `perf` (software clock, frame-pointer and DWARF call graphs) on release builds, and
call counts with `perf probe` uprobes.

- **PF2 `proto.check`.** One call on `bike-repair-booking` runs its 5 stored scenarios once (the
  scenario rules share one run per call through `PageFacts`; nothing ran twice): 30 steps, which
  settle the simulator's copy 100 times and pass over every binding 200 times (156 bound layers).
  97% of the call was in the simulator's steps: 70% normalising the copy after each action, 44%
  of it shaping text again for auto layout (`TextMeasure::text_height`); 32% applying bindings,
  half of the passes being `proto_run::set_variable`'s own just before the settle that applies
  them all again, each layer working out its modes up its parent chain (12%) and copying its
  paints to compare them. `proto.scenario.run` and `design.check` cost the same because they run
  the same scenarios.
- **PF1 `set_properties` and `undo`.** Each tool call that reads or changes a document lists the
  open documents before its first request and after its last (R2-01's revision, the identity in
  every result), each as a full `doc.list` engine command: 13% of a `set_properties` call, 19% of
  an `undo`. An agent's undo also copied the change log to look for others' changes (13%), and
  every agent command looked each of the document's layers up in the version before to name what
  it changed (10–18%, older than this milestone). Author bookkeeping in history did not stand
  out in the profiles.

The fixes (commits M11.41–M11.45) cache text heights per text and width, work out modes once per
pass and leave colours in place uncopied, have `set_variable` re-apply only the bindings that read
the variable (the settle after it still applies them all), read the documents in the headless
backend without a command (an attached app is still asked `doc.list`; revisions, identity,
`ifRevision` and authors unchanged), walk both versions once to find changed layers, and read the
change log in place.

## PF1: operations that existed at the baseline (budget: at most 10% slower, mean and p95)

Deciding run, n = 100 (10 rounds × 10), load 0.73 → 1.42:

| Operation | Baseline mean | New mean | Δ mean | Baseline p95 | New p95 | Δ p95 | Budget |
|---|---:|---:|---:|---:|---:|---:|---|
| `inspect_document {outline:true}` | 0.31 | 0.29 | −7% | 0.45 | 0.42 | −6% | met |
| `inspect_document {depth:64}` | 8.80 | 8.23 | −6% | 11.61 | 10.28 | −11% | met |
| `create_node` rectangle | 0.15 | 0.14 | −8% | 0.33 | 0.35 | +5% | met |
| `set_properties` | 0.09 | 0.08 | −7% | 0.12 | 0.13 | +6% | met (was +22% / +27%) |
| `batch` of 50 steps | 2.62 | 2.65 | +1% | 2.89 | 2.90 | +0% | met |
| `screenshot` of one frame | 150.39 | 147.74 | −2% | 191.65 | 169.23 | −12% | met |
| `undo` | 0.10 | 0.10 | +3% | 0.24 | 0.25 | +4% | met (was +34% / +39%) |
| `save_file` | 7.10 | 7.45 | +5% | 7.80 | 9.15 | +17% | mean met; **p95 missed in this run** |
| `open_file` | 1.83 | 1.95 | +7% | 2.19 | 2.48 | +13% | mean met; **p95 missed in this run** |

Milliseconds. The other runs (Δ mean / Δ p95, new against baseline):

| Operation | n = 30, load 1.1 | n = 30, load 0.9 | n = 100, load 1.4 → 3.0 |
|---|---|---|---|
| `set_properties` | −28% / −25% | +49% / +17% | −14% / −0% |
| `undo` | −13% / −16% | −9% / −0% | −2% / −2% |
| `batch` of 50 steps | +13% / +26% | −3% / −20% | −4% / −16% |
| `screenshot` of one frame | +6% / +20% | +5% / +11% | +8% / +3% |
| `save_file` | +0% / −3% | +18% / +47% | +19% / +59% |
| `open_file` | +1% / +15% | +16% / +48% | +2% / +4% |

`set_properties` and `undo` are within budget in every n = 100 run and, call by call in the
benchmark's own sequence, equal to or faster than the baseline (e.g. `set_properties` 0.044–0.052
ms against 0.044–0.058 ms per call after the first). `save_file` and `open_file` p95 swing from
−3% to +59% between runs; this work doesn't touch saving or loading (the before/after document
reads of those tools got cheaper). A focused interleaved comparison (20 rounds × 10, n = 200 per
binary, load 1.3) gave `save_file` median 16.54 → 16.71 ms and p95 37.29 → 42.44 ms (+14%) for
the new build and the same (+12%) for `4501b37b`, so if it is real it predates this work;
`open_file` p95 there was 2.48 → 2.30 ms. Not resolved: they stay open in
`handoff/CHECKLIST.md`.

### `figcraft-cli perf --layers 1000,10000 --instances 0` (engine and render, in-process)

Mean of two runs each, interleaved (baseline, `4501b37b`, new, baseline, `4501b37b`, new), load
2.6. Milliseconds unless noted.

| Operation | 1,000: base | prev | new | Δ base | 10,000: base | prev | new | Δ base |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| build + normalize (like open) | 11.11 | 14.09 | 10.57 | −5% | 56.94 | 60.01 | 58.97 | +4% |
| save .figcraft | 2.92 | 3.39 | 2.67 | −8% | 22.79 | 22.66 | 22.65 | −1% |
| load .figcraft | 4.04 | 5.04 | 4.00 | −1% | 42.19 | 44.37 | 44.19 | +5% |
| render 1440×900 @100% | 7.29 | 7.70 | 7.56 | +4% | 6.77 | 6.71 | 6.70 | −1% |
| render zoomed to fit | 10.30 | 11.21 | 9.45 | −8% | 13.41 | 12.98 | 13.78 | +3% |
| render zoomed to fit, exact | 9.70 | 10.21 | 10.20 | +5% | 38.46 | 42.98 | 42.00 | +9% |
| render zoomed out (2× fit) | 7.22 | 7.97 | 7.48 | +4% | 22.02 | 24.44 | 24.27 | +10% |
| pan 40 px @100% | 0.85 | 0.94 | 0.93 | +9% | 1.05 | 1.17 | 1.08 | +3% |
| pan 40 px zoomed out | 0.85 | 0.92 | 0.90 | +5% | 2.24 | 2.75 | 2.37 | +6% |
| move one layer, re-render @100% | 0.05 | 0.06 | 0.06 | +10% | 0.75 | 0.82 | 0.82 | +9% |
| move one layer, re-render zoomed out | 0.06 | 0.06 | 0.07 | +8% | 0.75 | 0.82 | 0.80 | +7% |
| hit test (µs each) | 16.92 | 16.06 | 16.20 | −4% | 33.84 | 34.64 | 33.27 | −2% |
| boolean of 24, top operand | 0.56 | 0.55 | 0.54 | −3% | 0.29 | 0.30 | 0.29 | −3% |
| boolean of 24, bottom operand | 5.47 | 5.08 | 5.12 | −6% | 4.89 | 4.89 | 4.72 | −3% |
| hover pointer move (µs each) | 16.79 | 16.76 | 16.59 | −1% | 34.36 | 34.85 | 33.57 | −2% |
| node.set fill on one layer | 0.06 | 0.08 | 0.06 | +9% | 0.01 | 0.02 | 0.01 | — |
| edit main component | 0.17 | 0.21 | 0.18 | +9% | 3.15 | 3.21 | 3.10 | −1% |
| document.inspect (depth 8) | 7.19 | 7.80 | 7.86 | +9% | 97.45 | 96.73 | 95.50 | −2% |
| select all + move every screen | 1.91 | 2.42 | 2.13 | +12% | 27.64 | 27.98 | 24.68 | −11% |

Rows under 0.05 ms (transform, text edit, undo, redo, drag preview, normalize after one edit) are
0.00–0.03 ms in all three. Within 10% except `select all + move every screen` at 1,000 layers
(+12%, 0.2 ms; `4501b37b` was +27%, so this work brought it down) and two rows at +10%. These
are means of two runs; the tool reports no percentiles. An earlier pair of runs with the
component table (load 2.1–2.4) put the first measurement in each process (1,000-layer build) at
+107%: the first text layout in a process indexes the fonts, and the two builds embed different
fonts. The table above is the second pair.

## PF2–PF5: new operations (new build only)

Deciding run, n = 100, load 0.73 → 1.42. Milliseconds.

| Operation | File | Mean | p95 | Budget (p95) | Before (`4501b37b`, n = 30) |
|---|---|---:|---:|---|---|
| `proto.check` | signup-flow | 13.56 | 34.48 | ≤ 50: met | 33.44 / 52.85 |
| `proto.check` | pantry-dashboard | 18.99 | 39.62 | ≤ 50: met | 83.05 / 104.42 |
| `proto.check` | bike-repair-booking | 35.31 | 55.09 | ≤ 50: **missed** | 109.74 / 128.74 |
| `proto.scenario.run` (all) | signup-flow | 10.05 | 11.42 | ≤ 150: met | 30.98 / 34.66 |
| `proto.scenario.run` (all) | pantry-dashboard | 17.05 | 21.06 | ≤ 150: met | 77.54 / 82.99 |
| `proto.scenario.run` (all) | bike-repair-booking | 30.42 | 33.95 | ≤ 150: met | 108.46 / 120.76 |
| `design.check` (all 18 rules) | signup-flow | 11.31 | 15.64 | ≤ 500: met | 30.54 / 32.81 |
| `design.check` (all 18 rules) | pantry-dashboard | 17.91 | 25.84 | ≤ 500: met | 78.12 / 85.33 |
| `design.check` (all 18 rules) | bike-repair-booking | 31.78 | 38.83 | ≤ 500: met | 107.10 / 116.59 |
| `design.get` | signup-flow / pantry-dashboard / bike-repair-booking | 0.25 / 0.30 / 0.28 | 0.39 / 0.38 / 0.41 | ≤ 20: met | |

The n = 30 run at load 1.1 agrees: `proto.check` p95 31.27 / 38.84 / 60.50 ms.

**Why `bike-repair-booking` still misses at p95.** Its p95 is the first `proto.check` in each
server process: in the deciding run the first call took 63.5 ms on average (n = 10) and the 90
later calls 32.2 ms mean, 43.6 ms p95. The first call is the first in the process to lay out
text, so it indexes the installed fonts, loads each font family the file uses and shapes every
text once; a profile of first calls (8 processes, `pantry-dashboard`) puts 22% of the call in
scanning the font folders, 18% in parsing font files and 12% in loading families. That work is
the same for any first call that lays out text (a screenshot, an export); headless runs index
fonts on first use on purpose (`figcraft-cli` main: "a run that lays out no text never pays for
the scan"). Indexing them in the background when the MCP server starts was tried and cut the
benchmark's first call by only 1–8 ms (load 4), because the benchmark calls `proto.check` a few
milliseconds after starting the server; it was not kept. Left as is: the per-step work is what
the budget was about, and its numbers are above; meeting p95 on the first call would need font
loading (or the first shaping) to get cheaper.

## PF6: save and reopen with a design context against the same file without one

`handoff/tools/pf6.py` (`save_file` then `open_file` of what was saved, 10 each per round, a fresh
server per round, the two files alternating), the copies without a context made by deleting the
`design` key from `document.json` in the `.figcraft` zip. Budget: within PF1's limit (10%) of the
file without one. Milliseconds, mean / p95.

| Operation | File | n = 200, load 3.4–3.7: without | with | Δ | n = 30, load 1.1: Δ |
|---|---|---|---|---|---|
| `save_file` | signup-flow | 11.79 / 38.10 | 11.53 / 36.29 | −2% / −5% | −12% / −3% |
| `save_file` | pantry-dashboard | 10.70 / 36.59 | 9.88 / 31.88 | −8% / −13% | +9% / +13% |
| `save_file` | bike-repair-booking | 13.72 / 45.52 | 13.59 / 45.16 | −1% / −1% | −5% / −8% |
| `open_file` | signup-flow | 1.35 / 1.64 | 1.58 / 2.17 | **+18% / +32%** | −3% / −11% |
| `open_file` | pantry-dashboard | 1.62 / 2.01 | 1.70 / 2.05 | +6% / +2% | −0% / −9% |
| `open_file` | bike-repair-booking | 2.22 / 4.61 | 2.12 / 2.48 | −4% / −46% | +10% / +10% |

Saving: met. Reopening: met for `pantry-dashboard` and `bike-repair-booking` in the larger runs,
**missed for `signup-flow`** (+0.23 ms mean, +18% / +32% in the n = 200 run and +15% / +33% in an
n = 100 run at load 2.7; the n = 30 run didn't show it). Its context is 10 KB of a 135 KB
`document.json` (8%, the largest share of the three: 5 scenarios, 3 assumptions, 3 decisions, an
end state); the cause wasn't profiled.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-19 | major | After the performance work (M11.41–M11.45, measured at `21e99ada`): causes found with profiles; PF1 `set_properties` and `undo` met; PF2 met for two examples, missed at p95 for `bike-repair-booking` (first call per process: font indexing and first shaping); PF1 `save_file` / `open_file` p95 and PF6 reopen of `signup-flow` not shown within budget; PF3–PF5 met; `figcraft-cli perf` added; n = 100 runs because n = 30 can't resolve these differences on the shared machine. |
| 2026-11-13 | major | Created: before/after MCP timings for M11; PF1 missed for `set_properties` and `undo`, PF2 missed for `proto.check`; PF3–PF5 met; PF6 not measured yet. |
