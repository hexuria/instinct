#!/usr/bin/env bash
# Mutation testing over the core crates (skill §2 "Logic" chaos layer). Survivors are reported in
# mutants.out/; triage notes live in docs/verification.md. Packs are covered by mutants-diff.yml.
set -euo pipefail
cd "$(dirname "$0")/.."
pkgs=(instinct-core instinct-text instinct-lexicon instinct-rules instinct-hdc instinct-graph instinct-explain)
args=()
for p in "${pkgs[@]}"; do args+=(--package "$p"); done
cargo mutants "${args[@]}" --test-tool nextest --timeout-multiplier 3 --no-shuffle -vV "$@"
