# Review report (T17)

Closing audit of the hexuria/pua build against docs/plan.md and the impeccable-rust bar.
Status as of the T11–T17 stack tip. Proven claims are bounded; Deferred items are intentional.

## Proven (with terminology and bounds)

| Claim | Bounds | Evidence |
|---|---|---|
| Decision path is integer-only | Floats only in `pua-jev/src/convert.rs` (one boundary) | `float_arithmetic` denied workspace-wide; convert module local `#[allow]` |
| `unsafe` is forbidden | Every crate `#![forbid(unsafe_code)]` | CI repo rules + architecture |
| Deterministic packs | No HashMap/HashSet/clocks/RNG/env in decision paths | clippy bans; generator-sequence / permutation proptests per pack |
| Offset map indexes original text | `Span` values from ocr-labels / text normalize | A §5.3 cases in `pua-pack-ocr-labels`; span equivariance proptests in `pua-text` |
| Interrupt never auto-applies | `AutoApply::Never` on interrupt | ADR 0008; autosteer eval FP=0 |
| TIN suggestions never say "valid" | Type + string scan | `SuggestionKind` / reason tests; bir-fields mutants |
| Gateway shape has no tier type | Public API is integer `ShapeFeatures` only | crate docs + compile-time surface test |
| WL fingerprints are relabeling-invariant | Complete-graph / call-DAG constructions | graph + pack layout/call-DAG proptests; C₆ vs 2×C₃ collision documented |
| Instruction-count gate wired | Callgrind Ir via gungraun 0.20, limit `ir=10%` | `benches/`, `scripts/bench-gate.sh`, `docs/benchmarks.md` |
| Fuzz targets for parsers | `normalize`, `lexicon_lookup`, `rules_match`, `graph_from_bytes`, `jev_reply_parse` | `fuzz/`; nightly smoke `scripts/fuzz-smoke.sh` |

Terminology: *invariant* = exact `==` after a declared free move; *equivariant* = output transforms with the input; *free move* = elementary generator in the §6 tables.

## Documented

- ADRs 0001–0009 (CI-first, misuse-resistant types, decide table, fold/Ñ, rules tokens, HDC dims, catalog non-authoritative, AutoApply, Jev float boundary).
- `docs/verification.md` — mutants equivalents, survivors killed, litmus notes per crate.
- `docs/eval/autosteer.md` — interrupt FP=0 vs keyword baseline.
- `docs/hdc-capacity.md` — measured decode/sign recall table.
- `docs/benchmarks.md` — load model, Ir statistic, decision rule, informational wall-clock budgets.

## Deferred

| Item | Why |
|---|---|
| Autosteer `target-selection` (HDC over live runs) | Spec Phase B; feature exists, off by default until PRD |
| Jev HTTP client | Spec non-goal; wire shapes only |
| Authoritative BIR catalog | Sample catalog labelled non-authoritative (q8) |
| Miri / sanitizers / Loom | No `unsafe`, no concurrency |
| Wall-clock CI gate | Replaced by Ir gate (ADR 0001); budgets informational |
| Pack ownership move to consumer repos | Open question q3 |

## Compat / deps

- MSRV **1.99.0** (toolchain file + `rust-version`).
- Workspace deny: no tokio/axum/sqlx/reqwest/hyper/typesafe-sdk/opengrok-*.
- Architecture direction: packs may use core/text/lexicon/rules/hdc/graph/explain — **not** jev.
- Permissive licenses only (`deny.toml`).

## Verification owners

| Failure mode | Owner | Status |
|---|---|---|
| Wrong decide / threshold / tie-break | `pua-core` unit + props | Done |
| Output changes under declared move | per-stage / per-pack proptests | Done through T14 |
| Offset map wrong original bytes | `pua-text` + ocr-labels | Done |
| NFC chunking ≠ whole-string NFC | differential proptest | Done |
| Panic on hostile input | fuzz + never-panics props | Targets listed; nightly smoke |
| Nondeterminism | clippy + goldens | Done |
| Replay drift | golden journals (autosteer) | Done for T10 |
| Invalid pack data accepted | two-phase types + exact `Err` | Done |
| HDC algebra / WL fingerprint | props §6.6 / §4.5 | Done |
| Weak tests | cargo-mutants (diff advisory; 0 MISSED before merge) | Ongoing per PR |
| Perf regression | gungraun Ir gate | Wired (T15) |
| Dep hazards / banned direction | cargo-deny + architecture | Done |

See also `docs/verification.md` for mutant triage detail.
