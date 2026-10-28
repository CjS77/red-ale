//! `red-ale setup`: register red-ale's MCP server with the agent Harnesses on this machine (`design/setup.md`).
//! The work is `red-ale-setup`'s; this is the command line around it: choosing Harnesses, showing the Setup plan,
//! asking, and reporting.

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Args;
use clap::builder::PossibleValuesParser;
use red_ale_setup::{Action, Entry, Env, FIRST_RELEASE, Harness, Plan, Scope};

/// The Harness ids, for `--only` and `--skip`.
fn ids() -> PossibleValuesParser {
    PossibleValuesParser::new(FIRST_RELEASE.map(|(id, _)| id))
}

/// The options of `design/setup.md`, "Command line".
#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools, reason = "each flag is one option of the command line")]
pub struct SetupArgs {
    /// Set up only these Harnesses (comma-separated ids; see --list). A Harness named here that can't be set up is
    /// a failure.
    #[arg(long, value_delimiter = ',', value_parser = ids())]
    only: Vec<String>,
    /// Leave these Harnesses alone (comma-separated ids).
    #[arg(long, value_delimiter = ',', value_parser = ids())]
    skip: Vec<String>,
    /// Write project-scoped entries in this directory instead of user ones, for the Harnesses that have them.
    #[arg(long, value_name = "DIR")]
    project: Option<PathBuf>,
    /// A Configuration file other than the default, which the entry then passes to `red-ale mcp`.
    #[arg(long, value_name = "FILE", conflicts_with = "remove")]
    config: Option<PathBuf>,
    /// Show the Setup plan and the exact change to each file, then stop.
    #[arg(long)]
    dry_run: bool,
    /// Apply without asking. Required when standard input is not a terminal.
    #[arg(long)]
    yes: bool,
    /// Remove the red-ale entry from every Harness config instead.
    #[arg(long)]
    remove: bool,
    /// Show each supported Harness: installed or not, its Harness config, and whether it has a red-ale entry.
    /// Changes nothing.
    #[arg(long, conflicts_with_all = ["only", "skip", "config", "dry_run", "yes", "remove"])]
    list: bool,
}

/// One selected Harness: its id, whether `--only` named it, and its adapter.
struct Selected {
    id: &'static str,
    named: bool,
    harness: Box<dyn Harness>,
}

/// Run `setup`; the exit status (`design/setup.md`, "Command line"): 0 when every selected Harness is set up, 1 when
/// any failed, 2 when changes need `--yes` and standard input is not a terminal.
pub fn run(args: &SetupArgs) -> Result<u8> {
    let env = Env::from_process();
    let scope = args.project.as_ref().map_or(Ok(Scope::User), |dir| std::path::absolute(dir).map(Scope::Project))?;
    if args.list {
        list(&env, &scope);
        return Ok(0);
    }
    let entry = (!args.remove).then(|| entry(args.config.as_ref())).transpose()?;
    let selected = select(args);
    let rows: Vec<(&Selected, Result<Plan, String>)> =
        selected.iter().filter_map(|s| plan(&env, &scope, entry.as_ref(), s).map(|p| (s, p))).collect();
    show(&rows, args.dry_run);
    let failed = rows.iter().any(|(_, plan)| plan.as_ref().map_or(true, |p| matches!(p.action, Action::Skip(_))));
    if args.dry_run || !rows.iter().any(|(_, plan)| plan.as_ref().is_ok_and(|p| p.action.changes())) {
        return Ok(u8::from(failed));
    }
    if !args.yes {
        if !std::io::stdin().is_terminal() {
            eprintln!("red-ale: standard input is not a terminal; run `red-ale setup --yes` to apply this plan");
            return Ok(2);
        }
        if !confirm()? {
            println!("Nothing changed.");
            return Ok(0);
        }
    }
    println!("Result:");
    let applied = rows.into_iter().map(|(s, plan)| apply(&env, s, plan)).filter(|ok| !ok).count();
    Ok(u8::from(applied > 0))
}

/// The red-ale entry for this binary.
fn entry(config: Option<&PathBuf>) -> Result<Entry> {
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).context("finding the red-ale binary")?;
    let config = config.map(std::path::absolute).transpose().context("finding --config")?;
    Ok(Entry::new(&exe, config.as_deref())?)
}

fn select(args: &SetupArgs) -> Vec<Selected> {
    FIRST_RELEASE
        .iter()
        .filter(|(id, _)| args.only.is_empty() || args.only.iter().any(|o| o == id))
        .filter(|(id, _)| !args.skip.iter().any(|s| s == id))
        .filter_map(|&(id, _)| red_ale_setup::harness(id).map(|harness| Selected { id, named: !args.only.is_empty(), harness }))
        .collect()
}

/// The plan for one Harness; `None` for one that isn't installed and wasn't named with `--only`.
fn plan(env: &Env, scope: &Scope, entry: Option<&Entry>, selected: &Selected) -> Option<Result<Plan, String>> {
    let harness = &selected.harness;
    if !harness.detect(env) {
        return selected.named.then(|| Err("not installed".to_owned()));
    }
    Some(Ok(entry.map_or_else(|| harness.plan_removal(env, scope), |e| harness.plan(env, scope, e))))
}

fn show(rows: &[(&Selected, Result<Plan, String>)], dry_run: bool) {
    if rows.is_empty() {
        println!("No supported harness is installed.");
        return;
    }
    println!("Setup plan:");
    rows.iter().for_each(|(selected, plan)| match plan {
        Err(reason) => println!("  {:<15} {:<8} {reason}", selected.id, "skip"),
        Ok(plan) => {
            let config = plan.config.as_ref().map(|c| c.display().to_string()).unwrap_or_default();
            match &plan.action {
                Action::Skip(reason) => println!("  {:<15} {:<8} {reason}", selected.id, "skip"),
                action => println!("  {:<15} {:<8} {config}", selected.id, verb(action)),
            }
            let diff = match (&plan.action, dry_run) {
                (_, true) => plan.exact_change(),
                (Action::Update { diff }, false) => Some(diff.clone()),
                _ => None,
            };
            diff.iter().flat_map(|d| d.lines()).for_each(|line| println!("      {line}"));
        }
    });
}

fn verb(action: &Action) -> &'static str {
    match action {
        Action::Add => "add",
        Action::Update { .. } => "update",
        Action::Remove => "remove",
        Action::Nothing => "nothing",
        Action::Skip(_) => "skip",
    }
}

fn confirm() -> Result<bool> {
    print!("Apply? [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

/// Apply one Harness's plan and report it; whether it ended set up.
fn apply(env: &Env, selected: &Selected, plan: Result<Plan, String>) -> bool {
    let harness = &selected.harness;
    let Ok(plan) = plan else {
        println!("  {:<15} failed   {} is not set up", selected.id, harness.name());
        return false;
    };
    match harness.apply(env, plan) {
        Ok(applied) => {
            let done = match applied.action {
                Action::Add => "added",
                Action::Update { .. } => "updated",
                Action::Remove => "removed",
                Action::Nothing | Action::Skip(_) => "unchanged",
            };
            let mut notes = vec![applied.config.display().to_string()];
            notes.extend(applied.backup.map(|b| format!("backup {}", b.display())));
            notes.extend(harness.restart().filter(|_| applied.action.changes()).map(str::to_owned));
            println!("  {:<15} {done:<8} {}", selected.id, notes.join("; "));
            true
        }
        Err(e) => {
            println!("  {:<15} failed   {e}", selected.id);
            false
        }
    }
}

fn list(env: &Env, scope: &Scope) {
    println!("{:<15} {:<10} {:<14} Harness config", "harness", "installed", "red-ale entry");
    FIRST_RELEASE.iter().for_each(|&(id, _)| {
        let Some(harness) = red_ale_setup::harness(id) else { return };
        let installed = if harness.detect(env) { "yes" } else { "no" };
        let (has_entry, config) = match (harness.read_entry(env, scope), harness.config(env, scope)) {
            (Ok(found), Ok(config)) => (if found.is_some() { "yes" } else { "no" }, config.display().to_string()),
            (Err(e), _) | (_, Err(e)) => ("?", e.to_string()),
        };
        println!("{id:<15} {installed:<10} {has_entry:<14} {config}");
    });
}
