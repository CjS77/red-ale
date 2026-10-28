//! Every file adapter on sample configs (`design/setup.md`, "Testing"): the result equals a golden file, applying
//! twice equals applying once, and removing after adding gives back the original. Each test runs in its own
//! temporary home; none reads the real one. `RED_ALE_BLESS=1` rewrites the golden files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use red_ale_setup::{Action, Entry, Env, Error, Harness, Os, Probe, Scope, harness};

/// A machine where Claude Desktop may be running and `opencode --version` prints `version`.
struct Fake {
    running: bool,
    version: Option<&'static str>,
}

impl Probe for Fake {
    fn running(&self, _names: &[&str]) -> bool {
        self.running
    }

    fn output(&self, _program: &Path, _args: &[&str]) -> Option<String> {
        self.version.map(str::to_owned)
    }
}

/// An adapter under test: its sample folder, Harness id and the `opencode --version` it sees.
struct Case {
    folder: &'static str,
    id: &'static str,
    version: Option<&'static str>,
    strict: bool,
}

const CASES: [Case; 8] = [
    Case { folder: "cursor", id: "cursor", version: None, strict: false },
    Case { folder: "vscode", id: "vscode", version: None, strict: false },
    Case { folder: "claude-desktop", id: "claude-desktop", version: None, strict: true },
    Case { folder: "antigravity", id: "antigravity", version: None, strict: false },
    Case { folder: "devin", id: "devin", version: None, strict: false },
    Case { folder: "zed", id: "zed", version: None, strict: false },
    Case { folder: "opencode-v1", id: "opencode", version: Some("1.19.4"), strict: false },
    Case { folder: "opencode-v2", id: "opencode", version: Some("opencode v2.2.0"), strict: false },
];

const SAMPLES: [&str; 5] = ["empty", "missing", "other", "comments", "old"];

const RED_ALE: &str = "/opt/red-ale/bin/red-ale";

fn env(home: &Path, running: bool, version: Option<&'static str>) -> Env {
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("opencode"), "").unwrap();
    let vars = [
        ("HOME", home.to_path_buf()),
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("APPDATA", home.join("AppData")),
        ("PATH", bin),
    ];
    let vars = vars.map(|(k, v)| (k.to_owned(), OsString::from(v)));
    Env::new(vars, Os::Linux, home.join("root"), Box::new(Fake { running, version }))
}

fn entry() -> Entry {
    Entry::new(Path::new(RED_ALE), None).unwrap()
}

fn data(kind: &str, case: &Case, sample: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/{kind}/{}/{sample}.json", case.folder))
}

/// A temporary home with the sample as the Harness's user config: the adapter, its environment, and the config path.
fn installed(case: &Case, sample: &str) -> (tempfile::TempDir, Box<dyn Harness>, Env, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), false, case.version);
    let harness = harness(case.id).unwrap();
    let config = harness.config(&env, &Scope::User).unwrap();
    if sample != "missing" {
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::copy(data("samples", case, sample), &config).unwrap();
    }
    (home, harness, env, config)
}

fn backups(config: &Path) -> Vec<PathBuf> {
    let name = config.file_name().unwrap().to_string_lossy().into_owned();
    let mut found: Vec<PathBuf> = fs::read_dir(config.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{name}.red-ale-backup-")))
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

fn add(harness: &dyn Harness, env: &Env) -> Action {
    let plan = harness.plan(env, &Scope::User, &entry());
    harness.apply(env, plan).unwrap_or_else(|e| panic!("{}: {e}", harness.id())).action
}

fn remove(harness: &dyn Harness, env: &Env) -> Action {
    let plan = harness.plan_removal(env, &Scope::User);
    harness.apply(env, plan).unwrap().action
}

fn check_golden(case: &Case, sample: &str, written: &str) {
    let golden = data("golden", case, sample);
    if std::env::var_os("RED_ALE_BLESS").is_some() {
        fs::create_dir_all(golden.parent().unwrap()).unwrap();
        fs::write(&golden, written).unwrap();
    }
    let expected = fs::read_to_string(&golden).unwrap_or_else(|e| panic!("{}: {e}", golden.display()));
    assert_eq!(written, expected, "{} on {sample}", case.folder);
}

/// The sample with its red-ale entry removed, as the adapter removes it.
fn without_entry(case: &Case, sample: &str) -> String {
    let (_home, harness, env, config) = installed(case, sample);
    remove(harness.as_ref(), &env);
    fs::read_to_string(config).unwrap()
}

#[test]
fn adding_gives_the_golden_file_once_and_removing_gives_back_the_original() {
    CASES.iter().for_each(|case| {
        SAMPLES.iter().filter(|s| !(case.strict && **s == "comments")).for_each(|&sample| {
            let (_home, harness, env, config) = installed(case, sample);
            let original = fs::read_to_string(&config).ok();
            let expected = if sample == "old" { "update" } else { "add" };
            assert!(matches!((add(harness.as_ref(), &env), expected), (Action::Add, "add") | (Action::Update { .. }, "update")));
            let written = fs::read_to_string(&config).unwrap();
            check_golden(case, sample, &written);

            let made = backups(&config);
            assert_eq!(made.len(), usize::from(original.is_some()), "{} on {sample}: one backup of an existing file", case.folder);
            made.iter().for_each(|b| assert_eq!(fs::read_to_string(b).ok(), original));

            assert!(matches!(add(harness.as_ref(), &env), Action::Nothing), "{} on {sample}: a second run does nothing", case.folder);
            assert_eq!(fs::read_to_string(&config).unwrap(), written);
            assert_eq!(backups(&config), made, "a second run makes no backup");

            assert!(matches!(remove(harness.as_ref(), &env), Action::Remove));
            let removed = fs::read_to_string(&config).unwrap();
            match (sample, original) {
                ("missing", _) => assert_eq!(removed, "{}\n"),
                ("old", _) => assert_eq!(removed, without_entry(case, sample), "{}: bytes outside the entry are kept", case.folder),
                (_, Some(original)) => assert_eq!(removed, original, "{} on {sample}", case.folder),
                (_, None) => unreachable!(),
            }
        });
    });
}

#[test]
fn a_file_that_does_not_parse_is_skipped_unchanged_with_no_backup() {
    CASES.iter().for_each(|case| {
        ["{ \"mcpServers\": ", "[1, 2]\n", "{ \"mcp\": 1, \"servers\": 1, \"context_servers\": 1, \"mcpServers\": 1 }"]
            .iter()
            .for_each(|text| {
                let (_home, harness, env, config) = installed(case, "empty");
                fs::write(&config, text).unwrap();
                let plan = harness.plan(&env, &Scope::User, &entry());
                let refused = matches!(plan.action, Action::Skip(Error::Invalid { .. } | Error::NotAnObject { .. }));
                assert!(refused, "{}: {:?}", case.folder, plan.action);
                assert!(harness.apply(&env, plan).is_err());
                assert_eq!(fs::read_to_string(&config).unwrap(), *text);
                assert!(backups(&config).is_empty());
            });
    });
}

#[test]
fn claude_desktop_takes_strict_json_only() {
    let case = &CASES[2];
    let (_home, harness, env, config) = installed(case, "comments");
    let plan = harness.plan(&env, &Scope::User, &entry());
    assert!(matches!(plan.action, Action::Skip(Error::Invalid { format: "JSON", .. })), "{:?}", plan.action);
    assert_eq!(fs::read(&config).unwrap(), fs::read(data("samples", case, "comments")).unwrap());
}

#[test]
fn claude_desktop_is_not_changed_while_it_runs() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), true, None);
    let harness = harness("claude-desktop").unwrap();
    let plan = harness.plan(&env, &Scope::User, &entry());
    let Action::Skip(reason) = &plan.action else { panic!("{:?}", plan.action) };
    assert_eq!(reason.to_string(), "quit Claude Desktop and run `red-ale setup` again");
    assert!(!harness.config(&env, &Scope::User).unwrap().exists());
}

#[test]
fn a_config_changed_after_planning_is_planned_again() {
    let (_home, harness, env, config) = installed(&CASES[0], "empty");
    let plan = harness.plan(&env, &Scope::User, &entry());
    fs::write(&config, "{ \"mcpServers\": { \"other\": { \"command\": \"x\" } } }\n").unwrap();
    assert!(matches!(harness.apply(&env, plan).unwrap().action, Action::Add));
    let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(written["mcpServers"]["other"]["command"], "x");
    assert_eq!(written["mcpServers"]["red-ale"]["command"], RED_ALE);
}

#[test]
fn opencode_needs_its_major_version() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), false, None);
    let plan = harness("opencode").unwrap().plan(&env, &Scope::User, &entry());
    assert!(matches!(plan.action, Action::Skip(Error::OpencodeVersion(_))), "{:?}", plan.action);
}

#[test]
fn the_entry_passes_a_configuration_file() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), false, None);
    let entry = Entry::new(Path::new(RED_ALE), Some(Path::new("/srv/red-ale.json"))).unwrap();
    let cursor = harness("cursor").unwrap();
    cursor.apply(&env, cursor.plan(&env, &Scope::User, &entry)).unwrap();
    let written = cursor.read_entry(&env, &Scope::User).unwrap().unwrap();
    assert_eq!(written, serde_json::json!({ "type": "stdio", "command": RED_ALE, "args": ["mcp", "--config", "/srv/red-ale.json"] }));
}

#[test]
fn project_scope_writes_in_the_project() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), false, Some("1.19.4"));
    let project = Scope::Project(home.path().join("work"));
    let expected = [
        ("cursor", ".cursor/mcp.json"),
        ("vscode", ".vscode/mcp.json"),
        ("antigravity", ".agents/mcp_config.json"),
        ("devin", ".devin/mcp_config.json"),
        ("zed", ".zed/settings.json"),
        ("opencode", "opencode.json"),
    ];
    expected.iter().for_each(|(id, file)| {
        assert_eq!(harness(id).unwrap().config(&env, &project).unwrap(), home.path().join("work").join(file));
    });
    let desktop = harness("claude-desktop").unwrap().plan(&env, &project, &entry());
    assert!(matches!(desktop.action, Action::Skip(Error::NoProjectScope(_))));
}

#[test]
fn harnesses_are_found_by_their_config_directory() {
    let home = tempfile::tempdir().unwrap();
    let env = env(home.path(), false, None);
    let dirs = [
        ("cursor", ".cursor"),
        ("vscode", ".config/Code/User"),
        ("claude-desktop", ".config/Claude"),
        ("antigravity", ".gemini/config"),
        ("devin", ".config/devin"),
        ("zed", ".config/zed"),
    ];
    dirs.iter().for_each(|(id, dir)| {
        let harness = harness(id).unwrap();
        assert!(!harness.detect(&env), "{id} is not installed yet");
        fs::create_dir_all(home.path().join(dir)).unwrap();
        assert!(harness.detect(&env), "{id} is found by {dir}");
    });
    assert!(harness("opencode").unwrap().detect(&env), "opencode is on PATH");
    assert!(harness("notepad").is_none(), "not a harness");
}
