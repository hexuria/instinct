# Instinct build plan

> **Historical (T1–T17).** This plan built the engine *and* the domain packs. Phase 2 moved the
> packs and `instinct-jev` to their consumers (ADR 0010, [architecture-audit.md](architecture-audit.md));
> task rows that name a pack describe work that now lives in those repos. ADRs 0004 and 0007 were
> planned here but never written: their decisions are recorded inline (data via `include_str!` + TOML
> parsed at load; owner defaults q5/q8/q14/q15) and, for the packs, travelled with the code.

Source of truth: docs/spec.md (semantics) and the impeccable-rust checklist (quality bar, see
CONTRIBUTING.md). This plan orders the work, gives every task acceptance criteria, assigns every
failure mode one owner, and lists the decisions that get an ADR. Status lives in docs/progress.md.

Every task follows the same loop: implement → verify locally on 1.99.0 (fmt, clippy -D warnings,
nextest, doc, deny, architecture) → push → CI green → fix → next. One stacked PR per task group.

## Cross-cutting rules (apply to every task)

- **Misuse resistance.** Newtypes for every unit (`Millis` similarity vs `Confidence`, `Span`,
  `OptionIndex`, `CandidateId`, `DataVersion`); validated two-phase types for all pack data
  (`*Spec` raw, deserialized from TOML → validated, compiled type with private fields); enums
  instead of bool pairs (`Fold`, `PunctRuns`, `Rounds`, `Profile`, `AutoApply`).
- **Errors.** `Result` with a crate error enum whenever the caller must branch on why; tests assert
  the exact variant; the error litmus test is run on every validation function (results recorded
  in docs/verification.md).
- **Integers only** in decision paths (`Millis`/`Confidence` i16, i32 accumulators). Floats only
  in `instinct-jev`'s probability conversion, once.
- **Determinism.** No HashMap/HashSet, clocks, RNG, env reads (clippy-enforced). Sorting gives
  canonical order. Every `Decision` carries `DataVersion`.
- **Data** is embedded with `include_str!` and parsed at pack load into validated tables
  (spec §3 allows `include_str!`). Hand-made fixtures are labelled synthetic.
- **Public surface** is minimal: private fields, accessors, `#[non_exhaustive]` on error enums.

## Tasks (dependency order)

| Id | Task | Crate(s) | Acceptance criteria |
|---|---|---|---|
| T0 | CI first | repo | PR gate, nightly, bench, release, deny, architecture, Dependabot, CODEOWNERS, AGENTS/CONTRIBUTING merged before code. **Done in PR #1.** |
| T1 | This plan + plan review | docs | Plan reviewed against spec and skill; gaps listed and fixed below. |
| T2 | Core types + decide | `instinct-core` | `Millis`, `Confidence`, `Span`, `DataVersion` (+ builder, domain-separated blake3), `Question` (validated Noul/Choice/Score), `Answer`, `Ranked`, `AbstainReason`, `Profile` table (§4.6), `Trail`/`TrailRecord`/`StageKind`/`ScorerKind`, `Decision`, `Pack` trait, `decide` and `rank_candidates` (Phase 2: `Pack` and `rank_candidates` removed; `CandidateSet` added). Proptests: determinism, candidate permutation invariance, option relabeling equivariance when the top-2 margin ≠ 0, option 0 wins exact ties, abstain iff below threshold/margin. Exact `Err` tests for every constructor. |
| T3 | Replay + explain | `instinct-explain` | `ReplayRecord<I>` (schema version, input hash, inputs, profile, decision), canonical JSON (serde, no maps), `diff` (answer / DataVersion / trail changes), text rendering of a trail. Golden test: serialize → bytes are stable across runs and equal to a committed fixture; round-trip property. |
| T4 | Canonicalization with offset map | `instinct-text` | NFC (chunked at stable starters, differential-tested against whole-string NFC), `Fold::{AsciiLower, UnicodeLower}` (Ñ-preserving), whitespace collapse and optional punctuation-run collapse outside protected spans, offset map (every canonical range maps to an original range), protected spans (fenced/inline code, URLs, paths, double-quoted text, versions/decimals), tokens and sentences as spans, confusable flags + skeleton. Proptests: `normalize(move(x)) == normalize(x)` for every elementary move; protected-span equivariance; span equivariance `fold(x'[s']) == fold(x[s])`; offset map monotone and in bounds; never panics on arbitrary input. Unicode fixtures `PEÑA`, `Ñiño`, combining vs precomposed Ñ, Cyrillic `ѕtop`. Fuzz target `normalize`. |
| T5 | Lexicon | `instinct-lexicon` | Validated `Lexicon` (no empty/duplicate/non-canonical terms, substring opt-in refused under 5 chars, ≤ 3 tokens); token-boundary exact, longest match across ≤ 3 tokens; typo repair for closed vocab (distance 1 for ≤ 4 chars, 2 otherwise; tie-break QWERTY adjacency → transposition → lexical; guard words never repaired; fixed penalty); OCR digit confusions for numeric fields only. Proptests: candidate multiset invariant under text moves; repair is deterministic and symmetric in insertion order of entries; never repairs inside protected spans. Exact `Err` tests. |
| T6 | Rules | `instinct-rules` | Validated `RuleSet` from `RuleSpec` (no regex, literal tokens + `{word}`/`{gerund}` slots, weight 1..=1000, unique ids, known classes, declared `ScorerKind` per class); token-keyed matching (token-boundary by construction); negation window (default 3, same sentence), question damper (halve), object scope ("stop using X" → steer via a more specific rule that suppresses the general one); protected tokens never match; per-class score Max/Sum. Proptests: cue inside a protected span never changes scores; Max-scorer critical set (edits outside critical spans and negation windows don't change the class score). |
| T7 | HDC | `instinct-hdc` | `Hv<D>` with `D ∈ {D1024, D2048, D4096}` as types (dimension mismatch is a compile error), FNV-1a + SplitMix64 encoder with tag `pua-hv1`, bind/permute/bundle (i32 accumulators, zero → +1), similarity in `Millis`, `Codebook` (sorted by id, unique), cleanup + iterative top-k decode, resonator (≤ 16 iterations, `NotConverged`). Proptests per §6.6. Capacity table measured by an example and committed (docs/hdc-capacity.md). |
| T8 | Graph | `instinct-graph` | `LabeledGraph` (u32 node arena, optional edge labels/direction), 1-WL refinement with blake3, `Rounds::{Fixed(h), ToStability}`, fingerprint tagged `wl-v1`, per-node colours; `spd-wl` feature (BFS distances). Proptests: invariance under node relabeling; documented collision C₆ vs 2×C₃; `spd-wl` separates the bridge pair (two triangles + bridge vs C₆ + chord). Fuzz target for graph building from bytes. |
| T9 | Jev adapter | `instinct-jev` | Wire shapes (serde) for Noul/Choice/Score requests and replies, float → `Confidence` conversion once (NaN/out-of-range are errors), off-menu guard, four distinct error kinds, `Escalation::{Answered, Fallback}` so a fallback is always labelled. No HTTP client. Wire shapes marked "verify against opengrok-server `src/jev/routes.rs` before consuming" (gap). |
| T10 | autosteer pack | `instinct-steer` | Question `Choice{delivery: [queue, steer, interrupt]}`; data in `data/*.toml`; pipeline text → lexicon → rules → decide; interrupt carries `AutoApply::Never`; confusable control words never fire. ≥ 200 synthetic labelled messages; eval test asserts interrupt false-positive = 0 at `standard` and reports per-class precision vs keyword baseline into docs/eval/autosteer.md (generated by a committed command). Invariance generator-sequence proptests (case, space, NFC/NFD, live-run swap). Golden replay journal. Offline replay CLI (example). Target selection behind the off-by-default `target-selection` feature (spec: waits for PRD Phase B). |
| T11 | gateway-shape pack | `instinct-gateway` | Integer `ShapeFeatures` only (fenced blocks, diff hunks, dominant script, homoglyphs, JSON keys, structured output); no tier type exists in the crate. Proptests per §6.2 table. 100 % on synthetic fixtures. |
| T12 | bir-fields pack | `instinct-bir` | TIN format repair (separators, OCR digit confusions) as suggestions only; the word "valid" can never be emitted (type + test); RDO/form-code longest exact match against a sample catalog labelled non-authoritative (q8); Ñ-preserving names, `PEÑA ≠ PENA`; field-order invariance; WL layout signature. |
| T13 | ocr-labels pack | `instinct-ocr` | COR label extraction via offset map; the three A §5.3 cases extract correctly; per-field confidence by match type; label diacritics NOT folded (owner decision q14, safe path); WL layout fingerprint stable under renumbering. |
| T14 | tool-selection pack | `instinct-toolbox` | Choice over tools (candidates, tie-break by id) or Abstain; namespaces symbolic; call-DAG WL fingerprint; invariant under tool-list permutation, description case/space, node relabeling. |
| T15 | Benches | `benches/` (`instinct-benches`) | gungraun Callgrind benches for each §5.3 path incl. pathological inputs (32 KB, max codebook, resonator worst case), wiring the bench gate; docs/benchmarks.md with load model, statistic and decision rule; wall-clock §5.3 budgets measured once and reported as informational. |
| T16 | Fuzz | `fuzz/` | cargo-fuzz targets: `normalize`, `lexicon_lookup`, `rules_match`, `graph_from_bytes`, `jev_reply_parse`; seeds committed; nightly smoke. |
| T17 | Review report | docs | docs/review-report.md: Proven (with terminology and bounds), Documented, Deferred, Compat/deps, Verification owners. docs/verification.md: mutants and litmus results. |

## Verification architecture

Instinct probe (done before writing this plan): no `unsafe`, no FFI, no threads, atomics, channels,
async, schedulers, retries, timeouts, persistence or recovery. Every public call is a sync, pure
function of (input, embedded data, profile). Parsers exist (TOML pack data, Jev reply JSON,
arbitrary user text). The "replay" in the spec is deterministic re-execution, not crash recovery.

| Failure mode | Class (skill table) | Owner | Notes |
|---|---|---|---|
| Wrong decision / threshold / tie-break | Deterministic logic | unit + property tests (`instinct-core`) | margin, abstain, option-0 tie, equivariance |
| Output changes under a declared move | Deterministic logic | generator-sequence proptests per stage and per pack | exact `==`, never tolerance |
| Offset map points at the wrong original bytes | Deterministic logic | property tests (span equivariance, in-bounds, monotone) + Unicode fixtures | the COR bug class |
| NFC chunking differs from real NFC | Reimplementation | differential proptest vs `unicode-normalization` whole-string NFC | trusted oracle |
| Panic / blow-up on hostile text, TOML, JSON or graph bytes | Untrusted input | cargo-fuzz (nightly smoke) + "never panics" proptests on PR | allocation bounded by input length |
| Nondeterminism (HashMap order, clocks, floats) | Deterministic logic | clippy bans (compiler-enforced at lint level) + determinism proptests + golden replay | `float_arithmetic` denied |
| Replay drift (same input, different bytes) | Deterministic logic | golden replay journals per pack (byte-identical) | `DataVersion` change forces fixture update |
| Invalid pack data accepted | Deterministic logic | validated two-phase types + exact-`Err` tests + error litmus | data errors fail pack load and CI |
| HDC algebra laws broken | Deterministic logic | property tests (§6.6) | |
| WL fingerprint not relabeling-invariant | Deterministic logic | property tests (random permutations) | collisions documented, not "fixed" |
| Weak tests (logic not pinned) | Logic chaos | cargo-mutants (diff on PR, core nightly) | non-blocking until baseline triaged |
| Performance regression | — | gungraun Ir gate (bench.yml) | instruction counts, not wall time |
| Dependency hazards | — | cargo-deny on PR + nightly; Dependabot | |
| Banned deps / wrong direction | — | architecture check + cargo-deny bans | |

**Not justified (and why):**

- **Miri / sanitizers:** no `unsafe` (forbid is compiler-enforced in every crate). Re-evaluate the
  moment any crate stops forbidding unsafe; the nightly unsafe audit fails if that happens.
- **Loom / shuttle / turmoil:** no concurrent implementation. `Pack: Send + Sync` holds immutable
  data only; the determinism-across-threads proptest covers "same answer on any thread".
- **TLA+:** no multiple actors, interleavings, retries, crash or recovery. Escalation to Jev is a
  consumer concern (spec §8) and lives in consumer repos.
- **Kani:** no unsafe and no small kernel whose symbolic coverage beats exhaustive enumeration or
  proptests. HDC bit algebra is exhaustively enumerated where cheap (e.g. all shifts for one
  vector) instead.
- **Lean/Verus:** no theorem that tests do not close at this scale. Candidate later: "canonicalize
  then decide is invariant" is a composition theorem the spec cites from GDL; the per-stage
  property tests own it in practice.

No second semantic model exists, so there is no conformance link to maintain. If one is added,
it must execute the golden replay journals.

## ADRs (docs/adr/)

| ADR | Decision |
|---|---|
| 0001 | CI-first, instruction-count bench gate, mutants non-blocking until baseline triaged; workflow files pushed via the owner's credentials because the box token lacks `workflow` scope |
| 0002 | Separate `Millis` (similarity −1000..=1000) and `Confidence` (0..=1000) newtypes instead of one `Millis` for both (spec §4.1 deviation) |
| 0003 | Trail types live in `instinct-core` (Pack returns Decision); `instinct-explain` owns replay, diff and rendering; spec's `Stage`/`CandidateGen`/`Scorer` traits deferred until a second implementation exists |
| 0004 (not written) | Pack data: TOML via `include_str!` parsed at load into validated tables; build.rs codegen discarded |
| 0005 | Rules match on tokens (sorted token index) instead of a byte-level Aho-Corasick automaton: token-boundary semantics by construction |
| 0006 | NFC by chunking at stable starters with a differential oracle; fold is `char::to_lowercase` (Unicode version pinned by the 1.99.0 toolchain and recorded in `DataVersion`) |
| 0007 (not written) | Owner-decision defaults (q14, q15, q8, q5): case and punctuation runs are symmetries for autosteer; label diacritics are NOT folded in ocr-labels; WL `h = 3`; sample catalogs only; no TIN checksum |
| 0008 | Interrupt never auto-applies, expressed as a type (`AutoApply`) |
| 0009 | Jev adapter: one float boundary (`convert.rs`), score levels matched by label not rung, off-menu guard on labels and probability keys, noul drift check, labelled fallback (`FallbackWhy`) |

## Plan review

Reviewed against docs/spec.md (§3–§13) and the skill checklist. Gaps found and fixed in this plan:

1. **Spec §4.1 uses one `Millis` for similarity and confidence.** A confidence of −300 would be
   expressible. Fixed: two newtypes (ADR 0002); conversion from similarity to confidence is an
   explicit, saturating function.
2. **Trail placement.** The spec puts the trail in `instinct-explain`, but `Pack::ask` in `instinct-core`
   returns it, and `instinct-explain` depends on `instinct-core`. Fixed: trail types in core (ADR 0003).
3. **Spec traits without a second implementation** (`Stage`, `CandidateGen`, `Scorer`) would be
   speculative public surface (skill §7). Fixed: deferred, documented in ADR 0003; `Pack` and
   `ScorerKind` are kept because packs and the trail need them.
4. **"Typo repair" could turn ordinary words into control words** ("step" → "stop"), an
   interrupt false positive. Fixed: guard words per pack, no repair under 4 chars, repair penalty
   large enough that a lone repaired cue cannot reach `standard` on its own; eval asserts zero
   interrupt false positives.
5. **Confusables.** The spec says a confusable inside a control word lowers confidence and is never
   silently folded. Fixed: skeleton match is recorded as a confusable hit that never fires a cue.
6. **Bench tool.** Spec §5.3 names criterion with a 20 % gate; the skill prefers instruction
   counts. Fixed: gungraun Ir gate at 10 % (deterministic metric, tighter is safe); wall-clock
   budgets reported informationally only (T15).
7. **Miri requested "only if unsafe exists".** Fixed: documented skip plus a nightly audit that
   fails if unsafe appears, so the skip cannot silently go stale.
8. **Missing fuzz targets for parsers.** Added T16 (TOML-fed lexicon/rules, Jev reply JSON, graph).
9. **Open questions** (q5 TIN checksum, q8 licensed lists, q13 Jev input, q14 case/diacritics,
   q15 WL rounds) get the safe recorded default (owner decisions, see the note at the top), not a guess.
10. **Target selection** is spec'd to wait for PRD Phase B. Fixed: behind an off-by-default
    feature so `standard` consumers cannot accidentally depend on it.
11. **CI permissions.** The box's GitHub token cannot write `.github/workflows`. Workflow changes
    go through the owner's credentialed machine; recorded in ADR 0001 and docs/progress.md.
