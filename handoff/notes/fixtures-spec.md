# Example prototypes (fixtures) spec — lead's decisions

Three original example files, native format, in `examples/`, each built by a reproducible generator script
that drives the headless MCP server (`figguremyui-cli mcp` after the rebrand; `figcraft-cli mcp` before it —
take the binary path as an argument) through `run_command`/`batch`/`create_node`/`set_properties`, so the
files can be rebuilt and reviewed as code. Content is synthetic (no real people, places or data); mention in
each brief that data is simulated and that there is no backend.

Scripts: `examples/build/build_examples.py` (stdlib-only Python 3, no dependencies; one function per file;
`--cli PATH --out DIR`). Commit the generated `.figcraft` files too. Record both in `ATTRIBUTION.md` if the
assets check asks (examples are not under `assets/`, so probably not).

## Shared design system inside each file (do not copy another product's kit)
- A variable collection `Theme` with modes `Light` (and optionally `Dark`) holding colour variables used by
  fills (surface, text, muted text, accent, danger, success, border) — text colours must pass WCAG AA (4.5:1)
  against the surfaces they sit on; check with `design.check` at stage `converging`.
- Components with variants built with `component.*` commands: `Button` (`State=Default|Pressed|Disabled`,
  pressed shown with While pressing → Change to Pressed and back), `Field` (`State=Empty|Filled|Error` with a
  label, a value text and an error message text), plus file-specific ones (cards, list rows, step indicator).
  Instances, not copies, on the screens. Auto layout everywhere it makes sense (vertical/horizontal, hug/fill,
  gaps, padding); constraints on the rest. No absolute positioning to paper over layout.
- Every screen is a top-level frame; flows start at the first screen; end screens are declared end states
  (`proto.endState.set` with a reason).
- Each file has a brief (`design.brief.set`: audience, sponsor, job, context, content, format, tone,
  constraints, success), at least two assumptions (untested), two decisions with rationale and rejected
  options, linked to the screens or components they shaped, and stage `converging`.

## 1. `examples/signup-flow.figcraft` — "Lớp bơi cộng đồng: đăng ký" (community swimming class sign-up)
Vietnamese UI copy with diacritics (and one long sentence that must wrap), phone screens 390×844.
Screens: Welcome → Sign-up form (name, phone or email, age group chooser as a segmented control or variant
toggle, consent checkbox) → Verify code → Done (end state). Variables: `name_filled`, `contact_filled`,
`consent` (booleans), `attempts` (number), `error_message` (string bound to the form's error text).
Interactions: Continue on the form is a conditional: if any field missing → set the missing fields' `Field`
instances to `State=Error` via change-to (or bound visibility of error texts) and set `error_message`, stay on
the screen; else navigate to Verify. Verify has a "Resend code" that opens a toast overlay that closes after a
delay, and a Back that returns to the form with values kept (variables unchanged). Disabled Continue state when
consent is off (variant), with a hint text visible.
Scenarios (stored, each with expectations after every step, written as one person in one situation):
 - S1 "Happy path": fill fields via `setVariable` steps standing in for typing (note says what the real control
   is), accept consent, Continue → Verify, enter code (setVariable) → Done; expect end state.
 - S2 "Missing contact shows an error": Continue with contact empty → still on Form, contact field
   `State=Error`, error text visible with the expected words.
 - S3 "Back keeps what was typed": go to Verify, Back → Form, variables still set, fields Filled.
 - S4 "Resend toast closes by itself": open toast overlay, `wait` → overlay closed.

## 2. `examples/pantry-dashboard.figcraft` — "Community pantry stock" dashboard
English copy, one desktop frame designed at 1440×900 that is **responsive by constraints and auto layout**:
header (fill width), a wrapping row of KPI cards (auto layout wrap, cards with min/max width, fill), a list of
low-stock items, and a side panel that moves below the content when narrow (use wrap or a vertical stack whose
children fill; if a structural change at small widths cannot be expressed with constraints/auto layout, make a
separate compact frame and say so in a decision — do not fake it with fixed copies). States with a string
variable `status` = `loading` | `ready` | `empty` | `error` and conditional visibility (bound `visible` via
boolean variables `is_loading`, `is_empty`, `is_error`, `is_ready` or one per state); buttons: Refresh
(loading → ready after delay), Filter "Only out of stock" (ready → empty when nothing matches, with an empty
state message and a "Clear filter" action back to ready), and a simulated failure toggle → error banner with
Retry → loading → ready. A detail overlay for a list row (open overlay, close on outside click and a Close
button).
Scenarios: S1 load → ready (wait); S2 filter to empty and clear; S3 error → retry → ready; S4 open a row
detail, click outside → closed.
Responsive check (in the test, not the file): resize the frame to 360, 768, 1440 and **1100** (a width not
designed for) with `set_properties {width}`; after each, `design.check {rules: ["text.clipped",
"layout.clippedContent"]}` reports nothing, and screenshots at each width are written for review.

## 3. `examples/bike-repair-booking.figcraft` — "Đặt lịch sửa xe đạp" multi-step service flow
Bilingual-friendly: Vietnamese titles, English secondary text allowed. Phone screens. Steps: Choose service
(three service cards: tune-up, flat tyre, brake check; one selected state via variants) → Details (date/time
chips as variants, notes) → Confirm (shows the chosen service and time via string variables bound to texts,
with "Change" links going back to the right step) → Booked (end state). Step indicator component with variants
per step. Back on each step. Choosing another service from Confirm → Choose → Details → Confirm shows the new
choice.
Scenarios: S1 book a tune-up; S2 change the service from Confirm and see it updated; S3 back from Details keeps
the selected service highlighted; S4 Booked is an end state and the flow check has no dead end/unreachable
screen.

## Long content, fonts
- In each file at least one text with long content (a 2–3 sentence Vietnamese paragraph in the sign-up file, a
  long item name in the dashboard list) in an auto-layout container with fill width so it wraps; the
  responsive/clipping checks must stay clean.
- A missing-font case is NOT put in the examples (it would make them render differently on every machine);
  instead the fixture test creates a copy with one text set to a non-installed family and asserts
  `text.missingFonts` names it and that layout still runs (no crash, text still laid out with a fallback).

## Tests (in-repo, run by CI)
`crates/mcp/tests/it/examples.rs` (or `apps/figcraft-cli/tests/it/examples.rs`; whichever compiles with the
headless backend cleanly): for each example file: open; `proto.check` has no blocker/major problem;
`proto.scenario.run` passes all; `design.check` at the file's stage has no blocker/major finding (record any
minor/note in the test as an explicit expected list, not ignored); save to a temp file, reopen, deep-compare
the inspected document, design context and scenarios, and run the scenarios again. The responsive test above
for the dashboard. A test that every example's `design.get` brief has audience, job, context and success.

## Review deliverables
PNG renders of every screen (and of simulated states for each scenario's final step via `proto.snapshot`) at
`scratchpad/fixtures-shots/` for the visual reviewer, with an index listing what each shows.
