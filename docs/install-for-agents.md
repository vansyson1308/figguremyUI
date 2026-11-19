# Installing Figgure My UI: a runbook for AI agents

This page is written for an AI coding agent (Claude Code, Codex or another MCP client) that a person
asked to install Figgure My UI and connect it. Follow it top to bottom, literally. Each step says
what to run, what you should see, and what to do when you don't.

**What was run.** Every command below that isn't marked *not run here* was run on Linux (Ubuntu
24.04, x86_64, Rust 1.97, Claude Code 2.1.328) on 2026-11-19, in a fresh clone, and did what this
page says. Commands marked *not run here* are for macOS or Windows, or need something this machine
didn't have (a display for the desktop app's window, the Codex CLI); they come from the vendors' or
the tools' documentation and the repository's README.

**What you install.** There are no release downloads of this fork yet, and no installers: you build
from source. Two binaries matter:

| Binary | Built by | What it is for |
|---|---|---|
| `figguremyui-cli` | `-p figcraft-cli` | The command line and the MCP server. Enough for an agent working **headless** (no window, nobody watching live). |
| `figguremyui` | `-p figcraft` | The desktop app. Needed for **attached** mode, where a person sees every change and can edit alongside you, and to play prototypes in a window. |

The Cargo packages keep their old `figcraft-*` names (`-p figcraft-cli` builds `figguremyui-cli`).
If the person did not say which they want, build both: the app costs one more build, and attached
mode is how a person follows your work.

When a Windows installer exists, it will put the app in `Programs\ArtCraft\Figgure My UI` under
the user's local app data (the installer policy this repository inherits, see
[`migration.md`](migration.md)). There are no fork installers yet, so use the from-source path
below on Windows too.

## 1. Check the prerequisites

Run these from any folder.

```sh
git --version
rustc --version
cargo --version
```

- `git` prints a version. If it is missing, ask the person to install Git.
- `rustc` must be **1.95 or newer** (the workspace's `rust-version`). Older: `rustup update stable`
  (*not run here*; this machine had 1.97). Missing: the person installs Rust from
  <https://rustup.rs>; don't pipe an installer into a shell without their go-ahead.
- A C toolchain for linking: on Linux `cc --version` prints a version (Debian/Ubuntu:
  `build-essential`); on macOS, `xcode-select --install` (*not run here*); on Windows, the Visual
  Studio Build Tools with the C++ workload that rustup asks for (*not run here*).

```sh
cc --version
df -h .
```

- **Disk:** a release build of both binaries took **2.9 GB** in `target/` here (the CLI alone
  **1.1 GB**), plus about **0.8 GB** of downloaded sources in Cargo's cache (`~/.cargo`). With
  less free space than that plus a margin, build the CLI only, or point `CARGO_TARGET_DIR` at a
  disk with room.
- **Network:** the build fetches crates from crates.io and three Git dependencies from
  github.com (`storytold/craft-libs`, `storytold/egui`, `storytold/winit`). Behind a proxy, Cargo
  and Git must be able to use it.

**Linux, desktop app only.** The headless CLI needs no system packages beyond the C toolchain. The
desktop app loads these libraries when it starts (not when it builds):

```sh
ldconfig -p | grep -E 'libxkbcommon-x11\.so|libwayland-client\.so|libGL\.so|libEGL\.so'
```

It should print a line for each of the four. If one of `libxkbcommon-x11`, `libwayland-client`,
`libGL` or `libEGL` is missing (here only `libwayland-client` and `libGL` were there), ask the person
to install them; on Debian/Ubuntu:
`sudo apt-get install -y libxkbcommon-x11-0 libwayland-client0 libgl1 libegl1` (*not run here*;
it needs root and changes the system). macOS and Windows need nothing extra.

**Fonts.** Text is laid out with the fonts installed on the computer. The examples are set in
[Inter](https://rsms.me/inter/); without it they fall back to another font (doctor warns). On a
machine with no fonts at all (a bare container), text draws nothing, and doctor fails its `fonts`
check: install any font package (Debian/Ubuntu: `fonts-dejavu-core`).

## 2. Get the source

```sh
git clone https://github.com/vansyson1308/figguremyUI
cd figguremyUI
```

If the person already has a clone, `cd` into it and run `git pull` instead (section 10). (Here the
steps below were run in a fresh clone of the branch that added `doctor`; the GitHub address was
checked to answer with `git ls-remote`.)

## 3. Build

Headless only (the MCP server and command line):

```sh
cargo build --release -p figcraft-cli
```

With the desktop app as well:

```sh
cargo build --release -p figcraft -p figcraft-cli
```

The binaries land in `target/release/`: `figguremyui-cli` and `figguremyui` (`.exe` on Windows).
A first build takes a while (here, on 4 cores with the sources already downloaded: **12 min** for
the CLI, then **9 min** more for the app); later builds reuse it. The warning
`craft-fonts fonts/manifest.txt skipped` is expected: the optional
[craft-fonts](https://github.com/storytold/craft-fonts) checkout next to the repository would build
fonts into the binaries, and everything works without it.

```sh
target/release/figguremyui --version
```

prints `Figgure My UI 0.18.1` (or the version you built), without opening a window.

The build failed? See [Troubleshooting](#11-troubleshooting).

## 4. Check the install with `doctor`

```sh
target/release/figguremyui-cli doctor --example examples/signup-flow.figcraft
```

On Windows (*not run here*): `target\release\figguremyui-cli.exe doctor --example examples\signup-flow.figcraft`.

`doctor` checks the install end to end, in its own process: it runs the MCP server's handshake,
lists the tools, asks for the capabilities, builds a small prototype and renders it, checks its flow
and runs its scenario, opens the example and runs the example's stored scenarios, and counts the
fonts. It writes no files, changes no client configuration and uses no network. Then it prints the
setup lines with the real path of the binary. Its checks here (the clone's folder written as
`/path/to/figguremyUI`):

```text
Checks
  ok    binary                   /path/to/figguremyUI/target/release/figguremyui-cli
  ok    mcp.initialize           serverInfo.name "figguremyui", protocol 2025-06-18
  ok    mcp.tools                26 tools
  ok    mcp.capabilities         headless backend, 688 commands, flow check and scenarios
  ok    mcp.document             built a two-screen prototype with batch (in memory, not saved)
  ok    mcp.screenshot           rendered a 240×160 PNG
  ok    mcp.check_prototype      2 screens, no blocker or major problem
  ok    mcp.run_scenarios        1 of 1 scenarios passed
  ok    example.open             /path/to/figguremyUI/examples/signup-flow.figcraft
  ok    example.scenarios        6 of 6 scenarios passed
  ok    example.check_prototype  no blocker or major problem (0 minor)
  ok    fonts                    27 families: 0 built in, 27 installed
```

then the setup lines for Claude Code, Codex and `mcpServers` JSON, headless and attached, and
`Result: all 12 checks passed.` The path is canonical: links in it are resolved.

- **Exit code 0** and `Result: all … checks passed`: the install works. Go on.
- **Exit code 1**: a line starting with `FAIL` names the check, and the `fix:` line under it says
  what to do. Do that, then run doctor again. [Troubleshooting](#doctor-failures) lists them.

For a machine-readable answer add `--json`: one object whose keys are documented in
[`cli.md`](cli.md#doctor---json---example-file---port-port). The fields you need:

```sh
target/release/figguremyui-cli doctor --json > "${TMPDIR:-/tmp}/figguremyui-doctor.json"; echo "exit $?"
jq '{ok, failed: [.checks[] | select(.ok | not)], claude: .setup.headless.claudeCode, codex: .setup.headless.codex}' "${TMPDIR:-/tmp}/figguremyui-doctor.json"
```

(`jq` is optional; any JSON parser reads it.) `--port PORT` changes the port in the attached-mode
lines (default 7979).

## 5. Optional: install the binary on your PATH

Building leaves the binaries in the clone, and the client setup can point there. To have
`figguremyui-cli` on the `PATH` instead (and keep working after the clone is deleted):

```sh
CARGO_TARGET_DIR=target cargo install --locked --path apps/figcraft-cli
```

`CARGO_TARGET_DIR=target` reuses the build from section 3 (here it finished in under a second);
without it, `cargo install` builds again from scratch in a temporary folder. The warnings that some
packages in `Cargo.lock` are *yanked* are expected with `--locked`: it keeps the versions the
repository was tested with. On Windows PowerShell (*not run here*):
`$env:CARGO_TARGET_DIR="target"; cargo install --locked --path apps/figcraft-cli`.

It lands in Cargo's `bin` folder: `~/.cargo/bin/figguremyui-cli` (`$CARGO_HOME/bin` when
`CARGO_HOME` is set; `%USERPROFILE%\.cargo\bin\figguremyui-cli.exe` on Windows, *not run here*).
`--root DIR` puts it in `DIR/bin` instead (here it was run with `--root` and a temporary folder,
so as not to change this machine's `~/.cargo/bin`). Check it, and run doctor **from the copy you
will register**, so the lines it prints carry that copy's path:

```sh
command -v figguremyui-cli
figguremyui-cli doctor
```

(PowerShell, *not run here*: `Get-Command figguremyui-cli`.) If `command -v` prints nothing, Cargo's
`bin` folder is not on the `PATH`: use the full path doctor would print, or ask the person to add it.

The desktop app installs the same way, `CARGO_TARGET_DIR=target cargo install --locked --path
apps/figcraft` (*not run here*); then `figguremyui` sits next to the CLI, and doctor says so.

## 6. Connect your client

Copy the lines **doctor printed** (they carry the real absolute path; the paths below are
placeholders). Choose the mode:

- **Headless** (`… mcp`): an engine session of your own, no window. Good for building and checking
  files; nobody sees it live. Verify flows with the simulator tools.
- **Attached** (`… mcp --attach 7979`): drives the app the person started with
  `figguremyui --control 7979` (doctor prints that line with the app's path). They see every change
  as you make it and can edit at the same time. Start the app before your client connects, in the
  person's desktop session (*not run here*: this machine has no display; the attached server's
  "can't reach the app" error below was run).

**Claude Code** (from the folder you work in):

```sh
claude mcp add --transport stdio figguremyui -- /abs/path/to/figguremyui-cli mcp
```

This was run here with the line doctor printed. Without `--scope` it is added for this folder only
(Claude Code's local scope). Add `--scope user` to have it in every project. `--scope project` writes
the project's `.mcp.json` for everyone who clones it: ask the person first. (The scopes are from the
README and were not run here.)

**Codex** (*not run here*: Codex wasn't installed):

```sh
codex mcp add figguremyui -- /abs/path/to/figguremyui-cli mcp
```

or the `[mcp_servers.figguremyui]` table in `~/.codex/config.toml` shown in the
[README](../README.md#codex).

**Any client that takes `mcpServers` JSON** (a project's `.mcp.json`, Claude Desktop's
`claude_desktop_config.json`): paste the snippet doctor printed.

The commands come from the [Claude Code](https://code.claude.com/docs/en/mcp) and
[Codex](https://developers.openai.com/codex/mcp) MCP documentation (checked 2026-11-13), as in the
README. A client that runs in the cloud (a web or remote session) runs on its own machine: it can't
start a binary on the person's computer.

## 7. Confirm the client sees it

```sh
claude mcp list
```

or `codex mcp list` (*not run here*: Codex wasn't installed), or `/mcp` inside either client.
`claude mcp list` starts the server to check it; here (Claude Code 2.1.328, after the
`claude mcp add` line doctor printed) it answered
`figguremyui: /…/target/release/figguremyui-cli mcp - √ Connected`. A running client session may
need a restart (or a new session) before the new tools appear.

## 8. First useful calls

Once the tools are there, start the way [`mcp.md`](mcp.md#design-context-and-verification)
describes. In short:

1. `get_capabilities`: which backend (headless or attached), whether there is a window, what this
   build can check, which document your calls act on.
2. `design_context`: read the brief. If it doesn't say who the design is for and what they need to
   do, ask the person or record your reading as an assumption.
3. Read outline first: `inspect_document {"outline": true}`, then `get_node` for detail on the
   layers you need, and `screenshot` to look.
4. Make small, undoable changes: one `batch` per intent.
5. Verify by running: `check_prototype`, `run_scenarios` (a `total` of 0 is nothing run, not a
   pass), `simulate_prototype`; then `check_design`.
6. `save_file`, and tell the person what is still open: untested assumptions, proposals waiting for
   them, checks that didn't run, what was simulated rather than seen.

To see the server answer without a client, from a POSIX shell:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"open_file","arguments":{"path":"examples/signup-flow.figcraft"}}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"run_scenarios","arguments":{}}}' \
  | target/release/figguremyui-cli mcp
```

The server logs one line to stderr and answers each request with one JSON line on stdout: the first
names `serverInfo` `figguremyui`, and the third's text block is the scenario report, with
`"passed": true`, `"total": 6` and `"failed": 0` for the sign-up example (escaped inside the JSON
string). In PowerShell (*not run here*), pipe the same lines with
`'line1','line2','line3' | target\release\figguremyui-cli.exe mcp`.

## 9. Safe defaults

- The app's control channel listens on **127.0.0.1 only**; `mcp --attach` connects there.
- To keep yourself to certain folders, start the headless server with the automation roots (each
  repeatable; the folders must exist). Reads are allowed inside a read or write root, writes only
  inside a write root:

  ```sh
  target/release/figguremyui-cli mcp --automation-read-root /work/project --automation-write-root /work/project/out
  ```

  Use folders that exist (here it was run with two scratch folders): the server says on stderr
  `file access confined to {"read":[…],"write":[…]}`, and `initialize`'s instructions name them.
  In a client config these go after `mcp` in the args. Attached, the **app** confines the calls:
  the person starts it as `figguremyui --control 7979 --automation-read-root DIR
  --automation-write-root DIR` (*not run here*); `mcp --attach` refuses the flags (exit 2, run
  here). Details:
  [`mcp.md`](mcp.md#confining-file-access).
- Confined calls reach no server: sharing, live sessions and plugin directories are refused.
- Changes in attached mode are your own undo steps; pass `ifRevision` so you don't overwrite what
  the person just did ([`mcp.md`](mcp.md#working-document-revisions-and-conflicts)).

## 10. Update and uninstall

**Update:** in the clone,

```sh
git pull
cargo build --release -p figcraft-cli
target/release/figguremyui-cli doctor
```

(add `-p figcraft` if you built the app). The binary's path doesn't change, so the client setup
stays valid; restart the client session to use the new build. If you installed with
`cargo install`, run the same `CARGO_TARGET_DIR=target cargo install --locked --path
apps/figcraft-cli` again: it prints `Replacing …` and replaces the installed copy.

**Uninstall:**

1. Remove the server from the client: `claude mcp remove figguremyui` (and `figguremyui-app` if
   you added it; give the same `--scope` you added it with). For Codex, delete the
   `[mcp_servers.figguremyui]` table from `~/.codex/config.toml`, or use the remove subcommand
   `codex mcp --help` lists (*not run here*). For a pasted JSON snippet, delete its entry.
2. Installed copies: `cargo uninstall figcraft-cli` (and `cargo uninstall figcraft` for the app;
   add `--root DIR` if you installed with one). Cargo names the package, not the binary.
3. The clone: delete its folder (with its `target/`). Ask the person first; it may hold their work.
4. The app's settings, if the app ran: `~/.config/FiggureMyUI` (or `$XDG_CONFIG_HOME/FiggureMyUI`)
   on Linux and FreeBSD, `~/Library/Application Support/FiggureMyUI` on macOS,
   `%APPDATA%\FiggureMyUI` on Windows. Ask before deleting: it holds the person's preferences and
   recovery copies.

## 11. Troubleshooting

### The build fails

The messages are as Cargo and rustc word them; none of these failures happened here, so the fixes
were not run here.

| What you see | What to do |
|---|---|
| `No space left on device` | Free space, or build in another place: `cargo clean`, then `CARGO_TARGET_DIR=/path/with/room cargo build --release -p figcraft-cli`. Building the CLI alone needs less. |
| ``linker `cc` not found`` (or `link.exe` on Windows) | Install the C toolchain from section 1. |
| `requires rustc 1.95 or newer` | `rustup update stable`, then build again. |
| A Git dependency fails to fetch (`failed to fetch` … github.com) | The network or proxy blocks github.com; Cargo needs it for `storytold/craft-libs`, `storytold/egui` and `storytold/winit`. |
| Killed while compiling (out of memory) | Build with fewer jobs: `cargo build --release -j 2 -p figcraft-cli`. |

### Doctor failures

| Check | Meaning | What to do |
|---|---|---|
| `error: unrecognized subcommand 'doctor'` (exit 2) | The checkout is older than `doctor` | `git pull` and build again; on a branch without it, check the install with the shell test in section 8. |
| `binary` | The binary can't find its own path | Run it by its full path. |
| `mcp.*` | The MCP server itself misbehaves | Rebuild from a clean checkout; if it persists, report it with doctor's output. |
| `example.open` | The `--example` file isn't there | Run doctor from the repository's root, or give the full path. |
| `example.scenarios`, `example.check_prototype` | The example file is not as shipped | `git checkout -- examples/signup-flow.figcraft`. |
| `fonts` | No font at all: text would draw nothing | Install a font package, or drop `--no-system-fonts` / `FIGGUREMYUI_NO_SYSTEM_FONTS`. |

Warnings don't fail the run: the desktop app not next to the CLI (only attached mode needs it), and
Inter not installed (the examples fall back to another font).

### Headless or attached?

- `can't reach the Figgure My UI app at 127.0.0.1:7979` from an attached server: the app isn't
  running with `--control 7979`, or on another port. Start it (`target/release/figguremyui --control
  7979`, *not run here*: this machine has no display), or use the headless line.
- The person doesn't see your changes: you are headless. Switch to the attached line.
- `screenshot {window: true}` and playing a prototype in a window need the app (attached); headless,
  `screenshot` renders the document and `simulate_prototype` plays it.
- Attached calls time out on big batches: add `--timeout 300` after `--attach 7979`.

### The client doesn't list the tools

Check that the path in the client config is the absolute path doctor printed and that the file is
there (`ls -l /abs/path/to/figguremyui-cli`), restart the client session, and run
`claude mcp list` / `codex mcp list` again. A path with spaces must stay quoted as doctor printed it.
