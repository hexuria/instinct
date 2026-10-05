# AGENTS.md: working rules for hexuria/pua

Humans and coding agents follow the same rules. The quality bar is the impeccable-rust checklist
(see CONTRIBUTING.md); the semantics are docs/spec.md; the task list is docs/plan.md; where work
stands is docs/progress.md.

## Hard rules

1. **Toolchain 1.99.0** (rust-toolchain.toml, owner rule). `rust-version` ≤ 1.99, stable-only
   features. Only fuzzing uses a nightly, and only a dated one (`$NIGHTLY` in workflows).
2. **Deterministic.** No floats in the decision path (`clippy::float_arithmetic` is denied), no
   clocks, RNG, env reads, thread-locals or `HashMap`/`HashSet` (clippy `disallowed-*`). Same input +
   same `DataVersion` + same profile gives a byte-identical `Decision`.
3. **`#![forbid(unsafe_code)]`** in every crate. Adding unsafe requires a Miri job first.
4. **No unwrap/expect in library code** (denied); tests may use them.
5. **Dependency direction** is `scripts/architecture.txt`. No tokio, axum, sqlx, reqwest, hyper,
   typesafe-sdk or opengrok-* anywhere (cargo-deny + architecture check).
6. **Abstain over guess.** Open spec questions get the safe path (abstain, feature gate, documented
   gap), never an invented answer (e.g. no TIN check digit, never emit "valid" for a TIN).
7. **Pack data is labelled.** Hand-made fixtures say they are synthetic. No benchmark or accuracy
   claim without a committed run that produced it.
8. **Git.** Small focused commits; stacked PRs merged bottom-up with squash so `main` stays linear;
   never force-push `main` or a merged branch.

## Anti-drift

> Any change to observable semantics names the verification boundary it affects.

- Concurrency, interleaving, scheduling, retry, cancellation, recovery, ownership, or liveness
  updates the system model, or the change states why that model is unaffected. (PUA has none
  today: every call is sync and pure, so no system model exists. Introducing one of these is an
  architecture change that must update docs/plan.md "Verification architecture" first.)
- Executable Rust behavior updates the Rust verification layer (unit, property, golden replay,
  fuzz). A theorem-owned kernel updates its proof (none exist). Workflow or DSL semantics (rule
  data) update conformance or differential tests (golden replay journals per pack).
- Do not clone one state machine across Rust, TLA+, Lean, and a DSL for symmetry. Passing
  independent suites does not establish equivalence.

Every PR fills in the "Verification impact" block of the PR template.

## Local loop

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked && cargo test --doc --workspace
python3 scripts/check_architecture.py && scripts/check_repo_rules.sh && cargo deny check
```
