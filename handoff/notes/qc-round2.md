# Visual/interaction QC round 2 (release f47cc033, renamed build, real app under Xvfb)

Reviewer: an independent QC agent that did not write the code or the round-1 fixes.
Build: `CARGO_INCREMENTAL=0 cargo build --release -p figcraft -p figcraft-cli` at `f47cc033`, once
without craft-fonts (system fonts) and once with `CRAFT_FONTS_DIR` pointing at a craft-fonts checkout
(Inter built in; the README screenshots come from this one). The app ran under Xvfb 1440×900 with
software GL (`run_app.sh`), a fresh empty `FIGGUREMYUI_CONFIG_DIR`, on unsaved working copies of
`examples/*.figcraft`. Input was real: `ui.input` moves, presses, releases, keys, typed text and
wheel scrolls at window points (presenter hotspots from `ui.inspect().presenter.hotspots`); no
`agent` on any of them. Exceptions, named where they occur: files were opened with `file.open`
(as the command line does), and the dashboard's custom presenter widths were set with
`proto.settings`. Every screenshot cited was read.

Screens: /tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/qc2/shots/
(contrast measured from the PNG pixels with `qc2/contrast.py`: background = the box's most common
colour, foreground = its pixel of highest contrast).

## Findings

| Id | Sev | One line |
|---|---|---|
| QC2-01 | P3 | A layer's canvas hover outline stays on while the pointer is in the Design context panel or on the left rail |
| QC2-02 | P3 | Bike booking, Choose service: the step indicator's middle segment sits under the phone's camera cut-out |
| QC2-03 | P3 | Pantry dashboard at 1024 px: KPI cards wrap 3 + 1, the fourth stretched across the whole row |
| QC2-04 | P3 | Presenter: a new custom size from `proto.settings` keeps the old scroll offset; the screen is blank |
| QC2-05 | P3 | `proto.present {start}` while the same prototype plays answers ok and doesn't restart |
| QC2-06 | P3 | Tab (and a new form's first field) scrolls the control only to the panel's edge; its focus ring is cut off |
| QC2-07 | P3 | Save as filter "Figgure My UI file (.figcraft)" isn't translated; the next filter is |
| QC2-08 | P3 | A stored check finding says "Raised by: check" |
| QC2-09 | P3 | `docs/migration.md` lists every MCP resource except `figguremyui://commands` |
| QC2-10 | P3 | Vietnamese Preferences still shows three English labels |
| QC2-11 | P3 | `ui.inspect().focus.region` is `designContext` while the keyboard is on the left rail; panel link buttons are all named "Remove this link" |

No P0, P1 or P2 found.

### QC2-01 · P3 · Canvas hover outline left on while the pointer is in the panel or on the rail
- Steps: with the Design context panel open, put the pointer on a layer on the canvas (e.g. the
  Welcome frame's Intro group of `signup-flow`, window point 503,400), then move it straight into the
  panel (372,400 → 300,400) without a move over empty canvas in between. Same when the pointer goes
  from the canvas straight to the left rail and clicks Context.
- Expected: nothing is hovered on the canvas once the pointer has left it.
- Actual: the layer keeps its orange hover outline (and an outlined frame keeps its orange name)
  until the pointer next moves over the canvas. A fast real mouse skips the 40 px between a frame
  and the panel easily. With the Layers panel showing it hardly shows, because a hovered layer row
  sets the hover each frame (`panels/layers.rs:1412`, `ds.hover = h`).
- Evidence: `015c-into-panel-crop.png` (Intro outlined, pointer in the panel); `013-vi-flow.png`,
  `017-vi-scen-run.png` (Verify code outlined while the pointer is in the panel);
  `100-editor-signup-fonts.png` (right after launch: pointer on the rail, Verify code outlined);
  `014c-hover-after-leave-crop.png` (cleared when the path crossed empty canvas).
- Suspected location: the leave branch in `crates/ui-egui/src/canvas/input.rs` (≈ lines 221–233
  and 353–360) isn't reached, or doesn't clear the session's hover, when the next frame's pointer is
  over the left sidebar; not confirmed in the code.

### QC2-02 · P3 · (example) bike booking: Choose service's step indicator under the camera cut-out
- Steps: play `bike-repair-booking` (iPhone 16 frame, the file's device), any scenario.
- Expected: nothing of the screen's content under the cut-out, as on Details and Confirm (their
  Back row sits left of it).
- Actual: on Choose service the "2. Thời gian" progress segment is hidden behind the cut-out.
- Evidence: `p-bike-S2-07.png`, zoom `p-bike-island-zoom.png`.
- Location: the example's generator, `examples/build/build_examples.py` (Choose service frame,
  node 80: auto layout padding top 24, while Details/Confirm use 12 plus a Back row).

### QC2-03 · P3 · (example) pantry dashboard at 1024 px: KPI cards 3 + 1
- Steps: present `pantry-dashboard` at a custom size 1024×800 (`proto.settings`, then Restart).
- Expected: a balanced row (four, or two and two); `docs/examples.md` gives 4 at 1100/1440, 2 at
  768, 1 at 360 and says nothing of 1024.
- Actual: three cards in the first row and "Volunteers on shift today" alone across the full width
  of the second. Nothing is clipped. 360, 768 and 1440 look as documented.
- Evidence: `w-1024-top.png`; `w-360-top.png`, `w-360-bottom.png`, `w-768-bottom.png`,
  `w-1440-top.png`.
- Location: the KPI row's wrap and the cards' minimum width in `examples/build/build_examples.py`.

### QC2-04 · P3 · Presenter: scroll not clamped after `proto.settings` changes the screen size
- Steps: present the dashboard at Custom size 360×800, scroll to the bottom (scroll 850 of 850),
  then `proto.settings {device: "Custom size", width: 768, height: 800}` while it plays.
- Expected: the scroll offset clamped to the new range (0–100).
- Actual: `presenter.scroll` stays `[0, 850]` with `scrollRange` `[0, 100]`; the viewport shows
  only the frame's empty background. Choosing a device in the presenter's own menu resets the
  scroll to 0 (`046c-ipadmini-after.png`), so a person using the player's menu doesn't see it; an
  agent, a script, or (not tried) the Prototype tab's size fields do.
- Evidence: `047-settings-scroll-unclamped.png` (blank 768 screen), also `w-768-top.png`.
- Suspected location: `crates/ui-egui/src/present/navigate.rs` ≈ line 49 clamps each stored
  scroll against `self.doc` before the relayout for the new screen; not confirmed.

### QC2-05 · P3 · `proto.present {start}` doesn't restart a prototype that already plays from it
- Steps: play `signup-flow` from Sign-up form (54) and go on to Verify code by clicks; then
  `engine.execute proto.present {start: 54}`.
- Expected: the player restarts at 54, or an answer that says it already plays.
- Actual: `{ok: true, result: {start: 54}}`; `presenter.current` stays 92 (Verify code), history
  `[54]`. With a different start it does start there. (A person uses Restart, which works.)
- Evidence: command output in this note's session (`play.py` runs S2/S6 before the fix to the
  script); inspect after the call: `[92, "Verify code", [54], 54]`.

### QC2-06 · P3 · Focus scrolled into view only to the panel's edge
- Steps: Design context ▸ Brief, press Tab until "What can't change?" / "How would we know it
  works?" fields; or Findings ▸ Add a judgement or feedback.
- Expected: the focused control and its focus ring entirely in view (a small margin).
- Actual: the control is scrolled just to the bottom edge: the ring's lower edge (and part of a
  two-line field) is cut off; the new form's first field shows at the very bottom.
- Evidence: `tab-brief-24.png`, `tab-brief-30.png`, zoom `tab-brief-30-zoom.png`; `135-add-form.png`.
- Location: `crate::a11y::scroll_focus` as used by the panel (`panels/design_context.rs:762`),
  the QC-04 fix.

### QC2-07 · P3 · Save as filter label not translated
- Steps: Vietnamese UI, File ▸ Save as.
- Expected: the file-type label in the UI language (the brand name itself stays).
- Actual (from the code; the native dialog couldn't be captured under Xvfb):
  `apps/figcraft/src/services.rs:491` and `:698` pass the literal `"Figgure My UI file (.figcraft)"`
  while the next filter goes through `tr(".fig (experimental)")` (`".fig (thử nghiệm)"` in vi.toml).
  The English label matches the spec.

### QC2-08 · P3 · "Raised by: check"
- Steps: Findings ▸ Ready ▸ Run the checks ▸ Store what they find; look at the stored F1.
- Actual: "Raised by / check" — the source id printed as if it were who raised it.
- Evidence: `134-stored-bottom.png`.
- Location: `crates/ui-egui/src/panels/design_context/findings.rs:251`
  (`part(ui, tr("Raised by"), &f.source)`).

### QC2-09 · P3 · migration note misses one resource
- `resources/list` returns `figguremyui://document`, `…/commands`, `…/comments`, `…/design`,
  `…/design/findings`, `…/prototype/flow`, `…/prototype/scenarios` (+ templates `…/node/{id}`,
  `…/command/{id}`). `docs/migration.md`'s Old → new table lists all but `commands`.
  `figcraft://commands` still reads (checked). Evidence: `qc2/mcp_smoke.log`.

### QC2-10 · P3 · Vietnamese Preferences: three labels in English
- Preferences in Vietnamese: "Invert zoom direction", "Board grid", "Graph paper" stay English.
  Upstream catalog gaps, not from the rename. Evidence: `008-vi-prefs.png`.

### QC2-11 · P3 · Panel focus names
- Tab from the panel's chips wraps to the rail: `ui.inspect().focus` says `region: designContext`
  for "Main menu", "File", "Assets"… (the rail, not the panel). And every layer link in an entry is
  followed by a button named only "Remove this link", so a screen reader user hears the same name
  several times in a row (D2 has three). Evidence: Tab walk output (Brief and Decisions sections,
  this session). Location: `crates/ui-egui/src/app/control.rs:48`; the link list in
  `panels/design_context.rs` (`layer_links`).

## Round 1 re-check (fixed items, original steps repeated on f47cc033)

| Id | Result | Evidence |
|---|---|---|
| QC-01 | **fixed** — ran scenarios (6/6), flow check (5 of 5) and checks (13 run, 0 reported) in sign-up; opened bike: no results shown (all `null`); tab click back to sign-up: its own results, not stale | `030-qc01-bike-findings.png`, `031-qc01-back-signup.png` |
| QC-02 | **fixed** — S6 by clicks: summary "Còn thiếu số điện thoại hoặc email." after the name, gone after the contact | `p-signup-S6-03.png`, `p-signup-S6-04.png` |
| QC-03 | **fixed** — S6 by clicks: Confirm ▸ Đổi (time) ▸ Details ▸ ‹ Quay lại → Choose service | `play.py` log (S6 step 7 → 80) |
| QC-04 | **fixed**, with QC2-06 — the focused control is scrolled into view | `tab-brief-24.png`, `tab-brief-30.png` |
| QC-05 | **fixed** — dark theme, frame names on the #F5F5F5 page #6E6E6E, 4.68:1 | `150-dark-decisions.png` |
| QC-06 | **fixed** — simulator caveat items in Vietnamese; "Tài nguyên" wraps on two lines; the English-messages note shows | `017-vi-scen-run.png`, `008-vi-prefs.png` |
| QC-08 | **fixed** — Store what they find: "Stored 1 new", checks not stale, no "changed since" line | `133-stored.png` |
| QC-09 | **fixed** — resend toast under its link; sync toast under Refresh, clear of the pager | `p-signup-S4-01.png`, `p-pantry-S3-03.png` |
| QC-10 | **fixed** — "5 of 5 screens" (headless: 5 non-component screens, all reachable) | `019-vi-flowcheck.png`, `headless.log` |
| QC-14 subset | **fixed** where seen — a link and its × together; a two-line field's ring goes round it; new forms focus their first field (feedback, assumption); "Tiếp tục" doesn't move (y 774 disabled and enabled); KPI placeholders while loading. Not seen: the compare table (no alternative made) | `150-dark-decisions.png`, `tab-brief-30-zoom.png`, `152-dark-add-assumption.png`, `p-signup-S5-03.png`, `w-360-top.png` |
| QC-14 rest | **fixed** — the dashboard at 360 no longer covers the component sheet | `124-pantry-360.png` |
| QC-15 | **fixed** — "Accepted · proposed by Example builder (agent) · accepted by you"; an agent's D4 "Proposed · proposed by Claude (agent) · Waiting for you to accept it" | `150-dark-decisions.png`, `018-vi-flow-bottom.png` |
| QC-16 | **fixed** — `focus.region` `designContext`, `entry` D1…, control names (with QC2-11) | Tab walk output |

Still as documented (not in round 1's fix list): QC-07 (suggestions show command ids, e.g.
"(design.assumption.set)", `133-stored.png`), QC-11 (pager "2 / 10" counts components), QC-12, QC-13.

## Rebrand

- **Window**: title "signup-flow – Figgure My UI" (`window.list`, X `_NET_WM_NAME`); WM class
  `io.github.vansyson1308.figguremyui` (matches the desktop entry's `StartupWMClass`); window icon
  256 px, the new icon (`004-wmicon-256.png`).
- **Home**: icon and "Figgure My UI" wordmark; "UI" #B64315 on white 5.52:1, #FF9A75 on #2C2C2C
  6.74:1; tile icons navy on orange 5.91:1 (`001-home.png`, `002-home-dark.png`).
- **Editor**: rail logo is the new mark; Share label navy on #FF7A4D 5.91:1; Move tool 5.97:1;
  selected rail item #B23A0A on #FFF0E9 5.39:1; frame-size label white on #C2410C 5.18:1;
  selected frame name #C2410D on #F5F5F5 4.75:1; menu highlight white on #C2410C 5.18:1; Run all
  (hover) navy on #F06D3C 5.05:1; Accept navy on #FF7A4D 5.91:1; dark links #FF9A75 6.74:1.
  No text measured below 4.5:1.
- **About** (EN light, EN dark, VI): exactly "Figgure My UI — design and verify interactive
  prototypes with people and AI agents. Based on FigCraft by the ArtCraft team and contributors.",
  the not-endorsed line, link #B33C0D 5.88:1, Contributors tab (upstream's list, unchanged), Models
  tab (`025-en-about.png`, `153-dark-about.png`, `011-vi-about.png`, `012-vi-about-contrib.png`,
  `026-en-about-models.png`).
- **Menus/Preferences**: Help ▸ "About Figgure My UI" / "Giới thiệu về Figgure My UI"; no FigCraft
  or ArtCraft mark anywhere else seen (Home, editor, panel, presenter, Preferences, menus, CLI help,
  MCP instructions).
- **CLI/MCP**: `figguremyui-cli --help` "Figgure My UI without a window"; `serverInfo`
  `{name: "figguremyui", title: "Figgure My UI", version: "0.18.1"}`; instructions contain no
  "FigCraft" or `figcraft://`.

## The three examples

All 17 stored scenarios were played in the presenter by clicks (a field tap for each typing step),
and every screen and overlay expectation held: sign-up S1–S6, bike S1–S6, pantry S1–S5
(`p-signup-*`, `p-bike-*`, `p-pantry-*`). The dashboard was presented at 360, 768, 1024 and 1440
(QC2-03 at 1024; QC2-04 found on the way) and on an iPhone 16 frame (`042-pantry-iphone16.png`).
Vietnamese and long texts: nothing clipped or overlapping in the shots read, apart from QC2-02.
The panel's results match headless for each file: scenarios 6/6, 5/5, 6/6; flow 5 of 5 (sign-up);
checks at Converging 13 rules, 0 reported, 5 not run; sign-up at Ready 18 rules, 1 reported
(`context.untestedAssumption`, "3 of 3 untested") — same as `figguremyui-cli run`
(`run-ready.out`) and `figguremyui-cli mcp` (`headless.log`).

## MCP headless smoke

`figguremyui-cli mcp` (`mcp_smoke.log`): initialize → serverInfo as above; `resources/list` only
`figguremyui://` (7 + 2 templates); `resources/read figcraft://document`, `figcraft://design`,
`figcraft://node/44`, `figcraft://commands` all read; an unknown URI is a -32002 error. Loop on a
copy of `signup-flow`: run_scenarios 6/6, check_prototype no problems, check_design 0; a text set
to #DDDDDD → `contrast.text` "1.35:1, needs 4.5:1"; recorded as F1; fixed; `design.finding.recheck`
→ F1 `verified` ("`contrast.text` no longer reports it"); save, reopen, scenarios 6/6;
`simulate_prototype` with `image: true` returned a 390×844 PNG.

## Not checked

- The native Save as dialog (rfd) wasn't captured under Xvfb: QC2-07 is from the code.
- The compare table of alternatives (no alternative made); a screen-reader pass; Windows/macOS.
- The CJK names in the language menu draw as boxes in the build without craft-fonts on this
  machine (no CJK system fonts); not checked with them.

## Fix status (QC fixer, on 9fde6a42)

Re-checked in the real app (debug build with craft-fonts, Xvfb 1440×900, fresh config dir, working
copies of the examples, real `ui.input` moves, clicks, keys and wheel). Screens:
/tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/qc2-fix/shots/.
Each test below was also run against the code before its fix and failed there.

| Id | Status | Cause, fix | Test | Real app |
|---|---|---|---|---|
| QC2-01 | **fixed** | Leaving the canvas ended the tools' hover (guides, measurements, widget hover) but never cleared the document's hovered layer, so its outline and frame name stayed until a hover over empty canvas. Leaving now clears the layer the canvas's own hover found; one a panel hovered since (a Layers row, drawn before the canvas) stays. | `canvas_hover_tests::leaving_the_canvas_from_a_layer_ends_its_hover` | `01a-hover-on-layer`, `01b-pointer-in-panel` (503,400 → 372 → 300: no outline); rail: `01c/01d-…-crop` |
| QC2-04 | **fixed** (reset, as the device menu) | `Presenter::sync` clamped the stored scroll before the layout for the new screen size (`respond_to` lays out again later and clamps nothing). Any change of the device's screen size (a device, rotation or custom size from the player's menu, the Prototype tab, an agent, an undo) now starts at the top, as the menu did: the old screen's offsets, pan and drag mean nothing on the new one, and the menu and `proto.settings` now agree. Other settings (background, colour) keep the place. | `proto_responsive_tests::a_new_screen_size_while_playing_starts_at_the_top_as_the_device_menu_does` | pantry at 360×800 scrolled to 1148 of 1148 (`04a-360-bottom`), then `proto.settings` 768×800: scroll `[0,0]`, range `[0,100]`, screen shows the top (`04b-768-after-settings`) |
| QC2-05 | **fixed** (restart while on screen) | `tabs::open_present` brings an open presentation tab of the same page back as it was unless `start` differs ("play again shows the tab as it was", tested). While that presentation is already on screen there is nothing to show again, so a call now starts it again from `start`, as Restart does; waiting behind the editor it still shows as it was. Written in `proto.present`'s params doc. | `present_tab_tests::present_again_while_it_plays_on_screen_starts_again` | 54 → ArrowRight → 92 (`05a-moved-on`), `proto.present {start: 54}` → current 54, history `[]` (`05b-present-again`); from the editor it shows 92 as it was |
| QC2-06 | **fixed** for Tab; new forms need a one-line follow-up | `a11y::scroll_focus` scrolled the control's own rect (for a multi-line field, its inset text area) to the panel's edge; the ring goes round the field's box and 3 pt past it. It now scrolls the ring's rect (the `ring_around` box when there is one, expanded by `RING_MARGIN`) into view. A new form's first field is scrolled by `panels/design_context.rs:503` (`r.scroll_to_me(None)`), which this pass could not touch: replace it with `crate::a11y::scroll_ring_into_view(ui, r.rect.expand(6.0))`. | `a11y::scroll_focus_tests::the_field_comes_into_view_with_its_whole_ring` | Brief, Tab to "What can't change?" and "How would we know it works?": whole ring with a gap below (`06-tab-06`, `06-tab-13-zoom`, `06-tab-14-zoom`) |
| QC2-07 | **fixed** | The native format's filter label was a literal. It is now `figcraft_ui_egui::native_file_type()` (`tr("Figgure My UI file (.figcraft)")`); en and vi entries ("Tệp Figgure My UI (.figcraft)"); other languages read English until translated, as for other new strings. | `services::save_filter_tests::save_filter_labels_come_from_the_catalog` (app), `i18n::tests::vietnamese_has_the_save_as_file_type_and_the_preferences_labels` | not seen: the native dialog can't be captured under Xvfb |
| QC2-09 | **fixed** | `docs/migration.md`'s MCP resources row now has `figcraft://commands` → `figguremyui://commands`. | `cli::the_migration_note_lists_every_mcp_resource` (runs `figguremyui-cli mcp`, checks every listed resource and template, new and old form) | — |
| QC2-10 | **fixed** | vi.toml: "Đảo ngược chiều thu phóng" (as "Thu phóng" elsewhere), "Lưới bảng trắng", "Giấy kẻ ô". | `i18n::tests::vietnamese_has_the_save_as_file_type_and_the_preferences_labels` | `10-vi-prefs` |

QC2-02, 03, 08 and 11 wait for the work on the design context and the examples.

## Fix status, second pass (design context and examples, on the M12 branch)

Screens: /tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/states-fix-shots/.

| Id | Status | Cause, fix | Test | Real app |
|---|---|---|---|---|
| QC2-02 | **fixed** | Choose service's auto layout started 24 px from the top, under the cut-out; the other steps start with a Back row. It now has the 56 px top inset the sign-up's Welcome has (`examples/build/build_examples.py`), and the example was regenerated. | `examples::each_example_passes_its_flow_check_scenarios_and_design_check` (unchanged: scenarios and checks still pass) | `12-bike-choose-service-presented` (iPhone 16 frame: the step bar is below the cut-out) |
| QC2-03 | **fixed** (4 across) | The KPI cards' minimum width (240) let three fit at 1024 but not four. 226 gives four across from 1024 (4 × 226 + 3 × 16 = 952 of the 960 inside the padding), two at 768 (three need 710 of 704), one at 360. | `examples::the_dashboard_holds_its_layout_at_five_widths_in_every_state` (1024 added: 4 per row) | presented at Custom size, responsive, Restart: `13-pantry-1024` (four across), `14-pantry-768` (two and two) |
| QC2-06 | **fixed** (the follow-up) | A new form's first field now calls `crate::a11y::scroll_ring_into_view(ui, r.rect.expand(6.0))` instead of `scroll_to_me`. | `design_context_tests::a_new_forms_first_field_shows_with_its_whole_focus_ring` | the declare form opens with its first field and ring in view (`03-declare-form-open`) |
| QC2-08 | **fixed** | A check's `source` is the word `check`; the panel showed it as who raised it. A check now says "Measured by the rule" and the rule's id; a judgement or feedback still says who raised it. | `design_context_tests::a_stored_check_says_which_rule_measured_it` | Findings ▸ Ready ▸ Run the checks ▸ Store what they find: F1 says "Measured by the rule / context.untestedAssumption", "Stored by you" (`10-stored-check-measured-by-rule`) |
| QC2-11 | **fixed** | The rail is drawn in the layers' region (F6 counts it with them), so `ui.inspect` named it as the Design context panel. A control on the rail is now reported as region `rail`; each link's remove button is named "Remove the link to {name}". | `design_context_tests::the_rail_is_its_own_focus_region`, `design_context_tests::links_go_both_ways_and_a_missing_layer_stays_listed` | Tab from the Brief chip lands on "Main menu" with `focus.region` `rail` (`11-rail-focus`); Shift+Tab through R1 names "Remove the link to Consent checkbox" and "… to Consent text" |
