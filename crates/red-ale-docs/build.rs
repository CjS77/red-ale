//! Compiles the hand-written Pages of `docs/` into the crate: writes `$OUT_DIR/sources.rs`, a list of every Markdown
//! file under `docs/` by its path relative to it, each with its text through `include_str!`. The list is sorted by
//! path, so the build is the same whatever order the file system gives.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    let manifest = env_path("CARGO_MANIFEST_DIR")?;
    let docs = manifest.join("../../docs").canonicalize()?;
    println!("cargo:rerun-if-changed={}", docs.display());
    let mut files = Vec::new();
    walk(&docs, &mut files)?;
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .filter_map(|p| relative(&docs, &p).map(|r| (r, p)))
        .collect();
    entries.sort();
    let lines: String = entries.iter().fold(String::new(), |mut out, (rel, path)| {
        let _ = writeln!(out, "    ({rel:?}, include_str!({:?})),", path.display().to_string());
        out
    });
    let code = format!(
        "/// Every Markdown file of `docs/`, by its path relative to it, with its text.\n\
         pub(crate) const SOURCES: &[(&str, &str)] = &[\n{lines}];\n"
    );
    fs::write(env_path("OUT_DIR")?.join("sources.rs"), code)
}

fn env_path(name: &str) -> io::Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

/// Every file under `dir`, recursively, each one watched for changes.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    fs::read_dir(dir)?.try_for_each(|entry| {
        let path = entry?.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            walk(&path, out)
        } else {
            out.push(path);
            Ok(())
        }
    })
}

/// `path` relative to `docs`, with `/` between its parts.
fn relative(docs: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path.strip_prefix(docs).ok()?.components().map(|c| c.as_os_str().to_str()).collect();
    parts.map(|p| p.join("/"))
}
