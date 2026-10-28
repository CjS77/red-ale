# `red-ale setup`: registering red-ale with agent harnesses

**Status: approved.** To be built in v0.5 ([`milestone5.md`](milestone5.md)).

`red-ale setup` registers red-ale's MCP server (`red-ale mcp`, [`mcp.md`](mcp.md)) with every agent harness installed
on the machine, and removes it again. The install scripts run it once the binary is in place ([Installation](#installation)); users run it
themselves after installing a new harness.

The research behind the per-harness details is [`notes/mcp-harnesses.md`](notes/mcp-harnesses.md).

## Terms

These go into [`glossary.md`](glossary.md) under a new "Installation" section.

- **Harness:** a program that runs an AI agent and starts MCP servers for it: Claude Code, Codex, opencode and so on.
  One name for all of them, whatever the harness calls itself (client, host, IDE, agent).
- **Harness config:** the file, or the harness's own CLI, where a harness keeps its MCP servers.
- **red-ale entry:** the one server entry `setup` owns in a Harness config, keyed by the name `red-ale`.
- **Setup plan:** what `setup` will do to each Harness config, shown before anything is changed.

## What it is not

- **Not a JSON-RPC method or MCP tool.** The mission says every operation is reachable over JSON-RPC. `setup` is the
  one deliberate exception, because:
    - it changes the machine, not a Book;
    - it must work before any server exists;
    - as an MCP tool it would let one agent rewrite every other agent's configuration.
- **Not a downloader.** Fetching and verifying the binary is the install scripts' job.
- **No HTTP transport.** Every entry starts `red-ale mcp` over stdio, the only transport red-ale serves.
- **No project scope by default.** Entries go in each harness's user config. Project files (`.mcp.json`,
  `.codex/config.toml`, …) are checked into other people's repositories, and `setup` does not write there unless
  asked (`--project`).

## Command line

```
red-ale setup [--only <harness,...>] [--skip <harness,...>] [--project <dir>]
              [--config <file>] [--dry-run] [--yes] [--remove]
red-ale setup --list
```

| Option             | Meaning                                                                                                                |
|--------------------|------------------------------------------------------------------------------------------------------------------------|
| (none)             | Detect every harness, show the Setup plan, ask once, apply it                                                          |
| `--list`           | Show each supported harness: installed or not, its Harness config, and whether it has a red-ale entry. Changes nothing |
| `--only`, `--skip` | Limit the harnesses, by the ids in [Harnesses](#harnesses)                                                             |
| `--project <dir>`  | Write project-scoped entries in `<dir>` instead of user ones, for the harnesses that have them                         |
| `--config <file>`  | A Configuration file other than the default, which the entry then passes to `red-ale mcp`                              |
| `--dry-run`        | Show the Setup plan and the exact change to each file, then stop                                                       |
| `--yes`            | Apply without asking. Required when standard input is not a terminal                                                   |
| `--remove`         | Remove the red-ale entry from every Harness config instead                                                             |

Exit status: 0 when every selected harness is set up (or already was); 1 when any failed, after applying the others;
2 for bad arguments. A harness that isn't installed is not a failure unless it was named with `--only`.

## The red-ale entry

Every harness gets the same server, translated into its format:

- **name:** `red-ale`;
- **command:** the absolute path of the running binary (`std::env::current_exe`, canonicalised). Absolute because
  desktop apps on macOS and Windows don't inherit the shell's `PATH`. The install scripts put the binary at a fixed
  path and upgrades replace it in place, so the entry stays valid across upgrades;
- **args:** `["mcp"]`, or `["mcp", "--config", <file>]` when `setup` was given `--config <file>`;
- **env:** none.

Every `red-ale mcp` started without `--config` uses the default Configuration file
([`api/configuration.md`](api/configuration.md), "Which file"), so every harness sees the same settings with no
option in its entry. `setup` doesn't create or change the Configuration.

## How it applies

1. **Detect.** For each harness, look for its binary on `PATH`, its app bundle or install directory, and its config
   directory. Any one counts as installed.
2. **Plan.** Read each Harness config and decide one action per harness:
    - *add*: no red-ale entry;
    - *update*: a red-ale entry that differs, shown as a diff;
    - *nothing*: an identical entry;
    - *skip*: the harness can't be set up, with the reason (file unreadable or invalid, harness CLI failed).
3. **Show and ask.** Print the plan; ask once (`Apply? [y/N]`) unless `--yes`. With `--dry-run`, print each file's
   exact change and stop.
4. **Apply** each harness on its own, so one failure doesn't stop the rest:
    - where the harness has a stable CLI for its servers, run it (it owns its format);
    - otherwise edit the file, under the rules below.
5. **Report** per harness what changed, the backup made, and whether the harness must be restarted.

### Editing a Harness config

These rules hold for every file `setup` writes:

- **Only the red-ale entry.** Every other server, setting, comment, key order and blank line is left as it was.
  JSON and JSON with comments are edited in place through a syntax tree that keeps the file's text (`jsonc-parser`, MIT), so only the
  red-ale entry's bytes change; a strict-JSON file gets no comments. TOML and YAML
  adapters, when a harness needs them, use editors that keep layout the same way.
- **Never a file it can't parse.** An invalid or unreadable file is *skip*, with the parse error. It is never
  replaced.
- **Back up first.** Before the first change, copy the file to `<file>.red-ale-backup-<UTC timestamp>`. Keep the
  original's permissions.
- **Atomic.** Write a temporary file in the same directory, flush it to disk, then rename it over the original. A
  symlinked config is resolved and its target written, so the link survives.
- **Not changed underneath.** Just before the rename, read the file again. If it changed since `setup` planned (the harness wrote it
  meanwhile), drop the change and plan again, up to three times, then *skip*.
- **Not while the app overwrites it.** A harness that holds its config in memory and writes it all back (Claude
  Desktop) is edited only while it isn't running.
- **Create when missing.** If the harness is installed but its config file doesn't exist yet, create it with only
  the red-ale entry.
- **Idempotent.** A second run is *nothing* everywhere and writes no file.

An existing red-ale entry is `setup`'s to change: *update* replaces it whole, after showing the diff. A user who
customised it (for example with `--book`) sees that in the diff and can answer no, or use `--skip`.

### Removing

`--remove` deletes the red-ale entry from each Harness config under the same rules (plan, ask, back up, atomic). It
leaves the Configuration file, which holds the user's settings.

## Harnesses

Paths, keys and quirks are cited in [`notes/mcp-harnesses.md`](notes/mcp-harnesses.md); `~` is `%USERPROFILE%` on
Windows. **CLI** means `setup` runs the harness's own command; **file** means it edits the Harness config under the
rules above.

### First release

| Id               | Harness                                                   | How                                                                                                                                                                                            | Harness config and entry                                                                                                                                                                                                                                           |
|------------------|-----------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `claude-code`    | Claude Code                                               | CLI: `claude mcp add -s user red-ale -- <abs> mcp`; `claude mcp remove -s user red-ale` first when updating, since `add` refuses an existing name                              | `~/.claude.json`, `mcpServers`. Never edited directly: Claude Code rewrites it constantly                                                                                                                                                                          |
| `codex`          | Codex (CLI, IDE extension and ChatGPT app share one file) | CLI: `codex mcp add red-ale -- …`, `codex mcp remove red-ale`                                                                                                                                  | `$CODEX_HOME/config.toml` (default `~/.codex/`), `[mcp_servers.red-ale]`                                                                                                                                                                                           |
| `opencode`       | opencode                                                  | File, after detecting the major version (`opencode --version`). Its CLI has no remove, and V2's `add` writes the project config by default                                                     | `$XDG_CONFIG_HOME/opencode/opencode.json[c]` on every OS (`~\.config\opencode` on Windows), JSONC. V1: `mcp.red-ale`; V2: `mcp.servers.red-ale`. Both: `type: "local"`, one `command` array holding the program and its arguments                                  |
| `claude-desktop` | Claude Desktop                                            | File, only while the app is not running: it holds the whole file in memory and writes it back, erasing a concurrent edit. Running: *skip*, "quit Claude Desktop and run `red-ale setup` again" | `claude_desktop_config.json` in `~/.config/Claude/`, `~/Library/Application Support/Claude/` or `%APPDATA%\Claude\` (the MSIX install reads `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude\`). Strict JSON, `mcpServers`, no `type` field |
| `cursor`         | Cursor (IDE and `agent` CLI)                              | File (no CLI add)                                                                                                                                                                              | `~/.cursor/mcp.json`, `mcpServers`                                                                                                                                                                                                                                 |
| `vscode`         | VS Code with Copilot                                      | File (`code --add-mcp` can't remove)                                                                                                                                                           | `User/mcp.json` in `~/.config/Code/`, `~/Library/Application Support/Code/` or `%APPDATA%\Code\`; JSONC, `servers`, `type: "stdio"`. Default profile only                                                                                                          |
| `copilot-cli`    | GitHub Copilot CLI                                        | CLI: `copilot mcp add red-ale -- …`                                                                                                                                                            | `$COPILOT_HOME/mcp-config.json` (default `~/.copilot/`), `mcpServers`, with `tools: ["*"]`. Detection must not mistake VS Code's `copilot-shim` on `PATH` for the CLI                                                                                              |
| `gemini`         | Gemini CLI                                                | CLI: `gemini mcp add -s user red-ale <abs> mcp …`, `gemini mcp remove -s user red-ale`                                                                                                         | `~/.gemini/settings.json` (`GEMINI_CLI_HOME`), `mcpServers`                                                                                                                                                                                                        |
| `antigravity`    | Antigravity                                               | File (its CLI's syntax is unconfirmed)                                                                                                                                                         | `~/.gemini/config/mcp_config.json`, `mcpServers`                                                                                                                                                                                                                   |
| `devin`          | Devin Desktop and Devin CLI (formerly Windsurf)           | CLI when `devin` is on `PATH` (`devin mcp add -s user red-ale -- …`), else file                                                                                                                | `~/.config/devin/mcp_config.json` (`%APPDATA%\devin\` on Windows), `mcpServers`. The old `~/.codeium/windsurf/` file is only imported now and is not written                                                                                                       |
| `zed`            | Zed                                                       | File                                                                                                                                                                                           | `settings.json` in `~/.config/zed/` (macOS too) or `%APPDATA%\Zed\`; JSONC, `context_servers`. Zed writes the file while running, so the change check below matters most here                                                                                      |

### Later

Goose, Cline (≥ 4.0), Kiro, Amp, JetBrains Junie, Qwen Code, Kilo Code, Factory Droid, Continue, Visual Studio,
Warp and Crush. Each is one more adapter. JetBrains AI Assistant keeps its servers in undocumented IDE settings, so
`setup --list` prints the JSON to paste into its settings page instead. Roo Code is archived and not supported.

### One name everywhere

Several harnesses read each other's configs: VS Code reads Copilot CLI's, Devin imports Claude Code's, Warp reads
Claude Code's and Codex's. Using the one name `red-ale` everywhere means such a harness sees one server, not two.

## Installation

The install scripts call `setup` as their last step:

```
curl -fsSL https://github.com/CjS77/red-ale/releases/latest/download/install.sh | sh
irm https://github.com/CjS77/red-ale/releases/latest/download/install.ps1 | iex
```

Each script:

1. picks the release archive for the machine (the asset names in `.github/workflows/release.yml`);
2. checks its SHA-256;
3. installs the binary at a fixed path (`~/.local/bin/red-ale` on Linux and macOS, `%LOCALAPPDATA%\Programs\red-ale\`
   on Windows) and says so if that directory is not on `PATH`;
4. runs `red-ale setup`, reading answers from the terminal (`/dev/tty`), since the script itself arrives on standard
   input.

`--no-setup` skips step 4; options after `--` are passed to `setup`. The scripts are a separate ticket; this
document fixes only what they rely on.

## Where the code lives

A new crate, `red-ale-setup`, used only by the binary. It has no access to the core and knows nothing about Books.
Each harness is one adapter behind a trait (detect, read entry, plan, apply, remove), translating the red-ale entry
into that harness's format. That keeps each external format out of the rest of the code, as the file Backends do for
GnuCash's.

## Testing

Tests never touch the real home directory. Each runs with `HOME`, `XDG_CONFIG_HOME`, `APPDATA` and the harness's own
variables (`CODEX_HOME`, …) pointed at a temporary directory.

- **Per adapter, on sample configs** (empty, missing, with other servers, with comments, with an old red-ale entry):
    - the result equals a golden file;
    - every byte outside the red-ale entry is unchanged;
    - applying twice equals applying once;
    - removing after adding gives back the original file.
- **Invalid files** are *skip*, and the file is byte-for-byte unchanged with no backup made.
- **Harness CLIs** are tested with a fake executable on `PATH` that records its arguments.
- **Atomicity:** an injected failure before the rename leaves the original untouched.
- **A smoke test per harness, run by hand before a release:** set up, start the harness, list its MCP tools, see
  red-ale's.

## Decided

1. **Harness CLI or file edit** where both exist: the CLI when it is stable, since the harness owns its format; the
   file when the CLI is experimental or missing.
2. **Which harnesses ship first:** the eleven under "First release", the rest later.
3. **Claude Desktop while running:** skipped, saying why.
4. **The Configuration:** entries pass no `--config`; every `red-ale mcp` uses the default Configuration file
   ([`api/configuration.md`](api/configuration.md), "Which file").
