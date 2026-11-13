# Visual/interaction QC round 1 (release 4501b37b, real app under Xvfb) — lead's triage

Screens: /tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/qc/shots/

## FIX in this round
- QC-01 · P1 · The Design context panel shows another file's checks, scenario results, flow check and comparison after a tab switch / file.open (results kept per window, not per document). In bike file: "Rules run: 18. Reported: 3.", decision text of the sign-up file; S1/S2 show "Passed · every step ran…" although never run in this file. Shots 190-bike-findings.png, 191-bike-flow-top.png, 200-dash-findings-before.png. Fix: results keyed by document (uid); switching shows that file's own results or none. Test: run in A, switch to B → nothing from A; back to A → A's results (or none, but never another file's).
- QC-02 · P2 · (example signup-flow) error summary stays after the person fixes the field/consent (shots m1.png, m2.png). Fix the generator: fixing a field / ticking consent hides or updates the summary; add scenario expectation for it.
- QC-03 · P2 · (example bike-repair-booking) "‹ Quay lại" on step 2 goes to step 3 after "Đổi" (history grows Confirm→Choose→…). Fix the wiring: Back on Details goes to Choose service (step order) after Đổi too; add a scenario covering it.
- QC-04 · P2 · Keyboard: Tab focus in the panel can move to controls outside the visible area; the panel doesn't scroll (shots 110, 111, 116). Fix: scroll the focused control into view. Test.
- QC-05 · P2 · Dark theme: frame names on the canvas #B3B3B3 on the light page #F5F5F5 = 1.92:1 (shot 92-dark-d4.png). Fix: canvas label colour chosen against the page's background colour (≥ 4.5:1 in both themes); test the contrast.
- QC-06 · P2 · Vietnamese: panel-generated text partly English: the simulator caveat items ("Mô phỏng (không mô phỏng: animation and timing; …)"); left-rail label cut "Tài ngu…". Fix: translate the panel's short forms of the not-modelled items (map the 8 known engine items to tr keys; unknown items fall back to English); rail labels fit (shorter Vietnamese label or wrap). Rule messages from the engine stay English: say so once in the panel ("Rule messages are in English") when the UI language isn't English.
- QC-08 · P3 · Storing findings immediately marks the panel's own results "changed since the checks ran". Fix: the panel's own store doesn't make its check results stale.
- QC-10 · P3 · "4 of 10 screens can be reached": component sets counted as screens; an overlay opened by an action reported reachable:false. Fix: panel counts only screens (not components/component sets); engine `proto.check`: an overlay opened by an action from a reachable screen is reachable (check `proto_verify/flow.rs`), with a test.
- QC-14 (subset) · P3 · compare table header cut without ellipsis; layer-link lists wrap with an orphaned "×"; focused brief field loses left padding; "Add an assumption" form doesn't focus its first field. Examples: sign-up "Tiếp tục" jumps 22 px when the hint hides (keep the space or move the hint), hint says "ở cuối trang" though mid-screen (fix wording); dashboard KPI cards show final numbers while loading (show placeholders while loading).
- QC-09 · P3 · (examples) the sign-up "Gửi lại mã" toast covers "Xác nhận"; the dashboard sync toast hides behind the presenter's pager bar without device frame. Fix: toasts positioned where they cover no primary action (top).
- QC-15 · P3 · "Accepted · by Example builder (agent)" reads as if the agent accepted. Fix: engine records `acceptedBy` on decisions and end states (set by `accept`, kept in file, skipped when empty; old files load); panel shows "Proposed by X · accepted by you/Y".
- QC-16 · P3 · `ui.inspect().focus` doesn't report the Design context panel (region "layers", control ""). Fix: report region `designContext` and the focused control's id/label.

## NOT in this round (documented as limitations / inherited behaviour)
- QC-07 · P3 · developer jargon in some engine refusals and rule suggestions shown raw (command ids). Partial: the panel keeps engine texts (shared with agents); note in docs.
- QC-11 · P3 · presenter's next/previous arrows include components and overlays (inherited player behaviour; not verified against the incumbent).
- QC-12 · P3 · hotspot hint tints the whole dashboard frame (its after-delay interaction counts as a hotspot; inherited).
- QC-13 · P3 · reduce motion doesn't affect prototype transitions (documented behaviour).
- QC-14 rest: dashboard at 360 grows tall and covers the component sheet on the canvas (layout of the example sheet; move the components further down in the generator if trivial).
