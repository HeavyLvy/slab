#!/bin/sh
# Installs the slab compiler (`slab`), the language server (`slab-lsp`) and, when VS Code is
# there, its extension, from the latest GitHub release, without a clone or a Rust toolchain:
#
#   curl -fsSL https://raw.githubusercontent.com/HeavyLvy/slab/main/install.sh | sh
#
# Linux (x86_64) and macOS (Apple silicon). On Windows use install.ps1; elsewhere
# building from source (`cargo install slab-lang slab-lsp`, needs Rust from https://rustup.rs) works anywhere.
#
# Environment:
#   SLAB_VERSION      install this version (0.1.0 or v0.1.0) instead of the latest
#   SLAB_INSTALL_DIR  where to put the programs (default: ~/.local/bin)
#   SLAB_NO_EXTENSION set to anything to skip installing the VS Code extension
set -eu

repo="HeavyLvy/slab"
dir="${SLAB_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
    Linux-x86_64 | Linux-amd64) platform="linux-x86_64"; vsce_target="linux-x64" ;;
    Darwin-arm64 | Darwin-aarch64) platform="macos-arm64"; vsce_target="darwin-arm64" ;;
    *) fail "no prebuilt binaries for $os $arch; build from source: install Rust from https://rustup.rs, then run: cargo install slab-lang slab-lsp" ;;
esac

if [ -n "${SLAB_VERSION:-}" ]; then
    version="${SLAB_VERSION#v}"
else
    # the latest release is a redirect to .../releases/tag/vX.Y.Z (no API call, no rate limit)
    url=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest") || fail "could not look up the latest release"
    version="${url##*/v}"
fi
[ -n "$version" ] || fail "could not work out the version to install"

name="slab-$version-$platform"
base="https://github.com/$repo/releases/download/v$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# fetches $1 into $tmp, checking it against the release's checksums when it has them
SUMS_TRIED=""
fetch() {
    curl -fsSL "$base/$1" -o "$tmp/$1" 2>/dev/null || return 1
    if [ -z "$SUMS_TRIED" ]; then
        SUMS_TRIED=1
        curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" 2>/dev/null || rm -f "$tmp/SHA256SUMS"
    fi
    [ -f "$tmp/SHA256SUMS" ] || return 0
    expected=$(grep " $1\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
    if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$tmp/$1" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$tmp/$1" | cut -d' ' -f1)
    else actual=""; fi
    if [ -n "$expected" ] && [ -n "$actual" ] && [ "$expected" != "$actual" ]; then
        fail "checksum mismatch for $1"
    fi
}

say "Downloading slab $version for $platform..."
fetch "$name.tar.gz" || fail "could not download $base/$name.tar.gz (is $version a release?)"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
for bin in slab slab-lsp; do
    install -m 755 "$tmp/$name/$bin" "$dir/$bin"
done
say "Installed slab $version to $dir (slab, slab-lsp)"

# the VS Code extension, from the same release, when VS Code (or VSCodium) is installed
install_extension() {
    [ -z "${SLAB_NO_EXTENSION:-}" ] || return 0
    code_bin=""
    for c in code codium; do
        if command -v "$c" >/dev/null 2>&1; then code_bin="$c"; break; fi
    done
    if [ -z "$code_bin" ]; then
        say "VS Code was not found, so the editor extension was not installed (see the release page for the .vsix)."
        return 0
    fi
    # the package for this platform has the language server inside; the universal one finds it on PATH
    for vsix in "slab-vscode-$vsce_target-$version.vsix" "slab-vscode-$version.vsix"; do
        if fetch "$vsix"; then
            if "$code_bin" --install-extension "$tmp/$vsix" --force >/dev/null 2>&1; then
                say "Installed the VS Code extension (reload open VS Code windows)."
            else
                say "Could not install the extension with '$code_bin'; install $vsix from the release page by hand."
            fi
            return 0
        fi
    done
    say "This release has no VS Code extension package."
}
install_extension

case ":$PATH:" in
    *":$dir:"*) ;;
    *) say "note: $dir is not on your PATH. Add this to your shell profile:  export PATH=\"$dir:\$PATH\"" ;;
esac
say "Try: slab --version"
