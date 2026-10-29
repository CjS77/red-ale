//! `install.sh` against a fake release served from a temporary directory (`RED_ALE_BASE_URL=file://…`), with `HOME`
//! pointed at another one. The fake `red-ale` records the arguments it is run with.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh")
}

/// The target install.sh picks on this machine.
fn target() -> String {
    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else {
        "unknown-linux-musl"
    };
    format!("{}-{os}", std::env::consts::ARCH)
}

/// A release directory holding `red-ale-<target>.tar.gz`, whose `red-ale` writes its arguments to `$HOME/called`,
/// and its `.sha256` in sha256sum's format.
fn release() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let staging = dir.path().join("staging");
    fs::create_dir_all(&staging).unwrap();
    let binary = staging.join("red-ale");
    fs::write(&binary, "#!/bin/sh\necho \"$*\" > \"$HOME/called\"\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(staging.join("LICENSE"), "GPL-3.0-or-later\n").unwrap();
    let archive = format!("red-ale-{}.tar.gz", target());
    let tar = Command::new("tar")
        .arg("-czf")
        .arg(dir.path().join(&archive))
        .arg("-C")
        .arg(&staging)
        .args(["red-ale", "LICENSE"])
        .status();
    assert!(tar.unwrap().success());
    let sum = Command::new("sha256sum").arg(&archive).current_dir(dir.path()).output().unwrap();
    assert!(sum.status.success());
    fs::write(dir.path().join(format!("red-ale-{}.sha256", target())), sum.stdout).unwrap();
    dir
}

fn install(release: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new("sh")
        .arg(script())
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("RED_ALE_BASE_URL", format!("file://{}", release.display()))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn installs_the_binary_and_skips_setup_when_asked() {
    let (release, home) = (release(), tempfile::tempdir().unwrap());
    let out = install(release.path(), home.path(), &["--no-setup"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let installed = home.path().join(".local/bin/red-ale");
    assert_eq!(
        fs::read(&installed).unwrap(),
        fs::read(release.path().join("staging/red-ale")).unwrap()
    );
    assert_eq!(fs::metadata(&installed).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(stderr(&out).contains("is not on your PATH"), "{}", stderr(&out));
    assert!(!home.path().join("called").exists(), "--no-setup runs no setup");
    let leftovers = fs::read_dir(home.path().join(".local/bin")).unwrap().count();
    assert_eq!(leftovers, 1, "only the binary");
}

#[test]
fn runs_setup_with_the_arguments_after_the_separator() {
    let (release, home) = (release(), tempfile::tempdir().unwrap());
    let out = install(release.path(), home.path(), &["--", "--only", "cursor", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        fs::read_to_string(home.path().join("called")).unwrap(),
        "setup --only cursor --dry-run\n"
    );
}

#[test]
fn refuses_an_archive_whose_checksum_does_not_match() {
    let (release, home) = (release(), tempfile::tempdir().unwrap());
    let sum = release.path().join(format!("red-ale-{}.sha256", target()));
    fs::write(&sum, format!("{}  red-ale-{}.tar.gz\n", "0".repeat(64), target())).unwrap();
    let out = install(release.path(), home.path(), &["--no-setup"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("checksum"), "{}", stderr(&out));
    assert!(!home.path().join(".local/bin/red-ale").exists());
}

#[test]
fn rejects_an_unknown_option() {
    let (release, home) = (release(), tempfile::tempdir().unwrap());
    let out = install(release.path(), home.path(), &["--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("go after --"), "{}", stderr(&out));
}
