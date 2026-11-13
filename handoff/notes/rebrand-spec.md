# Rebrand spec: FigCraft → Figgure My UI (lead's decisions)

Display name, exactly: **Figgure My UI**. Machine identifier: **`figguremyui`**. Fork repository:
`https://github.com/vansyson1308/figguremyUI`. The fork keeps upstream's history, licences, copyright
notices and contributor credits; it removes the old brand from what a person using the new product sees.

## What changes

1. **User-visible name.** Every user-visible "FigCraft" becomes "Figgure My UI": window and tab titles,
   About, menus, toasts, dialogs, error and status messages, CLI help and messages, MCP `serverInfo`
   (`name: "figguremyui"`, `title: "Figgure My UI"`) and instructions, the web build's page title and
   strings, packaging metadata, README and user documentation. In the UI catalogs the English source
   strings are the keys: change the `tr("…")` call, the `en.toml` key and value, and the key (and the brand
   word in the value) in every other locale, consistently, so the i18n tests still match. Brand names are
   not translated.
2. **Binaries.** `figcraft` → `figguremyui`, `figcraft-cli` → `figguremyui-cli`, `figcraft-server` →
   `figguremyui-server` (Cargo `[[bin]] name` / `default-run`). **Crate and package names stay `figcraft-*`**:
   they are internal identifiers; renaming 17 crates touches thousands of paths for no user benefit, and is
   recorded as a decision in the migration note. Update `CARGO_BIN_EXE_*` uses, xtask, packaging scripts and
   docs that name the binaries.
3. **App identity.** App id `io.github.vansyson1308.figguremyui` (desktop entry, metainfo, macOS bundle id,
   Flatpak id, Windows installer product name and upgrade code: a NEW upgrade code so the installer never
   upgrades or removes an installed upstream FigCraft). Linux icon theme files renamed to the new id.
4. **Native file format: unchanged.** `.figcraft` stays the native extension and `figcraft` stays the format
   discriminator (`crates/format/src/lib.rs` `EXTENSION`, `FORMAT`): files are interchangeable with upstream
   and every existing file opens. The file-type label shown to people becomes "Figgure My UI file (.figcraft)".
   The MIME type id is kept for compatibility.
5. **URL scheme for invite links.** New links use `figguremyui://`; links with the old `figcraft://` scheme are
   still accepted when pasted or opened from inside the app. Desktop integration registers only the new
   scheme (so the fork never takes over upstream's links).
6. **MCP resource URIs.** `figguremyui://document`, `…/node/{id}`, `…/command/{id}`, `…/comments`,
   `…/design`, `…/design/findings`, `…/prototype/flow`, `…/prototype/scenarios`. `resources/read` also accepts
   the `figcraft://` form of each (alias, tested). `resources/list` lists only the new ones.
7. **Environment variables.** Every runtime variable `FIGCRAFT_X` gets the name `FIGGUREMYUI_X`; the new name
   wins; the old name is still read when the new one is absent (one helper, tested). Build-time
   `option_env!` inputs likewise accept both. Docs list the new names and mention the old ones once as
   still accepted.
8. **Settings and recovery folders.** The app's settings folder follows the new app id, so it never shares or
   overwrites upstream FigCraft's settings. On first start, when the new settings file does not exist and
   upstream's does, copy (never move) the preferences file once, after checking it parses as valid
   preferences; if it does not parse, start with defaults and leave a note in the log. Never write into the
   old folder. Recovery copies of the old app are not moved (upstream keeps offering them); say so in the
   migration note. Tests with a temporary settings root.
9. **Updates.** `craft_update` `App` for the fork: `id "figguremyui"`, `name "Figgure My UI"`, `repo
   "vansyson1308/figguremyUI"`, `developer "Figgure My UI contributors"`, `bundle_id_override` = the app id,
   no `former_ids` (the fork must never download upstream's assets). The fork publishes no releases yet: the
   check finds none; that is fine and must not show an error to the person (verify what the prompt shows).
10. **Online services.** The default collaboration/sharing server and any default plugin-directory or
    library server that points at upstream's service (`wss://collab.craftapps.dev/` etc.) becomes **none**
    (`FIGGUREMYUI_DEFAULT_SERVER` unset → empty): online sharing stays off until the person enters a server
    in Preferences. The fork must not send files to upstream's service by default. Fix tests that assumed
    a non-empty default; keep the behaviour of an explicitly configured server.
11. **Icon and brand colour.** New original icon (a draft is in
    `scratchpad/icon/figguremyui.svg`: a navy tile, a cream outline "screen" card with two text lines and an
    orange card with a check mark — "design and verify"). Refine it if needed, keep it simple and original.
    Master `assets/app-icon/figguremyui.svg` (+ `figguremyui-small.svg`), regenerate all derived files
    (`cargo xtask icon` needs `resvg`: `cargo install resvg --locked` is allowed; otherwise render with the
    pre-installed Playwright Chromium and write the `.ico` with the code xtask uses). Remove the chameleon
    files from the app (they stay in git history; ATTRIBUTION gets a row for the new icon: original work for
    Figgure My UI, MIT OR Apache-2.0, and the chameleon row goes). Update `assets/app-icon/README.md`. The
    brand colour used in the app chrome (home screen tiles etc., where the FigCraft green `#02da96` was used)
    becomes the new brand colour; any text drawn on it must reach 4.5:1 (measure; pick a darker shade for
    text backgrounds if needed).
12. **ArtCraft marks.** Their licence (`docs/brand/LICENSE-brand.txt`) permits use only within FigCraft. The
    product, README and docs of Figgure My UI must not show them. Remove `docs/brand/` from the fork together
    with its ATTRIBUTION rows (history keeps them), and any link to getartcraft.com / the ArtCraft Discord in
    the app and README, except the attribution sentence below.
13. **Attribution.** `NOTICE` keeps its content and adds: "Figgure My UI is a fork of FigCraft
    (https://github.com/storytold/figcraft), made by the ArtCraft team and its contributors. Figgure My UI is
    not affiliated with or endorsed by them." The About window says "Figgure My UI — design and verify
    interactive prototypes with people and AI agents. Based on FigCraft by the ArtCraft team and
    contributors." and keeps the contributors list. LICENSE files unchanged. `contributors/` untouched.
14. **Docs.** README is rewritten separately (another task) — leave README.md alone except removing ArtCraft
    marks if the README task has not run. Other docs: replace the product name where it names this product;
    leave it where it names the upstream project historically (revision-history rows may stay as written).
    Add `docs/migration.md`: what changed, what stayed (crate names, file format, MIME type), old → new names
    (binaries, env vars, URL scheme, resource URIs, settings folder), what is still accepted, and what is not
    migrated.
15. **AGENTS.md.** Edit only the repository-specific part (from the line `# FigCraft — instructions for
    agents` to the end); the shared house-rules blocks above it are checked by CI against their source and
    must stay byte-identical. Update the product name, fork relationship, the icon/brand rules (the ArtCraft
    marks rule becomes "do not use them"), binary names, env vars, and keep every quality/data/architecture
    rule.
16. **Guard against the old name coming back.** `cargo xtask brand` (added to `cargo xtask ci` after
    `assets`): fails on the word `FigCraft` (any case of that exact word, also `Figcraft`) in user-visible text:
    every value and key of `crates/ui-egui/locales/*.toml`, Rust string literals in non-test source under
    `apps/` and `crates/*/src/` (skip `*_tests.rs`, `tests/`, `#[cfg(test)]` modules if feasible, else files
    named `*tests*`), `apps/figcraft-web/index.html` and other web assets, `packaging/**` metadata text,
    README.md and `docs/*.md` except an allowlist. Allowed, each with the reason printed: `NOTICE`,
    `LICENSE-*`, `ATTRIBUTION.md`, `docs/migration.md`, `docs/design/book-to-product.md` (research note
    written before the rename), revision-history table rows, lines carrying the marker `brand: compat`
    (old env var names, the `figcraft://` alias, the format discriminator) or `brand: attribution`. Lowercase
    machine identifiers `figcraft-*`/`figcraft_*` (crate names) and `.figcraft` (file extension) are allowed.
    Print each hit as `file:line: text`. Tests for the scanner itself in xtask.

## Verification required from the rebrand agent
- Whole workspace: `cargo xtask ci` (fmt, clippy -D warnings, tests, i18n report, layers, assets, docs, brand,
  wasm incl. plugins) or every part of it run separately, with real output.
- Start the real app (`scratchpad/tools/run_app.sh <port> target/debug/figguremyui`), screenshot Home, an
  open document, About, and confirm title/menus/About read "Figgure My UI" with the new icon (read the PNGs).
- `figguremyui-cli mcp` initialize → `serverInfo` and instructions; `resources/read` on an old URI works.
- Open every `examples/*.figcraft` and a file saved by the baseline binary
  (`scratchpad/baseline/bin/figcraft-cli-release-011f08e5` can create one) and compare.
- Env var alias test; settings migration test; updater shows no error with no releases.
