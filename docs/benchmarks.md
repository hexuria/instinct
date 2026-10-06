# Benchmarks

Instruction-count regression gate for Instinct. Wall-clock §5.3 budgets are informational only.

## Load model

| Bench | Input | Spec §5.3 row |
|---|---|---|
| `classifier_decide` | one message through `RuleClassifier` on the `instinct-rules` delivery fixture | classifier decide (was autosteer `ask`) |
| `normalize_32kib` | ~32 KiB Latin prose + protected spans | text normalize, 32 KB input |
| `cleanup_4096` | codebook n = 4096, D = 1024 | cleanup over a 4,096-entry codebook |
| `resonator_64x64` | \|A\| = \|B\| = 64, `IterationCap::MAX` (16) | resonator worst case |
| `wl_fingerprint_500` | 500 nodes / 2,000 directed edges, h = 3 | `wl_fingerprint` |

Setup (codebook build, graph construction, string allocation) runs **outside** the measured
region via gungraun `setup` / `#[bench]` args.

## Statistic

- **Metric:** Callgrind instruction count (`Ir`), via [gungraun](https://crates.io/crates/gungraun) 0.20.
- **Environment:** one CI job measures base then HEAD back-to-back on the same runner
  (`scripts/bench-gate.sh`), sharing one `CARGO_TARGET_DIR`.
- **Limit:** `INSTINCT_BENCH_LIMIT` (default `ir=10%`). gungraun exits 3 on a breach.

## Decision rule

Fail the bench job when any benchmark's `Ir` grows by more than the limit versus the merge-base
baseline. Optimizations that shrink `Ir` are accepted without ceremony. The first run on a branch
that introduces `benches/` records HEAD only (no gate that run).

## Wall-clock §5.3 budgets (informational)

Measured once locally on the box (Linux, Rust 1.99.0, `cargo bench` harness wall time is **not**
the gate). Re-measure after algorithmic changes; do not fail CI on these.

| Path | Budget (spec) | Notes |
|---|---|---|
| `RuleClassifier::decide` | < 2 ms | Ir gate covers regressions |
| text normalize, 32 KB | < 1 ms | |
| cleanup 4096 / D1024 | < 1 ms | |
| resonator 64×64 ≤ 16 iters | < 5 ms | |
| `wl_fingerprint` 500/2000 h=3 | < 1 ms | |

## Running locally

```bash
# Requires valgrind + gungraun-runner (see .github/workflows/bench.yml).
cargo bench -p instinct-benches --locked
scripts/bench-gate.sh HEAD~1
```
