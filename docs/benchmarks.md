# Benchmarks: milestone M11 before and after

> **Measured:** 2026-11-13 · **Baseline:** `011f08e5` · **New:** `4501b37b` · Both release builds of
> `figcraft-cli`. **Status: two budgets missed; the performance work to fix them is in progress**
> (`handoff/CHECKLIST.md`).

## How it was measured

- Machine: Intel Xeon @ 2.10 GHz, 4 vCPU, 15 GB RAM, Ubuntu 24.04, shared with other work (load
  average 3.2–3.9 at the start, 2.3 at the end).
- Each operation is a `tools/call` round trip over MCP stdio, headless, timed in the client (JSON
  encode, pipe, server, JSON decode). 3 rounds × 10 calls, the two binaries alternating round by
  round (baseline, new, baseline, new…) so both see the same load; samples pooled (n = 30).
- Mean and 95th percentile (nearest rank) are reported separately; a mean is never used to claim a
  percentile.
- Pre-existing operations ran on `examples/ridgeline-studio-dashboard.figcraft` (the largest example
  from before the milestone, 268 layers). The new operations ran on the three new examples.
- Script: `handoff/tools/bench.py BASELINE_CLI NEW_CLI EXAMPLES_DIR OUT_JSON [ROUNDS] [N]`.

## PF1: operations that existed at the baseline (budget: at most 10% slower, mean and p95)

| Operation | Baseline mean | New mean | Δ mean | Baseline p95 | New p95 | Δ p95 | Budget |
|---|---:|---:|---:|---:|---:|---:|---|
| `inspect_document {outline:true}` | 0.46 | 0.38 | −17% | 0.64 | 0.59 | −8% | met |
| `inspect_document {depth:64}` | 10.66 | 9.90 | −7% | 12.64 | 11.27 | −11% | met |
| `create_node` rectangle | 0.39 | 0.16 | −59% | 0.42 | 0.34 | −20% | met |
| `set_properties` | 0.10 | 0.12 | +22% | 0.13 | 0.16 | +27% | **missed** |
| `batch` of 50 steps | 3.25 | 3.26 | +0% | 3.73 | 3.77 | +1% | met |
| `screenshot` of one frame | 173.07 | 172.94 | −0% | 246.66 | 243.70 | −1% | met |
| `undo` | 0.11 | 0.15 | +34% | 0.28 | 0.39 | +39% | **missed** |
| `save_file` | 8.54 | 8.80 | +3% | 9.53 | 9.68 | +1% | met |
| `open_file` | 2.36 | 2.37 | +0% | 2.87 | 2.91 | +1% | met |

Milliseconds. The two misses are 0.02–0.11 ms in absolute terms. The suspected cause (not yet
confirmed by a profile): each tool call now reads the document's identity and revision in extra
backend requests (the safety features of this milestone: the working document, revisions and
conflict detection) and history steps record their authors.

## PF2–PF5: new operations (new build only)

| Operation | File | Mean | p95 | Budget (p95) |
|---|---|---:|---:|---|
| `proto.check` | signup-flow | 33.44 | 52.85 | ≤ 50: **missed** |
| `proto.check` | pantry-dashboard | 83.05 | 104.42 | ≤ 50: **missed** |
| `proto.check` | bike-repair-booking | 109.74 | 128.74 | ≤ 50: **missed** |
| `proto.scenario.run` (all) | signup-flow | 30.98 | 34.66 | ≤ 150: met |
| `proto.scenario.run` (all) | pantry-dashboard | 77.54 | 82.99 | ≤ 150: met |
| `proto.scenario.run` (all) | bike-repair-booking | 108.46 | 120.76 | ≤ 150: met |
| `design.check` (all 18 rules) | signup-flow | 30.54 | 32.81 | ≤ 500: met |
| `design.check` (all 18 rules) | pantry-dashboard | 78.12 | 85.33 | ≤ 500: met |
| `design.check` (all 18 rules) | bike-repair-booking | 107.10 | 116.59 | ≤ 500: met |
| `design.get` | signup-flow | 0.29 | 0.46 | ≤ 20: met |
| `design.get` | pantry-dashboard | 0.27 | 0.41 | ≤ 20: met |
| `design.get` | bike-repair-booking | 0.40 | 0.61 | ≤ 20: met |
| `save_file` | signup-flow / pantry-dashboard / bike-repair-booking | 8.98 / 7.65 / 10.93 | 15.41 / 11.62 / 19.40 | PF6: not yet compared against the same files without a design context |
| `open_file` | signup-flow / pantry-dashboard / bike-repair-booking | 1.85 / 2.08 / 2.66 | 2.78 / 2.49 / 3.24 | PF6: as above |

`proto.check` costs about as much as running every stored scenario, which suggests it is dominated by
the scenario rules (`scenario.fails`, `scenario.unvisited`) re-running every scenario, and each
simulated step re-applying bindings over the whole document. Not yet confirmed by a profile.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | major | Created: before/after MCP timings for M11; PF1 missed for `set_properties` and `undo`, PF2 missed for `proto.check`; PF3–PF5 met; PF6 not measured yet. |
