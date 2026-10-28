//! The CLI adapters, each run against a fake Harness CLI on `PATH` that records its arguments
//! (`design/setup.md`, "Testing"). `PATH` and the home directory are set in the `Env` given to the adapter, never in
//! this process's environment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use red_ale_setup::{Action, Entry, Env, Error, Harness, Os, Probe, Scope, harness};

struct NoProbe;

impl Probe for NoProbe {
    fn running(&self, _names: &[&str]) -> bool {
        false
    }

    fn output(&self, _program: &Path, _args: &[&str]) -> Option<String> {
        None
    }
}

const RED_ALE: &str = "/opt/red-ale/bin/red-ale";

/// A temporary home with fake CLIs in `~/bin`, which log `<working directory>: <arguments>` and fail when
/// `FAKE_FAIL` is set.
struct Machine {
    home: tempfile::TempDir,
    fail: bool,
}

impl Machine {
    fn new(programs: &[&str]) -> Self {
        let machine = Self { home: tempfile::tempdir().unwrap(), fail: false };
        programs.iter().for_each(|p| Machine::fake(&machine.bin(), p, ""));
        machine
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn bin(&self) -> PathBuf {
        self.home().join("bin")
    }

    fn fake(dir: &Path, program: &str, extra: &str) {
        fs::create_dir_all(dir).unwrap();
        let script = format!(
            "#!/bin/sh\n{extra}\nprintf '%s: %s\\n' \"$(pwd)\" \"$*\" >> \"$FAKE_LOG\"\n\
             if [ -n \"$FAKE_FAIL\" ]; then echo \"{program}: no such server\" >&2; exit 1; fi\n"
        );
        let path = dir.join(program);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn env(&self, path: &[PathBuf]) -> Env {
        let mut vars = vec![
            ("HOME".to_owned(), OsString::from(self.home())),
            ("PATH".to_owned(), std::env::join_paths(path).unwrap()),
            ("FAKE_LOG".to_owned(), OsString::from(self.home().join("log"))),
        ];
        if self.fail {
            vars.push(("FAKE_FAIL".to_owned(), OsString::from("1")));
        }
        Env::new(vars, Os::Linux, self.home().join("root"), Box::new(NoProbe))
    }

    fn default_env(&self) -> Env {
        self.env(&[self.bin()])
    }

    /// The calls the fake CLIs logged, and forget them.
    fn calls(&self) -> Vec<String> {
        let log = self.home().join("log");
        let calls = fs::read_to_string(&log).unwrap_or_default().lines().map(str::to_owned).collect();
        let _ = fs::remove_file(log);
        calls
    }

    /// The arguments of the calls logged, without their working directory, and forget them.
    fn args(&self) -> Vec<String> {
        self.calls().iter().map(|c| c.split_once(": ").map_or(c.clone(), |(_, args)| args.to_owned())).collect()
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.home().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn entry() -> Entry {
    Entry::new(Path::new(RED_ALE), None).unwrap()
}

fn apply_set(harness: &dyn Harness, env: &Env, scope: &Scope) -> Result<Action, Error> {
    let plan = harness.plan(env, scope, &entry());
    harness.apply(env, plan).map(|a| a.action)
}

/// What `claude mcp add -s user` writes (§1 "Entry"), with `command` set.
fn claude_json(command: &str) -> String {
    let entry = format!(r#"{{ "type": "stdio", "command": "{command}", "args": ["mcp"], "env": {{}} }}"#);
    format!(r#"{{ "numStartups": 3, "mcpServers": {{ "red-ale": {entry} }} }}"#)
}

#[test]
fn claude_code_adds_updates_by_removing_first_and_removes() {
    let machine = Machine::new(&["claude"]);
    let env = machine.default_env();
    let claude = harness("claude-code").unwrap();
    let home = machine.home().display().to_string();
    assert!(claude.detect(&env));

    let plan = claude.plan(&env, &Scope::User, &entry());
    assert!(matches!(plan.action, Action::Add));
    let shown = plan.exact_change().unwrap();
    assert_eq!(shown, format!("$ {home}/bin/claude mcp add -s user red-ale -- {RED_ALE} mcp\n"));
    claude.apply(&env, plan).unwrap();
    assert_eq!(machine.args(), [format!("mcp add -s user red-ale -- {RED_ALE} mcp")]);
    assert!(!machine.home().join(".claude.json").exists(), "the file is Claude Code's to write");

    machine.write(".claude.json", &claude_json(RED_ALE));
    assert!(matches!(apply_set(claude.as_ref(), &env, &Scope::User).unwrap(), Action::Nothing));
    assert!(machine.calls().is_empty(), "nothing to do runs nothing");

    machine.write(".claude.json", &claude_json("/usr/local/bin/red-ale"));
    assert!(matches!(apply_set(claude.as_ref(), &env, &Scope::User).unwrap(), Action::Update { .. }));
    let expected = ["mcp remove -s user red-ale".to_owned(), format!("mcp add -s user red-ale -- {RED_ALE} mcp")];
    assert_eq!(machine.args(), expected);

    let removal = claude.plan_removal(&env, &Scope::User);
    assert!(matches!(claude.apply(&env, removal).unwrap().action, Action::Remove));
    assert_eq!(machine.args(), ["mcp remove -s user red-ale".to_owned()]);
}

#[test]
fn claude_code_reads_the_config_directory_and_takes_a_project() {
    let machine = Machine::new(&["claude"]);
    let mut env_vars = machine.default_env();
    let project = machine.home().join("work");
    fs::create_dir_all(&project).unwrap();
    let claude = harness("claude-code").unwrap();
    assert_eq!(claude.config(&env_vars, &Scope::Project(project.clone())).unwrap(), project.join(".mcp.json"));
    apply_set(claude.as_ref(), &env_vars, &Scope::Project(project.clone())).unwrap();
    assert_eq!(machine.calls(), [format!("{}: mcp add -s project red-ale -- {RED_ALE} mcp", project.display())]);

    let dir = machine.home().join("claude-config");
    env_vars = Env::new(
        [
            ("HOME".to_owned(), OsString::from(machine.home())),
            ("PATH".to_owned(), OsString::from(machine.bin())),
            ("CLAUDE_CONFIG_DIR".to_owned(), OsString::from(&dir)),
        ],
        Os::Linux,
        machine.home().join("root"),
        Box::new(NoProbe),
    );
    assert_eq!(claude.config(&env_vars, &Scope::User).unwrap(), dir.join(".claude.json"));
}

#[test]
fn a_failing_cli_is_a_failure_with_its_message() {
    let mut machine = Machine::new(&["claude"]);
    machine.fail = true;
    let env = machine.default_env();
    let failed = apply_set(harness("claude-code").unwrap().as_ref(), &env, &Scope::User).unwrap_err();
    assert!(matches!(&failed, Error::Cli { message, .. } if message == "claude: no such server"), "{failed}");
    assert!(failed.to_string().contains("mcp add -s user red-ale"), "{failed}");
}

#[test]
fn a_cli_that_is_not_installed_is_skipped() {
    let machine = Machine::new(&[]);
    let env = machine.default_env();
    let gemini = harness("gemini").unwrap();
    assert!(!gemini.detect(&env));
    let plan = gemini.plan(&env, &Scope::User, &entry());
    assert!(matches!(plan.action, Action::Skip(Error::NotOnPath("gemini"))), "{:?}", plan.action);
}

#[test]
fn codex_reads_its_toml_and_has_no_project_scope() {
    let machine = Machine::new(&["codex"]);
    let env = machine.default_env();
    let codex = harness("codex").unwrap();
    machine.write(".codex/config.toml", "model = \"o4\"\n\n[mcp_servers.github]\ncommand = \"docker\"\n");
    assert!(matches!(apply_set(codex.as_ref(), &env, &Scope::User).unwrap(), Action::Add));
    assert_eq!(machine.args(), [format!("mcp add red-ale -- {RED_ALE} mcp")]);

    machine.write(".codex/config.toml", &format!("[mcp_servers.red-ale]\ncommand = \"{RED_ALE}\"\nargs = [\"mcp\"]\n"));
    assert!(matches!(apply_set(codex.as_ref(), &env, &Scope::User).unwrap(), Action::Nothing));
    let removal = codex.plan_removal(&env, &Scope::User);
    codex.apply(&env, removal).unwrap();
    assert_eq!(machine.args(), ["mcp remove red-ale".to_owned()]);

    machine.write(".codex/config.toml", "[mcp_servers\n");
    let plan = codex.plan(&env, &Scope::User, &entry());
    assert!(matches!(plan.action, Action::Skip(Error::Invalid { format: "TOML", .. })), "{:?}", plan.action);
    let project = codex.plan(&env, &Scope::Project(machine.home().to_path_buf()), &entry());
    assert!(matches!(project.action, Action::Skip(Error::NoProjectScope("Codex"))), "{:?}", project.action);
}

#[test]
fn copilot_cli_is_not_vs_codes_shim() {
    let machine = Machine::new(&[]);
    let (shims, real) = (machine.home().join("usr/share/code/bin/copilot-shim"), machine.home().join("real"));
    Machine::fake(&shims, "copilot", "");
    Machine::fake(&machine.bin(), "copilot", "# VSCODE_COPILOT_RUST_SHIM_V1");
    let copilot = harness("copilot-cli").unwrap();
    assert!(!copilot.detect(&machine.env(&[shims.clone(), machine.bin()])), "both shims are passed over");

    Machine::fake(&real, "copilot", "");
    let env = machine.env(&[shims, machine.bin(), real.clone()]);
    assert!(copilot.detect(&env));
    assert!(matches!(apply_set(copilot.as_ref(), &env, &Scope::User).unwrap(), Action::Add));
    assert_eq!(machine.args(), [format!("mcp add --tools * red-ale -- {RED_ALE} mcp")]);
    let shown = copilot.plan(&env, &Scope::User, &entry()).exact_change().unwrap();
    assert!(shown.starts_with(&format!("$ {}/copilot mcp add --tools '*' red-ale", real.display())), "{shown}");

    let entry = format!(r#"{{ "command": "{RED_ALE}", "args": ["mcp"], "tools": ["*"] }}"#);
    machine.write(".copilot/mcp-config.json", &format!(r#"{{ "mcpServers": {{ "red-ale": {entry} }} }}"#));
    assert!(matches!(apply_set(copilot.as_ref(), &env, &Scope::User).unwrap(), Action::Nothing));
}

#[test]
fn gemini_takes_the_command_without_a_separator() {
    let machine = Machine::new(&["gemini"]);
    let env = machine.default_env();
    let gemini = harness("gemini").unwrap();
    let with_config = Entry::new(Path::new(RED_ALE), Some(Path::new("/srv/red-ale.json"))).unwrap();
    gemini.apply(&env, gemini.plan(&env, &Scope::User, &with_config)).unwrap();
    assert_eq!(machine.args(), [format!("mcp add -s user red-ale {RED_ALE} mcp --config /srv/red-ale.json")]);
    let entry = format!(r#"{{ "command": "{RED_ALE}", "args": ["mcp"] }}"#);
    machine.write(".gemini/settings.json", &format!("{{\n  // comments are allowed\n  \"mcpServers\": {{ \"red-ale\": {entry} }}\n}}\n"));
    let removal = gemini.plan_removal(&env, &Scope::User);
    gemini.apply(&env, removal).unwrap();
    assert_eq!(machine.args(), ["mcp remove -s user red-ale".to_owned()]);
}

#[test]
fn devin_uses_its_cli_when_installed_and_its_file_otherwise() {
    let machine = Machine::new(&[]);
    let devin = harness("devin").unwrap();
    fs::create_dir_all(machine.home().join(".config/devin")).unwrap();
    let env = machine.default_env();
    assert!(matches!(apply_set(devin.as_ref(), &env, &Scope::User).unwrap(), Action::Add));
    assert!(machine.calls().is_empty());
    let file = machine.home().join(".config/devin/mcp_config.json");
    assert!(fs::read_to_string(&file).unwrap().contains(RED_ALE));

    fs::remove_file(&file).unwrap();
    Machine::fake(&machine.bin(), "devin", "");
    assert!(matches!(apply_set(devin.as_ref(), &env, &Scope::User).unwrap(), Action::Add));
    assert_eq!(machine.args(), [format!("mcp add -s user red-ale -- {RED_ALE} mcp")]);
    assert!(!file.exists(), "the CLI writes its own file");
}
