#!/usr/bin/env bash
# The documentation is an mdBook in docs/ (config: book.toml).
#   tools/docs.sh            build it into target/book (open target/book/index.html)
#   tools/docs.sh serve      build, serve at http://localhost:3000 and rebuild on every change
#   tools/docs.sh check      only run the tests that keep it honest
# Needs mdBook: `cargo install mdbook --locked`.
set -euo pipefail
cd "$(dirname "$0")/.."

# the standard library page is written from the library's sources, and the examples are compiled
UPDATE_DOCS=1 cargo test -q --test stdref >/dev/null
cargo test -q --test docs_examples --test stdref

case "${1:-build}" in
  build) mdbook build ;;
  serve) mdbook serve --open ;;
  check) ;;
  *) echo "usage: tools/docs.sh [build|serve|check]" >&2; exit 2 ;;
esac
