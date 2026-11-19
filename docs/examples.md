# Example prototypes

Three example files show the prototype workflow end to end: screens built from components and
variables, real states reached by interactions, stored scenarios that check them, and a design
context (brief, assumptions, decisions) that explains them. They are original and their data is
synthetic: the places, people, prices and stock figures are made up, there is no backend, and
nothing is sent anywhere.

| File | What it is | Screens |
|---|---|---|
| `examples/signup-flow.figcraft` | "Lớp bơi cộng đồng: đăng ký", a sign-up for a ward's community swimming class (Vietnamese UI, phone, 390×844) | Welcome → Sign-up form → Verify code → Done |
| `examples/pantry-dashboard.figcraft` | "Community pantry stock", a responsive stock dashboard for a neighbourhood food pantry (English, desktop frame designed at 1440×900) | Pantry dashboard, with the Sync toast and Item detail overlays |
| `examples/bike-repair-booking.figcraft` | "Đặt lịch sửa xe đạp", booking a bike repair (Vietnamese titles, English secondary lines, phone, 390×844) | Choose service → Details → Confirm → Booked |

Each file also holds its components (below the screens; beside them in the dashboard, which grows
tall at phone width), a `Theme` variable collection (colours whose text pairs pass WCAG AA), text
styles, a collection of prototype state variables, one flow start, five or six stored scenarios,
three untested assumptions and three decisions with the options they rejected, the states its
screens are declared to have where time passes (each linked to what shows it), one required
element of the brief linked to its layers, at stage `converging`. The end states, declared
states, required elements and decisions the builder proposed record who accepted them
(`acceptedBy`, see below).

## Running the scenarios headless

```sh
cargo build -p figcraft-cli
figguremyui-cli mcp   # then, from an MCP client:
#   open_file {"path": "examples/signup-flow.figcraft"}
#   run_scenarios {}            → passed, total, failed and each scenario's steps
#   check_prototype {}          → screens, flow problems (none in these files)
#   check_design {}             → findings at the file's stage (none in these files)
#   simulate_prototype {"start": <frame>, "steps": [...], "image": true}   → a state as a picture
```

or without a client, as the user, with a script for `figguremyui-cli run`:

```json
[{"command": "file.open", "params": {"path": "examples/bike-repair-booking.figcraft"}},
 {"command": "proto.scenario.run", "params": {}}]
```

The scenarios run on the simulator (`proto.simulate`): it follows the player's rules for which
state comes next, not timing, scrolling, devices or hit-testing (each result lists what it leaves
out in `notModelled`). A pass says the prototype reaches that state, not that a person used it.

## What is simulated

- **Typing.** A prototype has no text input. Tapping a field fills it with a sample value (a
  set-variable interaction), and scenarios stand in for typing with `setVariable` steps whose
  notes say what the real control is.
- **The backend.** Codes, bookings and stock numbers are fixed sample values. The dashboard's
  "Simulate a failed sync" toggle stands in for the network failing; its loading state is a toast
  that closes after a delay.
- **Choices** (segments, chips, service cards, the filter, the toggle) are instances whose variant
  is bound to a string variable that the choice's click sets, so the state survives Back.

## The examples

### Sign-up flow (`signup-flow.figcraft`)

A parent or an adult signs up for a beginners' swimming class from a QR code on the pool's
poster. Variables `name_filled`, `contact_filled`, `consent` (booleans), `attempts` (number) and
`error_message` (string, bound to the form's error summary), plus the fields' states.
"Tiếp tục" is a greyed instance (State=Disabled) while consent is off; a tap on it names the box
above it. The hint under it stays (its words change once the box is ticked), so the button doesn't
move. With consent, it checks the fields: missing ones go to `State=Error` with a message under them
and in the summary, `attempts` goes up, and the form stays; otherwise it goes to Verify. The
summary follows the fixes (`error_kind` says what it is about): filling one field changes it to
what is still missing, and it goes once nothing is (ticking the box clears the consent message).
Verify's "Gửi lại mã" opens a toast right under the link, over no button, that closes itself, and
"‹ Quay lại" returns to the form with everything kept. Done is a declared end state.

Scenarios: S1 Happy path · S2 Missing contact shows an error · S3 Back keeps what was typed ·
S4 Resend toast closes by itself · S5 Continue explains itself while consent is off · S6 Fixing
the form updates the error.

Declared states: Sign-up form `error` (the Error summary), Verify code `error` (the Code field
showing `State=Error`), `success` (Done) and `other` "Code sent again" (the toast). Required
element R1: consent to be contacted and to the pool's safety rules, on the Sign-up form, linked
to the Consent text and the Consent checkbox.

### Pantry dashboard (`pantry-dashboard.figcraft`)

One frame that is responsive by auto layout: the header, the KPI cards and the main row wrap,
with fill children and minimum widths, and the frame hugs its height. It keeps its layout from
360 to 1440 px: four KPI cards per row at 1100 and 1440 px, two at 768, one at 360; the side panel
moves under the list when the frame is narrower than 648 px (the test checks 360, 768, 1100 and
1440 px in every state). State is a string variable `status` (`loading`, `ready`, `empty`,
`error`) with booleans bound to the visibility of the loading placeholders, the list, the empty
state and the error banner. The dashboard loads after a delay; Refresh and Retry show a syncing
toast that finishes after a delay; "Only out of stock" leads to the empty state (nothing is out
of stock) with "Clear filter"; with "Simulate a failed sync" on, the next sync fails and the
banner keeps the last numbers with Retry. While a count loads, the headline numbers show
placeholders (—). The syncing toast opens at the top, where the presenter's bar never covers it.
Each row opens an item detail overlay that closes on
an outside click or with Close.

Scenarios: S1 Load, then ready · S2 Filter to empty and clear · S3 Error, retry, ready ·
S4 Row detail closes on an outside click · S5 Row detail closes with its button.

Declared states of the Pantry dashboard: `loading` (Loading), `empty` (Empty state), `error`
(Error banner) and `success` (List), each shown by a scenario. Required element R1: when the
numbers were last synced, linked to the Updated text.

### Bike repair booking (`bike-repair-booking.figcraft`)

Three service cards (one selected at a time), then a day and a time as chips with an optional
note, then a summary whose texts are bound to the chosen service, slot, price and note, with
"Đổi" links back to the step that owns each choice, then Booked (a declared end state). A Steps
component shows where the viewer is; Details and Confirm have Back, which goes to the step before
in step order (not back through the history, which after "Đổi" would lead to Confirm). Choose
service is the flow start and has no Back.

Scenarios: S1 Book a tune-up · S2 Change the service from Confirm · S3 Back from Details keeps
the service highlighted · S4 Booked is where the flow ends · S5 Continue waits for a day and a
time · S6 Back after changing the time goes to step 1.

Declared states: Choose service `error` and Details `error` (each screen's hint, which names what
is missing when "Tiếp tục" is tapped too early) and Confirm `success` (Booked). No `empty` state is
declared: nothing in this flow can have nothing to show, so at stage `ready` `states.errorEmpty`
would ask that question of Confirm, on purpose, for a person to answer. Required element R1: the
estimated price on the summary before booking, linked to the Value text of the
estimated-price row on Confirm.

## Rebuilding them

The files are generated, not edited by hand:

```sh
cargo build -p figcraft-cli
python3 examples/build/build_examples.py --cli target/debug/figguremyui-cli --out examples
python3 examples/build/build_examples.py --cli target/debug/figguremyui-cli --out /tmp/x --only pantry-dashboard --shots /tmp/x/shots
```

The script (Python 3, standard library only) starts `figguremyui-cli --no-system-fonts mcp` and
builds each file with tool calls, as an AI agent would; the same binary gives the same bytes. It
stops with a non-zero exit, naming the call and its error, when a tool fails, and when an example
doesn't pass its checks. It needs a `figguremyui-cli` built with craft-fonts (`CRAFT_FONTS_DIR`), since
the files use Inter.

Because the builder is an AI agent, the end states and screen states it declares, the required
elements it lists and the decisions it records are proposals. The script's separate step `accept_as_maintainer()` (on by default;
`--no-accept-as-maintainer` skips it) then accepts them with `figguremyui-cli run`, which acts as the
user. The maintainers' acceptance step stands in for a person's review of these example files; in
your own files an agent's end states and decisions stay proposals until you accept them. An
accepted proposal keeps its author (the builder) and records who accepted it (`acceptedBy: user`),
and the Design context panel shows both ("proposed by Example builder (agent) · accepted by you").

`crates/mcp/tests/it/examples.rs` keeps them working: flow check, scenarios, design check (the
`states.*` and `context.requiredMissing` rules included, with no finding at `converging`), the
declared states and required elements, save and reopen, the dashboard at four widths, a missing
font, and an end state that stays a proposal without the acceptance step.
