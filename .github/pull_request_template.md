## What and why

<!-- One paragraph. Link the docs/plan.md task id (e.g. T3). -->

## Checks run locally (toolchain 1.99.0)

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo nextest run --workspace --all-features --locked` (+ `cargo test --doc`)
- [ ] `python3 scripts/check_architecture.py` and `scripts/check_repo_rules.sh`
- [ ] `cargo deny check`

## Verification impact

> Any change to observable semantics names the verification boundary it affects.

```
[ ] Pure Rust deterministic behavior
[ ] Crash-recovery / replay           (replay records, golden journals, DataVersion)
[ ] Property-test / fuzz surface
[ ] No verification architecture impact

Reason:
Affected invariants:
Tests or proofs updated:
```

Concurrency, persistence, TLA+, proof-kernel and unsafe boxes are omitted on purpose: PUA has no
concurrent kernel, no persistence and `#![forbid(unsafe_code)]` everywhere (docs/plan.md). If this
PR introduces any of those, add the box back and update docs/plan.md "Verification architecture".

## Data changes (packs only)

- [ ] `DataVersion` changed and the golden replay diff is attached
- [ ] CHANGELOG entry (data changes that alter output bump MINOR)
