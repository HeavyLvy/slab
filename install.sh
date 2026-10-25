#!/usr/bin/env bash
# Installs slab on this machine:
#   * the compiler (`slab`) and the language server (`slab-lsp`) with `cargo install`
#   * the VS Code extension (highlighting + language server client)
#
# Usage: ./install.sh [--skip-cli] [--skip-extension] [--dry-run] [--uninstall]
# Environment: CODE=<path>  the VS Code command to use (default: code, then codium)
set -euo pipefail
cd "$(dirname "$0")"

EXTENSION_ID="heavylvy.slab-vscode"
cli=1
extension=1
dry=0
uninstall=0

for arg in "$@"; do
    case "$arg" in
        --skip-cli) cli=0 ;;
        --skip-extension) extension=0 ;;
        --dry-run) dry=1 ;;
        --uninstall) uninstall=1 ;;
        -h | --help)
            sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown option: $arg (try --help)" >&2
            exit 2
            ;;
    esac
done

run() {
    echo "+ $*"
    if [ "$dry" -eq 0 ]; then "$@"; fi
}

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: '$1' not found. $2" >&2
        exit 1
    fi
}

find_code() {
    if [ -n "${CODE:-}" ]; then echo "$CODE"; return; fi
    for c in code codium; do
        if command -v "$c" >/dev/null 2>&1; then echo "$c"; return; fi
    done
    return 1
}

if [ "$uninstall" -eq 1 ]; then
    if [ "$cli" -eq 1 ]; then
        need cargo "Install Rust from https://rustup.rs."
        run cargo uninstall slab || true
    fi
    if [ "$extension" -eq 1 ] && code_bin=$(find_code); then
        run "$code_bin" --uninstall-extension "$EXTENSION_ID" || true
    fi
    exit 0
fi

if [ "$cli" -eq 1 ]; then
    need cargo "Install Rust from https://rustup.rs."
    # the compiler (slab) is the root crate, the language server (slab-lsp) a workspace member
    run cargo install --path . --locked --force
    run cargo install --path crates/slab-lsp --locked --force
    cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
    case ":$PATH:" in
        *":$cargo_bin:"*) ;;
        *) echo "note: $cargo_bin is not on your PATH; add it so 'slab' and 'slab-lsp' can be found." ;;
    esac
fi

if [ "$extension" -eq 1 ]; then
    need node "Install Node.js (https://nodejs.org) to build the extension."
    need npm "Install Node.js (https://nodejs.org) to build the extension."
    if ! code_bin=$(find_code); then
        echo "error: neither 'code' nor 'codium' found. Install the VS Code command ('Shell Command: Install code command in PATH'), or set CODE=<path>." >&2
        exit 1
    fi
    (
        cd editors/vscode
        run npm install --no-audit --no-fund
        run npm run package
        if [ "$dry" -eq 0 ]; then
            vsix=$(ls -t ./*.vsix | head -n 1)
        else
            vsix="./slab-vscode-<version>.vsix"
        fi
        run "$code_bin" --install-extension "$vsix" --force
    )
    echo "Reload VS Code windows that are already open (Developer: Reload Window)."
fi

echo "Done. Try: slab --help"
