#!/usr/bin/env bash
# Instruction-count regression gate (skill §4). Usage: scripts/bench-gate.sh <base-ref>
#
# Builds and runs the gungraun (Callgrind) benches of the `instinct-benches` crate at <base-ref> with
# --save-baseline=base, then at HEAD against that baseline with --callgrind-limits=$Instinct_BENCH_LIMIT.
# Both runs share one target dir and one runner, back to back (old then new), which removes
# cross-machine noise. Callgrind Ir is deterministic for a fixed binary, so the limit can be tight.
# gungraun exits 3 on a limit breach, which fails the job.
#
# Always pass `--bench paths` so cargo does not feed gungraun flags to the crate's lib test harness.
set -euo pipefail
cd "$(dirname "$0")/.."
base="${1:?base ref}"
limit="${Instinct_BENCH_LIMIT:-ir=10%}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export GUNGRAUN_HOME="$CARGO_TARGET_DIR/gungraun"

has_benches() { [[ -f "$1/benches/Cargo.toml" ]]; }
if ! has_benches .; then echo "bench: no benches/ crate yet, skipping"; exit 0; fi

run_paths() {
  cargo bench -p instinct-benches --bench paths --locked -- "$@"
}

wt="$(mktemp -d)/base"
cleanup() { git worktree remove --force "$wt" >/dev/null 2>&1 || true; }
trap cleanup EXIT
if git worktree add --detach "$wt" "$base" >/dev/null 2>&1 && has_benches "$wt"; then
  (cd "$wt" && cargo bench -p instinct-benches --bench paths --locked -- --save-baseline=base)
  run_paths --baseline=base --callgrind-limits="$limit"
else
  echo "bench: base $base has no benches/ crate; recording HEAD only (no gate this run)"
  run_paths --save-baseline=head
fi
