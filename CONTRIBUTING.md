# Contributing to hexuria/instinct

Read AGENTS.md first; its hard rules and anti-drift rule apply to every change.

## Quality bar (impeccable-rust)

Run what applies, and say in the PR what you ran and what you skipped:

1. **Testing.** Assert invariants. Test error paths with the exact `Err` variant. Error litmus test:
   temporarily skip a `return Err(..)`; if the suite still passes, coverage is broken.
2. **Chaos.** proptest for every declared invariance (spec §5.1/§5.2 and each pack's Invariances
   table) with exact `==`; differential tests against a trusted oracle for reimplementations (e.g.
   NFC segmentation vs whole-string NFC); cargo-mutants on the diff.
3. **Exhaustive verification.** Not used: no unsafe, no concurrency (see docs/plan.md for why Loom,
   Kani, TLA+ and Lean are not justified, and what would change that).
4. **Benchmarks.** Instruction counts (gungraun/Callgrind) gate regressions in CI; see
   docs/benchmarks.md for load model, statistic and decision rule. Wall-clock numbers are
   informational only.
5. **Documentation.** ADRs in docs/adr/ for real decisions: alternatives discarded, downsides
   accepted, deliberate gaps. No silent `todo!()` (denied by clippy).
6. **Misuse resistance.** Newtypes over aliases, validated two-phase types, enums instead of bool
   soup, private fields with accessors.
7. **Compatibility.** Minimal public surface; cargo-semver-checks and cargo-public-api run on release.
8. **Dependencies.** cargo-deny (advisories, licenses, bans, sources) on every PR and nightly.
9. **Stagnation.** Dependabot weekly; toolchain bumps are owner decisions recorded in an ADR.

## Verification impact template

```
Verification impact

[ ] Pure Rust deterministic behavior
[ ] Crash-recovery / replay
[ ] Property-test / fuzz surface
[ ] No verification architecture impact

Reason:
Affected invariants:
Tests or proofs updated:
```

## Releases

Tag `vX.Y.Z` on `main` with a CHANGELOG entry. Data changes that alter output bump MINOR and
`DataVersion`; public type changes bump MAJOR (MINOR while 0.x). Consumers pin tags or revs.
