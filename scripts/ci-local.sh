#!/usr/bin/env bash
# Run the PR gate locally (same commands as .github/workflows/ci.yml). Fails fast.
set -euo pipefail
cd "$(dirname "$0")/.."
step() { echo "== $*"; }
step fmt;     cargo fmt --all --check
step clippy;  cargo clippy --workspace --all-targets --all-features --locked -q -- -D warnings
step test;    cargo nextest run --workspace --all-features --locked --no-tests=pass --status-level fail --final-status-level fail
step doctest; cargo test --workspace --all-features --locked --doc -q 2>&1 | grep -E "test result|FAILED|error" | sort | uniq -c
step doc;     RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked -q
step rules;   scripts/check_repo_rules.sh && python3 scripts/check_architecture.py
step deny;    cargo deny --all-features check -s 2>&1 | tail -1
echo "ci-local: ok"
