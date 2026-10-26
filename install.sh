#!/bin/sh
# Installs the slab compiler (`slab`) and language server (`slab-lsp`) from the latest GitHub
# release, without a clone or a Rust toolchain:
#
#   curl -fsSL https://raw.githubusercontent.com/HeavyLvy/slab/main/install.sh | sh
#
# Linux (x86_64) and macOS (Apple silicon). On Windows use install.ps1; elsewhere
# `cargo install slab-lang slab-lsp` builds it from source.
#
# Environment:
#   SLAB_VERSION      install this version (0.1.0 or v0.1.0) instead of the latest
#   SLAB_INSTALL_DIR  where to put the programs (default: ~/.local/bin)
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
    Linux-x86_64 | Linux-amd64) platform="linux-x86_64" ;;
    Darwin-arm64 | Darwin-aarch64) platform="macos-arm64" ;;
    *) fail "no prebuilt binaries for $os $arch; build from source with: cargo install slab-lang slab-lsp" ;;
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

say "Downloading slab $version for $platform..."
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz" || fail "could not download $base/$name.tar.gz (is $version a release?)"

# check the archive against the release's checksums when it has them
if curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" 2>/dev/null; then
    expected=$(grep " $name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
    if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
    else actual=""; fi
    if [ -n "$expected" ] && [ -n "$actual" ] && [ "$expected" != "$actual" ]; then
        fail "checksum mismatch for $name.tar.gz"
    fi
fi

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
for bin in slab slab-lsp; do
    install -m 755 "$tmp/$name/$bin" "$dir/$bin"
done
say "Installed slab $version to $dir (slab, slab-lsp)"

case ":$PATH:" in
    *":$dir:"*) ;;
    *) say "note: $dir is not on your PATH. Add this to your shell profile:  export PATH=\"$dir:\$PATH\"" ;;
esac
say "Try: slab --version"
say "Editor support: install the \"slab\" extension in VS Code (it finds slab-lsp there)."
