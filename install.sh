#!/bin/sh
# Install red-ale and register it with the agent harnesses on this machine (design/setup.md, "Installation").
#
#   curl -fsSL https://github.com/CjS77/red-ale/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --no-setup
#   curl -fsSL .../install.sh | sh -s -- -- --only claude-code,cursor
#
# Options:
#   --no-setup   install the binary only; don't run `red-ale setup`
#   -- ARGS      pass ARGS to `red-ale setup`
#
# Environment:
#   RED_ALE_VERSION    a release tag (v0.5.0) instead of the latest release
#   RED_ALE_BASE_URL   where the release assets are, instead of GitHub (for testing)
#
# It downloads the archive for this machine (the asset names in .github/workflows/release.yml), checks its SHA-256,
# installs the binary at ~/.local/bin/red-ale, replacing any earlier one in one rename, and runs `red-ale setup`,
# which reads its answer from the terminal because this script itself arrives on standard input.
#
# SPDX-License-Identifier: GPL-3.0-or-later

set -eu

RELEASES="https://github.com/CjS77/red-ale/releases"

say() {
    printf 'red-ale install: %s\n' "$*" >&2
}

die() {
    say "$*"
    exit 1
}

usage() {
    cat <<'EOF'
Usage: install.sh [--no-setup] [-- SETUP-OPTIONS]

Installs red-ale at ~/.local/bin/red-ale, then runs `red-ale setup SETUP-OPTIONS`.

  --no-setup   install the binary only
  -- ARGS      pass ARGS to `red-ale setup` (for example `-- --only cursor --yes`)

RED_ALE_VERSION picks a release tag; RED_ALE_BASE_URL replaces the download location.
EOF
}

# The Rust target of the release archive for this machine.
target() {
    arch=$(uname -m)
    case "$arch" in
        x86_64 | amd64) arch=x86_64 ;;
        aarch64 | arm64) arch=aarch64 ;;
        *) die "there is no red-ale build for $arch" ;;
    esac
    case "$(uname -s)" in
        Linux) echo "$arch-unknown-linux-musl" ;;
        Darwin) echo "$arch-apple-darwin" ;;
        *) die "there is no red-ale build for $(uname -s); Windows builds are not available yet" ;;
    esac
}

base_url() {
    if [ -n "${RED_ALE_BASE_URL:-}" ]; then
        echo "${RED_ALE_BASE_URL%/}"
    elif [ -n "${RED_ALE_VERSION:-}" ]; then
        case "$RED_ALE_VERSION" in
            v*) echo "$RELEASES/download/$RED_ALE_VERSION" ;;
            *) echo "$RELEASES/download/v$RED_ALE_VERSION" ;;
        esac
    else
        echo "$RELEASES/latest/download"
    fi
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https,file' -fsSL -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "need curl or wget to download red-ale"
    fi
}

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{ print $1 }'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    else
        die "need sha256sum or shasum to check the download"
    fi
}

# Run `red-ale setup`, asking on the terminal. Without one it still shows the plan, and changes nothing unless
# `--yes` was passed.
setup() {
    if (: </dev/tty) 2>/dev/null; then
        "$@" </dev/tty
    else
        say "no terminal to ask on: pass '-- --yes' to apply the plan, or run 'red-ale setup' later"
        "$@"
    fi
}

main() {
    run_setup=1
    while [ $# -gt 0 ]; do
        case "$1" in
            --no-setup) run_setup=0 ;;
            --) shift; break ;;
            -h | --help) usage; exit 0 ;;
            *) die "unknown option '$1' (options for red-ale setup go after --)" ;;
        esac
        shift
    done
    [ -n "${HOME:-}" ] || die "HOME is not set"

    target=$(target)
    base=$(base_url)
    archive="red-ale-$target.tar.gz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 1' HUP INT TERM

    say "downloading $base/$archive"
    download "$base/$archive" "$tmp/$archive" || die "could not download $base/$archive"
    download "$base/red-ale-$target.sha256" "$tmp/sha256" || die "could not download $base/red-ale-$target.sha256"
    expected=$(awk 'NR == 1 { print $1 }' "$tmp/sha256")
    actual=$(sha256 "$tmp/$archive")
    if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
        die "the checksum of $archive does not match red-ale-$target.sha256; nothing was installed"
    fi

    mkdir "$tmp/unpacked"
    tar -xzf "$tmp/$archive" -C "$tmp/unpacked"
    binary=$(find "$tmp/unpacked" -type f -name red-ale | head -n 1)
    [ -n "$binary" ] || die "$archive holds no red-ale binary"

    dir="$HOME/.local/bin"
    mkdir -p "$dir"
    cp "$binary" "$dir/.red-ale.new.$$"
    chmod 755 "$dir/.red-ale.new.$$"
    mv -f "$dir/.red-ale.new.$$" "$dir/red-ale"
    say "installed $dir/red-ale"
    case ":${PATH:-}:" in
        *":$dir:"*) ;;
        *) say "$dir is not on your PATH; add it to use 'red-ale' by name" ;;
    esac

    if [ "$run_setup" = 1 ]; then
        setup "$dir/red-ale" setup "$@"
    fi
}

main "$@"
