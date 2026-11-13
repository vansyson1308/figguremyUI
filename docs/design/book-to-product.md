# From *The Shape of Design* to the product: a research note

> **Written:** 2026-11-13 · **Repository state read:** `011f08e5` · **Kind:** research, not a
> specification. Nothing here is implemented by this document. Product name used below: *the
> product* (Figgure My UI; the code still says FigCraft).

## 1. Source, licence and method

**Source.** Frank Chimero, *The Shape of Design*, first edition, 2012, ISBN 978-0-9854722-0-7.
Foreword by Liz Danzico; editor Mandy Brown. 121 PDF pages: foreword, introduction, ten chapters
in three parts, endnotes, acknowledgements. The copyright page gives the book's site as
`shapeofdesignbook.com` (as printed; not checked by us).

**Licence.** The copyright page (PDF 003) states Creative Commons
Attribution-NonCommercial-ShareAlike 3.0 Unported. That licence is **not** on this repository's
licence allowlist and is not compatible with shipping text under MIT OR Apache-2.0. Consequences
we hold ourselves to:

- This note is independent commentary in our own words. It paraphrases and cites; it does not
  reproduce or adapt the book's text. It contains six short phrases quoted from the book, each under
  fifteen words, in quotation marks with a page reference (later mentions of the improvisers'
  maxim refer back to its first citation), and no illustrations.
- The book and its text conversion are not in the repository and must not be added to it.
- Agent guidance derived from this note (section 8) is original wording. Nothing surfaced through
  the MCP server, the app or the documentation may quote the book. The product may be commercial;
  the book's licence is non-commercial.
- Ideas are cited to their chapter so a reader can check them. Where we go beyond what the text
  says, it is labelled as our inference.

**What was read.** A full-text Markdown conversion of the PDF supplied to the team (3,832 lines,
with `PDF page NNN · Printed page N` markers). It was read completely and in order, front matter
to the last page, for this note. Page references below are of the form *PDF 018 / p. 23* and come
from those markers, not from memory of the book.

**What was not viewed.** The rendered PDF pages. We did not see any illustration, photograph,
screenshot, diagram, typography or page layout. The conversion carries only short converter notes
for full-page illustrations. A book about design argues partly through its own form; that part of
the argument is missing from this note. To check against the PDF:

| What | PDF pages |
|---|---|
| Full-page illustrations (converter note only, no text on the page) | 013, 023, 030, 040, 053, 062, 071, 081, 091, 099, 121 |
| Plates with captions only (two paintings; three chairs) | 017, 067 |
| Figures the running text points at but the conversion does not contain | the activity screen of a photo-sharing app (end of 088, before 089); an error dialog (end of 095, before 096); a hotel WiFi page with one button (097) |
| Signs set as display text (we saw the words, not the setting) | 079, 094, 096 |
| Pages with noticeably little text, which may hold an inline drawing (unverified) | 020, 027, 033, 046, 066 |
| Blank page; a stray glyph that is probably an ornament | 117; 107 |

**Repository reading.** To judge present support we read `docs/architecture.md`,
`docs/prototyping-parity.md`, the head of `docs/components-parity.md`,
`docs/auto-layout-parity.md`, `docs/gaps.md` and `ROADMAP.md`, the Conventions, Tools, Prototypes,
Comments, Branches, Developer Mode, Resources and Prompts parts of `docs/mcp.md`,
`crates/engine/src/proto_run.rs`, the command lists of `crates/engine/src/cmd/proto.rs`,
`proto_actions.rs` and `comment.rs`, the type list of `crates/doc/src/proto.rs`, the `Document`
struct in `crates/doc/src/lib.rs`, `crates/mcp/src/prompts.rs`, the instructions string in
`crates/mcp/src/server.rs` and the `Presenter` struct in `crates/ui-egui/src/present.rs`. Nothing
was built or run; statements about behaviour rest on code and documents as read. "Repo support
today" in section 4 describes the committed state at `011f08e5`. Uncommitted work on this
milestone appeared in the working tree while the note was being written; it is not counted as
existing support and is not assessed here.

**Limits of the source.** The book is a reflective essay. Its evidence is anecdote and analogy
from painting, jazz, cooking and poetry. It contains no interface guidelines, no thresholds and no
empirical studies. The author calls it a map of the terrain, not a blueprint (PDF 011 / p. xv).
Anything numeric in the product (contrast ratios, target sizes) must come from, and be cited to,
an external standard, never to this book.

## 2. How to read the mapping

Three layers are kept apart throughout, because the distance between them is where misuse starts.

- **Author's idea** — what Chimero argues, paraphrased, with page references. If it is not in the
  text, it is not here.
- **Team inference** — our reasoning from the idea towards interface prototyping by people and
  agents. The book never discusses AI agents, prototyping tools, flow graphs or linting. Every
  such link is ours and may be wrong.
- **Product decision** — what the product does or deliberately does not do. Decisions are
  proposals to the lead until implemented.

Each chapter section has the same nine fields. "Kind of translation" is one of *feature*,
*function*, *workflow*, *agent guidance*, *evaluation principle* or *none*. "Status" is one of
`already there`, `needs fixing`, `new in this milestone`, `deferred (reason)`,
`not suitable (reason)`; where a chapter yields several proposals, each carries its own status.
"Not suitable" and "agent guidance only" are expected results, not failures of the mapping.

Three reductions are refused wherever they tempt: **context is not responsive breakpoints**,
**delight is not animation**, and **message, tone and format are not three decorative sliders**.

## 3. Foreword and introduction

**Foreword (Liz Danzico; PDF 006–008 / pp. ix–xi).** *Author's idea:* the foreword frames the book
through jazz and call-and-response: process matters more than the finished object, and good work
is made by responding to other people's contributions. Its extended example is the numbering of
the United States interstate highways, designed in the 1920s so the system could grow and so a
traveller could tell where they were from the crossing of two roads; it is presented as a tool
and a platform that is still unfinished. *Team inference:* this is the earliest statement of two
ideas the body develops: a made thing can be a structure for others to add to (chapter 8), and
stable addressing lets many contributors orient themselves. The second is our reading; the
foreword does not generalise it. *Product decision:* none on its own. It supports keeping stable
node ids as the anchor for every record this note proposes (already the case in the document
model).

**Introduction (PDF 009–011 / pp. xiii–xv).** *Author's idea:* the mark of good design is movement: in the maker
while working, out to an audience, and on from person to person. Plans are routinely upset by the
world, so the fitting way forward is responsive rather than regimented. The three parts cover the
maker's inner process, the setting around the work, and the audience. Success should also be
judged by soft qualities such as enthusiasm, engagement and resonance, and by one plain question:
"Does this help us to live well?" (PDF 011 / p. xv). Two foundations are named: design imagines a
future and works towards it, and design is made for other people. *Team inference:* the
introduction is the best guard against over-engineering this mapping. A book that calls itself a
map and prefers soft measures gives little warrant for numeric scoring of designs.
*Product decision:* no aggregate design score anywhere in the product (see chapter 6 and section
6, D).

## 4. Chapters

### Chapter 1 — How and Why

- **Main argument** *(Author's idea)*. Imitating the surface of good work without knowing its
  purpose is empty (the mockingbird that sings a car alarm, PDF 014–015 / pp. 19–20). Making
  alternates between a near mode, concerned with craft, and a far mode, where the maker steps back
  and judges the last move against the whole; the two form a tight loop (PDF 016, 018 / pp. 21,
  23). Purpose is neglected because craft questions are easier to pose and craft failures easier
  to recognise than failures of strategy; yet knowing the objective keeps work heading the right
  way despite missteps, while polished work without one goes in circles (PDF 018 / p. 23). The
  finished piece hides the choices that produced it. Explaining why choices were made, not only
  how, is what makes them open to discussion and transferable to someone else's situation; the
  middle rungs between blank page and result deserve the conversation (PDF 019–020 / pp. 24–25).
- **Pages.** PDF 014–022 (plate on 017, illustration on 023) / printed 19–27.
- **User problem** *(Team inference)*. An agent can produce a plausible screen in seconds. The
  person then cannot tell why it is arranged that way, cannot argue with it, and cannot tell
  whether it serves their purpose. Agent output is the extreme case of the finished surface that
  hides its choices.
- **Repo support today.** None for purpose. `Document` (`crates/doc/src/lib.rs`) has no brief,
  goal or decision field. The nearest stores are per-node `plugin_data` and `annotations`
  (`crates/doc/src/node.rs`), a flow's `description` (`crates/doc/src/proto.rs`) and comment
  threads; none is structured for purpose or rationale. The MCP prompts
  (`crates/mcp/src/prompts.rs`: `mobile_screen`, `design_system`, `landing_page`,
  `prototype_flow`) are pure How: fixed steps, default brand colours, no question about who or
  why. The instructions string (`crates/mcp/src/server.rs`) covers coordinates and verification of
  numbers and pixels only. The near/far loop exists for craft (`screenshot`, `get_node`) but there
  is nothing to step back *against*.
- **Kind of translation.** Feature (brief, decisions) + workflow (make, then step back) + agent
  guidance.
- **Concrete proposal** *(Product decision)*.
  1. A brief stored in the document, readable and editable by the person in a panel and by agents
     through commands. Field wording and gating are constrained by chapters 3, 5, 6 and 8; see
     section 6, A.
  2. A decision record: what was chosen, why, what else was considered, who proposed it (person or
     named agent), linked to node ids. Grain: structural choices (the middle rungs), not every
     property value.
  3. Rewrite the existing How-only prompts so each begins by reading the brief or asking for the
     missing purpose, and ends by stepping back against it.
- **Risk of misreading.** Turning Why into a form that must be filled before work may start; the
  book argues the opposite in chapter 3. Generating rationale after the fact so that every node
  has a paragraph: that is the same veneer in prose. Treating a recorded reason as a verified one.
- **How to test.** Engine tests: brief and decisions survive save, reopen, undo, redo, duplicate
  and delete of linked nodes (links report as dangling, never panic). MCP test: read and write
  round trip. UI: panel screenshot with a partially filled brief. Prompt test: a rewritten prompt
  with an empty brief yields a question or an explicit assumption record before any `create_node`.
- **Status.** Brief and decisions: `new in this milestone`. Prompts and instructions:
  `needs fixing`.

### Chapter 2 — Craft and Beauty

- **Main argument** *(Author's idea)*. Design is the old human habit of changing a situation
  towards a better one (the hand axe, PDF 025 / p. 30). Beauty is craft that goes past what
  function requires; usefulness comes first, and beauty is welcome once a thing is necessary and
  useful (PDF 026 / p. 31). The source of high craft is care for a particular audience: a
  teacher's verdict on a student portfolio, "Needs more love" (PDF 028 / p. 33), is read as a
  missing commitment to someone, not as a missing technique. Work has enough care when the
  maker's enthusiasm reaches the audience, and designed things have a quality that shows only in
  use: the axe was made for the hand, the violin for playing (PDF 029 / p. 34).
- **Pages.** PDF 024–029 (illustration on 030) / printed 29–34.
- **User problem** *(Team inference)*. Agent-made screens are often tidy and lifeless: correct
  spacing, nobody in mind. And prototypes are judged from still pictures although their quality
  only appears when used.
- **Repo support today.** Craft at the level of mechanics is strong: auto layout, constraints,
  styles, variables, components (`docs/auto-layout-parity.md`, `docs/components-parity.md`). Use
  is possible only by a person in the player (`crates/ui-egui/src/present/`); an agent can look
  (`screenshot`) but cannot use.
- **Kind of translation.** Evaluation principle + agent guidance. No feature of its own.
- **Concrete proposal** *(Product decision)*.
  1. Order of critique: usefulness findings are reported before polish findings, always.
  2. "Verified" for a flow means it was run, not looked at; this is one of the reasons for the
     headless simulator (chapter 7, section 6, C).
  3. Agent guidance: name the person and situation a screen is for before polishing it.
- **Risk of misreading.** A beauty or craft score. Reading care as effort and gold-plating.
  Claiming that an agent "cares": the product must not put feelings into an agent's mouth.
  Treating consistency checks as a measure of love; they are a floor at most.
- **How to test.** Check report ordering in the findings output. Guidance is checked in section 8
  form: a transcript shows the flow check and a scenario run before the agent says a flow works.
- **Status.** Ordering and "verified means run": `new in this milestone` as part of C and D. Any
  scoring of beauty: `not suitable (the text grounds beauty in care for an audience, which a rule
  cannot measure)`.

### Chapter 3 — Improvisation and Limitations

- **Main argument** *(Author's idea)*. Work begins from other people's contributions, as in
  linked-verse poetry where each poet builds on the previous lines (PDF 031–032 / pp. 37–38).
  Define the objectives first, then set them aside so that early ideas are not crushed by them,
  and bring them back as a rudder once there is momentum (PDF 032–034 / pp. 38–40). Momentum can
  be had by deliberately producing bad ideas; recorded, they become material (PDF 033–034 /
  pp. 39–40). The improviser's rule "Yes, and…" (PDF 035 / p. 41) means accept what is offered and
  add to it. Judgment is essential but must be timed: its severity should match how developed the
  idea is (PDF 035–036 / pp. 41–42). Rules fixed beforehand are what make free play possible; the
  example is a recording session run from short written instructions (PDF 036–037 / pp. 42–43).
  Improvisation is wasteful and most ideas are discarded (PDF 037 / p. 43). Limits can be
  self-imposed and are of three kinds: on content, on structure, on tone (PDF 038 / p. 44).
- **Pages.** PDF 031–039 (illustration on 040) / printed 37–45.
- **User problem** *(Team inference)*. Ask an agent for options and it returns one design in three
  colours. Ask it to improve a screen and it replaces the person's work. Run a checker on a rough
  idea and it drowns in polish complaints.
- **Repo support today.** Mechanisms for holding variants exist: pages, sections, duplicating
  frames, `version.*`, and file-level `branch.create` / `branch.review` (`docs/mcp.md`, Branches).
  None records the constraints an exploration ran under, which lever each variant moves, or which
  was chosen and why. Branch review compares properties, not structure or order of information.
  Nothing marks the maturity of a piece of work.
- **Kind of translation.** Feature (alternative sets) + workflow + agent guidance.
- **Concrete proposal** *(Product decision)*.
  1. An alternative set: the original (kept), the constraints declared **before** generation,
     grouped as content, structure and tone, and the variants. Discarded variants are parked, not
     deleted.
  2. A maturity marker on a frame or set (exploring, converging, ready). Checks select their rule
     profile from it: structure-level rules while exploring, everything when ready.
  3. Agent guidance: build on what the person made (duplicate and transform); never overwrite it.
     The "worst idea first" exercise is an optional technique, not a feature.
- **Risk of misreading.** "Yes, and" as "the agent never disagrees": the text keeps judgment and
  only delays it. Objectives being set aside as licence to ignore the brief: they come back as the
  rudder. Forcing alternatives on every request: chapter 4 says established pairings recur because
  they work (PDF 044 / p. 50).
- **How to test.** Engine tests: creating a set keeps the original untouched and one undo removes
  the whole set; parked variants survive reopen; a set cannot be created without at least one
  stated constraint. Check test: the same frame yields fewer findings under the exploring profile
  and the omitted rules are listed as "not run at this stage", not silently skipped.
- **Status.** Minimal alternative set and maturity marker: `new in this milestone`. Structural
  comparison beyond facts already computable: `deferred (needs a tree-diff of information order)`.

### Chapter 4 — Form and Magic

- **Main argument** *(Author's idea)*. Seen from far enough away, design work has three
  interdependent traits: the message (what is said, the promise the work makes, its utility), the
  tone (the arrangement and inflection; explicitly not the same as style, which is a surface
  device in service of tone) and the format (the artefact, including where it sits and how it is
  met) (PDF 043–044 / pp. 49–50). The author compares them to levers with well-known settings
  that recur because they work; every setting stays subordinate to the objective (PDF 044 / p. 50).
  Assumptions drift out of step with circumstances, so the start of a project is the moment to
  re-examine them and restate the root objective (PDF 045 / p. 51). New settings should be judged
  from experience of all three (PDF 045–046 / pp. 51–52). A request for a faster horse is a How
  question loyal to an existing format; the Why question behind it names the need, and a good one
  is specific enough to observe yet open to several answers; iterate on how needs are met, not on
  the shape of old solutions (PDF 049 / p. 55). Work must be useful before it can be more than
  useful (PDF 050 / p. 56). Assumptions are to be tested, and one must know what is actually
  achievable (PDF 051 / p. 57).
- **Pages.** PDF 041–051 / printed 47–57.
- **User problem** *(Team inference)*. People ask for an artefact ("a settings page", "an
  onboarding carousel") when they have a need. Agents comply literally. And "make it friendlier"
  is answered with rounder corners.
- **Repo support today.** Format-level material is rich (device presets, frames, overlays,
  components, variables with modes). Nothing holds a message or a tone in words, nor a list of
  assumptions with a tested state.
- **Kind of translation.** Agent guidance + two brief fields + the labelling scheme for
  alternatives. Explicitly **not** a control surface.
- **Concrete proposal** *(Product decision)*.
  1. The brief separates what the work promises (message) from how it should come across (tone,
     in words, each word tied to at least one concrete choice) from the form it takes and where
     it is met (format).
  2. Each variant in an alternative set states which of the three it changes and what it holds
     fixed. A set whose variants differ only in surface style is reported as such.
  3. The brief lists assumptions, each with a state (untested, checked by rule, simulated,
     observed with people) and who set it.
  4. Success signals in the brief must be observable statements; the form says so in plain words.
  5. Agent guidance: restate the need behind a requested artefact once before building it.
- **Risk of misreading.** Three sliders or pickers labelled Message, Tone, Format: the text calls
  them interdependent and subordinate to the objective; a slider implies independence and
  decoration. "Magic" as novelty for its own sake: the examples are of people who knew their
  field's limits intimately. Second-guessing every request: restate once, then follow the person.
- **How to test.** Engine test: a set with style-only differences (same tree, same text, same
  order; paints differ) gets the "style only" fact. Brief test: assumption states round-trip and
  only a person can set "observed with people". Guidance: transcript check for one restatement of
  need when a request names an artefact.
- **Status.** Brief fields, assumption states, lever labels: `new in this milestone`. A
  message/tone/format control surface: `not suitable (reduces an analytical frame to decoration)`.

### Chapter 5 — Fiction and Bridges

- **Main argument** *(Author's idea)*. Making, like speaking, presupposes someone to receive it;
  the audience gives the work its worth (PDF 054–055 / pp. 61–62). Design arises among three
  parties: a client who commissions, an audience who uses, and a designer between them. Their
  wants need to be compatible, not identical (PDF 055, 058 / pp. 62, 65). Design is a bridge with
  traffic both ways; its products are not ends in themselves and are judged by the consequences of
  the connection they make (PDF 057 / p. 64). During production the audience is usually absent, so
  the designer stands in for them while being paid by someone else: a double allegiance (PDF 058 /
  p. 65). Images of a better future are useful untruths that act as hypotheses and oblige their
  authors to make them true; the corrosive kind misrepresents what is already there with no intent
  to close the gap. Persuasion can lead people to pasture or over a cliff (PDF 059–061 /
  pp. 66–68).
- **Pages.** PDF 054–061 (illustration on 062) / printed 61–68.
- **User problem** *(Team inference)*. A prototype is exactly such a useful untruth: it shows
  something that does not exist yet. With an agent in the loop the risks multiply: invented
  content looks like data, an agent's own review is reported as if people had been asked, and a
  green check reads as "this works for users".
- **Repo support today.** Agents already act under their own name in comments, with an agent flag
  (`crates/engine/src/cmd/comment.rs`, `docs/mcp.md` Comments). Nothing distinguishes kinds of
  evidence, and nothing records who commissions versus who uses.
- **Kind of translation.** Evaluation principle + function (provenance labels) + agent guidance.
- **Concrete proposal** *(Product decision)*.
  1. One provenance vocabulary on every finding, scenario result and assumption state: *measured
     by rule*, *simulated*, *agent opinion* (with the agent's name), *person's opinion*, *observed
     with people* (entered by a person, with a note of when and with whom). The last cannot be set
     by an agent.
  2. The brief names the commissioner and the audience separately and has room for where their
     interests differ.
  3. Agent guidance: never call a simulation or an agent review user research; label placeholder
     content and invented scenario detail as assumed; do not propose patterns that misrepresent
     the product to the person using it.
- **Risk of misreading.** The remark that a design has to carry people somewhere (PDF 057 / p. 64)
  is about the effect of design on people, not about navigation; it does not justify dead-end
  detection. The double allegiance is not permission for deceptive interfaces; the same pages draw the line
  at misrepresenting what exists.
- **How to test.** Engine test: an MCP-origin command that tries to set *observed with people* is
  refused with a clear message; the same from the app succeeds. Report test: every finding and
  result serialises a provenance value; none is empty. Text test: no user-facing string or prompt
  contains "user research", "usability test" or "validated with users" for agent-origin output.
- **Status.** Provenance labels and the two-party fields: `new in this milestone`. Records of real
  sessions with people beyond a labelled note: `deferred (no evidence of need yet; vocabulary
  reserved now)`. Dark-pattern detection by rule: `not suitable (judgment; guidance only)`.

### Chapter 6 — Context and Response

- **Main argument** *(Author's idea)*. There are no fixed points; the maker keeps balance by
  moving in response (PDF 063 / p. 71). The model is an artist who arrives without a plan, spends
  a long time observing the room, and makes the piece as a response to it; likewise portrait
  photographers wait for the sitter to appear as they are (PDF 063–064 / pp. 71–72). A problem
  takes time to show itself and is never fully known; one draws a line and proceeds (PDF 065 /
  p. 73). Decisions are tried in place and checked by eye; design is about outcomes and
  consequences more than objects (PDF 065 / p. 73). One brief for a chair yields three very
  different chairs: the objective fixes some constants (a seat, human proportions), the rest
  varies with maker, time, place and culture, and the success of one does not make the others
  worse (PDF 066–068 / pp. 74–76). Culture sets the target and good work moves it, so a hit is
  temporary; design is larger than problem solving (PDF 068–069 / pp. 76–77).
- **Pages.** PDF 063–069 (plate on 067) / printed 71–77.
- **User problem** *(Team inference)*. Agents start drawing before looking: they ignore the
  document's existing components, tokens, neighbouring screens and open comments. Briefs are
  treated as finished when they are a first guess. Variants get ranked as if one must win.
- **Repo support today.** Observation is well served: `inspect_document` with `outline`,
  `get_node`, `node.find`, `style.list`, variable and component commands, `comment.list`,
  `figcraft://document`. Trying in place is served for looks (`screenshot`, device presets,
  `proto.settings {responsive}`), not for behaviour. Nothing records that a decision was made
  under a brief that has since changed.
- **Kind of translation.** Workflow + agent guidance + evaluation principle.
- **Concrete proposal** *(Product decision)*.
  1. **Context means the situation the work is met in and the surroundings it joins**: who, where,
     in what state, next to which existing screens and system. It is recorded in words in the
     brief ("where and when is this used"). Breakpoints and device sizes are one small
     consequence of context, never its definition.
  2. The brief is revisable and keeps its revisions. Decisions and scenario results made before a
     revision of the field they depend on are shown as possibly stale, not as invalid.
  3. Alternatives are compared by facts and trade-offs and are never ranked by a single number;
     constants derived from the objectives are listed separately from what varies.
  4. Agent guidance: observe before making (outline, tokens, components, comments, flows) and say
     what was found.
- **Risk of misreading.** Equating context with responsive layout. Reading the artist's
  preference for intuition over logic as permission to skip evidence: in the text every adjustment
  is still checked by eye. The absence of fixed solutions as "anything goes": the chairs share what the objective demands.
- **How to test.** Engine test: revising a brief field flags linked decisions as stale and a
  re-confirmation clears the flag. Comparison output contains no aggregate score field (schema
  test). Guidance: transcript shows an outline read and a token/component listing before the first
  create on an existing document.
- **Status.** Observation tools: `already there`. Revisable brief with staleness, score-free
  comparison: `new in this milestone`. "Context" panel that is a breakpoint list:
  `not suitable (it is the reduction this chapter argues against)`.

### Chapter 7 — Stories and Voids

- **Main argument** *(Author's idea)*. For work to resonate and travel it needs narrative; a story
  is change over time at any scale (PDF 072–073 / pp. 81–82). A painting of a late-night diner
  holds attention because it withholds its plot and the viewer must complete it (PDF 073–075 /
  pp. 82–84). Students critiquing abstract compositions gave better feedback as small stories
  than as formal description: loose in wording, precise in consequence (PDF 075–076 / pp. 84–85).
  To use narrative as material, find where time passes: down a page, between spreads, in a scroll,
  or when an application notices a week has gone by since it was opened (PDF 076–077 / pp. 85–86).
  Stories create empathy; one life closely observed stands for many; and whether we care about
  the person matters more than the story's polish (PDF 077–080 / pp. 86–89).
- **Pages.** PDF 072–080 (illustration on 081) / printed 81–89.
- **User problem** *(Team inference)*. Prototype flows are wired as graphs of screens, and nobody
  writes down whose path it is or what has changed for them by the end. Tests of a prototype, if
  any, are click scripts that mean nothing to the person reviewing them. States that exist only
  because time passes (first use, nothing yet, waiting, failure, coming back) are the ones that
  go missing.
- **Repo support today.** Flows have a name, a start frame and a description
  (`crates/doc/src/proto.rs`, `proto.flow.*`). Interactions cover ten triggers and eleven actions
  (`docs/prototyping-parity.md`). The player's state (current screen, history, overlays, scroll)
  lives in `Presenter` in the UI crate, mixed with texture handles
  (`crates/ui-egui/src/present.rs`); pure pieces already sit in the engine
  (`crates/engine/src/proto_run.rs`: branch choice, navigation target, change-to target, variable
  and mode setting, hotspots). Attached to a running app, an agent can inject pointer events in
  screen coordinates and read `presenter` from `inspect_ui` (`docs/control-protocol.md`); there is
  no headless run, no addressing by node id and no assertion.
- **Kind of translation.** Feature (scenarios) + function (headless run) + evaluation principle.
- **Concrete proposal** *(Product decision)*.
  1. A scenario is stored as a short story plus steps: who, in what situation, what they start
     with, what should be different at the end; then the steps (act on node X) and expectations
     (screen, overlay, variable value, variant shown). The story is for people; the steps are for
     the machine. Invented detail is labelled as assumed.
  2. A headless run executes the steps against the same state machine the player uses and returns
     per-step results with provenance *simulated* and the document revision it ran against.
  3. The places where time passes become a checklist the brief or scenario can declare per
     screen: first use, empty, loading, error, returning. The missing-states check compares what is declared
     with what exists; with nothing declared it reports "no states declared", not a failure.
- **Risk of misreading.** Storytelling as marketing copy or animated onboarding. Personas with
  invented biographies presented as findings. The "void" as an excuse for unfinished flows: the
  text is about inviting an audience's imagination, not about missing screens. A passing scenario
  as proof that people will succeed.
- **How to test.** Engine tests: a scenario with navigate, overlay, set-variable, conditional and
  change-to steps reaches the expected state; a wrong expectation fails with the step index and
  the actual state; deleting a target node makes the scenario report a broken step, not panic.
  Equivalence test: the UI player and the headless run, given the same steps on the same file,
  end in the same screen, overlays and variable values. Save and reopen keeps scenarios and last
  results.
- **Status.** Scenarios and headless run: `new in this milestone`. Declared-states check:
  `new in this milestone` in its declared form. Timed and gesture-following behaviour (delays,
  drags that follow the pointer, springs) in the headless run: `deferred (start with discrete
  triggers; state the limit in the result)`.

### Chapter 8 — Frameworks and Etiquette

- **Main argument** *(Author's idea)*. Besides resonance, audience-centred work aims at
  engagement, which is served by frameworks: structures that let others contribute (PDF 082 /
  p. 91). The motivating doubt is "What if the audience is smarter than I am?" (PDF 082 / p. 91);
  a framework gathers what they know and shares ownership (PDF 083 / p. 92). A framework has an
  action to perform, the pull of a larger worthwhile work, and etiquette (PDF 083 / p. 92).
  Contributions should change hands often, in tight loops, so that each contributor can see their
  effect (PDF 084 / p. 93). Adding salt to someone else's soup is the model: small, low risk,
  judged by tasting, correctable; the framework is not the dish and is noticed only when out of
  balance (PDF 084–085 / pp. 93–94). The designer sets a few good limits, steps back, and must
  judge when to yield control and when to steer, usually lightly (PDF 085–086 / pp. 94–95). Good
  rules make a contributor feel half done already; rules stated in specialist jargon kill the wish
  to take part (PDF 086–087 / pp. 95–96). Etiquette, the rules of engagement, is the designer's
  concern too, and norms from one medium do not transfer one-to-one to another (PDF 087 / p. 96).
  A screen can be dense and efficient and still fail at what it is for: an activity list that
  never made the author feel thanked did so once, when a bug enlarged the faces (PDF 088–090 /
  pp. 97–99).
- **Pages.** PDF 082–090 (illustration on 091) / printed 91–99.
- **User problem** *(Team inference)*. This chapter describes the product itself: a structure in
  which a person and one or more agents contribute to one piece of work. The person's problems
  are loss of control (the agent changed things I did not ask about), loss of authorship (whose
  decision was that?), and forms that speak a specialist's language.
- **Repo support today.** Several rules of engagement exist: every MCP mutation is one undo step
  and `batch` groups several; agents comment under their own name and cannot edit others'
  messages; the `fix_comments` prompt says to change only what a comment mentions and to reply
  before resolving; `wait_for_changes` gives a tight loop on comments (`docs/mcp.md`). Missing:
  the same etiquette for anything other than comments (no proposed/accepted state on agent-made
  records), and no record of who made a decision.
- **Kind of translation.** Workflow + agent guidance + two constraints on the features of A–D.
- **Concrete proposal** *(Product decision)*.
  1. Authorship and status on every record this milestone adds (brief revisions, decisions,
     assumptions, alternatives, dismissals): author (person or named agent) and, for agent-made
     ones, *proposed* until a person accepts. An agent cannot accept its own proposal.
  2. The brief form is written in plain questions, every field optional, a partly filled brief
     valid. No field label uses method jargon.
  3. Findings can be dismissed by a person with a reason ("deliberate"); the dismissal is kept as
     a decision and the finding is not re-reported until the node changes.
  4. Agent guidance: contribute in small undoable units, name what changed, leave everything else
     alone, and treat a rule violation the person likes as a question, not an error.
- **Risk of misreading.** Reading the advice to step back as "the agent runs free": in the text the
  one who sets the framework is the designer, and here that is the person. Importing human
  collaboration norms wholesale (an agent that "agrees" or "apologises"): the text itself warns
  against one-to-one transfer of manners between media. Turning the findings list into a dense
  dashboard of counts; show findings on the nodes they concern.
- **How to test.** Engine tests: an agent-origin `accept` on an agent-authored record is refused;
  a dismissed finding stays dismissed across re-checks until its node's relevant property changes.
  Copy review: brief labels pass a plain-language check by someone outside the team (a person's
  judgment; say so in the result). Undo test: each agent contribution to brief or decisions is one
  undo step.
- **Status.** Undo units and agent naming in comments: `already there`. Proposed/accepted and
  dismiss-with-reason: `new in this milestone`. Plain-language brief: `new in this milestone`
  (a constraint on A). Attribution of every document edit in history to person or agent:
  `deferred (cross-cutting change to history; comments and records cover the need for now)`.

### Chapter 9 — Delight and Accommodation

- **Main argument** *(Author's idea)*. Delight is not needed for a thing to work, but its source
  is in use, and it is not the same as making people happy: it is a memorable experience of
  superior fit, of feeling understood (PDF 092–093 / pp. 101–102). Treated as a gimmick to polish
  a weak idea with novelty it is misused; it should build lasting benefit (PDF 093 / p. 102).
  Examples of fit: a spell checker learning one's habitual typos, a basket appearing as one's
  hands fill (PDF 094 / p. 103). A low-bridge warning hung at the bridge's height shows message,
  tone and format working together; delight is where empathy, surprise and clarity overlap
  (PDF 094–095 / pp. 103–104). An oddly worded error message became a moment of company; things
  going wrong are among the best occasions (PDF 095–096 / pp. 104–105). A mandatory exit sign was
  made part of the room: "Every requirement is an opportunity for delight" (PDF 096 / p. 105).
  Clarity balances adding detail and removing it; mostly value comes from removing friction, as
  in a network login reduced to one button (PDF 096–097 / pp. 105–106). Empathy is the root; cold,
  over-composed work signals not knowing the audience (PDF 098 / p. 107).
- **Pages.** PDF 092–098 (illustration on 099) / printed 101–107.
- **User problem** *(Team inference)*. "Make it delightful" is heard as "add motion". Error, empty
  and waiting states are left as afterthoughts. Required elements (consent text, permission
  prompts) are shrunk and hidden. Nobody counts the steps.
- **Repo support today.** Motion is well supplied (eight transitions, thirteen easings, smart
  animate), which makes the wrong reading the cheap one. Variants can hold states; nothing says
  which states a screen or component ought to have. Nothing measures the length of a path.
- **Kind of translation.** Evaluation principle + two small functions + agent guidance.
- **Concrete proposal** *(Product decision)*.
  1. **Delight is fit, clarity and care at the moments that go wrong; it is not animation.** No
     feature, check or prompt in the product uses the word for motion or decoration.
  2. Steps-to-goal: a scenario run reports the number of actions and screens from start to its
     end state. Reported as a fact, shown side by side for alternatives, never as a grade.
  3. Required elements are listed in the brief's constraints; a check reports a listed element
     that cannot be found on the screens it applies to, or is below the legibility threshold.
  4. Declared states (chapter 7) include error and empty; the agent guidance asks for those
     before any embellishment.
- **Risk of misreading.** A "delight" toggle or generator. Surprise as a rule: it cannot be
  linted and fades when expected (PDF 095 / p. 104). "Remove friction" as an absolute: chapter 10
  describes slowing a reader down on purpose (PDF 106–107 / pp. 115–116); deliberate friction is
  legitimate when it is a recorded decision. Humorous copy everywhere.
- **How to test.** Engine tests: steps-to-goal equals the count of executed steps and distinct
  screens on fixtures; the required-element check finds and misses as expected on fixtures.
  Search test: the word "delight" appears in no command id, finding rule or prompt.
- **Status.** Steps-to-goal and required elements: `new in this milestone` (small, ride on C and
  D). A delight feature or score: `not suitable (the chapter defines it as an outcome of empathy,
  not a property that can be added)`.

### Chapter 10 — Gifts and Giving

- **Main argument** *(Author's idea)*. Made things add to the atmosphere of everyone's life beyond
  what is bought and sold (PDF 100–102 / pp. 109–111). Following Lewis Hyde, the best work is like
  a gift: its value is partly outside price; talent brings an obligation to use it for others
  (PDF 102–103 / pp. 111–112). Makers go past the point of financial sense, in a chef's phrase the
  "long, hard, stupid way" (PDF 103 / p. 112), and audiences sense it (PDF 104 / p. 113). Gifts
  must move on to have value; context prepares the receiver to appreciate; good work closes
  desires more than it opens new ones (PDF 105–106 / pp. 114–115). A gift's success is the
  recipient's experience, and its quality lies in thought, response to context and tailoring;
  money is the untailored gift (PDF 107–108 / pp. 116–117). A business card is not finished until
  it is handed to someone; the measure of a design is that it can be passed on (PDF 108–109 /
  pp. 117–118). Attention is the scarcest thing an audience gives and deserves work of matching
  quality; an artist who made himself available to whoever asked, without promoting himself, is
  held up as a model (PDF 109–110 / pp. 118–119). What we make stands for promises between people
  (PDF 112 / p. 121).
- **Pages.** PDF 100–113 / printed 109–122.
- **User problem** *(Team inference)*. Generic output is the untailored gift: a template with the
  default accent colour. A prototype that only its maker can open or understand has not been
  given. Agents spend the person's attention freely: long reports, unsolicited redesigns.
- **Repo support today.** Hand-over mechanics: atomic save and reopen, versions, `devStatus`,
  comment links, `comment.prompt` as a Markdown hand-over of feedback; sharing a prototype link is
  listed as missing (`docs/prototyping-parity.md`). Starter templates and the creation prompts
  are untailored by construction (`template.list`, `crates/mcp/src/prompts.rs`).
- **Kind of translation.** Evaluation principle + agent guidance + one small function.
- **Concrete proposal** *(Product decision)*.
  1. Definition of done for this milestone's records: everything round-trips through save and
     reopen and reads the same from the panel and from MCP, so the next person or agent can pick
     the work up.
  2. A hand-over summary command, modelled on `comment.prompt`: brief, accepted decisions, open
     assumptions, scenario results with provenance, undismissed findings, as bounded Markdown.
  3. Agent guidance: respond to what was asked and to open comments; cap and group what is
     reported; end with what is still open.
- **Risk of misreading.** The chef's phrase as a mandate for more output or longer
  runs; here it argues for not skipping verification, nothing more. Gift language applied to an
  agent's motives. Deliberate slowing-down as a general interface principle. Using the chapter's
  remarks on attention to justify engagement-maximising patterns; the text argues for rewarding
  attention, not harvesting it.
- **How to test.** Round-trip tests listed under chapters 1, 3 and 7. Summary test: bounded size,
  stable order, includes provenance for every result, contains no finding the person dismissed.
- **Status.** Round-trip as definition of done: `new in this milestone` (a test obligation on
  A–D). Hand-over summary: `deferred (cheap, but only useful once A, C and D exist; first
  follow-up)`. Sharing a prototype link: `deferred (needs sharing, gap 2 in docs/gaps.md)`.

## 5. Endnotes and sources of the book

The endnotes (PDF 114–116 / pp. 125–127) cite epigraphs, artworks, a small number of books and
several web pages retrieved in 2012. What matters for the product:

- **The book's own sources are essays and interviews, not studies.** The chapter 6 material on
  responding to a room comes from Lawrence Weschler's book of conversations with the artist
  Robert Irwin; the "adjacent possible" from Steven Johnson's *Where Good Ideas Come From*; the
  gift argument from Lewis Hyde's *The Gift*; the closing thought from George Kubler's *The Shape
  of Time*. None supports a quantitative rule. Product rules that need numbers must cite their own
  standards.
- **Some anecdotes are unsourced.** The story of an advertising man rewording a blind man's sign
  (PDF 079 / p. 88) is introduced as an old story heard second-hand and has no endnote; the
  "faster horse" remark is given as famous, without a source (PDF 049 / p. 55). Neither should be
  repeated in product copy or agent guidance as fact.
- **Nothing in the endnotes changes a product decision.** They are listed here so that later
  readers do not assume the book carries empirical weight it does not claim.
- The acknowledgements (PDF 118–120 / pp. 129–131) name readers and backers and contain no
  argument. Names are not reproduced here.

## 6. Critique of the lead's hypothesis (A–D)

The order data → engine command → MCP → UI → save/reopen → tests fits the repository's
architecture rule that everything is a command, and chapter 8's demand that the person sees and
can correct every contribution. The disagreements are about content.

### A. Design brief and rationale

**Where the book supports it.** Strongly. Chapter 1: choices become discussable only when their
reasons are visible; chapter 4: restate objectives and re-examine assumptions at the start, and a
good Why is observable; chapter 5: three parties and a stand-in for the absent audience.

**Where it does not, and what to change.**

1. **A brief must not gate work.** Chapter 3 says to define objectives and then put them away so
   that early ideas survive, bringing them back as the rudder (PDF 032–034). A panel that blocks
   or nags until six fields are filled contradicts the source. Every field optional; no check
   complains about an incomplete brief while work is marked exploring.
2. **A brief is never complete.** Chapter 6 says a problem is never fully known (PDF 065). Keep
   revisions; mark dependent decisions and results as possibly stale; do not present a filled
   brief as a finished requirement.
3. **"Job to be done" and "success signals" are jargon.** Chapter 8's point about rules stated in
   specialist terms (PDF 086–087) applies to our own form. Suggested plain questions, all
   traceable: *Who is this for? Who is asking for it, and where might they want something
   different? What should a person be able to do or know afterwards? Where and when will they
   meet it? How should it come across, and which choices carry that? What must be in it, like it
   or not? What are we assuming without having checked? How would we notice it works?*
4. **"Who" is two parties.** The lead's fields have one. Chapter 5 separates commissioner from
   audience and makes the tension between them the designer's job (PDF 055, 058).
5. **Decisions need status and authorship, not just links.** Proposed or accepted, by whom,
   superseded by what, and what else was considered. Without this, agent rationale becomes
   after-the-fact decoration, which chapter 1 names as the problem.
6. **Over-application to avoid:** a rationale per node; mandatory rationale on every agent edit;
   treating a written reason as a verified one.

### B. Purposeful alternatives

**Where the book supports it.** Chapter 3 (produce many, expect waste, limits first), chapter 4
(vary the three traits, not the surface), chapter 6 (one brief, several valid answers).

**Where it does not, and what to change.**

1. **Constraints come first and come in three kinds** (content, structure, tone; PDF 038). A set
   without stated constraints is three guesses. Constants should be the ones the objectives
   demand (the seat and proportions of the chairs).
2. **No winner by score.** Chapter 6 is explicit that one success does not diminish another
   (PDF 067–068). Comparison is a table of facts and trade-offs; the person chooses; the choice
   is recorded as a decision.
3. **Parked, not deleted.** Rejected ideas are material later (PDF 034).
4. **Alternatives grow from what exists.** The linked-verse model (PDF 032) and "yes, and"
   (PDF 035) point to duplicating and transforming the person's design, not regenerating from
   nothing. "Keeping the original" in the lead's wording is right; make it structural: the set
   references the original, and creating a set never edits it.
5. **Criticism scaled to maturity** (PDF 036). Full checks on a rough variant are a category
   error; this needs the maturity marker, which A–D do not mention.
6. **Not for everything.** Established settings recur because they work (PDF 044). Offer
   alternatives for structural or contested choices; do not make them a default step.
7. **Honesty about "compare structure":** a real structural comparison needs a tree diff of
   information order that the repository does not have (branch review is property-level). For
   this milestone compare what is computable: node tree outline, order of text content, component
   use, steps-to-goal from scenarios, which constraints each variant meets; leave trade-offs as
   authored text with provenance.

### C. Flow and state verification

**Where the book supports it.** Only indirectly, and the document should say so wherever C is
justified by this book. Support: quality shows only in use (chapter 2); assess after each move
(chapter 1); try in place and check (chapter 6); a story is change over time, so look where time
passes (chapter 7); contributions are judged by observing their effect (chapter 8). The book
never mentions flows, state machines or testing. C is our instrument for the far mode.

**What to change.**

1. **One state machine.** The player's state lives in the UI crate with rendering state. If the
   headless run is a second implementation, it will verify something other than what the person
   sees, and its green results will be the corrosive kind of untruth chapter 5 describes. Move
   the state transitions into the engine and have the UI player call them. This is the largest
   cost in the milestone and the one that should not be cut. An equivalence test is mandatory.
2. **Report facts, not verdicts, from the graph.** "Screen X has no way out and is not declared
   an end" is a fact; whether it is a defect depends on intent. Unreachable frames are often
   parked alternatives (see B.3) and must not be flagged when parked.
3. **Scenarios as stories with steps**, not click scripts (chapter 7), with invented detail
   labelled.
4. **Results say what they are.** *Simulated*, with the document revision and the list of
   behaviours the run does not model (timing, pointer-following drags, scrolling, widgets).
5. **Add steps-to-goal** as a by-product (chapter 9).
6. **Do not cite PDF 057 for dead-end detection.** It is a tempting line and a wrong one.

### D. Evidence-based critique loop

**Where the book supports it.** Chapter 1 supports a critical far mode and, importantly, warns
that craft failures are easier to see than failures of purpose (PDF 018). That sentence is both
the warrant for D and its main risk: an automatic checker makes the easy kind of criticism
abundant and free, and will crowd out the other kind unless the product resists it. Chapter 8
supports the re-check loop (taste, adjust). Chapter 9 supports attention to failure states.

**Where it does not, and what to change.**

1. **The book supplies no thresholds.** Contrast, target size and legibility limits come from
   accessibility standards and platform guidelines. Each rule must name its external source and
   version in its evidence; none may be attributed to this book. Whoever implements a rule reads
   the standard's text first; the numbers are not to be taken from this note.
2. **"Inconsistent spacing and type scale" needs a reference frame.** Measured against a
   universal grid it imposes a convention as a given, which chapter 4 warns about (PDF 045).
   Measure against the document's own variables and styles: "value not on any token of this
   file" is evidence; "not a multiple of 8" is taste. With no tokens, the rule reports that it
   has nothing to compare with.
3. **A third lane is missing: findings about purpose records.** Deterministic, but about the
   brief and decisions, not pixels: an assumption still untested on a frame marked ready; a
   decision made under a since-revised brief; a required element not found; declared states not
   present; a screen in a flow that no scenario visits. These keep purpose on the same page as
   craft, which is the point of chapter 1.
4. **Severity by consequence** for the person using the design (cannot complete, hindered,
   cosmetic), following chapter 6's emphasis on outcomes, not abstract high/medium/low.
5. **Dismiss with a reason.** Chapter 8's enlarged-faces bug is a case of a violation that was
   right (PDF 089–090). A person can mark a finding deliberate; that is a decision record.
   Agents cannot dismiss.
6. **Profile by maturity** (chapter 3), and say which rules did not run.
7. **No score, and no gamified totals.** Supported by the introduction's soft measures and
   chapter 6. Agree with the lead; extend it to comparisons in B.
8. **Opinion is allowed but fenced.** Agent or human judgment is a separate list with author and
   provenance, phrased against the brief, never merged into rule findings and never counted with
   them.
9. **"Missing states" is deterministic only against a declaration.** Without declared states it
   is a guess presented as a rule.

### What A–D leave out

- **Rules of engagement between person and agent** beyond comments (chapter 8): proposed versus
  accepted, who may dismiss, do not touch what was not asked.
- **Evidence provenance** as one vocabulary across C and D (chapter 5).
- **Maturity** of work (chapter 3).
- **Observation before making** as an agent habit (chapter 6); the tools exist, the guidance
  does not.
- **Hand-over** as part of done (chapter 10).
- **The existing prompts and instructions**, which teach agents the mockingbird's habit
  (chapter 1). Fixing them is the cheapest high-value item in the milestone.

## 7. Ranked backlog

Ranked by impact on the person, evidence of need (from the book's argument and from what the
repository lacks), cost, and architectural fit. Cost: S under a day, M a few days, L a week or
more of one agent, tests included; estimates, not measurements. Command names are suggestions;
the catalog and parity tooling (`crates/engine/src/catalog.rs`, `cargo xtask parity`) decide how
product-specific ids are registered.

| # | Item | Impact | Evidence of need | Cost | Fit | Milestone? | Reason |
|---|---|---|---|---|---|---|---|
| 1 | Prototype state machine moved into the engine; UI player calls it; equivalence test | High | Ch. 2, 7; today state lives in `ui-egui` | L | Good: engine already holds the pure pieces (`proto_run.rs`); keeps the UI thin | **Yes** | Without it C is a second, unverified implementation |
| 2 | Flow graph facts (`proto.check`): starts, broken targets, unreachable, terminal screens against declared ends | High | Ch. 7; nothing exists | S | Good: pure read over `interactions` and flows | **Yes** | Cheapest real verification |
| 3 | Agent guidance: rewritten instructions, rewritten creation prompts, new prompts for brief, alternatives, verify, critique | High | Ch. 1; current prompts are How-only | S | Good: `crates/mcp/src/prompts.rs`, `server.rs` | **Yes** | Highest value per hour; no model change |
| 4 | Brief in the document: plain questions, optional fields, two parties, assumptions with states, revisions; commands, MCP, panel, round trip | High | Ch. 1, 4, 5, 6, 8 | M | Good: one serde field on `Document`, commands, panel | **Yes** | Everything else steps back against it |
| 5 | Scenarios as story plus steps; headless run; per-step results; steps-to-goal | High | Ch. 2, 7, 9 | M (after 1) | Good | **Yes** | The person-readable half of C |
| 6 | Provenance vocabulary on findings, results, assumptions; agent cannot set "observed with people" | High | Ch. 5 | S | Good: one enum, used by 4, 5, 8 | **Yes** | Cross-cutting; costly to retrofit |
| 7 | Decision records: author, status, considered options, node links, staleness on brief revision | Medium–high | Ch. 1, 6, 8 | M | Good | **Yes** (minimal) | Needed by dismissals and by alternative choice |
| 8 | Deterministic checks with findings: contrast, text clipping, small targets, off-token values, declared-states, required elements, purpose-record lane; re-check; dismiss with reason | High | Ch. 1, 8, 9 (limits: no thresholds from the book) | M–L | Good: read-only over doc, layout, text | **Yes**, four or five rules | Start narrow; each rule cites its standard |
| 9 | Maturity marker and check profiles | Medium | Ch. 3 | S | Good | **Yes** | Small; prevents D from punishing exploration |
| 10 | Alternative sets: constraints first, original referenced, lever labels, parked state, fact table | Medium–high | Ch. 3, 4, 6 | M | Fair: new container concept beside sections and branches | **Yes**, minimal | Data, commands and fact table; no generator in the engine |
| 11 | Hand-over summary (bounded Markdown) | Medium | Ch. 10 | S | Good: mirrors `comment.prompt` | Deferred | Only useful after 4, 5, 8 land; first follow-up |
| 12 | Timed, drag-following and scrolling behaviour in the headless run | Medium | Ch. 7 | L | Fair | Deferred | Discrete triggers cover most scenarios; state the limit |
| 13 | Structural comparison of alternatives (order-of-information diff) | Medium | Ch. 6 | L | Fair | Deferred | Needs a tree diff the repo lacks |
| 14 | Person or agent attribution on every history step | Medium | Ch. 8 | L | Cross-cutting | Deferred | Records and comments cover authorship for now |
| 15 | Records of sessions with real people linked to assumptions | Medium | Ch. 5 | M | Good | Deferred | No evidence of need yet; label reserved in 6 |
| 16 | Message/tone/format controls; delight generator; beauty or design score; persona generator; "context = breakpoints" panel | — | Refused by ch. 2, 4, 6, 9 | — | — | Not suitable | Each is a reduction the source argues against |

If the milestone must shrink, cut from the bottom of the "Yes" rows: 10, then 9, then the rule
count in 8. Do not cut 1, 3 or 6.

## 8. Agent design guidance

Operational, checkable, in our own words. "Checkable" means a reviewer can decide from a session
transcript and the document whether it was followed. Tool names are today's or section 7's
suggestions.

| # | Behaviour | From |
|---|---|---|
| 1 | Before creating or restructuring a screen, read the brief. If the purpose of that screen is not stated, ask, or record your reading as an assumption marked untested and yours. Do not start from a template default. | Ch. 1 |
| 2 | On an existing document, look first: outline of the page, styles, variables, components, flows, open comments. Reuse what is there and say in one line what you found. | Ch. 6 |
| 3 | After each structural change, step back: take a screenshot and state in one or two sentences what the change does for the stated purpose. Do not report the list of tool calls as the result. | Ch. 1 |
| 4 | When you choose between structural options, record a decision: what, why, what else was considered, which nodes. Never write a reason you did not use. | Ch. 1 |
| 5 | Build on the person's work. Duplicate and transform; do not delete, replace or restyle a frame a person made unless asked. If you disagree, put a proposal beside it. | Ch. 3 |
| 6 | When asked for options, state the fixed constraints first (content, structure, tone), then make variants that differ in structure, order of information or form, and say what each changes and gives up. Colour or style changes alone are not options. | Ch. 3, 4 |
| 7 | Match criticism to maturity. On work marked exploring, report structural findings only and say that polish checks were not run. | Ch. 3 |
| 8 | Put message, tone and form into words before choosing visuals. For each tone word, name at least one concrete choice that carries it. Do not present a palette as a tone. | Ch. 4 |
| 9 | When a request names an artefact, restate the need behind it once. If a different form would serve the need better (fewer screens, another entry point), say so once, then follow the person's choice. | Ch. 4 |
| 10 | You stand in for the audience; you are not the audience. Never describe your review or a simulation as user research, user feedback or usability testing. Use the provenance labels as given. | Ch. 5 |
| 11 | Label what is made up: placeholder content, invented scenario detail, faked states. Never invent quotations, figures or study results. | Ch. 5 |
| 12 | Do not propose patterns that misrepresent the product to the person using it (disguised opt-outs, false urgency, hidden costs). If asked for one, state the concern once and record it as a decision for the person to accept. | Ch. 5 |
| 13 | Write each scenario as one specific person in one situation with a start and an end, then encode its steps. Check the places where time passes: first use, nothing yet, waiting, failure, return. | Ch. 7 |
| 14 | Verify by use. After wiring interactions, run the flow check and the scenarios. Do not say a flow works on the strength of a screenshot. Report failed and unrun scenarios as such. | Ch. 2, 7 |
| 15 | Contribute in small undoable units: one batch per intent. Name the node ids you changed. Touch nothing outside the request or the comment. | Ch. 8 |
| 16 | Your brief edits, decisions and alternatives are proposals until a person accepts them. Never accept your own proposal and never dismiss a finding. | Ch. 8 |
| 17 | When a person keeps something a rule flags, treat it as their decision: ask for the reason if none is recorded, do not "fix" it again. | Ch. 8 |
| 18 | Before adding anything for charm, check that error and empty states exist and whether a step can be removed; report steps-to-goal before and after. Do not add motion and call it delight. | Ch. 9 |
| 19 | Treat required elements (consent, permissions, errors, legal text) as part of the design. Do not hide them or shrink them below the legibility rule. | Ch. 9 |
| 20 | Spend the person's attention carefully: report findings grouped by consequence, most serious first, within a stated cap; do not repeat dismissed ones; do not volunteer a redesign. | Ch. 10 |
| 21 | Finish by handing over: the saved file reopens with brief, decisions and scenarios intact; your last message lists open assumptions, failed or unrun checks, and proposals awaiting acceptance. | Ch. 10 |
| 22 | Never produce a single quality score or rank variants by one number. Compare by facts and trade-offs. | Intro., Ch. 6 |

## 9. Traceability

| Chapter | Idea (author's, paraphrased) | Product decision | Where it will live | Implemented in |
|---|---|---|---|---|
| Foreword | Systems designed so others can add and orient themselves | Stable node ids anchor every new record | `crates/doc` | TBD (lead fills in) |
| Intro. | Soft measures; the plain question about living well; a map, not a blueprint | No aggregate design score | Findings and comparison schemas | TBD (lead fills in) |
| 1 | Reasons make choices discussable | Brief stored in the document | `crates/doc`, engine commands, MCP, panel | TBD (lead fills in) |
| 1 | The middle rungs deserve the conversation | Decision records linked to nodes | `crates/doc`, engine commands, MCP, panel | TBD (lead fills in) |
| 1 | Near and far modes in a tight loop | Step-back guidance; rewritten prompts and instructions | `crates/mcp/src/prompts.rs`, `server.rs` | TBD (lead fills in) |
| 1 | Craft failures are easier to see than purpose failures | Purpose-record lane in findings | Check command | TBD (lead fills in) |
| 2 | Useful first, then beautiful | Usefulness findings ordered before polish | Check command output | TBD (lead fills in) |
| 2 | Quality shows in use | "Verified" means run | Scenario run; guidance 14 | TBD (lead fills in) |
| 3 | Objectives set, put aside, brought back | Brief never gates work; no brief nagging while exploring | Brief panel; check profiles | TBD (lead fills in) |
| 3 | Limits before play; three kinds | Alternative sets require stated constraints | Alternative-set data and commands | TBD (lead fills in) |
| 3 | Accept and add | Original referenced, never edited; guidance 5 | Alternative-set commands; MCP guidance | TBD (lead fills in) |
| 3 | Criticism scaled to maturity | Maturity marker; rule profiles | `crates/doc`, check command | TBD (lead fills in) |
| 3 | Discarded ideas are material | Parked variants kept | Alternative-set data | TBD (lead fills in) |
| 4 | Message, tone, format; tone is not style | Brief fields in words; lever labels on variants; style-only fact | Brief; alternative-set comparison | TBD (lead fills in) |
| 4 | Re-examine assumptions; test them | Assumptions with states | Brief | TBD (lead fills in) |
| 4 | Need behind the requested form | Guidance 9 | MCP guidance | TBD (lead fills in) |
| 5 | Commissioner, audience, maker | Two-party fields | Brief | TBD (lead fills in) |
| 5 | Maker stands in for an absent audience | Provenance vocabulary; agent cannot claim observation | `crates/doc` enum; engine refusal; MCP guidance | TBD (lead fills in) |
| 5 | Useful untruths versus misrepresentation | Label assumed content; guidance 11, 12 | Scenario data; MCP guidance | TBD (lead fills in) |
| 6 | Observe, then respond | Guidance 2 (tools exist) | MCP guidance | TBD (lead fills in) |
| 6 | A problem is never fully known | Brief revisions; staleness marks | Brief; decision records | TBD (lead fills in) |
| 6 | One brief, several good answers | Fact table, no ranking | Alternative-set comparison | TBD (lead fills in) |
| 6 | Outcomes over objects | Severity by consequence | Check command | TBD (lead fills in) |
| 7 | Story is change over time | Scenario = story plus steps plus expectations | `crates/doc`, engine commands, MCP, panel | TBD (lead fills in) |
| 7 | Find where time passes | Declared states per screen; declared-states check | Brief or scenario data; check command | TBD (lead fills in) |
| 7, 2 | (Team inference) use must be reproducible to be checked | One state machine in the engine; headless run; flow graph facts | `crates/engine` (from `crates/ui-egui/src/present/`), `proto.*` commands | TBD (lead fills in) |
| 8 | Contributors see their effect; small and correctable | One undo unit per contribution (exists); re-check loop | Engine history; check command | TBD (lead fills in) |
| 8 | Rules of engagement are designed | Proposed and accepted; agents cannot accept or dismiss | Engine commands | TBD (lead fills in) |
| 8 | Jargon-free rules invite contribution | Plain-question brief form | Panel copy; MCP field docs | TBD (lead fills in) |
| 8 | A violation can be right | Dismiss with reason, kept as decision | Check command; decision records | TBD (lead fills in) |
| 9 | Delight is fit, not gimmick | No feature named for it; guidance 18 | Naming rule; MCP guidance | TBD (lead fills in) |
| 9 | Things going wrong are occasions | Error and empty among declared states | Declared-states check | TBD (lead fills in) |
| 9 | Requirements are material | Required elements in brief; check | Brief; check command | TBD (lead fills in) |
| 9 | Value from removing friction | Steps-to-goal fact | Scenario run | TBD (lead fills in) |
| 10 | Not finished until given | Round trip as definition of done; hand-over summary (deferred) | Tests; later command | TBD (lead fills in) |
| 10 | Attention is scarce | Bounded, grouped reports; guidance 20 | Check command output; MCP guidance | TBD (lead fills in) |
| 10 | Available in response | Guidance 20 (no unsolicited redesign) | MCP guidance | TBD (lead fills in) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | major | Created from a full reading of the text conversion of the book and the repository at `011f08e5`. Rendered pages and illustrations not viewed. |
