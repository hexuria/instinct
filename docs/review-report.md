# Review report (T17)

Closing audit of the hexuria/instinct build against docs/plan.md and the impeccable-rust bar.
Status as of the T11–T17 stack tip. Proven claims are bounded; Deferred items are intentional.

> **Phase 2 update (2026-10-05).** The packs and `instinct-jev` moved to their consumers (ADR 0010).
> Rows below that name a pack are kept as the record of what was proven **before** the move; the
> tests moved with the code. Current engine status and remaining debt:
> [architecture-audit.md §11](architecture-audit.md#11-phase-2-follow-up-2026-10-05).

## Proven (with terminology and bounds)

| Claim | Bounds | Evidence |
|---|---|---|
| Decision path is integer-only | No floats in the engine at all since Phase 2 (the Jev boundary moved to opengrok-server `opengrok-jev`) | `float_arithmetic` denied workspace-wide, no `#[allow]` left |
| `unsafe` is forbidden | Every crate `#![forbid(unsafe_code)]` | CI repo rules + architecture |
| Deterministic packs | No HashMap/HashSet/clocks/RNG/env in decision paths | clippy bans; generator-sequence / permutation proptests per pack |
| Offset map indexes original text | `Span` values from `instinct-text` normalize (the only user of the offset map; `instinct-ocr` used its own scanner) | span equivariance proptests in `instinct-text` |
| Interrupt never auto-applies | `AutoApply::Never` on interrupt | ADR 0008; autosteer eval FP=0 |
| TIN suggestions never say "valid" | Type + string scan | `SuggestionKind` / reason tests; bir-fields mutants |
| Gateway shape has no tier type | Public API is integer `ShapeFeatures` only | crate docs + compile-time surface test |
| WL fingerprints are relabeling-invariant | Complete-graph / call-DAG constructions | graph + pack layout/call-DAG proptests; C₆ vs 2×C₃ collision documented |
| Instruction-count gate wired | Callgrind Ir via gungraun 0.20, limit `ir=10%` | `benches/`, `scripts/bench-gate.sh`, `docs/benchmarks.md` |
| Fuzz targets for parsers | `normalize`, `lexicon_lookup`, `rules_match`, `graph_from_bytes` (`jev_reply_parse` moved out with `instinct-jev`) | `fuzz/`; nightly smoke `scripts/fuzz-smoke.sh` |

Terminology: *invariant* = exact `==` after a declared free move; *equivariant* = output transforms with the input; *free move* = elementary generator in the §6 tables.

## Documented

- ADRs 0001, 0002, 0003, 0005, 0006, 0008, 0009 and (Phase 2) 0010: CI-first, Millis vs Confidence, trail in core / deferred traits, token-keyed rules, text canonicalization, AutoApply (moved to NativeChat), Jev float boundary (moved to opengrok-server), engine/consumer boundary. There is no ADR 0004 or 0007.
- `docs/verification.md` — mutants equivalents, survivors killed, litmus notes per crate.
- `docs/eval/autosteer.md` — interrupt FP=0 vs keyword baseline (moved to hexuria/nativechat `crates/autosteer/EVAL.md`).
- `docs/hdc-capacity.md` — measured decode/sign recall table.
- `docs/benchmarks.md` — load model, Ir statistic, decision rule, informational wall-clock budgets.

## Deferred

| Item | Why |
|---|---|
| Autosteer `target-selection` (HDC over live runs) | Spec Phase B. Only the feature name existed (no code behind it); dropped in the NativeChat handoff |
| Jev HTTP client | Spec non-goal; wire shapes only |
| Authoritative BIR catalog | Sample catalog labelled non-authoritative (q8) |
| Miri / sanitizers / Loom | No `unsafe`, no concurrency |
| Wall-clock CI gate | Replaced by Ir gate (ADR 0001); budgets informational |
| Pack ownership move to consumer repos | Done in Phase 2 (draft handoff PRs; ADR 0010) |

## Compat / deps

- MSRV **1.99.0** (toolchain file + `rust-version`).
- Workspace deny: no tokio/axum/sqlx/reqwest/hyper/typesafe-sdk/opengrok-*.
- Architecture direction: seven engine crates only (`scripts/architecture.txt`); no packs, no jev.
- Permissive licenses only (`deny.toml`).

## Verification owners

| Failure mode | Owner | Status |
|---|---|---|
| Wrong decide / threshold / tie-break | `instinct-core` unit + props | Done |
| Output changes under declared move | per-stage / per-pack proptests | Done through T14 |
| Offset map wrong original bytes | `instinct-text` + ocr-labels | Done |
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
