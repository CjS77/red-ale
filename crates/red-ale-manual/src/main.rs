//! `red-ale-manual build <dir> [--base /repo]`: red-ale's Manual as a static website
//! (`design/manual.md`, "The static site").
//!
//! It is a binary of its own because it is built from a checked-out source tree: the landing page in
//! `docs/landing-page/` is compiled in, and the images in `assets/` are copied beside the site. An installed `red-ale`
//! has neither.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, Subcommand};
use red_ale_docs::SiteFile;
use red_ale_rpc::Dispatcher;
use red_ale_rpc::manual::manual;
use red_ale_service::Settings;

/// red-ale-manual: build the Manual as a static website.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write the Manual as a static website: the landing page as its main page, one HTML page per Page, indexes, a
    /// search page, `schema.json`, `llms.txt` and `llms-full.txt`, with `assets/` copied beside them. The directory must
    /// be empty or not exist yet.
    Build {
        /// Where to write the site.
        dir: PathBuf,
        /// The URL path the site is served at: `/` for a domain's root (the default), or `/repo` for a GitHub Pages
        /// project site at `user.github.io/repo`, which serves it at `/repo/`. The asset links are written under it.
        #[arg(long, default_value = "/")]
        base: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Build { dir, base } => build(&dir, &base),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("red-ale-manual: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn build(dir: &Path, base: &str) -> Result<()> {
    if !base.starts_with('/') {
        bail!("--base must start with '/', as GitHub Pages serves a project site at /repo");
    }
    ensure_empty(dir)?;
    let files = manual().site(&Dispatcher::new(Settings::default()).schema(), base);
    files.iter().try_for_each(|file| write(dir, file))?;
    let assets = copy_assets(dir)?;
    println!("wrote {} files and {assets} assets to {}", files.len(), dir.display());
    Ok(())
}

/// A site never goes into a directory that holds something else.
fn ensure_empty(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))?;
    if entries.next().is_some() {
        bail!("{} is not empty; give an empty or new directory", dir.display());
    }
    Ok(())
}

fn write(dir: &Path, file: &SiteFile) -> Result<()> {
    let path = dir.join(&file.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(&path, &file.bytes).with_context(|| format!("cannot write {}", path.display()))
}

/// The repository's `assets/`, which the pages ask for under the site's base.
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Copy `assets/` beside the site, everything in it kept as it is. Returns how many files were copied.
fn copy_assets(dir: &Path) -> Result<usize> {
    let from = assets();
    copy_dir(&from, &dir.join("assets")).with_context(|| format!("cannot copy {} to {}", from.display(), dir.join("assets").display()))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(to)?;
    std::fs::read_dir(from)?.try_fold(0, |count, entry| {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            Ok(count + copy_dir(&entry.path(), &target)?)
        } else {
            std::fs::copy(entry.path(), &target)?;
            Ok(count + 1)
        }
    })
}
