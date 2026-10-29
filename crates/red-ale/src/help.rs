//! `red-ale help`: the Manual in the terminal (`design/manual.md`, "The doors", CLI). Pages print as text, with their
//! Examples as `red-ale exec` command lines.

use std::io::Write;

use anyhow::{Context as _, Result};
use clap::Args;
use red_ale_docs::{Door, Format, Hit, Manual, Page};
use red_ale_rpc::manual::manual;

/// How many Pages `--search` lists.
const SEARCH_HITS: usize = 10;
/// How many close matches a name that is no Page suggests.
const SUGGESTIONS: usize = 5;
/// The exit status when nothing matched, as `grep` uses it.
const NOT_FOUND: u8 = 1;

#[derive(Debug, Args)]
pub struct HelpArgs {
    /// What to print: a Page ID (`concept/value-and-amount`), a method (`transaction.post`), an error code
    /// (`unbalanced`) or a report template (`"Balance Sheet"`). Without it, the Start Page. A red-ale command's name
    /// (`exec`) prints that command's options.
    #[arg(conflicts_with = "search")]
    page: Option<String>,
    /// List the Pages that match these words, best first: Page ID, title and summary.
    #[arg(long, num_args = 1.., value_name = "WORDS")]
    search: Option<Vec<String>>,
}

/// Print what `args` asks for. `cli` is the command line's own definition, for a command's options. The result is
/// the exit status.
pub fn run(args: &HelpArgs, cli: clap::Command) -> Result<u8> {
    let manual = manual();
    match (&args.search, &args.page) {
        (Some(words), _) => search(manual, &words.join(" ")),
        (None, None) => print_page(manual, manual.start().context("the Manual has no Start Page")?),
        (None, Some(name)) => match manual.find(name) {
            Some(page) => print_page(manual, page),
            None => command_or_unknown(manual, name, cli),
        },
    }
}

fn print_page(manual: &Manual, page: &Page) -> Result<u8> {
    write!(std::io::stdout().lock(), "{}", manual.render(page, Format::Text, Door::Cli))?;
    Ok(0)
}

fn search(manual: &Manual, query: &str) -> Result<u8> {
    let hits = manual.search(query, None, SEARCH_HITS);
    if hits.is_empty() {
        eprintln!("red-ale: no Page of the Manual matches \"{query}\"");
        return Ok(NOT_FOUND);
    }
    let mut out = std::io::stdout().lock();
    hits.iter()
        .try_for_each(|h| writeln!(out, "{}  {}\n    {}", h.id, h.title, h.summary))?;
    Ok(0)
}

/// `red-ale help exec` prints `exec`'s options, as clap's own `help` command did; any other name that is no Page
/// gets the closest Pages.
fn command_or_unknown(manual: &Manual, name: &str, mut cli: clap::Command) -> Result<u8> {
    cli.build();
    match cli.find_subcommand_mut(name) {
        Some(command) => {
            command.print_long_help()?;
            Ok(0)
        }
        None => unknown(manual, name),
    }
}

fn unknown(manual: &Manual, name: &str) -> Result<u8> {
    let hits = manual.search(name, None, SUGGESTIONS);
    let mut err = std::io::stderr().lock();
    writeln!(err, "red-ale: no Page of the Manual is named \"{name}\"")?;
    if !hits.is_empty() {
        writeln!(err, "\nThe closest Pages:\n{}", suggestions(&hits))?;
    }
    writeln!(err, "\n`red-ale help --search <words>` searches the Manual.")?;
    Ok(NOT_FOUND)
}

/// One line per hit: Page ID and title, the IDs in one column.
fn suggestions(hits: &[Hit]) -> String {
    let width = hits.iter().map(|h| h.id.len()).max().unwrap_or(0);
    hits.iter()
        .map(|h| format!("  {:<width$}  {}", h.id, h.title))
        .collect::<Vec<_>>()
        .join("\n")
}
