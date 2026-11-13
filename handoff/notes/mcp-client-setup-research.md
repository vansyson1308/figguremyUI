# MCP client setup research (2026-11-13, fetched live; pages show no dates)

## Claude Code — https://code.claude.com/docs/en/mcp (docs.claude.com/... redirects here); newest version seen referenced v2.1.317
- `claude mcp add [options] <name> -- <command> [args...]`; `--` separates Claude's options (--transport/-t, --env/-e KEY=value repeatable, --scope) from the server command.
- If the server name follows `--env` directly, the CLI reads it as another pair: put e.g. `--transport stdio` between.
- Scopes: local (default; this project, private; ~/.claude.json under the project path), project (.mcp.json at the project root, shared; needs approval in interactive sessions; `claude mcp reset-project-choices`), user (~/.claude.json, all projects). Precedence local > project > user > plugin > claude.ai connectors.
- .mcp.json: {"mcpServers": {"name": {"command": "...", "args": [...], "env": {}}}}; ${VAR} and ${VAR:-default} expand; per-server `timeout` (ms) for tools; `MCP_TIMEOUT=10000 claude` startup timeout.
- `claude mcp list` (health), `claude mcp get <name>`, `claude mcp remove <name>`, `/mcp` inside Claude Code.
- Windows: Claude Code closes a stdio server's stdin, ends the process tree after 300 ms. No cmd /c note found (last ~27k chars of the page unread).
- `claude mcp add-from-claude-desktop` only on macOS and WSL.

## Claude Desktop — https://modelcontextprotocol.io/docs/develop/connect-local-servers (MCP project guide, not Anthropic-hosted)
- Settings > Developer > Edit Config; restart fully after editing.
- macOS ~/Library/Application Support/Claude/claude_desktop_config.json; Windows %APPDATA%\Claude\claude_desktop_config.json; Linux ~/.config/Claude/claude_desktop_config.json. Absolute paths required. `env` object allowed.
- Logs: macOS ~/Library/Logs/Claude; Windows %APPDATA%\Claude\logs.

## Codex — https://developers.openai.com/codex/mcp (308 → https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
- `codex mcp add <server-name> --env VAR1=VALUE1 -- <stdio server-command>`; `codex mcp list`; `codex mcp --help`; `/mcp` in the TUI. remove/get not documented on the page.
- ~/.codex/config.toml (project: .codex/config.toml, trusted projects only):
  [mcp_servers.NAME] command (required), args, env (table), env_vars (list to forward), cwd, startup_timeout_sec (default 10), tool_timeout_sec (default 60), enabled = false to disable.
- "the ChatGPT desktop app, Codex CLI and IDE extension share this configuration".

## Cloud
- Claude Code on the web (https://code.claude.com/docs/en/claude-code-on-the-web): isolated Anthropic-managed VM, "separated from your machine"; nothing stated about reaching a local stdio server or localhost on the user's computer. Remote Control reaches sessions that run on your machine.
- Codex: "Local MCP servers and transports may not be available in the cloud; check connectivity and authentication separately." "ChatGPT web doesn't read local Codex configuration files."
