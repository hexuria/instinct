# Verification log

How each owner in docs/plan.md "Verification architecture" was exercised, and what's left.
Updated per crate. The final summary is in docs/review-report.md.

## Mutation testing (cargo-mutants)

Run: `scripts/mutants.sh` (whole workspace, nightly) or `cargo mutants -p <crate>` locally. In
CI, `mutants-diff.yml` tests only the mutants in a PR's diff and is advisory: a survivor is
either killed by a new test or listed below as equivalent, with the reason.

### Equivalent mutants (accepted survivors)

| Location | Mutant | Why it is equivalent |
|---|---|---|
| `pua-core/src/score.rs` `Millis::saturating` | `<` → `<=` (lower clamp), `>` → `>=` (upper clamp) | At the boundary value the clamp returns the boundary itself (`-1000`/`1000`), the same value the unclamped branch returns. |
| `pua-core/src/score.rs` `Confidence::saturating` | same two boundary mutants | Same reason at `0`/`1000`. |
| `pua-core/src/answer.rs` `is_canonical` | `w[0].0 < w[1].0` → `<=` | Equal option indices are duplicates, which the duplicate check in the same function rejects anyway. |
| `pua-core/src/version.rs` `DataVersion::from_hex` | `(hi << 4) \| lo` → `^` | `hi << 4` and `lo` (< 16) have disjoint bits, so OR and XOR agree. |

### Survivors killed by new tests

| Crate | Mutants | Test |
|---|---|---|
| pua-core (local run, T2) | thresholds, conversions, accessors | `tests/props.rs::exact_boundaries_and_conversions` |
| pua-core (CI mutants-diff on #3) | `is_canonical` `>` → `>=`; `Span::contains` `&&` → `\|\|`; `Span::overlaps` `<` → `<=`; `Trail::is_empty` → `true` | same test (Ranked tie order, span relations); `trail::tests` |
