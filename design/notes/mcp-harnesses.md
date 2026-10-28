# MCP harnesses: where each one keeps its MCP server config

Research note for `red-ale setup`, which will register red-ale's MCP server (`red-ale mcp`, stdio) with every agent
harness installed on the user's machine. It describes other tools, not red-ale. Researched 2026-10-28.

**Method.** Facts come from official documentation pages and, where docs are thin or the tool is open source, from
the tool's source at a named tag or commit. Some behaviour was tested on this machine: Claude Code 2.1.312, codex-cli
0.164.0, opencode 1.19.4 and VS Code 1.143.1. Those tests used throwaway config directories (`CLAUDE_CONFIG_DIR`,
`CODEX_HOME`) and never touched a real config. Claude Desktop 4.29.0 was read from Anthropic's Linux `.deb`, unpacked
but not installed; its code is minified, so it is cited by function name. Closed-source CLIs (Amp, Cursor CLI) were
read with `strings` or by grepping the bundle, never run.

**Status labels.** **Unconfirmed** means no official doc or source states it. **Source-derived** means the source code
says so but the docs don't. **Tested** means it was seen on this machine.

Formats change fast in this area. Re-check each harness before relying on a detail. Every section below names the
version it describes.

---

## Summary

`~` is `%USERPROFILE%` on Windows unless stated otherwise. "User" is the per-user (global) scope. Project scopes are
listed in each section.

| # | Harness | User config: Linux | macOS | Windows | Format | Key | Own CLI to add |
|---|---|---|---|---|---|---|---|
| 1 | Claude Code | `~/.claude.json` (top-level `mcpServers`; moves to `$CLAUDE_CONFIG_DIR/.claude.json`) | same | same | JSON | `mcpServers` | `claude mcp add -s user red-ale -- <abs> mcp` |
| 2 | Codex CLI + IDE ext. + ChatGPT app | `$CODEX_HOME/config.toml`, default `~/.codex/config.toml` | same | same | TOML | `[mcp_servers.<name>]` | `codex mcp add red-ale -- <abs> mcp` (global only) |
| 3 | opencode V1 / V2 | `$XDG_CONFIG_HOME/opencode/opencode.json[c]`, default `~/.config/opencode/` | same (`~/.config`) | `%USERPROFILE%\.config\opencode\` | JSONC | V1 `mcp.<name>`; V2 `mcp.servers.<name>` | V1 `opencode mcp add red-ale -- <abs> mcp`; V2 adds `--global`. Neither has remove |
| 4 | Gemini CLI (Enterprise only now) | `~/.gemini/settings.json` (or `$GEMINI_CLI_HOME/.gemini/`) | same | same | JSON with comments | `mcpServers` | `gemini mcp add -s user red-ale <abs> mcp` |
| 5 | Claude Desktop | `$XDG_CONFIG_HOME/Claude/claude_desktop_config.json`, default `~/.config/Claude/` (beta; source-derived) | `~/Library/Application Support/Claude/claude_desktop_config.json` | `%APPDATA%\Claude\…`, but MSIX: `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude\…` | strict JSON | `mcpServers` | none |
| 6 | Cursor (IDE + `agent` CLI) | `~/.cursor/mcp.json` | same | same (unconfirmed in docs) | JSON | `mcpServers` | none (`agent mcp` has no `add`); deeplink |
| 7 | VS Code + Copilot | `~/.config/Code/User/mcp.json` | `~/Library/Application Support/Code/User/mcp.json` | `%APPDATA%\Code\User\mcp.json` | JSONC | `servers` | `code --add-mcp '<json>'` (default profile only) |
| 8 | Windsurf → Devin Desktop / Devin CLI | `$XDG_CONFIG_HOME/devin/mcp_config.json`, default `~/.config/devin/` | `~/.config/devin/mcp_config.json` | `%APPDATA%\devin\mcp_config.json` | JSON (comments probably OK) | `mcpServers` | `devin mcp add -s user red-ale -- <abs> mcp` |
| 9 | Zed | `$XDG_CONFIG_HOME/zed/settings.json`, default `~/.config/zed/` | `~/.config/zed/settings.json` | `%APPDATA%\Zed\settings.json` | JSONC | `context_servers` | none |
| 10 | GitHub Copilot CLI | `$COPILOT_HOME/mcp-config.json`, default `~/.copilot/mcp-config.json` | same | same | JSON | `mcpServers` | `copilot mcp add red-ale -- <abs> mcp` (user only) |
| 11 | Goose (CLI + Desktop) | `$XDG_CONFIG_HOME/goose/config.yaml`, default `~/.config/goose/` | `~/.config/goose/config.yaml` | `%APPDATA%\Block\goose\config\config.yaml` | YAML | `extensions` | none (interactive `goose configure` only) |
| 12 | Cline ≥ 4.0 (VS Code, CLI, JetBrains) | `~/.cline/data/settings/cline_mcp_settings.json` | same | same | strict JSON | `mcpServers` | `cline mcp add red-ale --yes -- <abs> mcp` |
| 13 | Kiro (IDE + `kiro-cli`) | `~/.kiro/settings/mcp.json` (or `$KIRO_HOME`) | same | same (unconfirmed) | JSON | `mcpServers` | `kiro-cli mcp add --name red-ale --scope global --command <abs> --args mcp` |
| 14 | Amp (CLI only now) | `~/.config/amp/settings.json[c]` (or `$AMP_SETTINGS_FILE`) | same | `%USERPROFILE%\.config\amp\settings.json` | JSONC | `"amp.mcpServers"` | `amp mcp add red-ale -- <abs> mcp` |
| 15a | JetBrains AI Assistant | IDE options dir, undocumented XML | — | — | unknown (XML) | — | none: paste JSON in the UI, or "Import from Claude" |
| 15b | JetBrains Junie (plugin + CLI) | `~/.junie/mcp/mcp.json` | same | same (unconfirmed) | JSON | `mcpServers` | none (interactive `/mcp` only) |
| + | Google Antigravity (IDE + `agy`) | `~/.gemini/config/mcp_config.json` | same | same (unconfirmed) | JSON | `mcpServers` | `agy mcp add` (changelog; syntax unconfirmed) |
| + | Visual Studio 2022 17.14+ / 2026 | — | — | `%USERPROFILE%\.mcp.json` | JSON | `servers` | none |
| + | Continue | `~/.continue/mcpServers/red-ale.yaml` (or `$CONTINUE_GLOBAL_DIR`) | same | same | YAML | `mcpServers` (list) | none |
| + | Qwen Code | `~/.qwen/settings.json` (or `$QWEN_HOME`) | same | same | JSON | `mcpServers` | `qwen mcp add -s user red-ale <abs> mcp` |
| + | Kilo Code (opencode fork) | `~/.config/kilo/kilo.json[c]` (or `$KILO_CONFIG_DIR`) | same | unconfirmed | JSONC | `mcp` | `kilo mcp add red-ale -- <abs> mcp` |
| + | Factory Droid | `~/.factory/mcp.json` | same | same | JSON | `mcpServers` | `droid mcp add red-ale <abs> mcp` |
| + | Crush | `$XDG_CONFIG_HOME/crush/crushrc` | same | `%USERPROFILE%\.config\crush\crushrc` | Bash (crushrc); legacy `crush.json` | `mcp add` builtin | none at shell level (unconfirmed) |
| + | Warp | `~/.warp/.mcp.json` | same | — | JSON | `mcpServers` | none (it also reads Claude Code and Codex configs) |

`<abs>` is the absolute path of the red-ale binary. Write it in every harness (see "Cross-cutting findings").

**What each entry looks like for `red-ale mcp`.** Per-harness literals are in the sections below.

| Harness | Executable | Arguments | Environment | `type` | Enable flag |
|---|---|---|---|---|---|
| Claude Code | `command` | `args` | `env` | optional (`"stdio"`) | — |
| Codex | `command` | `args` | `env` | none (inferred) | `enabled` |
| opencode | `command[0]` | rest of `command` array | `environment` | `"local"`, required | V1 `enabled`, V2 `disabled` |
| Gemini CLI, Qwen | `command` | `args` | `env` | optional | — |
| Claude Desktop | `command` | `args` | `env` | **not allowed** (schema has none) | — |
| Cursor | `command` | `args` | `env` | docs say required, examples omit it | — |
| VS Code, Visual Studio | `command` | `args` | `env` | `"stdio"` (inferred if missing) | — |
| Zed | `command` | `args` | `env` | none | `enabled` |
| Copilot CLI | `command` | `args` (required) | `env` | optional (`"local"`) | — (`tools: ["*"]` required) |
| Goose | `cmd` | `args` (required) | `envs` | `stdio`, required | `enabled`, required |
| Cline, Kiro | `command` | `args` | `env` | none | `disabled` |
| Amp, Junie, Antigravity, Devin | `command` | `args` | `env` | none | Devin, Antigravity: `disabled` |
| Continue | `command` | `args` | `env` | optional | — (list item needs `name`) |

---

## Cross-cutting findings

1. **Always write an absolute `command`.** Claude Desktop's MCP guide says paths must be absolute
   ([connect-local-servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)). Cursor's docs say the
   command must be on the system path "or contain its full path" ([cursor docs/mcp](https://cursor.com/docs/mcp.md)).
   Goose silently drops `PATH` from a server's `envs` (§11). Copilot CLI passes only `PATH` from its environment (§10).
   macOS GUI apps launched from Finder don't get the shell's `PATH`. Use the canonicalised
   `std::env::current_exe()`, and `red-ale.exe` with its full path on Windows.
2. **Use the harness's own non-interactive CLI where one exists.** Claude Code, Codex, Gemini, opencode (add only),
   VS Code, Devin, Copilot CLI, Cline, Kiro, Amp, Qwen, Kilo and Droid have one. These CLIs choose the right file and
   keep comments. Edit files directly only for Claude Desktop, Cursor, Zed, Goose, Junie, Antigravity, Continue,
   Visual Studio and Crush, and for removals in opencode.
3. **Re-adding a name behaves differently from CLI to CLI.**
   - **Fails:** Claude Code (exit 1, tested) and Kiro (without `--force`; predecessor source).
   - **Replaces silently:** Codex (tested), `code --add-mcp` (source), Gemini (source) and Crush (docs).
   - `setup` should run remove-then-add, or check with get/list first.
4. **Some files are rewritten by the running app, so editing them is unsafe:**
   - **Claude Code's `~/.claude.json`** is rewritten constantly (tested). Use `claude mcp add`.
   - **Claude Desktop** writes back its whole in-memory config, so an edit made while it runs is lost
     (source-derived).
   - **Goose** re-serialises all of `config.yaml` on every save, which drops comments.
   - **VS Code** re-serialises `mcp.json` with `JSON.stringify`, which drops comments and unknown top-level keys.
   - **Zed** edits `settings.json` in place, keeping comments.
   - **Cline** has several concurrent writers and uses a `<file>.lock` directory.
5. **One harness may read another's file, so red-ale can show up twice:**
   - VS Code reads `~/.copilot/mcp-config.json` by default (and Claude Desktop, Cursor and Windsurf files when enabled).
   - Devin CLI imports `~/.claude.json`, `.cursor/mcp.json`, Windsurf, opencode and Zed configs, all by default.
   - Warp reads Claude Code and Codex configs.
   - JetBrains AI Assistant can import from Claude Desktop.
   - Claude Desktop's Code tab reads `claude_desktop_config.json`.

   Use the one name `red-ale` everywhere, so harnesses that merge by name collapse duplicates. How each harness
   handles a name clash across sources is unconfirmed outside Claude Code (whole entry, by precedence) and opencode (V2
   wins).
6. **Several harnesses have moved or been retired since early 2026.** Windsurf is now Devin Desktop and its Cascade
   agent is gone. Gemini CLI is now for Enterprise users only, and individuals use Antigravity. Amp's IDE extensions
   are shut down. Cline 4.0 moved its file. Goose moved organisation. Roo Code is archived. Detection must not assume
   the 2025 layouts.

---

## 1. Claude Code

Version checked: 2.1.312 (local). Docs: https://code.claude.com/docs/en/mcp

### Locations and scopes

| Scope | File | Where the entry goes |
|---|---|---|
| local (default for `claude mcp add`) | `~/.claude.json` | `projects["<abs project path>"].mcpServers.<name>` |
| project | `<project>/.mcp.json` (meant to be committed) | `mcpServers.<name>` |
| user | `~/.claude.json` | top-level `mcpServers.<name>` |
| managed (admin) | macOS `/Library/Application Support/ClaudeCode/managed-mcp.json`; Linux and WSL `/etc/claude-code/managed-mcp.json`; Windows `C:\Program Files\ClaudeCode\managed-mcp.json` | same format as `.mcp.json` |

- **Scope docs:** [mcp#local-scope](https://code.claude.com/docs/en/mcp#local-scope),
  [#project-scope](https://code.claude.com/docs/en/mcp#project-scope),
  [#user-scope](https://code.claude.com/docs/en/mcp#user-scope),
  [managed-mcp](https://code.claude.com/docs/en/managed-mcp).
- **A managed file is exclusive.** When `managed-mcp.json` is present, only its servers load. Admins can also use the
  `managedMcpServers`, `allowedMcpServers` and `deniedMcpServers` settings.
- **Precedence:** local > project > user > plugin > claude.ai connectors. The winning entry is used whole; fields are
  not merged ([#scope-hierarchy-and-precedence](https://code.claude.com/docs/en/mcp#scope-hierarchy-and-precedence)).
- **Project servers ask for approval.** Interactive sessions ask before using `.mcp.json` servers; `claude -p` and SDK
  runs don't.
- **`CLAUDE_CONFIG_DIR`** moves the config directory ([env-vars](https://code.claude.com/docs/en/env-vars)). The docs
  don't say where `.claude.json` goes then. **Tested:** with `CLAUDE_CONFIG_DIR=X`, `claude mcp add -s user` wrote
  `X/.claude.json`.
- **Windows:** `~` is `%USERPROFILE%` ([settings](https://code.claude.com/docs/en/settings)).

### Entry

JSON with no comments. This is what `claude mcp add -s user` writes (tested):

```json
{
  "mcpServers": {
    "red-ale": { "type": "stdio", "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

- **`type` is optional for stdio:** "Claude Code reads an entry with no `type` as a stdio server"
  ([mcp](https://code.claude.com/docs/en/mcp)). Tested with `add-json`.
- **Expansion:** `.mcp.json` supports `${VAR}` and `${VAR:-default}` in `command`, `args`, `env`, `url` and
  `headers`.
- **`CLAUDE_PROJECT_DIR`:** Claude Code puts the project root in the server's environment under this name.
- **Server names:** letters, digits, `-` and `_` only.

### CLI (stable)

```
claude mcp add [-s local|user|project] [-e KEY=VAL ...] [-t stdio] <name> -- <command> [args...]
claude mcp add-json [-s scope] <name> '<server object json>'
claude mcp get <name>
claude mcp list
claude mcp remove [-s scope] <name>
claude mcp add-from-claude-desktop [-s scope]    # macOS and WSL only
```

- **`-e` takes several values**, so a name placed straight after it is read as another `KEY=value`. Put the name
  first ([mcp](https://code.claude.com/docs/en/mcp)).
- **Tested behaviour:**
  - Re-adding an existing name in the same scope exits 1.
  - `remove` without `-s` for a name in two scopes exits 1.
  - Removing a missing name exits 1.

### Detection

- `claude` on PATH. The native installer puts it in `~/.local/bin/claude`, or `%USERPROFILE%\.local\bin\claude.exe`
  on Windows. Homebrew, WinGet and Linux packages also exist ([setup](https://code.claude.com/docs/en/setup)).
- Fallback: `~/.claude/` or `~/.claude.json` exists.

### Quirks

- **Don't edit `~/.claude.json` by hand.** Claude Code writes it "for itself"
  ([settings](https://code.claude.com/docs/en/settings)), and keeps five backups in `~/.claude/backups/`
  ([claude-directory](https://code.claude.com/docs/en/claude-directory)). Its mtime changed during this session with no
  command of ours writing to it. Use the CLI. Editing `.mcp.json` directly is fine.
- **Reload:** whether a running session picks up a new server is not documented. Expect a new session, or `/mcp` →
  reconnect (**unconfirmed**).
- **Windows `cmd /c`:** the docs no longer give the old "wrap `npx` in `cmd /c`" advice. A native `.exe` should not
  need it (**unconfirmed**).
- **Timeouts:** `MCP_TIMEOUT` (startup, default 30000 ms) and `MCP_TOOL_TIMEOUT`
  ([env-vars](https://code.claude.com/docs/en/env-vars)).
- **Admin override:** a managed `managed-mcp.json` or a locked `mcp` setting makes user servers be ignored. Verify with
  `claude mcp get red-ale` afterwards.

---

## 2. OpenAI Codex (CLI, IDE extension, ChatGPT desktop app)

Version checked: codex-cli 0.164.0 (local). Source: `openai/codex@rust-v0.165.0`. The docs moved:
`developers.openai.com/codex/mcp` redirects to https://learn.chatgpt.com/docs/extend/mcp.

### Locations

- **User:** `$CODEX_HOME/config.toml`, with `CODEX_HOME` defaulting to `~/.codex`
  ([config-advanced](https://developers.openai.com/codex/config-advanced),
  [config-basic](https://developers.openai.com/codex/config-basic)).
- **`CODEX_HOME` must already exist.** If it is set, it must name an existing directory, or Codex fails. It is
  canonicalised ([`codex-rs/utils/home-dir/src/lib.rs:13-63`](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/utils/home-dir/src/lib.rs#L13-L63)).
- **Project:** `.codex/config.toml` files from the project root down to the working directory, with the closest
  winning. They are read only in **trusted** projects. Trust is recorded in the user config as
  `[projects."<path>"] trust_level = "trusted"` ([config-basic](https://developers.openai.com/codex/config-basic),
  [config-reference](https://developers.openai.com/codex/config-reference)).
- **Precedence:** CLI flags and `-c` > project > profile > user > cloud-managed > system `/etc/codex/config.toml` >
  built-in ([config-basic](https://developers.openai.com/codex/config-basic)).
- **Admin allowlist:** a `requirements.toml` can carry an `mcp_servers` allowlist; servers not on it are disabled.
- **One file for all three clients.** "The ChatGPT desktop app, Codex CLI, and IDE extension share this
  configuration" ([extend/mcp](https://learn.chatgpt.com/docs/extend/mcp)). One registration covers all of them.
- **WSL:** inside WSL, Codex uses the WSL user's `~/.codex`, separate from Windows' (**unconfirmed**; follows from home
  resolution).

### Entry

TOML. This is what `codex mcp add red-ale --env RED_ALE_LOG=info -- red-ale mcp` writes (tested):

```toml
[mcp_servers.red-ale]
command = "/abs/path/red-ale"
args = ["mcp"]

[mcp_servers.red-ale.env]
RED_ALE_LOG = "info"
```

- **No `type` key.** `command` means stdio and `url` means streamable HTTP. An HTTP-only key on a stdio entry is an
  error ([`codex-rs/config/src/mcp_types.rs:391-540`](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/config/src/mcp_types.rs#L391)).
- **Other stdio keys** ([config-reference](https://developers.openai.com/codex/config-reference)):
  - `env_vars` (variable names to pass through), `cwd`, `enabled`, `required`;
  - `startup_timeout_sec` (**default 10**) or `startup_timeout_ms`, and `tool_timeout_sec` (default 60);
  - `enabled_tools` and `disabled_tools`;
  - `default_tools_approval_mode` and `tools.<tool>.approval_mode`.

### CLI (stable)

```
codex mcp add <NAME> [--env KEY=VALUE ...] -- <COMMAND>...
codex mcp list [--json]
codex mcp get <NAME> [--json]
codex mcp remove <NAME>
```

- **No scope flag.** `add` always writes the global `$CODEX_HOME/config.toml` ("Added global MCP server"), so there is
  no CLI way to write a project server ([`codex-rs/cli/src/mcp_cmd.rs:297-529`](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/cli/src/mcp_cmd.rs#L297)).
- **Names** must match `[A-Za-z0-9-_:@/.]+` ([mcp_cmd.rs:1144-1155](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/cli/src/mcp_cmd.rs#L1144)).
- **Tested:** `add` on an existing name replaces the entry, including dropping its old `env`, and exits 0. `remove`
  of a missing name exits 0.
- **Writes are safe.** The CLI edits with `toml_edit`, so comments survive. The write is atomic, and only
  `[mcp_servers]` is touched ([`codex-rs/core/src/config/edit.rs:443-496,744-790`](https://github.com/openai/codex/blob/rust-v0.165.0/codex-rs/core/src/config/edit.rs#L443)).

### Detection

- `codex` on PATH, from npm `@openai/codex`, Homebrew cask, or the install script
  ([cli](https://developers.openai.com/codex/cli)).
- The IDE extension's id is `openai.chatgpt` ([ide](https://developers.openai.com/codex/ide)).
- Fallback: `~/.codex/` exists.

### Quirks

- **Restart:** after adding a server, the IDE extension needs "Restart extension" and the ChatGPT app needs a restart
  ([extend/mcp](https://learn.chatgpt.com/docs/extend/mcp)). A running TUI probably needs a restart too
  (**unconfirmed**).
- **Startup timeout:** the 10 s default is short. Set `startup_timeout_sec` if `red-ale mcp` can be slow to start.
- **Direct edits** are low-risk, because Codex doesn't rewrite the file on every run. If you write the file, use
  `toml_edit`.

---

## 3. opencode

Versions checked: V1 1.19.4 (local) and source `v1.19.5`; V2 source `v2.2.0`. The repository is now
`anomalyco/opencode`; `sst/opencode` redirects there.

### V1 and V2 are separate release lines

OpenCode 1.x and 2.x are separate release lines, both maintained: v1.19.5 shipped 2026-10-28 and v2.2.0 on 2026-10-27.
Both install as `opencode`. The V2 installer also leaves an `opencode2` shim (`v2.2.0:install:69,344-366`).

**The MCP layouts differ** ([migrate-v1](https://dev.opencode.ai/v2/docs/migrate-v1/), checked):

> "V2 groups servers under `mcp.servers`, replaces `enabled` with the inverse `disabled`, and separates timeout
> purposes"

- In V1, `timeout` is a number of milliseconds. In V2 it is an object with `startup`, `catalog` and `execution` keys.
  Remote OAuth keys become snake_case (`clientId` becomes `client_id`).
- **V2 reads V1 files without rewriting them.** V2 "normalizes supported V1 and native V2 fields in memory without
  rewriting the source file". When both forms set the same value, the native V2 value wins (same page;
  `v2.2.0:packages/core/src/config/normalize.ts:249-305`).
- **V1 reads the V2 `mcp.servers` envelope only from v1.18.24** (2026-08-28, commit 03afae5b9, #45421;
  `v1.19.5:packages/opencode/src/config/v2-compat.ts:244-311`). Older V1 releases don't.
- **Conflicting report:** third-party issue aovestdipaperino/tokensave#689 says V2 2.0.28 ignored a flat V1 entry.
  That contradicts the source (**unconfirmed** either way).
- **Write the native layout for the detected major version, never both,** then check with `opencode mcp list`.
- Both lines use `"$schema": "https://opencode.ai/config.json"`, and that schema describes V1. Editors therefore flag
  valid V2 configs.

### Locations

| Scope | Path |
|---|---|
| Global (all OSes) | `$XDG_CONFIG_HOME/opencode/` or `~/.config/opencode/`, including on Windows (`%USERPROFILE%\.config\opencode\`, not `%APPDATA%`). Files: `opencode.jsonc`, `opencode.json`, legacy `config.json`. V1: `v1.19.5:packages/core/src/global.ts:3,13`; V2: `v2.2.0:packages/util/src/global-roots.ts:4-18`. |
| Global file merge (V1) | All three files are loaded in the order `config.json`, `opencode.json`, `opencode.jsonc`, so later files win (`v1.19.5:packages/opencode/src/config/config.ts:140-148,272-274`). |
| Project | `opencode.json[c]`, found by walking up from the working directory to the worktree root, plus `.opencode/opencode.json[c]` (`v1.19.5:packages/opencode/src/config/paths.ts:10-41`). |
| System | `/etc/opencode/`, `/Library/Application Support/opencode/`, `%ProgramData%\opencode\` (`v2.2.0:packages/core/src/config/discovery.ts:27-38`). |
| Env (V1) | `OPENCODE_CONFIG` (an extra file), `OPENCODE_CONFIG_DIR` (an extra directory), `OPENCODE_CONFIG_CONTENT` (inline JSON) ([docs/config](https://opencode.ai/docs/config/)). |
| Env (V2) | `OPENCODE_CONFIG_DIR` **replaces** the global directory (`v2.2.0:packages/util/src/global.ts:79`). V2 support for `OPENCODE_CONFIG` is **unconfirmed**. |

### Entry

- **Format:** JSON or JSONC. The docs say "OpenCode supports both JSON and JSONC (JSON with Comments)"
  ([docs/config](https://opencode.ai/docs/config/)).
- **`command` is one array** holding the executable and its arguments. There is no `args`.
- **The env map is called `environment`**, not `env`.
- **`type: "local"` is required.**
- Source: [docs/mcp-servers](https://opencode.ai/docs/mcp-servers/); `v2.2.0:packages/core/src/v1/config/mcp.ts:6-26`;
  `v2.2.0:packages/schema/src/mcp.ts:26-39`.

V1:
```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "red-ale": { "type": "local", "command": ["/abs/path/red-ale", "mcp"], "enabled": true }
  }
}
```

V2:
```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "red-ale": { "type": "local", "command": ["/abs/path/red-ale", "mcp"] }
    }
  }
}
```

### CLI

- **V1** (`opencode mcp add|list|auth|logout|debug`, from local `--help`): `opencode mcp add red-ale [--env K=V] --
  /abs/red-ale mcp`.
  - It runs without prompts from v1.17.0 when given a name.
  - It always writes the **global** file (`opencode.json` if present, else `opencode.jsonc`, else it creates
    `opencode.json`), using the flat V1 layout.
  - With no name it is interactive (`v1.19.5:packages/opencode/src/cli/cmd/mcp.ts:394-500`).
- **V2:** `opencode mcp add <name> [--global] [--env K=V]... -- <cmd> [args...]`.
  - Its **default target is the project config** in the working directory; pass `--global` for the user file.
  - It writes `mcp.servers.<name>` with `jsonc-parser`, which keeps comments
    (`v2.2.0:packages/cli/src/commands/handlers/mcp/add.ts:18-68`;
    [v2 docs/mcp-servers](https://dev.opencode.ai/v2/docs/mcp-servers/)).
- **Neither line has `mcp remove`.** Removal means editing the file.

### Detection

- `opencode` on PATH. The curl install puts it in `~/.opencode/bin/opencode`; npm and brew installs also exist.
- `opencode --version` shows the major version: `1.19.4` here. A V2 build printed `opencode v2.0.28` (third-party
  report).
- `opencode2` on PATH suggests V2.
- `~/.config/opencode/` exists.

### Quirks

- **Reload:** V2 watches its config and reloads MCP (`v2.2.0:packages/core/src/config.ts:320-340`). V1 probably
  needs a restart (**unconfirmed**).
- **Timeout default:** V1's schema says 5000 ms (`v1/config/mcp.ts:23-25`); the docs say 30000 ms.
- **Which V1 file to edit:** V1 merges `opencode.json` and `opencode.jsonc`. Edit the one that already holds `mcp`.
- **Editing:** use a JSONC-preserving edit, not a serde round-trip.

---

## 4. Google Gemini CLI

Version checked: source `google-gemini/gemini-cli@v0.66.0` (2026-10-27). Docs: https://geminicli.com/docs/tools/mcp-server/

**Status.** The README banner says free and Google AI Pro/Ultra access "moved to Antigravity CLI on June 18, 2026".
Gemini CLI is now "Enterprise Code Assist only" ([README](https://github.com/google-gemini/gemini-cli), checked).
Antigravity is covered as an addition below; it uses a different file.

### Locations

| Scope | Path |
|---|---|
| User | `~/.gemini/settings.json`. Home is `GEMINI_CLI_HOME` if set (`packages/core/src/utils/paths.ts:13,19-27`; `packages/core/src/config/storage.ts:55-59,78-79`). |
| Project ("workspace") | `<cwd>/.gemini/settings.json`. Not used when the working directory is home (`packages/cli/src/config/settings.ts:1158-1162`). |
| System overrides | `/etc/gemini-cli/settings.json`, `/Library/Application Support/GeminiCli/settings.json`, `C:\ProgramData\gemini-cli\settings.json`, or `GEMINI_CLI_SYSTEM_SETTINGS_PATH` (`settings.ts:106-117`). |
| System defaults | `system-defaults.json` in the same directory, or `GEMINI_CLI_SYSTEM_DEFAULTS_PATH` (`settings.ts:120-129`). |

### Entry

- **Format:** JSON with comments allowed, read through `strip-json-comments` (`settings.ts:27,949`). The CLI writes
  with `comment-json` and an atomic rename, so comments survive (`packages/cli/src/utils/commentJson.ts`).
- **`mcpServers` is a top-level key** and has `requiresRestart: true` (`packages/cli/src/config/settingsSchema.ts:161-174`).
- **Fields** (`additionalProperties: false`, `settingsSchema.ts:3038-3110`): `command`, `args`, `env`, `cwd`, `url`,
  `httpUrl`, `headers`, `tcp`, `type` (`stdio` | `sse` | `http`), `timeout` (ms, default 600000), `trust`,
  `description`, `includeTools`, `excludeTools`, and OAuth fields.
- **`type` is optional;** a `command` means stdio.
- **The old settings migration doesn't matter here.** The V1→V2 settings migration kept `mcpServers` at the top level
  (`v0.10.0:packages/cli/src/config/settings.ts:263-266`).

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

### CLI (stable, non-interactive)

```
gemini mcp add [-s user|project] [-t stdio|sse|http] [-e KEY=value]... [--timeout ms] [--trust] <name> <commandOrUrl> [args...]
gemini mcp remove [-s user|project] <name>
gemini mcp list
gemini mcp enable|disable <name>
```

- **The default scope is `project`.** Pass `-s user`.
- **Put flags before `<name>`.** Unknown options after the name pass through to the server.
- **`-e` splits on `=`**, so anything after a second `=` is dropped.
- **Re-adding replaces** the entry.
- **Project scope fails** in the home directory or an untrusted folder.
- Source: `packages/cli/src/commands/mcp/add.ts:47-226`, `remove.ts:62-80`.

### Detection

- `gemini` on PATH (npm `@google/gemini-cli`); `gemini --version`.
- `~/.gemini/` alone doesn't prove Gemini CLI is installed: Antigravity uses that directory too.

### Quirks

- **A restart is needed.**
- **Project-scope servers need a trusted folder,** and `mcp.allowed` / `mcp.excluded` can block servers.
- **Extensions are an alternative.** An extension is `~/.gemini/extensions/<name>/gemini-extension.json` with its own
  `mcpServers` map, but `settings.json` wins on a name clash
  ([extensions reference](https://geminicli.com/docs/extensions/reference/)). Not worth it for `setup`.

---

## 5. Claude Desktop

Version checked: 4.29.0, Linux `.deb` unpacked, not installed. Docs:
https://modelcontextprotocol.io/docs/develop/connect-local-servers

### Locations of `claude_desktop_config.json`

| OS | Path | Status |
|---|---|---|
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` | docs |
| Windows (classic install) | `%APPDATA%\Claude\claude_desktop_config.json` | docs |
| Windows (MSIX) | `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude\claude_desktop_config.json` | source-derived plus bug reports |
| Linux (beta) | `$XDG_CONFIG_HOME/Claude/claude_desktop_config.json`, default `~/.config/Claude/` | source-derived |

- **Linux:** there is now an official "Claude Desktop on Linux (beta)" for Debian and Ubuntu, installed with
  `apt install claude-desktop` ([desktop-linux](https://code.claude.com/docs/en/desktop-linux)).
  - Its docs give no config path.
  - In 4.29.0, `getAppConfigPath()` is `path.join(app.getPath("userData"), "claude_desktop_config.json")`, and
    `productName` is `Claude`. Electron puts `userData` under `$XDG_CONFIG_HOME/<name>` on Linux
    ([Electron app.getPath](https://www.electronjs.org/docs/latest/api/app#appgetpathname)).
- **Windows MSIX:** the official installer is now MSIX
  ([support 12622703](https://support.claude.com/en/articles/12622703-deploy-claude-desktop-for-windows)).
  - MSIX redirects new AppData files into a per-package location
    ([MSIX behind the scenes](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes)).
  - The app code knows two package family names, `Claude_pzs8sxrjxfjjc` and `AnthropicPBC.Claude_fnn82j28hfe8t`.
  - Open issue [anthropics/claude-code#26073](https://github.com/anthropics/claude-code/issues/26073) says "Edit
    Config" opens `%APPDATA%\Claude\…` while the app reads the LocalCache copy, so MCP servers silently don't load.
  - **Rule:** if `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\` exists, write there.
- **Env vars** (source-derived, undocumented): `CLAUDE_USER_DATA_DIR` moves `userData`. In "3p" deployment mode the
  directory gets a `-3p` suffix.

### Entry

- **Strict JSON.** The 4.29.0 schema is exactly `{command: string, args?: string[], env?: Record<string,string>,
  extensionId?: string}`. There is **no `type`, `cwd` or `url`**.
- **Invalid entries** are skipped with a dialog.
- **Comments break the whole file.** It is parsed with `JSON.parse`, so a comment fails the load ("Couldn't load app
  settings"). After a failed load the app refuses to write the file at all.

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

### CLI

- **None.** Edit the file (Settings → Developer → Edit Config) or install a Desktop Extension.
- `claude mcp add-from-claude-desktop` copies the other way, from Desktop to Code.

### Detection

- **macOS:** `/Applications/Claude.app`; bundle id `com.anthropic.claudefordesktop`
  ([support 12622667](https://support.claude.com/en/articles/12622667-enterprise-configuration)).
- **Windows:** `Get-AppxPackage -Name Claude`, `%LOCALAPPDATA%\Packages\Claude_*`, or `%APPDATA%\Claude`.
- **Linux:** `dpkg -s claude-desktop`, `/usr/bin/claude-desktop`, or `~/.config/Claude/`.

### Quirks

- **Restart required:** "completely quit Claude Desktop and restart it"
  ([connect-local-servers](https://modelcontextprotocol.io/docs/develop/connect-local-servers)).
- **The app rewrites this file while it runs** (source-derived).
  - The file also holds app settings (shortcuts, preferences, feature flags).
  - The app loads the file once and, on any settings change, writes its whole in-memory copy back.
  - An edit made while the app runs is lost on the next write.
  - **Safe procedure:** check the app isn't running (ask the user to quit it). Merge only `mcpServers.red-ale`, keep
    every other key, keep strict JSON, write atomically, then tell the user to relaunch.
- **Absolute paths are required** (docs troubleshooting). Whether chat-surface servers see the login-shell PATH is
  **unconfirmed**.
- **Admin policy** `isLocalDevMcpEnabled` can disable local servers
  ([support 12622667](https://support.claude.com/en/articles/12622667-enterprise-configuration)).
- **Desktop Extensions (`.mcpb`, formerly `.dxt`) are an alternative.**
  - An extension is a zip with `manifest.json`; `server.type = "binary"` supports per-platform settings
    ([anthropics/mcpb](https://github.com/anthropics/mcpb),
    [MANIFEST.md](https://github.com/anthropics/mcpb/blob/main/MANIFEST.md)).
  - It avoids the MSIX path and app-rewrite problems, at the cost of packaging.
- **The Code tab** inside the Desktop app also reads this file and prefers its definition on a name clash
  ([desktop](https://code.claude.com/docs/en/desktop)).

---

## 6. Cursor (IDE and `agent` CLI)

Versions checked: Cursor CLI 2026.10.22-27dcf5f (bundle grepped, not run). Docs: https://cursor.com/docs/mcp.md,
https://cursor.com/help/customization/mcp.md

### Locations

- **Global:** `~/.cursor/mcp.json`. **Project:** `<project>/.cursor/mcp.json`. They are merged, and the project entry
  wins on a name clash ([help/customization/mcp](https://cursor.com/help/customization/mcp.md)).
- **Windows:** `%USERPROFILE%\.cursor\mcp.json`. **Unconfirmed in docs**, but the CLI bundle uses
  `path.join(os.homedir(), ".cursor", "mcp.json")`.
- **No env var** is documented to move it.
- **WSL:** Cursor on Windows reads the Windows-side file
  ([forum](https://forum.cursor.com/t/looking-in-wrong-spot-to-discover-mcp-servers/159930), community).

### Entry

- **`type`:** the stdio table lists `type: "stdio"` as **Required**, but every docs example omits it. Writing it is
  safe.
- **Fields:** `command`, `args`, `env`, `envFile`.
- **Interpolation:** `${env:NAME}`, `${userHome}`, `${workspaceFolder}`.
- **Comments:** JSONC tolerance is **unconfirmed**, so write strict JSON.

```json
{
  "mcpServers": {
    "red-ale": { "type": "stdio", "command": "/abs/path/red-ale", "args": ["mcp"] }
  }
}
```

### CLI and deeplink

- **`agent` (alias `cursor-agent`) has no way to add or remove a server.** Its subcommands are `agent mcp list |
  list-tools | login | enable | disable` ([cli/mcp](https://cursor.com/docs/cli/mcp)). The bundle's command definitions
  have no `add`, even though a staff [forum reply](https://forum.cursor.com/t/can-cursor-cli-support-add-mcp-server-parameter/167812)
  claims there is one. Write the JSON file.
- **Global servers are auto-approved in the CLI;** project servers need approval
  ([cli/changelog](https://cursor.com/docs/cli/changelog)).
- **Deeplink:** `cursor://anysphere.cursor-deeplink/mcp/install?name=red-ale&config=<base64 of the server object>`.
  Cursor asks the user to confirm ([install-links](https://cursor.com/docs/mcp/install-links)).

### Detection

- `~/.cursor/`, `agent` or `cursor-agent` in `~/.local/bin`, or the `cursor` shell command.
- App paths `/Applications/Cursor.app`, `~/.config/Cursor` and `%LOCALAPPDATA%\Programs\cursor` are **unconfirmed**.

### Quirks

- **Restart:** "Save the file and restart Cursor" after manual edits
  ([help/customization/mcp](https://cursor.com/help/customization/mcp.md)).
- **Absolute path:** the command must be on the system path "or contain its full path", so use an absolute path.
- **Windows** (community reports, **unconfirmed**):
  - command paths with spaces get split
    ([forum](https://forum.cursor.com/t/windows-mcp-path-with-spaces-fails-please-add-in-app-bug-reporting/153443));
  - `${userHome}` doesn't expand
    ([forum](https://forum.cursor.com/t/userhome-on-windows-isnt-expanded-in-mcp-config-the-same-as-on-macos-linux/172568)).
- **Admin network controls:** with "MCP Network Controls" on, stdio servers don't start on Windows.
- **Rewrites:** whether Cursor rewrites `mcp.json` while running is **unconfirmed**. UI toggle state seems to live
  elsewhere.

---

## 7. VS Code with GitHub Copilot (agent mode)

Version checked: VS Code 1.143.1 (local and source). Docs:
https://code.visualstudio.com/docs/copilot/reference/mcp-configuration,
https://code.visualstudio.com/docs/copilot/customization/mcp-servers

### Locations

- **VS Code format (`servers`):** workspace `.vscode/mcp.json`, and the user profile's `mcp.json`.
  - Default profile: `<user data>/User/mcp.json`. Other profiles: `User/profiles/<id>/mcp.json`, unless the profile
    shares the default's file (`src/vs/platform/userDataProfile/common/userDataProfile.ts:203,273`).
  - Linux `~/.config/Code/User/`, macOS `~/Library/Application Support/Code/User/`, Windows `%APPDATA%\Code\User\`
    ([settings docs](https://code.visualstudio.com/docs/configure/settings)).
  - Insiders uses `Code - Insiders` (**unconfirmed** in docs).
- **Copilot portable format (`mcpServers`):** workspace `.mcp.json`, and user `$COPILOT_HOME/mcp-config.json`
  (default `~/.copilot/mcp-config.json`). This is the same file as Copilot CLI (§10).
  - VS Code's guided flow now recommends this "Copilot Global" file.
  - The profile and workspace `mcp.json` "remain available for compatibility and are labeled deprecated in the guided
    flow" ([mcp-servers, "Choose a configuration destination"](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)).
- **Legacy `"mcp"` key in `settings.json`:** migrated to `mcp.json` on startup, and an error is shown if it reappears
  (`src/vs/workbench/contrib/mcp/browser/mcpMigration.ts:50-56,100`). Don't write it.

### Entry

- **Format:** JSONC, parsed with `allowTrailingComma` (`src/vs/platform/mcp/common/mcpResourceScannerService.ts:298`).
- **Top-level keys:** `servers`, `inputs`, `sandbox`.
- **`type` is inferred if missing.** The docs say `type: "stdio"` is required, but the reader infers stdio from
  `command` (`mcpResourceScannerService.ts:414-416`).
- **Other fields:** `cwd`, `envFile`, `dev`, `sandboxEnabled`.

```json
{
  "servers": {
    "red-ale": { "type": "stdio", "command": "/abs/path/red-ale", "args": ["mcp"] }
  }
}
```

The Copilot portable form is `{"mcpServers": {"red-ale": {"command": "/abs/path/red-ale", "args": ["mcp"]}}}`. It
allows only `type`, `command`, `args`, `env` and `cwd` (`src/vs/platform/mcp/common/mcpCopilotGlobalConfiguration.ts:17-38`).

### CLI (stable)

```
code --add-mcp '{"name":"red-ale","type":"stdio","command":"/abs/path/red-ale","args":["mcp"]}'
```

- **Always writes the default profile's user `mcp.json`.** There is no workspace option, and `--profile` doesn't apply
  (`src/vs/code/node/cliProcessMain.ts:306-314,342-343`; `src/vs/platform/mcp/common/mcpManagementService.ts:725-726`).
- **Silently overwrites** an existing entry with the same name (`mcpResourceScannerService.ts:124-135`).
- **No CLI list or remove.**
- **URL handler:** `vscode:mcp/install?<urlencoded {name,...config}>` opens a confirmation editor
  (`src/vs/workbench/contrib/mcp/browser/mcpWorkbenchService.ts:63-90`).

### Detection

- `code` / `code-insiders` on PATH, or `~/.config/Code/User/` exists.
- Copilot Chat now ships inside VS Code (`extensions/copilot` in the repo).

### Quirks

- **No restart needed:** VS Code watches `mcp.json` (`mcpManagementService.ts:399-403`). Servers from outside the
  workspace get a trust prompt on first start.
- **VS Code's own writes use `JSON.stringify`.** They drop comments and any unknown top-level keys, and delete the file
  if it ends up empty (`mcpResourceScannerService.ts:333-341,371-384`).
- **Settings Sync** can copy the profile `mcp.json` to machines without red-ale. It doesn't sync
  `~/.copilot/mcp-config.json` ([manage-mcp-servers](https://code.visualstudio.com/docs/copilot/customization/mcp-servers)).
- **Discovery:** `chat.mcp.discovery.enabled` reads Copilot CLI's file by default, and Claude Desktop, Cursor and
  Windsurf files when enabled ([mcp-configuration](https://code.visualstudio.com/docs/copilot/reference/mcp-configuration)).
  Writing both the profile file and `~/.copilot/mcp-config.json` can list red-ale twice.
- **Remote and dev containers:** user-profile stdio servers run locally. Remote setups need `devcontainer.json`
  `customizations.vscode.mcp.servers` or the remote user config. `code --add-mcp` from WSL is **unconfirmed**.

---

## 8. Windsurf, now Devin Desktop / Devin CLI

Docs: https://docs.devin.ai/cli/extensibility/mcp/configuration.md, https://docs.devin.ai/desktop/changelog.md

### Status

Checked against the [Devin Desktop changelog](https://docs.devin.ai/desktop/changelog.md):
- v3.0.12 (2026-06-02): "Windsurf is now Devin Desktop."
- v3.9.19 (2026-09-08): "**Cascade** has been removed." Devin Local is the only agent.

The old Cascade MCP page now says its config "applies to the legacy Cascade agent only", and that Devin Local uses the
Devin CLI config files ([desktop/cascade/mcp](https://docs.devin.ai/desktop/cascade/mcp)). **There is no longer a
separate Windsurf target.**

### Locations

- **User:** `$XDG_CONFIG_HOME/devin/mcp_config.json`, default `~/.config/devin/mcp_config.json`; Windows
  `%APPDATA%\devin\mcp_config.json`
  ([global-vs-local](https://docs.devin.ai/cli/reference/configuration/global-vs-local.md)).
- **Project:** `.devin/mcp_config.json` (shared) and `.devin/mcp_config.local.json` (gitignored). `devin mcp add`
  defaults to the **local** one.
- **Older layout:** before v3000.3, servers sat under `mcpServers` in `config.json`. They are migrated on startup.
- **Imports, all on by default:** the CLI reads `~/.codeium/{windsurf,windsurf-next,windsurf-insiders}/mcp_config.json`,
  project `.cursor/mcp.json`, Claude Code files (including `~/.claude.json`), opencode and Zed. Each can be switched
  off with `read_config_from` in `~/.config/devin/config.json`
  ([read-config-from](https://docs.devin.ai/cli/reference/configuration/read-config-from.md)).
- **Doc conflict:** the FAQ table lists `~/.codeium/mcp_config.json`, which contradicts the other pages. Treat it as a
  doc error.

### Entry

- **Fields:** `command`, `args`, `env`, `disabled`. No `type`.
- **Format:** the config-file page says "JSON with Comments"
  ([config-file](https://docs.devin.ai/cli/reference/configuration/config-file.md)). Whether that covers
  `mcp_config.json` is **unconfirmed**.

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"] }
  }
}
```

### CLI (stable)

Source: [commands](https://docs.devin.ai/cli/reference/commands.md).

```
devin mcp add -s user red-ale -- /abs/path/red-ale mcp     # -s local|project|user (default local); -e KEY=VALUE
devin mcp list | get <name> | remove [-s scope] <name> | enable|disable [-s scope] <name>
```

### Detection

- `devin` in `~/.local/bin/`, or `%LOCALAPPDATA%\devin\bin\devin.exe` on Windows.
- `devin-desktop`, or the legacy `windsurf` / `surf`, in `~/.codeium/windsurf/bin/`.
- App data in `~/Library/Application Support/Devin/`, `%APPDATA%\Devin\` or `~/.config/Devin/`
  ([devin-desktop-faq](https://docs.devin.ai/desktop/devin-desktop-faq.md)).
- `~/.config/devin/` exists.

### Quirks

- **Don't write the legacy `~/.codeium/windsurf/mcp_config.json`.** It is only imported now.
- **Duplicates:** red-ale registered for Claude Code will also be imported here. How Devin handles a name collision is
  **unconfirmed**.
- **Reload:** the changelog mentions "Refresh only edited MCPs". Hot reload in Devin Local and the CLI is
  **unconfirmed**.
- **Admin allowlist:** it matches the server name and anchored regexes on `command`/`args`, so an absolute path may
  not match ([desktop/cascade/mcp, "MCP Allowlist"](https://docs.devin.ai/desktop/cascade/mcp)).

---

## 9. Zed

Source: `zed-industries/zed@5e8998a2a7`. Docs: https://zed.dev/docs/ai/mcp

### Locations

From `crates/paths/src/paths.rs:122,278,497`:
- **User file:** `<config_dir>/settings.json`. `config_dir` is:
  - Linux: `$FLATPAK_XDG_CONFIG_HOME/zed` if set, else `$XDG_CONFIG_HOME/zed`, default `~/.config/zed`;
  - macOS: `~/.config/zed` (not under `~/Library`);
  - Windows: `%APPDATA%\Zed`.
- **`--user-data-dir <dir>`** moves it to `<dir>/config` (`crates/cli/src/main.rs:97`).
- **Project:** `.zed/settings.json`. Its `context_servers` start only in trusted worktrees.
- **Don't touch `global_settings.json`** in the same directory; Zed manages it.

### Entry

- **Format:** JSONC (comments and trailing commas allowed).
- **Key:** `context_servers`. The stdio variant has `command`, `args`, `env`, `timeout`, `enabled` (default true) and
  `remote`. There is no `type` (`crates/settings_content/src/project.rs:508,596`).
- **Older forms are migrated away:** the nested `"command": {"path", "args", "env"}` form (`m_2025_06_27`) and
  `"source": "custom"` (`m_2025_11_25`) are rewritten by `crates/migrator`. Write the flat form.

```jsonc
{
  "context_servers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

### CLI

None. Zed extensions can provide servers, but that needs a WASM extension.

### Detection

- `zed` on PATH. The official script installs `~/.local/bin/zed` and `~/.local/zed.app` (`script/install.sh:57,115`).
- macOS: `/Applications/Zed.app`.
- The config directory exists.
- Distro packages may name the binary `zeditor` (**unconfirmed**).

### Quirks

- **Settings reload live,** and changed servers restart (`crates/project/src/context_server_store.rs:475,1742`). No
  restart is needed.
- **Zed edits `settings.json` itself** from its settings UI. It edits only the changed part with tree-sitter, so
  comments elsewhere survive (`crates/settings_json/src/settings_json.rs:13-23`). red-ale should do the same: patch only
  `context_servers.red-ale` as text, never round-trip the whole file through serde, and write atomically.

---

## 10. GitHub Copilot CLI

Docs: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference,
https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers,
https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference

### Locations

- **User:** `$COPILOT_HOME/mcp-config.json`, default `~/.copilot/mcp-config.json`.
  - `COPILOT_HOME` replaces the whole directory. `--config-dir` is deprecated.
  - Old XDG locations are migrated into `~/.copilot` at startup, so `XDG_CONFIG_HOME` no longer applies
    ([cli-config-dir-reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference)).
- **Project:** `.mcp.json` in any directory from the working directory up to the git root, and `.github/mcp.json`.
  Both load only in trusted folders. `.vscode/mcp.json` is not read
  ([add-mcp-servers](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)).
- **Priority:** `--additional-mcp-config` > plugins > workspace > user.
- **Other files in `~/.copilot`:** `config.json` is "managed automatically", so don't edit it. `settings.json` is the
  user settings file.

### Entry

Checked against the "Local server configuration fields" table in the command reference:
- **Required:** `command`, `args` and **`tools`** (`["*"]` for all tools).
- **Optional:** `type` (`"local"` or `"stdio"`, default `"local"`), `env` (with `$VAR`, `${VAR}`, `${VAR:-default}`),
  `cwd`, `timeout` (ms, default 30000), `deferTools`, `disableToolCache`, `slowConnectionThresholdMs`.

```json
{
  "mcpServers": {
    "red-ale": { "type": "local", "command": "/abs/path/red-ale", "args": ["mcp"], "env": {}, "tools": ["*"] }
  }
}
```

Whether the file accepts comments is not documented, so write plain JSON.

### CLI (stable, documented)

```
copilot mcp add [--env K=V]... [--tools "*"] [--timeout ms] red-ale -- /abs/path/red-ale mcp
copilot mcp list [--json] | get <name> | remove <name> | enable|disable <name>
```

- **User scope only.** There is no scope flag.
- **Interactive equivalents:** `/mcp add`, `/mcp edit`, `/mcp delete`.
- **Reload:** servers added through the CLI are available without a restart. Hand edits to a running session are not
  documented to reload.
- **Tool cache:** the CLI caches each server's tool list and refreshes it in the background on startup, so a new
  red-ale tool may appear one start late.

### Detection: the VS Code shim trap

- **A `copilot` on PATH doesn't prove the CLI is installed.** VS Code ships a shim called `copilot`
  (`microsoft/vscode@27fdd60025:copilot_shim/REQUIREMENTS.md:53-66`).
  - DEB and RPM packages add `/usr/bin/copilot` pointing to `/usr/share/code/bin/copilot-shim/copilot`, and the
    Copilot extension adds the shim to integrated-terminal PATHs.
  - **Seen on this machine:** `/usr/bin/copilot` resolves to `/usr/share/code/bin/copilot-shim/copilot`, and
    `copilot --version` prints "GitHub Copilot CLI was not found…".
- **Telling them apart:**
  1. **Path:** a resolved path whose parent directory is `copilot-shim` is the shim.
  2. **Marker:** shim binaries contain the string `VSCODE_COPILOT_RUST_SHIM_V1`, and are about 1 MB against about
     150 MB for the real CLI (REQUIREMENTS.md:555-570).
  3. **Exit code:** with stdin not a terminal, the shim never prompts. If no real CLI exists it exits **127** after one
     stderr line (REQUIREMENTS.md:688-697).
  4. **Real CLI locations:** after the PATH entries the shim would skip, the real CLI is found at `$PREFIX/bin`,
     `~/.local/bin`, or on Windows `%LOCALAPPDATA%\GitHubCopilotCLI\copilot.exe` (REQUIREMENTS.md:431-470).
- **`~/.copilot` existing isn't proof either.** VS Code creates it. Here it holds only `config.json`, `ide/` and
  `logs/`.

### Quirks

- **`tools` is required.** Leave it out and the entry may be rejected.
- **Policy:** an enterprise allowlist can block servers.
- **Copilot coding agent (cloud)** is configured on GitHub.com under repo Settings → Copilot → MCP servers, and runs
  in the cloud, so it doesn't apply here
  ([configure-mcp-servers](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers)).
- **Copilot in JetBrains** uses a separate `~/.config/github-copilot/intellij/mcp.json` with a `servers` key. This was
  seen locally and is **unconfirmed** in docs. The Windows path is unknown.

---

## 11. Goose

The repository moved from `block/goose` to `aaif-goose/goose` (`block/goose` redirects). Version checked: v1.56.0
(2026-10-23). Docs: https://goose-docs.ai/docs/guides/config-files

### Locations

| Scope | Path |
|---|---|
| User (Linux and macOS) | `$XDG_CONFIG_HOME/goose/config.yaml`, default `~/.config/goose/config.yaml`. macOS uses XDG too (`crates/goose/src/config/paths.rs:22-30`; etcetera 0.11 `choose_app_strategy`). |
| User (Windows) | `%APPDATA%\Block\goose\config\config.yaml` ("Block" kept for compatibility) (`paths.rs:19-26`). |
| Override | `GOOSE_PATH_ROOT` (absolute paths only), giving `$GOOSE_PATH_ROOT/config/config.yaml` (`paths.rs:9-11,40-46`). |
| System | `/etc/goose/config.yaml`, `%PROGRAMDATA%\goose\config.yaml` (`crates/goose/src/config/base.rs:156-172`). |
| Extra files | `GOOSE_ADDITIONAL_CONFIG_FILES` (`base.rs:175-178`). |
| Project | none found (**unconfirmed**). |

The CLI and Desktop share the one config, through the same crate and `Config::global()` (source-derived).

### Entry

- **Format:** YAML, under a top-level `extensions:` map (`crates/goose/src/config/extensions.rs:13-20`).
- **Stdio fields** (`crates/goose/src/agents/extension.rs:156-179`):
  - required: `type: stdio`, `name`, `cmd`, `args`, `enabled`;
  - optional: `envs` (alias `env`), `env_keys`, `timeout` (seconds), `cwd`, `description`, `available_tools`.
- **Map key:** should equal `name_to_key(name)`: lowercased, with spaces removed (`extensions.rs:22-31`).
- **Blocked env keys:** `envs` silently drops 31 keys, including `PATH`, `LD_LIBRARY_PATH`, `LD_PRELOAD` and
  `DYLD_*` (`extension.rs:91-152`).

```yaml
extensions:
  red-ale:
    type: stdio
    name: red-ale
    enabled: true
    cmd: /abs/path/red-ale
    args: ["mcp"]
    envs: {}
    timeout: 300
```

### CLI

- **No non-interactive add.** `goose configure` is an interactive menu
  ([using-extensions](https://goose-docs.ai/docs/getting-started/using-extensions)).
- **`goose session --with-extension "<cmd>"`** loads a server for one session only.
- **`goose mcp-probe <cmd>`** tests a stdio server (`crates/goose-cli/src/cli.rs:1210-1215`).
- **The deeplink can't register red-ale.** `goose://extension?cmd=…` accepts only commands on a fixed list (`npx`,
  `uvx`, `docker`, `jbang`, `goose`, …) (`ui/desktop/src/components/settings/extensions/deeplink.ts:15-30,113-128`).

### Detection

- `goose` on PATH. **Name clash:** `pressly/goose` is a common database-migration tool with the same name, so confirm
  with `goose --version` or `goose info`.
- `Goose.app` on macOS; Linux deb, rpm or flatpak; a Windows installer.
- The config directory exists.

### Quirks

- **Every save rewrites the whole file** with `serde_yaml`, through a locked temp file and a rename. Comments are lost
  and key order may change (`base.rs:638-675`). Edit with a structure-keeping YAML writer, and take the same lock if
  possible.
- **Restart:** "Direct edits to config files usually require restarting goose"
  ([config-files](https://goose-docs.ai/docs/guides/config-files)).
- **Absolute `cmd`:** use one, because `PATH` can't be set through `envs`.

---

## 12. Cline (VS Code extension, CLI, JetBrains)

Versions checked: Cline v4.1.26 and cli-v3.0.74 source. Docs: https://docs.cline.bot/mcp/mcp-overview.md,
https://docs.cline.bot/getting-started/config.md

### Locations (Cline ≥ 4.0.0, 2026-06-26)

- **One shared file:** `~/.cline/data/settings/cline_mcp_settings.json`. The MCP overview gives it for the CLI (checked,
  override `CLINE_MCP_SETTINGS_PATH`).
  - That the IDE extension and JetBrains use the same file comes from source and the 4.0.0 changelog
    (`apps/vscode/src/hosts/vscode/mcp-settings-legacy-migration.ts:254-266`; `sdk/packages/shared/src/storage/paths.ts:158-192,447-453`).
- **Override order:** `CLINE_MCP_SETTINGS_PATH` > `CLINE_DATA_DIR/settings/` > `CLINE_DIR/data/settings/` >
  `~/.cline/data/settings/`. The CLI also takes `--config <dir>`.
- **Legacy files are migrated once and then ignored on 4.x** (`mcp-settings-legacy-migration.ts:237-378`):
  - `<editor user data>/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`;
  - `~/Documents/Cline/MCP/cline_mcp_settings.json`.
- **No project scope** is documented.

### Entry

- **Format:** strict JSON (`JSON.parse`).
- **Shapes:** the documented flat form, and a nested `{"transport": {...}}` form that `cline mcp add` writes. Both are
  accepted (`sdk/packages/core/src/extensions/mcp/config-loader.ts:63-220`; `apps/cli/src/wizards/mcp/settings.ts:101-104`).
- **Fields:** `command`, `args`, `env`, `cwd`, `timeout` (seconds), `disabled`, `autoApprove`. `type` is optional.

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "disabled": false, "autoApprove": [] }
  }
}
```

### CLI (npm `cline`)

Source: [cli-reference](https://docs.cline.bot/cli/cli-reference.md).

```
cline mcp add red-ale --yes -- /abs/path/red-ale mcp     # without --yes: interactive wizard, needs a TTY
cline mcp remove <name>
cline config mcp [--json]                                # list
```

`add` has **no env flag** (`sdk/packages/core/src/services/mcp-install.ts:8-14`).

### Detection

- `cline` on PATH, or `~/.cline/` exists.
- VS Code extension id `saoudrizwan.claude-dev` (`code --list-extensions`).

### Quirks

- **Concurrent writers:** the CLI, every VS Code window and JetBrains can all write the file. Cline locks it with a
  `<file>.lock` **directory**, stale after 10 s, and writes through a temp file and a rename
  (`config-loader.ts:292-349`). red-ale should use `cline mcp add --yes`, or honour that lock and write atomically.
- **Reload:** the extension watches the file. Per-server `timeout` changes apply from the next session.

---

## 13. Kiro (IDE and `kiro-cli`)

Docs: https://kiro.dev/docs/mcp/configuration.md, https://kiro.dev/docs/reference/cli-commands.md. Kiro CLI, formerly
Amazon Q Developer CLI, is closed source. The open-source predecessor `aws/amazon-q-developer-cli@15cc8f3cd1` is
supporting evidence only.

### Locations

- **User:** `~/.kiro/settings/mcp.json`. **Workspace:** `.kiro/settings/mcp.json`. Both the IDE and the CLI read them,
  and the workspace wins.
- **Agent configs:** `~/.kiro/agents/*.json` and `.kiro/agents/*.json` can carry their own `mcpServers`. These rank
  above both files. With `includeMcpJson: true` an agent also gets the `mcp.json` servers
  ([custom-agents/configuration-reference](https://kiro.dev/docs/custom-agents/configuration-reference.md)). Users of
  custom agents may not see red-ale unless that flag is set; the default is **unconfirmed**.
- **`KIRO_HOME`** moves the global Kiro directory
  ([cli-commands, "Environment Variables"](https://kiro.dev/docs/reference/cli-commands.md)).
- **Windows:** `%USERPROFILE%\.kiro\settings\mcp.json` is **unconfirmed**.

### Entry

- **Fields:** `command`, `args`, `env`, `disabled`, `autoApprove`, `disabledTools`. No `type`.
- **Env expansion:** `${VAR}`. The IDE expands only variables the user has approved in settings.
- **Format:** JSON. JSONC support is **unconfirmed**.

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "disabled": false, "autoApprove": [] }
  }
}
```

### CLI

```
kiro-cli mcp add --name red-ale --scope global --command /abs/path/red-ale --args mcp [--env k=v,...] [--force]
kiro-cli mcp remove --name red-ale [--scope global]
kiro-cli mcp list [workspace|global]
```

- **Scope** is `workspace` or `global` ([cli-commands](https://kiro.dev/docs/reference/cli-commands.md)).
- **`--args`** is missing from the reference table but appears in the [MCP docs](https://kiro.dev/docs/mcp.md)
  examples.
- **Predecessor behaviour** (current Kiro **unconfirmed**): without `--force` it refuses an existing name, and without
  `--scope` it writes global (`crates/chat-cli/src/cli/mcp.rs:88-180`).
- **Install link:** `https://kiro.dev/launch/mcp/add?name=red-ale&config=<urlencoded server JSON>` shows a
  confirmation ([mcp/servers](https://kiro.dev/docs/mcp/servers.md)).

### Detection

- `kiro-cli` on PATH (`~/.local/bin` for zip installs), the `kiro` IDE shell command, or `~/.kiro/`.
- App paths are **unconfirmed**.

### Quirks

- **Hot reload:** a file watcher reloads `mcp.json`, and changed servers restart between turns
  ([configuration, "Hot-reload"](https://kiro.dev/docs/mcp/configuration.md)).
- **CLI writes** re-serialise the file through serde (predecessor), so comments and unknown keys may be lost
  (**unconfirmed** for Kiro).

---

## 14. Amp

Docs: https://ampcode.com/docs/cli/settings, https://ampcode.com/docs/customize/mcp. The CLI is closed source; the
help strings below come from `strings` on `@ampcode/cli-linux-x64` 0.0.1793192846, not from running it.

### Status

- **The VS Code and Cursor extensions are gone.** They "will self-destruct on March 5" (2026), per the post of
  2026-02-19 ([the-coding-agent-is-dead](https://ampcode.com/news/the-coding-agent-is-dead)). Only the CLI matters.
- **The npm package is now `@ampcode/cli`** ([npm-package-changes](https://ampcode.com/news/npm-package-changes)).

### Locations

- **User:** `~/.config/amp/settings.json` or `settings.jsonc`; on Windows `%USERPROFILE%\.config\amp\` (docs).
  Whether `XDG_CONFIG_HOME` moves it is **unconfirmed**.
- **Override:** `AMP_SETTINGS_FILE` or `--settings-file`.
- **Workspace:** the nearest `.amp/settings.json[c]` up to the repository root.
- **MCP precedence:** `--mcp-config` > workspace > user > skill-provided `mcp.json`
  ([customize/mcp](https://ampcode.com/docs/customize/mcp)).

### Entry

- **The top-level key is literally `"amp.mcpServers"`**, with the dot in the key name.
- **Fields:** `command`, `args`, `env`. No `type`. `${VAR}` expansion works.
- **Format:** JSON or JSONC.

```json
{
  "amp.mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

### CLI (documented)

```
amp mcp add red-ale [--env KEY=VAL]... [--workspace] -- /abs/path/red-ale mcp    # default: global settings
amp mcp remove <name>       # workspace first, then global
amp mcp list | doctor [name] | approve <name>
```

### Detection

- `amp` on PATH. The curl installer puts it in `~/.amp/bin`, or `$AMP_HOME/bin` (`https://ampcode.com/install.sh`).
- `~/.config/amp/` exists.

### Quirks

- **Workspace servers need `amp mcp approve`;** user servers don't, so register at user scope.
- **Unconfirmed:** whether the CLI keeps comments when it writes `settings.jsonc`, and whether running sessions reload.

---

## 15. JetBrains IDEs

**IDE config directory**
([directories doc](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html)):
- Linux: `~/.config/JetBrains/<Product><Version>/`
- macOS: `~/Library/Application Support/JetBrains/<Product><Version>/`
- Windows: `%APPDATA%\JetBrains\<Product><Version>\`
- The `idea.config.path` property moves it.
- Detection: these directories exist.

### 15a. AI Assistant

- **Configuration is through the UI.** Settings | Tools | AI Assistant | Model Context Protocol (MCP) → Add takes
  pasted JSON in the `{"mcpServers": {...}}` shape. Each server has a level, Global or Current project. Apply starts
  the server. An **Import from Claude** button imports Claude Desktop's servers
  ([ai-assistant/mcp](https://www.jetbrains.com/help/ai-assistant/mcp.html)).
- **On-disk storage is undocumented** (**unconfirmed**).
  - Locally, `options/McpToolsStoreService.xml` holds a JSON array inside an XML attribute. Whether that is the client
    config is unclear.
  - The IDE writes `options/*.xml` on exit, so a hand edit would be overwritten.
- **Recommendation:** don't write it. Print the JSON snippet, or point the user at "Import from Claude" after
  registering with Claude Desktop.

### 15b. Junie (IDE plugin and Junie CLI)

- **Files:** user `~/.junie/mcp/mcp.json`; project `.junie/mcp/mcp.json`
  ([junie-cli-mcp-configuration](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html)).
  - The plugin's MCP Settings page opens an `mcp.json` but names no path
    ([junie-plugin-mcp-settings](https://junie.jetbrains.com/docs/junie-plugin-mcp-settings.html)). That the plugin
    shares the CLI's user file is **unconfirmed**.
  - The older `~/.junie/mcp.json` path is outdated.
  - The Windows path is **unconfirmed**.
- **Relocation:** `--mcp-location <path>` / `mcp-locations` in `config.json` add search folders
  ([junie-cli-configuration](https://junie.jetbrains.com/docs/junie-cli-configuration.html)).
- **Entry:** plain JSON, no `type`.

```json
{
  "mcpServers": {
    "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} }
  }
}
```

- **CLI:** `junie` (beta since March 2026, [junie-cli](https://junie.jetbrains.com/docs/junie-cli.html)) has no
  `mcp add`, only the interactive `/mcp` screen. Hand-added servers are "imported… and enabled by default". Use `/mcp`
  → Reconnect.
- **Detection:** `junie` on PATH, or `~/.junie/` exists.
- **JetBrains Air** (a newer agentic IDE) uses project `.air/mcp.json`
  ([air/mcp-servers](https://www.jetbrains.com/help/air/mcp-servers.html)). Its user scope was not researched.

---

## Additions: harnesses not in the original list

### Google Antigravity (IDE, desktop app, `agy` CLI)

Antigravity replaced Gemini CLI for individual users on 2026-06-18.

- **Files:** global `~/.gemini/config/mcp_config.json`, shared by the IDE, app and CLI; workspace
  `.agents/mcp_config.json` ([antigravity docs/mcp](https://antigravity.google/docs/mcp)).
  - Older guides use `~/.gemini/antigravity/mcp_config.json`, which is outdated.
  - The Windows path is **unconfirmed**.
- **Entry:** `mcpServers` with `command`, `args`, `env`, `cwd`, `disabled`, `enabledTools`, `disabledTools`,
  `timeoutSeconds`. No `type`. Remote servers use `serverUrl`.

  ```json
  { "mcpServers": { "red-ale": { "command": "/abs/path/red-ale", "args": ["mcp"], "env": {} } } }
  ```
- **CLI:** the CLI changelog mentions `agy mcp add|remove|list|enable|disable` for user config
  ([changelog](https://www.antigravity.google/docs/changelog?tab=cli)). The exact syntax is **unconfirmed**.
  `agy plugin import gemini` imports Gemini CLI extensions
  ([gcli-migration](https://antigravity.google/docs/cli/gcli-migration)).
- **Detection:** `agy` on PATH (**unconfirmed** location `~/.local/bin`), or `~/.gemini/config/`. Don't confuse it
  with Gemini CLI's `~/.gemini/settings.json`.
- **Reload:** `/mcp` in the CLI, or Refresh in the IDE.

### Visual Studio 2022 (17.14+) and 2026, Windows only

- **Files, read in order:** `%USERPROFILE%\.mcp.json` (global), `<solution>\.vs\mcp.json`, `<solution>\.mcp.json`,
  `<solution>\.vscode\mcp.json`, `<solution>\.cursor\mcp.json`
  ([visualstudio/ide/mcp-servers](https://learn.microsoft.com/en-us/visualstudio/ide/mcp-servers), updated 2026-10-14).
- **Entry:** the VS Code-style `servers` key. The docs show only an HTTP example; the stdio shape is inferred from
  VS Code's schema (**unconfirmed**).

  ```json
  { "servers": { "red-ale": { "type": "stdio", "command": "C:\\path\\red-ale.exe", "args": ["mcp"] } } }
  ```
- **Reload:** saving reloads automatically.
- **Tools start disabled.** A new server's tools must be enabled in the tool picker. VS 2026 18.7+ also shows a trust
  dialog when a server's config changes.
- **CLI:** none. **Detection:** `vswhere.exe` under `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\`
  (**unconfirmed** as an MCP signal).
- **Note:** `%USERPROFILE%\.mcp.json` is a generic name that other tools may also read.

### Continue (VS Code/JetBrains extension, `cn` CLI)

- **Files:**
  - Global directory: `~/.continue`, or `CONTINUE_GLOBAL_DIR` (`continuedev/continue:core/util/paths.ts:27-35`).
  - Block files are loaded from `~/.continue/mcpServers/*.yaml` and `<workspace>/.continue/mcpServers/*.yaml`
    (`core/config/yaml/loadYaml.ts:57-62`).
  - **Write our own `~/.continue/mcpServers/red-ale.yaml`** and never edit `config.yaml`
    ([deep-dives/mcp](https://docs.continue.dev/customize/deep-dives/mcp)).
- **Entry:** YAML. A block file needs `name`, `version` and `schema`, and `mcpServers` is a **list**
  ([reference](https://docs.continue.dev/reference)).

  ```yaml
  name: red-ale
  version: 0.0.1
  schema: v1
  mcpServers:
    - name: red-ale
      type: stdio
      command: /abs/path/red-ale
      args: ["mcp"]
  ```
- **Agent mode only:** MCP works only in Agent mode.
- **CLI:** none (**unconfirmed** for `cn`).
- **Detection:** `~/.continue/`, or the VS Code extension `Continue.continue`.

### Qwen Code (`qwen`, a Gemini CLI fork)

- **Files:** user `~/.qwen/settings.json` (moves with `QWEN_HOME`, `QwenLM/qwen-code:packages/core/src/config/storage.ts:193-202`);
  project `.qwen/settings.json` ([qwen-code docs, mcp](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/)).
- **Entry:** `mcpServers` with `command`, `args`, `env`, `cwd`, `timeout`, `trust`. No `type`.
  `{"mcpServers": {"red-ale": {"command": "/abs/path/red-ale", "args": ["mcp"]}}}`
- **CLI:** `qwen mcp add -s user red-ale /abs/path/red-ale mcp`; `qwen mcp list`; `qwen mcp remove <name>`.
- **Reload:** restart a running session.
- **Detection:** `qwen` on PATH, or `~/.qwen/`.

### Kilo Code (extension and `kilo` CLI, an opencode fork)

- **Files:** global `~/.config/kilo/kilo.json[c]` (or `KILO_CONFIG_DIR`); project `kilo.json[c]`, `.kilo/` or
  `.kilocode/` ([using-mcp-in-kilo-code](https://kilo.ai/docs/features/mcp/using-mcp-in-kilo-code);
  `Kilo-Org/kilocode:packages/opencode/src/cli/cmd/mcp.ts`).
- **Entry:** the opencode V1 shape, `{"mcp": {"red-ale": {"type": "local", "command": ["/abs/path/red-ale", "mcp"], "enabled": true}}}`.
- **CLI:** `kilo mcp add red-ale -- /abs/path/red-ale mcp` (global, non-interactive).
- **Detection:** `kilo` on PATH, or `~/.config/kilo/`. The npm package name and extension id are **unconfirmed**.

### Factory Droid (`droid`)

Source: [factory docs, mcp](https://docs.factory.com/cli/configuration/mcp).

- **Files:** `~/.factory/mcp.json`; project or ancestor `.factory/mcp.json`.
- **Entry:** `mcpServers` with `command`, `args`, `env`, `disabled`; `type: "stdio"` optional.
- **CLI:** `droid mcp add red-ale /abs/path/red-ale mcp [--env K=V]`, plus `list` and `remove`.
- **Reload:** automatic when `mcp.json` changes.

### Crush (Charmbracelet)

Sources: `charmbracelet/crush` README §Configuration and `docs/config/README.md`; release v0.98.2.

- **Files:**
  - The config is now **`crushrc`**, a Bash script run by Crush's built-in interpreter.
  - Lookup: `./.crushrc`, `./crushrc`, then `$XDG_CONFIG_HOME/crush/crushrc`.
  - `crush.json` is deprecated but supported. If both formats are in one directory they merge, with a warning.
  - `CRUSH_GLOBAL_CONFIG` moves the global config.
- **Entry:** the crushrc line `mcp add red-ale --command /abs/path/red-ale --args mcp`. It is idempotent: it adds the
  server or updates one with the same name.
  - Legacy JSON form: `{"mcp": {"red-ale": {"type": "stdio", "command": "...", "args": ["mcp"]}}}`.
- **Quirk:** edit whichever format already exists. A shell-level `crush mcp add` is **unconfirmed**.

### Warp

- **Files:** `~/.warp/.mcp.json` and project `.warp/.mcp.json`, with `mcpServers`.
- **It also reads other harnesses' files:** Claude Code's `~/.claude.json` and `.mcp.json`, Codex's
  `~/.codex/config.toml`, and `~/.agents/.mcp.json`. Third-party servers auto-start only if the user enables
  "Auto-spawn servers from third-party agents"
  ([warp docs, mcp](https://docs.warp.dev/agents/capabilities/mcp.md)).
- **No dedicated step needed:** registering with Claude Code or Codex already reaches Warp. An open bug (#9879) reports
  that `~/.warp/.mcp.json` servers don't auto-start.

### Considered and skipped

- **Roo Code:** archived on GitHub on 2026-05-15 (`gh api repos/RooCodeInc/Roo-Code` gives `archived: true`).
- **LM Studio:** uses an `mcp.json` in Cursor's `mcpServers` notation ([lmstudio docs](https://lmstudio.ai/docs/app/mcp)).
  The path `~/.lmstudio/mcp.json` is **unconfirmed**, and it is niche for this use.

---

## Unconfirmed, collected

- **Claude Code:** whether a running session picks up a new server without restarting. `.claude.json` moving to
  `$CLAUDE_CONFIG_DIR/` is tested but not documented.
- **Codex:** whether a running TUI needs a restart. WSL config is separate from Windows.
- **opencode:** whether V2 ignores flat V1 entries (third-party report vs source). V1 hot reload. V2 support for
  `OPENCODE_CONFIG`.
- **Claude Desktop:**
  - Linux and MSIX paths are source-derived.
  - `CLAUDE_USER_DATA_DIR` is undocumented.
  - Whether chat-surface servers see the shell PATH.
- **Cursor:** Windows path (docs). JSONC tolerance. Whether any env var moves `~/.cursor`. Whether the app rewrites
  `mcp.json`.
- **VS Code:** Insiders directory name. `--add-mcp` from WSL or remote shells.
- **Devin:** whether `mcp_config.json` accepts comments. Hot reload. Behaviour on duplicate names.
- **Zed:** distro binary names.
- **Copilot CLI:** comments in `mcp-config.json`. The Copilot-in-JetBrains file path.
- **Goose:** no project scope.
- **Kiro:** Windows path. JSONC. `add` behaviour on duplicates. `includeMcpJson` default.
- **Amp:** `XDG_CONFIG_HOME`. Comment preservation. Reload.
- **JetBrains:** AI Assistant storage. Junie Windows path, and whether the plugin shares the CLI's file.
- **Antigravity:** Windows path. `agy mcp add` syntax.
- **Visual Studio:** stdio entry shape.
- **Crush:** shell-level `mcp add`.
- **LM Studio:** path.
