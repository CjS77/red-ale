//! Planning and writing a change to a Harness config file, under the rules of `design/setup.md`, "Editing a Harness
//! config": only the red-ale entry changes, a file that doesn't parse is never written, the original is backed up
//! first, the write is atomic, and a file changed since it was planned is planned again.

use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::json::{DocError, Dialect, Document};
use crate::{Action, Change, Entry, Error};

/// Where a Harness keeps its servers and how it writes the red-ale entry.
#[derive(Debug, Clone)]
pub(crate) struct Location {
    /// The Harness config, as configured (it may be a symbolic link).
    pub(crate) path: PathBuf,
    pub(crate) dialect: Dialect,
    /// The keys from the top of the file to the object holding the servers by name.
    pub(crate) keys: &'static [&'static str],
    /// The red-ale entry in the Harness's format.
    pub(crate) render: fn(&Entry) -> Value,
}

/// A planned change to one file.
#[derive(Debug)]
pub(crate) struct Edit {
    /// The file to write: the Harness config with symbolic links resolved, so a link survives.
    path: PathBuf,
    /// The file as planned from; `None` if it did not exist.
    before: Option<Vec<u8>>,
    after: String,
}

impl Edit {
    /// A unified diff of the whole file, for `--dry-run`.
    pub(crate) fn diff(&self) -> String {
        let before = self.before.as_deref().map(String::from_utf8_lossy).unwrap_or_default();
        let name = self.path.display().to_string();
        let old = if self.before.is_some() { name.as_str() } else { "/dev/null" };
        similar::TextDiff::from_lines(before.as_ref(), self.after.as_str()).unified_diff().header(old, &name).to_string()
    }
}

/// The red-ale entry the file at `location` holds now.
pub(crate) fn read_entry(location: &Location) -> Result<Option<Value>, Error> {
    let (_, _, document) = read(location)?;
    document.entry(location.keys).map_err(|e| doc_error(location, e))
}

/// The action and edit that make the file at `location` hold what `change` asks.
pub(crate) fn plan(location: &Location, change: &Change) -> Result<(Action, Edit), Error> {
    let (path, before, mut document) = read(location)?;
    let current = document.entry(location.keys).map_err(|e| doc_error(location, e))?;
    let wanted = match change {
        Change::Set(entry) => Some((location.render)(entry)),
        Change::Remove => None,
    };
    let action = match (&current, &wanted) {
        (None, None) => Action::Nothing,
        (Some(old), Some(new)) if old == new => Action::Nothing,
        (None, Some(_)) => Action::Add,
        (Some(old), Some(new)) => Action::Update { diff: entry_diff(old, new) },
        (Some(_), None) => Action::Remove,
    };
    let edited = match &wanted {
        _ if !action.changes() => Ok(()),
        Some(value) => document.set(location.keys, value),
        None => document.remove(location.keys),
    };
    edited.map_err(|e| doc_error(location, e))?;
    Ok((action, Edit { path, before, after: document.text() }))
}

/// Read and parse the file at `location`: its resolved path, its bytes (`None` if missing) and its document.
fn read(location: &Location) -> Result<(PathBuf, Option<Vec<u8>>, Document), Error> {
    let path = resolve(&location.path)?;
    let before = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(source) => return Err(Error::Read { path, source }),
    };
    let text = std::str::from_utf8(before.as_deref().unwrap_or_default())
        .map_err(|e| doc_error(location, DocError::Parse(e.to_string())))?;
    let document = Document::parse(text, location.dialect).map_err(|e| doc_error(location, e))?;
    Ok((path, before, document))
}

/// The file a path names, through any symbolic links; the path itself if nothing is there yet.
fn resolve(path: &Path) -> Result<PathBuf, Error> {
    match fs::canonicalize(path) {
        Ok(resolved) => Ok(resolved),
        Err(e) if e.kind() == io::ErrorKind::NotFound => match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_symlink() => Err(Error::DanglingLink(path.to_path_buf())),
            _ => Ok(path.to_path_buf()),
        },
        Err(source) => Err(Error::Read { path: path.to_path_buf(), source }),
    }
}

fn doc_error(location: &Location, error: DocError) -> Error {
    let path = location.path.clone();
    match error {
        DocError::Parse(message) => Error::Invalid { path, format: location.dialect.describe(), message },
        DocError::NotAnObject(key) if key.is_empty() => Error::NotAnObject { path, key: "the top level".to_owned() },
        DocError::NotAnObject(key) => Error::NotAnObject { path, key },
    }
}

pub(crate) fn entry_diff(old: &Value, new: &Value) -> String {
    let pretty = |v: &Value| serde_json::to_string_pretty(v).map(|s| s + "\n").unwrap_or_default();
    let (old, new) = (pretty(old), pretty(new));
    similar::TextDiff::from_lines(&old, &new).unified_diff().header("red-ale entry now", "red-ale entry after").to_string()
}

/// How a commit ended.
#[derive(Debug)]
pub(crate) enum Committed {
    /// The file was written; this is the backup of what it held, if it existed.
    Written(Option<PathBuf>),
    /// The file changed since it was planned, and was left as it is.
    Changed,
}

/// Write an edit: to a temporary file in the same directory, flushed to disk, then renamed over the file once the
/// file is checked to be as planned and backed up. `before_rename` runs just before the rename, for tests to inject
/// a failure there.
pub(crate) fn commit(edit: &Edit, before_rename: &dyn Fn(&Path) -> io::Result<()>) -> Result<Committed, Error> {
    let write_error = |source| Error::Write { path: edit.path.clone(), source };
    let dir = edit.path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(write_error)?;
    let permissions = fs::metadata(&edit.path).ok().map(|m| m.permissions());
    let temp = temporary(dir, &edit.after, permissions.as_ref()).map_err(write_error)?;
    let now = match fs::read(&edit.path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(source) => return Err(Error::Read { path: edit.path.clone(), source }),
    };
    if now != edit.before {
        return Ok(Committed::Changed);
    }
    let backup = edit.before.as_deref().map(|bytes| back_up(&edit.path, bytes, permissions.as_ref())).transpose()?;
    before_rename(&edit.path).map_err(write_error)?;
    temp.persist(&edit.path).map_err(|e| write_error(e.error))?;
    sync_dir(dir);
    Ok(Committed::Written(backup))
}

fn temporary(dir: &Path, text: &str, permissions: Option<&Permissions>) -> io::Result<tempfile::NamedTempFile> {
    let mut temp = tempfile::Builder::new().prefix(".red-ale-setup-").tempfile_in(dir)?;
    temp.write_all(text.as_bytes())?;
    if let Some(permissions) = permissions {
        temp.as_file().set_permissions(permissions.clone())?;
    }
    temp.as_file().sync_all()?;
    Ok(temp)
}

/// Copy `bytes`, the file at `path` as checked, to `<path>.red-ale-backup-<UTC timestamp>` with the file's
/// permissions. Never overwrites an earlier backup.
fn back_up(path: &Path, bytes: &[u8], permissions: Option<&Permissions>) -> Result<PathBuf, Error> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".red-ale-backup-{stamp}"));
    let backup = PathBuf::from(backup);
    let write = || -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&backup)?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions.clone())?;
        }
        file.write_all(bytes)?;
        file.sync_all()
    };
    write().map_err(|source| Error::Write { path: backup.clone(), source })?;
    Ok(backup)
}

/// Flush the rename to disk. The file is already in place, so a failure here is not reported.
fn sync_dir(dir: &Path) {
    if cfg!(unix) {
        let _ = fs::File::open(dir).and_then(|d| d.sync_all());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn location(path: PathBuf) -> Location {
        Location {
            path,
            dialect: Dialect::Jsonc,
            keys: &["servers"],
            render: |entry| json!({ "command": entry.command }),
        }
    }

    fn entry() -> Change {
        Change::Set(Entry::new(Path::new("/opt/red-ale"), None).unwrap())
    }

    fn backups(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().contains(".red-ale-backup-")).count()
    }

    #[test]
    fn a_failure_before_the_rename_leaves_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, "{ \"servers\": {} }\n").unwrap();
        let (_, edit) = plan(&location(path.clone()), &entry()).unwrap();
        let failed = commit(&edit, &|_| Err(io::Error::other("injected")));
        assert!(matches!(failed, Err(Error::Write { .. })), "{failed:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"servers\": {} }\n");
        let leftovers = fs::read_dir(dir.path()).unwrap().flatten();
        let leftovers = leftovers.filter(|e| e.file_name().to_string_lossy().starts_with(".red-ale-setup-"));
        assert_eq!(leftovers.count(), 0, "the temporary file is removed");
    }

    #[test]
    fn a_file_changed_since_the_plan_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, "{}").unwrap();
        let (_, edit) = plan(&location(path.clone()), &entry()).unwrap();
        fs::write(&path, "{ \"theme\": \"dark\" }").unwrap();
        assert!(matches!(commit(&edit, &|_| Ok(())), Ok(Committed::Changed)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"theme\": \"dark\" }");
        assert_eq!(backups(dir.path()), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_followed_and_kept_and_permissions_are_kept() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let (target, link) = (dir.path().join("real.json"), dir.path().join("mcp.json"));
        fs::write(&target, "{}\n").unwrap();
        fs::set_permissions(&target, Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let (_, edit) = plan(&location(link.clone()), &entry()).unwrap();
        let Committed::Written(Some(backup)) = commit(&edit, &|_| Ok(())).unwrap() else { panic!("no backup") };
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert!(fs::read_to_string(&target).unwrap().contains("/opt/red-ale"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{}\n");
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("mcp.json");
        std::os::unix::fs::symlink(dir.path().join("gone.json"), &link).unwrap();
        assert!(matches!(plan(&location(link), &entry()), Err(Error::DanglingLink(_))));
    }
}
