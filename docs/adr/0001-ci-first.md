# ADR 0001: CI first, instruction-count bench gate, workflow push path

Status: accepted (2026-10-05)

## Decision

- CI lands as its own PR before any library code (PR #1). `ci-ok` aggregates the PR gate.
- Benchmarks gate on Callgrind instruction counts (gungraun 0.20), base and head measured back to
  back in one job; limit `ir=10%`.
- cargo-mutants runs on the PR diff and nightly on core crates, **non-blocking** until the
  surviving-mutant baseline is triaged (docs/verification.md). It becomes blocking once survivors
  are either killed or written down as equivalent mutants.
- Fuzzing uses a dated nightly (`nightly-2026-10-01`); everything else uses the 1.99.0 pin.
- The box's GitHub OAuth token lacks the `workflow` scope, so files under `.github/workflows/` are
  committed from the owner's machine (which has it). All other changes go through normal PRs.

## Alternatives discarded

- **criterion with a 20 % wall-clock gate (spec §5.3).** Shared CI runners make wall time noisy;
  a 20 % gate would either flap or hide real regressions. Instruction counts are deterministic for
  a fixed binary, so a tighter limit is safe. Wall-clock budgets are still measured and reported
  (docs/benchmarks.md), but never gate.
- **Blocking mutants from day one.** Would stall every PR on equivalent mutants before a baseline
  exists.
- **Installing toolchains with a third-party action.** `rustup toolchain install` reads
  rust-toolchain.toml directly, so the pin has exactly one source.

## Downsides accepted

- Callgrind Ir does not see cache or branch-prediction effects; that is accepted for a gate and
  covered by the informational wall-clock run.
- Workflow edits need the owner's machine until the box token gains `workflow` scope
  (`gh auth refresh -s workflow`).
