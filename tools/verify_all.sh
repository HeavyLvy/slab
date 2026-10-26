#!/usr/bin/env bash
# Builds every verification pack and checks it on a real Minecraft 26.3 server (see CLAUDE.md).
# Usage: tools/verify_all.sh [--jar PATH] [--java PATH]   (extra args go to verify_pack.py)
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q --examples --bin slab

cargo run -q --example zoo -- target/zoo
cargo run -q --example programs -- target/programs
cargo run -q -- build examples/projects/e2e -o target/e2e/e2e >/dev/null
cargo run -q -- build examples/projects/limit -o target/limit/limit >/dev/null

status=0
run() { echo "=== $1"; shift; python3 tools/verify_pack.py "$@" 2>&1 | grep -E "^(FAIL|OK:|check failed|Failed to load|Command execution stopped)|PROBLEMS" || true; }
python3 tools/verify_pack.py target/zoo --run zoo:all "$@" >target/verify-zoo.log 2>&1 && echo "zoo:      OK" || { echo "zoo:      FAILED (target/verify-zoo.log)"; status=1; }
python3 tools/verify_pack.py target/programs --run prog:run_all --checks target/programs.checks.json "$@" >target/verify-programs.log 2>&1 && echo "programs: OK" || { echo "programs: FAILED (target/verify-programs.log)"; status=1; }
python3 tools/verify_pack.py target/e2e/e2e --setup "forceload add 0 0" "forceload add 29999984 0" --run e2e:world_test --checks examples/projects/e2e/checks.json "$@" >target/verify-e2e.log 2>&1 && echo "e2e:      OK" || { echo "e2e:      FAILED (target/verify-e2e.log)"; status=1; }
python3 tools/verify_pack.py target/limit/limit --checks examples/projects/limit/checks.json "$@" >target/verify-limit.log 2>&1 && echo "limit:    OK" || { echo "limit:    FAILED (target/verify-limit.log)"; status=1; }
exit $status
