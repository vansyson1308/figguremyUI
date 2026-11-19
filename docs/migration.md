# From FigCraft to Figgure My UI: what changed in the rename

Figgure My UI is a fork of [FigCraft](https://github.com/storytold/figcraft), made by the ArtCraft
team and its contributors; it is not affiliated with or endorsed by them. The fork keeps
upstream's history, licences, copyright notices and contributor credits. Its owner approved the
rename on 2026-11-19. This page is for anyone who used FigCraft, or a build of this repository
from before the rename: what has a new name, what kept its name and why, what the app still
accepts under the old names, and what you have to do by hand.

In short: the product name, the binaries, the app id, the settings folder, the environment
variables, the invite-link scheme and the MCP names changed. Documents did not: a `.figcraft` file
opens in both apps.

## What changed

- **The name.** Windows, tabs, menus, About, messages, CLI help, the MCP server, the web page title
  and the packaging metadata say **Figgure My UI**. About says the app is based on FigCraft by the
  ArtCraft team and contributors, and keeps the contributors list.
- **The binaries** are `figguremyui` (desktop app), `figguremyui-cli` (command line and MCP server)
  and `figguremyui-server` (collaboration server).
- **The app id** is `io.github.vansyson1308.figguremyui`: desktop entry, AppStream metainfo, macOS
  bundle id, Flatpak id and the Linux icon files. The Windows installer has an upgrade code of its
  own, so it never upgrades or removes an installed FigCraft.
- **Settings** live in a `FiggureMyUI` folder of their own (table below), so the two apps never
  share or overwrite each other's settings.
- **Environment variables** are `FIGGUREMYUI_*`.
- **Invite links** use `figguremyui://`, and only that scheme is registered with the system, so
  the fork never takes over upstream's links.
- **The MCP server** introduces itself as `figguremyui` and its resources use `figguremyui://`.
- **Updates** are looked for in [vansyson1308/figguremyUI](https://github.com/vansyson1308/figguremyUI)'s
  releases, never upstream's. There are none yet, so the check finds nothing.
- **Online sharing** has no default server: it stays off until you enter one in Preferences
  (sharing on your local network works without one). Upstream's builds use upstream's public
  server; this fork sends nothing there.
- **The app icon** is Figgure My UI's own original icon. The ArtCraft name and logos are not used
  anywhere in the product or its documentation.

## What stayed, and why

| Kept | Why |
|---|---|
| Crate, package and directory names: `figcraft-*` / `figcraft_*` (`crates/engine` is still the `figcraft-engine` package, `apps/figcraft-cli` still builds with `-p figcraft-cli`) | They are internal identifiers nobody using the app sees. Renaming the 17 crates would touch thousands of paths and every open branch for no benefit to anyone using the app. `cargo run -p figcraft -- …` still works and runs the `figguremyui` binary; builds land in `target/debug/figguremyui`, `target/release/figguremyui-cli` and so on. |
| The native format: the `.figcraft` extension and the `figcraft` format discriminator inside the file | Files stay interchangeable with upstream, and every existing file opens. The file type shows as "Figgure My UI file (.figcraft)". |
| The MIME type `application/x-figcraft` | File associations, file managers and other tools that already know the type keep recognising the files. |
| `CRAFT_FONTS_DIR` | It is the shared craft-fonts build input, not a FigCraft name. |
| Identifiers other programs depend on: the plugin manifest file `figcraft-plugin.json`, the module name `figcraft` that plugins import from, widget data keys starting with `figcraft.`, `figcraft.connect.json`, the browser build's `window.figcraft` API, its `figcraft:` page events and its `figcraft-server` meta tag | Renaming them would break existing plugins, embeddings and pages; they are not product names people read in the app. |
| The collaboration protocol and the server's data directory format | They are not part of the rename: an existing server's data directory is read as before. |
| The deployment names in `packaging/server/` (the `figcraft` system user, `/etc/figcraft-server`, `/var/lib/figcraft-server`, the `figcraft-server` systemd unit and Caddy site, the `figcraft.example.com` placeholder) | Only the binary is renamed ([`server.md`](server.md#deploy-on-a-linux-server)); an existing deployment keeps its folders and units. |

## Old → new names

| What | FigCraft (old) | Figgure My UI (new) |
|---|---|---|
| Display name | FigCraft | Figgure My UI |
| Desktop app binary | `figcraft` (`figcraft.exe`) | `figguremyui` (`figguremyui.exe`) |
| Command line and MCP server | `figcraft-cli` | `figguremyui-cli` |
| Collaboration server | `figcraft-server` | `figguremyui-server` |
| App id | `ai.storyteller.figcraft` | `io.github.vansyson1308.figguremyui` |
| Environment variables | `FIGCRAFT_<NAME>` | `FIGGUREMYUI_<NAME>` (old name still read, [below](#environment-variables)) |
| Invite links | `figcraft://join?…` | `figguremyui://join?…` (old links still accepted) |
| Copied fills, strokes, effects and layers (clipboard marker) | `figcraft://clipboard/…` | `figguremyui://clipboard/…` (old marker still accepted on paste) |
| MCP `serverInfo` | `figcraft` | `name: "figguremyui"`, `title: "Figgure My UI"` |
| MCP server name in client configs (by convention) | `figcraft`, `figcraft-app` | `figguremyui`, `figguremyui-app` |
| MCP resources | `figcraft://document`, `figcraft://node/{id}`, `figcraft://command/{id}`, `figcraft://comments`, `figcraft://design`, `figcraft://design/findings`, `figcraft://prototype/flow`, `figcraft://prototype/scenarios` | `figguremyui://document`, `figguremyui://node/{id}`, `figguremyui://command/{id}`, `figguremyui://comments`, `figguremyui://design`, `figguremyui://design/findings`, `figguremyui://prototype/flow`, `figguremyui://prototype/scenarios` (old URIs still read) |
| Settings folder, Linux and FreeBSD | `$XDG_CONFIG_HOME/FigCraft` or `~/.config/FigCraft` | `$XDG_CONFIG_HOME/FiggureMyUI` or `~/.config/FiggureMyUI` |
| Settings folder, macOS | `~/Library/Application Support/FigCraft` | `~/Library/Application Support/FiggureMyUI` |
| Settings folder, Windows | `%APPDATA%\FigCraft` | `%APPDATA%\FiggureMyUI` |
| File type label | FigCraft file | Figgure My UI file (.figcraft) |
| Update source | upstream's releases | `vansyson1308/figguremyUI` releases (none published yet) |
| Default sharing server | upstream's public server | none |

A portable copy (a `portable` file beside the executable) keeps its settings in `config/` beside
the executable, as before.

### Environment variables

Every variable the app, the CLI, the server or the build reads under a `FIGCRAFT_` name now has a
`FIGGUREMYUI_` name: for example `FIGGUREMYUI_CONFIG_DIR`, `FIGGUREMYUI_NO_SYSTEM_FONTS`,
`FIGGUREMYUI_CA_FILE`, `FIGGUREMYUI_SERVER_KEY`, `FIGGUREMYUI_CONTROL_PORT`,
`FIGGUREMYUI_NO_DESKTOP_INTEGRATION`, `FIGGUREMYUI_TITLEBAR`, and at build time
`FIGGUREMYUI_DEFAULT_SERVER`, `FIGGUREMYUI_DEFAULT_FONT`, `FIGGUREMYUI_WEB_FONTS`,
`FIGGUREMYUI_EMBED_CJK`, `FIGGUREMYUI_EMBED_SCRIPTS`, `FIGGUREMYUI_BUILD_SHA` and
`FIGGUREMYUI_BUILD_DATE`, and in the packaging scripts `FIGGUREMYUI_VERSION`,
`FIGGUREMYUI_MAINTAINER` and `FIGGUREMYUI_WASM_MAX_BYTES`. The new name wins. When it is not set,
the old `FIGCRAFT_` name is still read, so existing scripts, services and build settings keep
working; move them to the new names when you can.

Two exceptions. `FIGGUREMYUI_GRAPHICS_ATTEMPT` is internal (the app sets it for itself when it
retries graphics start-up), so only the new name is read. The inputs of the test suites and
benchmarks keep their `FIGCRAFT_` names (`FIGCRAFT_FIG_SAMPLES`, `FIGCRAFT_PENPOT_SAMPLE`,
`FIGCRAFT_PDF_SAMPLE`, `FIGCRAFT_SKETCH_SAMPLES`, `FIGCRAFT_SKETCHTOOL`, `FIGCRAFT_EXPORT_BENCH_*`,
`FIGCRAFT_GLASS_PNG`, `FIGCRAFT_BENCH_LAYERS`, `FIGCRAFT_EXAMPLE_SHOTS`,
`FIGCRAFT_TEST_PIPE_FONT_DIR`): they are read only by developers' test runs, never by the app.

## What is still accepted under the old names

- **`FIGCRAFT_*` environment variables**, when the `FIGGUREMYUI_*` one is absent.
- **`figcraft://` invite links**, pasted into the app (the Join dialog, the home screen) or given on
  its command line. The system only opens `figguremyui://` links in Figgure My UI: a `figcraft://`
  link you click in a browser or a chat still goes to upstream FigCraft, if it is installed. Paste
  it instead.
- **`figcraft://clipboard/…`**: fills, strokes, effects and layers copied in an older build paste.
- **MCP**: `resources/read` takes the `figcraft://` form of every resource URI; `resources/list`
  lists only the `figguremyui://` ones.
- **Your preferences.** On the first start, when the Figgure My UI settings folder has no
  preferences file and upstream's `FigCraft` folder has one, the app copies it (never moves it)
  once, after checking that it reads as valid preferences. If it doesn't, the app starts with
  defaults and notes it in the log. The old folder is never written.
- **Every `.figcraft` file**, in both directions.

## What is not carried over

- **Recovery copies** stay in upstream's settings folder, where upstream FigCraft keeps offering
  them. Open that folder (table above) if you need one.
- **Installed plugins** stay in upstream's folder too: install them again in Figgure My UI
  (**Plugins ▸ Manage plugins…**, or from your server's plugin directory).
- **Anything else in the old folder** is left alone, and later changes to upstream's preferences
  are not copied again.
- **The system's link handler.** Turning on **Open invite links in Figgure My UI** registers only
  the new scheme; it doesn't remove upstream's registration.

## MCP

A client set up for upstream (or for a pre-rename build of this repository) runs `figcraft-cli`,
which no longer exists in a new build. Point it at the new binary: rebuild with
`cargo build --release -p figcraft-cli` (the package keeps its name) and use
`target/release/figguremyui-cli`.

- **Claude Code:** remove the old entry and add the new one (the server name is your choice;
  `figguremyui` by convention):

  ```bash
  claude mcp remove figcraft
  ```

  ```bash
  claude mcp add --transport stdio figguremyui -- /path/to/figguremyUI/target/release/figguremyui-cli mcp
  ```

- **A `.mcp.json`, `claude_desktop_config.json` or similar:** change the `command` path and,
  optionally, the server's key:

  ```json
  {
    "mcpServers": {
      "figguremyui": { "command": "/path/to/figguremyUI/target/release/figguremyui-cli", "args": ["mcp"] }
    }
  }
  ```

- **Codex** (`~/.codex/config.toml`): rename `[mcp_servers.figcraft]` to
  `[mcp_servers.figguremyui]` and set `command` to the new path.

The arguments (`mcp`, `--attach PORT`, `--timeout`, `--automation-read-root`,
`--automation-write-root`, `--no-system-fonts`), the tools, the prompts and the command ids are
unchanged. A prompt or script that reads `figcraft://document` keeps working; new ones should use
`figguremyui://document`. More in [`mcp.md`](mcp.md).

## Release workflow follow-up

The release workflows still pass upstream's names, and they are **not** changed with the rename:
CI stays shared, and a feature or rename pull request doesn't change it (house rule 11 in
[`AGENTS.md`](../AGENTS.md)). Until a separate CI pull request changes them, the fork's release
pipeline is not usable. It also needs the signing secrets of a `release` environment, which the
fork doesn't have ([`releasing.md`](releasing.md#secrets)); without them the builds come out
unsigned. These are the values to change:

| File | Input | Now | Change to |
|---|---|---|---|
| `.github/workflows/release.yml` | `app-name` | `figcraft` | `figguremyui` |
| `.github/workflows/release.yml` | `app-title` | `FigCraft` | `Figgure My UI` |
| `.github/workflows/windows-arm64.yml` | `app-name` | `figcraft` | `figguremyui` |
| `.github/workflows/windows-arm64.yml` | `app-dir` | `FigCraft` | `Figgure My UI` (the `INSTALLFOLDER` directory in `packaging/windows/figguremyui.wxs`) |
| `.github/workflows/windows-arm64.yml` | `install-relative-path` | `Programs/ArtCraft/FigCraft` | `Programs/ArtCraft/Figgure My UI` |

The Windows installer still puts the app in a suite folder named `ArtCraft`
(`Programs\ArtCraft\Figgure My UI` per user, `Program Files\ArtCraft\Figgure My UI` per machine),
because `packaging/windows/check-install-policy.ps1` requires the Craft suite's folder. Choosing a
folder of the fork's own is a decision to make before the first Windows release; if it changes,
`install-relative-path` changes with it. Check the values against the WiX source when making the
change, and that the shared workflows
in `storytold/craft-common-actions` accept the new values (`app-name` names the release files, so
they start with `figguremyui-` afterwards).
