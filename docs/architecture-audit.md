# Architecture audit (Phase 1): PUA as a domain-agnostic decision engine

Status: **audit only, nothing changed.** No crate, pack or public API is modified by this document.
Every destructive step below is **blocked until Uriah approves it** (§10).

- Audited commit: `main` @ `6534c15` (2026-10-05), Rust 1.99.0.
- Baseline at that commit (run locally for this audit): `cargo nextest run --workspace --all-features
  --locked` → **235/235 pass**; `cargo test --doc --workspace` → all pass;
  `scripts/check_architecture.py` → ok; `scripts/check_repo_rules.sh` → ok.
- Method: code first, docs second. Every claim cites a file. Behaviour claims marked **(probed)** were
  checked by running a scratch binary against the crates at `6534c15`.

## 0. The principle this audit applies

> **PUA owns HOW a deterministic decision is made. Consumers own WHAT the decision means.**

```text
observation
  → canonicalize / extract structure          pua-text, pua-graph
  → finite candidates                         pua-core (Question options / candidate ids)
  → deterministic evidence                    pua-lexicon, pua-rules, pua-graph, pua-hdc
  → scores                                    pua-core::Scores  (Max = raise_to, Sum = add)
  → threshold + margin                        pua-core::decide  (Profile thresholds)
  → answer | abstain                          pua-core::Answer + Trail + DataVersion → pua-explain
```

Mathematically: `x → φ(x) → evidence → scores(c₁…cₙ) → decision | abstain`.

## 1. What is actually implemented today (code, not docs)

| Crate | Path | src lines¹ | Real content | Consumers inside the repo |
|---|---|---:|---|---|
| `pua-core` | `crates/pua-core` | 2,229 | `Millis`, `Confidence`, `Span`, `DataVersion`, `Question`, `Answer`, `Ranked`, `AbstainReason`, `Scores`, `decide`, `Profile`/`Thresholds`/`HdcMode`, `CandidateId`/`rank_candidates`/`CandidatePick`, `Trail`, `Decision`, `Pack` | every crate |
| `pua-text` | `crates/pua-text` | 1,092 | chunked NFC + offset map, protected spans, `Fold`, `PunctRuns`, tokens, sentences, confusables | lexicon, rules, steer, gateway, bir (`name.rs`), toolbox |
| `pua-lexicon` | `crates/pua-lexicon` | 1,047 | validated closed vocabulary, longest exact ≤ 3 tokens, opt-in substring, SymSpell repair, guards, confusable flags; `ocr` module (digit table) | rules, steer, bir (`ocr` only) |
| `pua-rules` | `crates/pua-rules` | 1,056 | token-keyed rules, slots, specificity, negation window, question damper, repairs, Max/Sum | steer only |
| `pua-hdc` | `crates/pua-hdc` | 712 | typed `Hv<D>`, seeded encoder, bind/permute/bundle, `Accumulator`, `Codebook` cleanup/decode, `resonate` (≤ 16) | **none** (bench only; `pua-steer` lists it as an optional dep that no code uses) |
| `pua-graph` | `crates/pua-graph` | 415 | `LabeledGraph`, 1-WL (`wl_refine`), `Rounds`, fingerprints, per-node colours, `spd-wl` feature | bir, ocr, toolbox |
| `pua-explain` | `crates/pua-explain` | 283 | `ReplayRecord`, JSON lines, `InputHash`, `check`, `diff`, `render_trail` | steer **examples/tests only** |
| `pua-jev` | `crates/pua-jev` | 801 | Jev wire shapes, float → `Confidence` once, off-menu guard, `Escalation`/`FallbackWhy` | **none** (fuzz only) |
| `pua-steer` | `packs/pua-steer` | 532 | the only `impl Pack`; text → lexicon → rules → decide; `AutoApply`; 221-row eval; golden journal | — |
| `pua-gateway` | `packs/pua-gateway` | 297 | `shape_of` → integer `ShapeFeatures` (no `Question`, no `decide`) | — |
| `pua-bir` | `packs/pua-bir` | 435 | `suggest_tin`, sample `Catalog`, `fold_name`, `FormRecord`, `layout_fingerprint` (no `decide`) | — |
| `pua-ocr` | `packs/pua-ocr` | 275 | `extract` (COR label → value span), `MatchType` confidences, `layout_fingerprint` (no `decide`, **no `pua-text`**) | — |
| `pua-toolbox` | `packs/pua-toolbox` | 208 | `select` (candidate set → `Question::choice` → `decide`), `overlap_score`, `call_dag_fingerprint` | — |

¹ `src/` lines excluding `tests.rs` files (inline `#[cfg(test)]` modules are included).

Facts that drive the recommendations:

1. **The engine crates are already domain-agnostic in code.** A search of `crates/*/src` for
   `queue|steer|interrupt|delivery` finds only test fixtures, doc examples, and one real leak: the
   `AbstainReason::Confusable` message, `"confusable control word"` (`crates/pua-core/src/answer.rs:32,43`).
2. **The generic pipeline is trapped inside a pack.** `Autosteer::pipeline`
   (`packs/pua-steer/src/lib.rs:163-299`) is ~140 lines of fully generic code: normalize → lexicon
   lookup → trail → confusable abstain → rules → copy class scores into `Scores` → `decide` → trail.
   The only domain parts are the question name, the three class names and `AutoApply`.
3. **The generic candidate-set selector is also trapped inside a pack.** `pua_toolbox::select`
   (`packs/pua-toolbox/src/lib.rs:78-129`) sorts a candidate set by id, builds a `Question::choice`,
   scores each candidate by token overlap and calls `decide`. Nothing in it is about tools except the
   names. Meanwhile `pua-core` already ships `CandidateId`, `rank_candidates` and
   `RankedCandidates::pick` (`crates/pua-core/src/candidate.rs:84-170`), which **no crate uses**.
4. **Three of five packs make no decision.** `pua-gateway`, `pua-bir` and `pua-ocr` never build a
   `Question` or call `decide`. They are feature extractors and format helpers for one consumer each.
5. **`pua-hdc` (and its resonator) has no consumer.** It is exercised by its own tests, the capacity
   example and two benches. `Profile::thresholds().hdc` (`HdcMode`) is never read by any code
   (`crates/pua-core/src/profile.rs:9-17,40`); it is only hashed into `Profile::table_bytes`.
6. **Nobody depends on PUA yet.** The repo has no tags or releases, and the root `Cargo.toml` of
   nativechat, opengrok-server, open-ai-gateway, buwiz-forms and hyper-use has no `hexuria/pua`
   dependency (checked through the GitHub API on 2026-10-05). Moving code out therefore needs **no
   compatibility wrappers** and breaks no consumer build.

## 2. Pack contents, classified

Categories: **GEP** = generic engine primitive · **GDS** = generic reusable data structure ·
**DP** = domain policy · **PA** = product adapter · **T/E** = test / evaluation · **DEAD** = dead or redundant.

### 2.1 `packs/pua-steer` (NativeChat / opengrok delivery advice)

| Item | Path | Class | Finding |
|---|---|---|---|
| `Autosteer::pipeline` | `src/lib.rs:163-299` | **GEP** | Generic text → lexicon → rules → decide with trail. Extract (§5.1). |
| `Autosteer::from_toml` (parse + validate + `DataVersion`) | `src/lib.rs:109-156` | **GEP** + DP | Parsing/validation/versioning is generic; the `[queue, steer, interrupt]` class-order check (`:124-140`) is policy. |
| Confusable → `Abstain` | `src/lib.rs:229-258` | **GEP** | A generic "guarded term seen through a homoglyph → abstain" policy switch. |
| `empty_ranked()` + abstains with an empty ranked list | `src/lib.rs:318-323` | **DEAD** (contract bug) | `Answer::Abstain` docs promise the ranked options (`crates/pua-core/src/answer.rs:7-8`); the pack returns an empty list on refusal, config mismatch and confusables. Fix in core (§5.3). |
| `option::{QUEUE, STEER, INTERRUPT}`, question `"delivery"` | `src/lib.rs:48-57,141` | **DP** | Meaning of the options. Moves to NativeChat. |
| `Advice`, `AutoApply`, `label_of` | `src/advice.rs` | **DP** | "Interrupt never auto-applies" (ADR 0008) is a consumer rule. Moves with the consumer. |
| `Input` | `src/input.rs:16-50` | **PA** | Wraps a `&str`. |
| `LiveRun`, `Input::with_runs`, `live_runs()` | `src/input.rs:5-14,31-49` | **DEAD** | Phase B placeholder; nothing reads it. |
| Feature `target-selection` + optional `pua-hdc` dep | `Cargo.toml:13-14` | **DEAD** | No `cfg(feature = "target-selection")` exists anywhere. |
| `pua-explain` as a normal dependency | `Cargo.toml` `[dependencies]` | **DEAD** (misplaced) | Used only by `examples/` and `tests/`; should be a dev-dependency. |
| `prelude` module | `src/lib.rs:326-330` | **DEAD** | Convenience re-exports for examples. |
| `data/lexicon.toml`, `data/rules.toml` | `data/` | **DP** | The delivery vocabulary and cue weights. Moves to NativeChat; a labelled copy should stay as an engine fixture (§5.1). |
| `data/eval.jsonl` (221 rows), `examples/eval.rs`, `examples/replay.rs`, `examples/write_golden.rs`, `tests/eval_gate.rs`, `tests/props.rs`, `docs/eval/autosteer.md`, `docs/goldens/autosteer/journal.jsonl` | — | **T/E** | Valuable engine regression evidence. Keep a fixture copy in the engine; the product gate moves with the data. |

### 2.2 `packs/pua-gateway` (open-ai-gateway request shape)

| Item | Path | Class | Finding |
|---|---|---|---|
| `shape_of`, `ShapeFeatures`, `ShapeFeatures::fingerprint` | `src/lib.rs:87-182` | **PA** | Integer features for `oag-router`. No candidates, scores or abstain: not a PUA decision. |
| `DominantScript`, `script_of`, `dominant_raw` | `src/lib.rs:34-80,189-250` | **PA** | Script histogram. Used once; not worth an engine primitive yet. |
| fenced-block count via `pua-text` protected spans; homoglyph count via `pua-text` confusables | `src/lib.rs:122-170` | (uses **GEP**) | Already generic; the consumer keeps calling `pua-text`. |
| `json_shape`, `walk_json`, `text_asks_structured` (`response_format`, `json_schema`, `structured_outputs`) | `src/lib.rs:253-293` | **DP** | OpenAI-API vocabulary. |
| `is_hunk_header`, `count_fences_raw`, `count_homoglyphs_raw` | `src/lib.rs:184-232` | **PA** | Fallbacks for over-long input. |
| `json_keys` doc says "Distinct JSON object keys" | `src/lib.rs:97` | **DEAD** (wrong doc) | It counts every key occurrence: `[{"a":1},{"a":2},{"a":3}]` → `3` **(probed)**. |
| `tests/props.rs`, `src/tests.rs`, bench `gateway_shape_mixed` | — | **T/E** | Move with the code. |

### 2.3 `packs/pua-bir` (buwiz-forms BIR fields)

| Item | Path | Class | Finding |
|---|---|---|---|
| `suggest_tin`, `Suggestion`, `SuggestionKind`, `TinError` | `src/tin.rs` | **DP** | BIR TIN format rules. |
| `SuggestionKind::Catalog` | `src/tin.rs:16` | **DEAD** | Never constructed (only its `name()` is tested). |
| `TinError::NotNumeric { at }` doc "Byte offset in the original field" | `src/tin.rs:70` | **DEAD** (wrong doc) | The offset is into the separator-stripped string: `"12-3X"` → `at: 3`, original offset is 4 **(probed)**. |
| `Catalog`, `CodeKind`, `data/catalog.toml` | `src/catalog.rs` | **DP** | Sample RDO/form codes (non-authoritative). |
| `Catalog::longest_exact` | `src/catalog.rs:106-116` | **DEAD** | Same as `get`: it tests `needle == code`, so "longest" never applies (`longest_exact("RDO-39 extra") = None`) **(probed)**. The real longest-match primitive is `pua-lexicon`. |
| `fold_name`, `names_equal` | `src/name.rs` | **DEAD** (thin wrapper) | `pua_text::normalize(..).canonical()` with `UnicodeLower`. Consumers call `pua-text` directly. |
| `FormRecord` | `src/lib.rs:33-54` | **DEAD** | A sorted, de-duplicated key list (`BTreeMap` semantics). |
| `layout_fingerprint` | `src/lib.rs:58-76` | **DEAD** (redundant math) | 1-WL on a **complete graph** of hashed keys only depends on the key multiset, so it equals a hash of the sorted keys **(probed: same result for different values and order)**. Duplicated in `pua-ocr`. |
| `data_version` | `src/lib.rs:79-84` | **PA** | |
| uses `pua_lexicon::ocr::repair_numeric_field` | `src/tin.rs:7` | — | The only consumer of `pua-lexicon::ocr` (§3). |

### 2.4 `packs/pua-ocr` (buwiz-forms COR OCR)

| Item | Path | Class | Finding |
|---|---|---|---|
| `LABELS` (COR vocabulary) | `src/lib.rs:25-40` | **DP** | |
| `MatchType::confidence` (900 / 800 / 700) | `src/lib.rs:56-63` | **DP** | Product constants. |
| `extract`, `match_line`, `find_label_value`, `label_len_in`, `line_has_label_only` | `src/lib.rs:106-226` | **PA** | "LABEL: value" spans over lines in original offsets. Correct for multibyte prefixes **(probed)**, but implemented **without `pua-text`**: no offset map, no NFC. One consumer, so it is not an engine primitive yet. |
| `eq_label`, `chars_eq` | `src/lib.rs:228-246` | **DEAD** (duplicate) | Re-implements a case fold that `pua-text` already owns. |
| labels re-sorted on every line | `src/lib.rs:123-124` | **DEAD** (waste) | Sort once. |
| `layout_fingerprint` | `src/lib.rs:253-272` | **DEAD** (redundant math) | Same complete-graph construction as `pua-bir`. |
| `pua-text` dependency | `Cargo.toml` | **DEAD** | Declared but never imported. |

### 2.5 `packs/pua-toolbox` (tool choice)

| Item | Path | Class | Finding |
|---|---|---|---|
| `select` | `src/lib.rs:78-129` | **GEP** | A generic "choose one of a consumer-supplied candidate set, or abstain". Extract (§5.2). |
| `overlap_score`, `is_token_sep` | `src/lib.rs:143-169` | **GEP** | Token-overlap evidence. Re-tokenizes with its own splitter (byte length ≥ 3) instead of `pua-text` tokens. |
| `ToolId`, `Candidate` | `src/lib.rs:33-75` | **GDS** (duplicate) | Duplicates `pua_core::CandidateId` (which validates length and emptiness; `ToolId` does not). |
| `chosen_id` | `src/lib.rs:132-141` | **PA** (misuse hazard) | Returns the top id **even when the answer is `Abstain`** **(probed)**. A caller can act on an abstain without noticing. |
| `version` | `src/lib.rs:171-183` | **GEP** | Candidate-set `DataVersion`; omits `Profile::table_bytes` and the normalize config, which `pua-steer` includes. |
| duplicate ids → `"too few tools"` | `src/lib.rs:84-85` | **DEAD** (wrong message) | `Question::choice` rejects duplicates, and the trail says "too few tools" **(probed)**. |
| `call_dag_fingerprint` | `src/lib.rs:186-205` | **GEP** (thin) | Hash string labels → `LabeledGraph` → WL. The "hash a string to a `u64` node label" step is repeated in bir, ocr and toolbox. |
| question name `"tool"` | `src/lib.rs:84` | **DP** | |

## 3. Engine crates, module by module

| Item | Path | Verdict | Why |
|---|---|---|---|
| `Millis`, `Confidence`, `Span`, `DataVersion`, `Question`, `Answer`, `Ranked`, `Scores`, `decide`, `Trail`, `Decision` | `crates/pua-core/src` | **KEEP** | The decision contract. |
| `decide` vs `RankedCandidates::pick` | `crates/pua-core/src/decide.rs:119-170`, `candidate.rs:140-170` | **MERGE** | Two copies of the same threshold/margin gate. One private `gate(top, second, thresholds)` should serve both. |
| `CandidateId`, `rank_candidates`, `RankedCandidates`, `CandidatePick` | `crates/pua-core/src/candidate.rs` | **MERGE** | Unused, while toolbox rebuilds the same idea. Keep `CandidateId`; turn the rest into one `CandidateSet` (sorted, unique) that converts to a `Question` and maps `OptionIndex` back to an id (§5.2). |
| `Pack` trait | `crates/pua-core/src/pack.rs:48-66` | **REMOVE** (after §5.1) | One implementation (`Autosteer`). No code is generic over `Pack`; replay uses a closure (`crates/pua-explain/src/lib.rs:152`). See §6. |
| `HdcMode`, `Thresholds::hdc` | `crates/pua-core/src/profile.rs:9-17,40` | **REMOVE** | Never read. A profile does not switch stages in code, contrary to spec §4.6. |
| `Profile` presets `fast`/`standard`/`deep` | `crates/pua-core/src/profile.rs` | **KEEP** | Thresholds as tables. Custom thresholds are optional later (§5.4). |
| `AbstainReason::Confusable` text "confusable control word" | `crates/pua-core/src/answer.rs:32,43` | **RENAME** (text only) | "Control word" is a steer concept. Use "confusable token matched a guarded term". |
| `StageKind`, `ScorerKind` | `crates/pua-core/src/trail.rs` | **KEEP** | Generic stage labels. `Escalation` stays useful for consumer-side escalation. |
| `pua-text` (all) | `crates/pua-text/src` | **KEEP** | Canonicalization, offset map, protected spans, confusables. Doc mentions of opengrok/autosteer (`lib.rs:76,99`) become neutral. |
| `pua-lexicon` core | `crates/pua-lexicon/src/lib.rs`, `repair.rs` | **KEEP** | Bounded candidate generation and typo repair. |
| `pua-lexicon::ocr` (hard-coded `O→0 l→1 I→1 S→5 B→8`) | `crates/pua-lexicon/src/ocr.rs` | **MOVE TO CONSUMER** (default) | One consumer (`pua-bir`). Alternative: generalize into a data-driven character-confusion table if a second consumer appears. |
| `pua-rules` | `crates/pua-rules/src` | **KEEP** + home of the extracted classifier (§5.1) | Rule contexts (`negation`, `question`) are generic. It already depends on core, text and lexicon, so no new crate is needed. |
| `pua-hdc` (all, incl. resonator) | `crates/pua-hdc/src` | **KEEP** | Protected machinery. Document it honestly as "available evidence source, not yet wired into a decision path". Composition already works without new API (§5.5). |
| `pua-graph` | `crates/pua-graph/src` | **KEEP** | Add one small helper for the thrice-repeated "string → `u64` node label" hash. |
| `pua-explain` | `crates/pua-explain/src` | **KEEP** | Generic replay and diff. |
| `pua-jev` | `crates/pua-jev` | **MOVE TO CONSUMER** (recommended; Uriah decides) | Wire shapes of one vendor service, mirrored from opengrok-server. No in-repo consumer. Moving it makes PUA float-free with **zero** `float_arithmetic` exceptions. The integer-only part worth keeping (refuse an off-menu label) is a few lines that can sit in `pua-core`. |
| `benches/` | `benches/benches/paths.rs` | **KEEP**, repoint | `autosteer_ask` → classifier on the fixture data; `gateway_shape_mixed` leaves with the gateway. |
| `fuzz/` | `fuzz/fuzz_targets` | **KEEP** | `jev_reply_parse` leaves with `pua-jev`. |

## 4. Decision tables

### KEEP

| What | Why |
|---|---|
| `pua-core` contract types and `decide` | This is the "how". |
| `pua-text` | Every invariance is enforced here once (spec §5.2). |
| `pua-lexicon` (minus `ocr`) | Finite candidate generation with bounded repair. |
| `pua-rules` | Deterministic symbolic evidence with Max/Sum scorer labels. |
| `pua-hdc` incl. resonator | Deterministic geometric / resonance evidence. Owner wants it, and it is sound and measured (`docs/hdc-capacity.md`). |
| `pua-graph` incl. `spd-wl` | Deterministic structural evidence. |
| `pua-explain` | Byte-identical replay is the proof of determinism. |
| CI, fuzz, mutants, Ir bench gate, architecture check | Unchanged quality bar. |

### MOVE TO CONSUMER

| What | To | Why |
|---|---|---|
| `packs/pua-steer` domain parts: `data/*.toml`, `option`, `Advice`/`AutoApply`, `Input`, eval gate | `hexuria/nativechat` (an `autosteer` module); opengrok-server reuses the same data if it adopts | Option meanings and the interrupt rule are product policy. |
| `packs/pua-gateway` (whole crate) | `hexuria/open-ai-gateway` (`oag-router` or a small `oag-shape` crate) | A feature extractor, not a decision. |
| `packs/pua-bir` (TIN, catalog) | `hexuria/buwiz-forms` (`bir-core` / `bir-rules`) | BIR rules and data. |
| `packs/pua-ocr` (COR extraction) | `hexuria/buwiz-forms` (`bir-desktop` next to `cor_ocr.rs`) | COR labels and confidences are product constants. |
| `pua-lexicon::ocr` | `hexuria/buwiz-forms` with `pua-bir` | Single consumer. |
| `crates/pua-jev` | `hexuria/opengrok-server` (next to its `JevDoor`) | Vendor wire format; the consumer already owns the HTTP side. **Owner decision.** |

### MERGE

| What | Into | Why |
|---|---|---|
| `Autosteer::pipeline` + `from_toml` | `pua-rules::RuleClassifier` (§5.1) | The one generic pipeline in the repo. |
| `pua_toolbox::select` + `ToolId`/`Candidate` + core `rank_candidates`/`pick` | `pua_core::CandidateSet` + token-overlap evidence in `pua-lexicon` (§5.2) | Two half-implementations of "choose from a set". |
| `decide` and `RankedCandidates::pick` gates | one private gate in `pua-core` | Duplicate logic. |
| bir/ocr `layout_fingerprint` | delete; consumers hash the sorted labels | Complete-graph WL adds nothing. |
| three "string → `u64` label" hashes | one `pua-graph` helper | Repeated primitive. |

### REMOVE

| What | Path | Why |
|---|---|---|
| `Pack` trait | `crates/pua-core/src/pack.rs:48-66` | One implementation, no generic user (§6). `Decision` stays. |
| `HdcMode` / `Thresholds::hdc` | `crates/pua-core/src/profile.rs` | Never read. |
| `CandidatePick`, `RankedCandidates::pick` | `crates/pua-core/src/candidate.rs` | Replaced by the merged gate and `CandidateSet`. |
| `target-selection` feature, `LiveRun`, `Input::with_runs` | `packs/pua-steer` | No code behind them. |
| `packs/` directory, its workspace members, `scripts/architecture.txt` entries, `CODEOWNERS` lines | repo root | After the moves and merges above. |
| `Catalog::longest_exact`, `SuggestionKind::Catalog`, `fold_name`, `FormRecord` | `packs/pua-bir` | Redundant (the consumer copy should drop them too). |

### RENAME

| From | To | Why |
|---|---|---|
| `AbstainReason::Confusable` display `"confusable control word"` | `"confusable token matched a guarded term"` | Remove a domain word from core. The variant name stays. |
| "Predictable Universal Advisor … packs" framing in crate descriptions | "deterministic, abstain-first decision engine over finite candidates" | Describe what the code is. |
| No crate renames | — | The seven engine names already say what they do. |

### DOCUMENTATION FIX

Stale or false claims, with file:line at `6534c15`:

| File:line | Claim | Reality |
|---|---|---|
| `README.md:69-73` | "**Spec only.** … Every crate is an empty stub" | 13 crates implemented, 235 tests. |
| `README.md:3-8` | PUA "make[s] … queue, steer or interrupt; which label; which field fix; which tool" through packs | The engine decides; consumers give meaning. |
| `README.md:41-48` | Consumer table; "`pua-toolbox` … is also specified" | No consumer depends on PUA yet; toolbox is implemented. |
| `README.md:53` | Pin `tag = "v0.1.0"` | No tag exists. |
| `docs/spec.md:3` | "draft for review, spec only. No code beyond empty crate stubs." | Implemented. |
| `docs/spec.md:105-106` | "Today the repo holds only … empty stubs" | Implemented. |
| `docs/spec.md:108-111` | packs hold `build.rs` codegen and `fixtures/` | `include_str!` + TOML parsed at load; no `fixtures/` directories. |
| `docs/spec.md:85`, `:232-235` | `Stage` / `CandidateGen` / `Scorer` / `Decider` traits | Never implemented (deferred by ADR 0003). |
| `docs/spec.md:95` | trail lives in `pua-explain` | It lives in `pua-core` (ADR 0003). |
| `docs/spec.md:214` | one `Millis` for similarity and confidence | Two newtypes (ADR 0002). |
| `docs/spec.md:248` | fold named `UnicodeCaseFold` | Code: `Fold::UnicodeLower`. |
| `docs/spec.md:155-157`, `:282-283` | `aho-corasick` automata, `criterion` | Token-keyed rules (ADR 0005); gungraun Ir gate. |
| `docs/spec.md:338-342` | profiles switch stages and the resonator | Thresholds only; `HdcMode` is never read. |
| `docs/spec.md:490` | criterion bench, 20 % gate | Callgrind Ir, 10 % (`docs/benchmarks.md:24`). |
| `docs/spec.md:584`, `docs/plan.md:44`, `packs/pua-ocr/src/lib.rs:1` | COR extraction "via the offset map" | `pua-ocr` does not use `pua-text`. Its spans are original offsets from its own scanner. |
| `docs/spec.md:730` | "Today: spec only, with empty crate stubs" | Implemented. |
| `docs/plan.md:33` | `decide_choice` | Function is `decide`. |
| `docs/plan.md:99,102,131`, `crates/pua-graph/src/lib.rs:47`, `packs/pua-bir/src/lib.rs:5`, `packs/pua-bir/data/catalog.toml:1` | ADR 0004 and ADR 0007 | Those files do not exist in `docs/adr/`. |
| `docs/review-report.md:25` | "ADRs 0001–0009" with topics such as "decide table", "HDC dims" | Seven ADRs exist: 0001, 0002, 0003, 0005, 0006, 0008, 0009. |
| `docs/review-report.md:13` | offset map proven via "ocr-labels" | Only `pua-text` uses the offset map. |
| `docs/review-report.md:35`, `docs/progress.md:100`, `packs/pua-steer/src/lib.rs:8-9`, `src/input.rs:1-6` | `target-selection` "feature exists" | The feature name exists; no code is behind it. |
| `CHANGELOG.md:9-21` | lists crates through `pua-steer` | Missing gateway, bir, ocr, toolbox, benches and the pack rename. |
| `crates/pua-core/Cargo.toml:3`, `CHANGELOG.md:13` | "Pack trait" as a core feature | See §6. |
| `packs/pua-gateway/src/lib.rs:97` | "Distinct JSON object keys" | Counts all occurrences. |
| `packs/pua-bir/src/tin.rs:70` | "Byte offset in the original field" | Offset into the stripped string. |
| `crates/pua-core/src/lib.rs:16-25`, `src/question.rs:330-338` | doc example uses `delivery`/`queue`/`steer`/`interrupt` | Use a neutral example in the engine. |

## 5. Target architecture

### 5.1 `RuleClassifier` (extracted from `pua-steer`, lives in `pua-rules`)

This is grounded line for line in `packs/pua-steer/src/lib.rs:109-299`. It adds no new concept;
it names the pipeline that already exists.

```rust
// pua-rules
pub enum OnConfusable { Abstain, Ignore }            // steer uses Abstain today

pub struct ClassifierSpec {
    pub domain: String,          // DataVersion domain tag; NativeChat passes "pua-steer/1" to keep its goldens
    pub question: String,        // e.g. "delivery" — the consumer's word, not PUA's
    pub normalize: NormalizeConfig,
    pub lexicon: LexiconSpec,
    pub rules: RuleSetSpec,      // options = rule classes, in declared order; class 0 = safe default
    pub on_confusable: OnConfusable,
}

pub struct RuleClassifier { /* Question, Lexicon, RuleSet, NormalizeConfig, DataVersion */ }

impl RuleClassifier {
    pub fn new(spec: &ClassifierSpec) -> Result<Self, ClassifierError>;
    pub fn question(&self) -> &Question;
    pub fn data_version(&self) -> DataVersion;
    pub fn decide(&self, text: &str, profile: Profile) -> Decision; // pure; trail included
}
```

What the consumer then owns (NativeChat example):

```rust
let steer = RuleClassifier::new(&ClassifierSpec {
    domain: "pua-steer/1".into(), question: "delivery".into(),
    normalize: NormalizeConfig { fold: Fold::UnicodeLower, punct_runs: PunctRuns::Collapse },
    lexicon: toml::from_str(LEXICON_TOML)?, rules: toml::from_str(RULES_TOML)?,
    on_confusable: OnConfusable::Abstain,
})?;
let d = steer.decide(message, Profile::Standard);
let label = d.answer().chosen().and_then(|i| steer.question().options()?.get(i));
let auto_apply = if label.map(|l| l.as_str()) == Some("interrupt") { AutoApply::Never } else { AutoApply::Allowed };
```

Verification gate for this step: the moved golden journal (`docs/goldens/autosteer/journal.jsonl`)
must replay **byte-identical** through `RuleClassifier` with the same domain tag, and the 221-row
eval must give the same confusion matrix. A labelled copy of the delivery data stays in
`crates/pua-rules/tests/fixtures/` as the engine's realistic regression fixture.

### 5.2 `CandidateSet` (merged from `pua-toolbox` + unused core candidate code)

```rust
// pua-core
pub struct CandidateSet(Box<[CandidateId]>);          // sorted by id, unique: input order cannot matter
impl CandidateSet {
    pub fn new(ids: impl IntoIterator<Item = CandidateId>) -> Result<Self, CandidateError>; // Duplicate(id)
    pub fn question(&self, name: &str) -> Result<Question, QuestionError>;
    pub fn id(&self, i: OptionIndex) -> Option<&CandidateId>;
    pub fn chosen<'a>(&'a self, a: &Answer) -> Option<&'a CandidateId>; // None on Abstain, by construction
}
// pua-lexicon (or pua-text): token-overlap evidence on pua-text tokens
pub fn overlap(query: &Normalized<'_>, candidate: &Normalized<'_>) -> Confidence;
```

This removes the `chosen_id`-on-abstain hazard. The first step keeps toolbox's exact scoring so
`packs/pua-toolbox/tests/props.rs` stays green unchanged. Switching to `pua-text` tokens changes
scores and is a separate, labelled behaviour change.

### 5.3 Small core fixes (additive first)

- One private gate shared by `decide` and the candidate path.
- `Answer::abstain(scores: &Scores, why)` so an abstain always carries every option ranked. This
  closes the empty-ranked bug in steer and toolbox.
- Neutral `AbstainReason::Confusable` text.

### 5.4 Optional, only when a consumer needs it

- Custom `Thresholds` (validated: `min_margin ≥ 1`, so an exact tie still always abstains, which
  `decide.rs` relies on today because every preset has a positive margin).

### 5.5 HDC and graph as evidence (no new API)

The existing types already compose. A consumer that wants resonance evidence does this:

```rust
let hit = codebook.cleanup(&query, floor);           // pua-hdc: Nearest { index, similarity, margin }
scores.raise_to(idx_of(hit.index), hit.similarity.to_confidence())?;   // Max scorer
trail.push(TrailRecord::new(StageKind::Hdc, "...").millis(hit.similarity));
// resonate(...) → Resonance::NotConverged maps to AbstainReason::NotConverged
```

We should not build an `Evidence` trait or a `dyn` evidence list until two evidence sources must be
combined generically by PUA itself. Today the composition point is `Scores` (Max = `raise_to`,
Sum = `add`), which is already zero-dispatch and allocation-light. This matches ADR 0003's rule:
no trait without a second implementation.

### 5.6 The resulting shape

```text
consumer repo (NativeChat · open-ai-gateway · buwiz-forms · opengrok-server)
   data: lexicon.toml, rules.toml, labels, codebooks · option meanings · AutoApply · wire shapes · Jev door
                │  git dependency pinned by tag/rev
                ▼
   pua-rules (RuleClassifier) ──► pua-lexicon ──► pua-text ──► pua-core ◄── pua-hdc
                                                                  ▲   ▲
                                                     pua-graph ───┘   └─── pua-explain
```

Allowed dependency edges (the new `scripts/architecture.txt` would encode exactly these):

| Crate | May depend on |
|---|---|
| `pua-core` | — |
| `pua-text` | `pua-core` |
| `pua-lexicon` | `pua-core`, `pua-text` |
| `pua-rules` | `pua-core`, `pua-text`, `pua-lexicon` |
| `pua-hdc` | `pua-core` |
| `pua-graph` | `pua-core` |
| `pua-explain` | `pua-core` |

Never allowed: any edge from a `pua-*` crate to a consumer crate or to consumer concepts. The
existing bans (tokio, axum, sqlx, reqwest, hyper, typesafe-sdk, opengrok-*) stay.

Result: **13 workspace crates → 7** (8 if `pua-jev` stays), plus `benches` and `fuzz`.

## 6. `Pack` trait: drop

- One implementation: `impl Pack for Autosteer` (`packs/pua-steer/src/lib.rs:302`).
- No generic user: `ReplayRecord::check` takes a closure (`crates/pua-explain/src/lib.rs:152`);
  examples and tests call `Autosteer` directly through the trait only for `ask`.
- Its GAT `Input<'a>` lets every pack invent its own input shape. That is the opposite of a
  generic engine.
- `RuleClassifier::decide(text, profile) -> Decision` covers the one real case. A consumer that
  wants its own trait can define it in its own repo.
- `Decision` stays in `pua-core`; it is the journaled unit.

## 7. Consumer migrations

No consumer depends on PUA today (§1 fact 6), so each item is a **code handoff**, not a breaking
upgrade. Each consumer pins the first PUA tag cut after the refactor.

| Consumer | Takes | Replaces `pua-*` calls with |
|---|---|---|
| NativeChat (`OnSend::Auto`, spec §7.1) | `data/lexicon.toml`, `data/rules.toml`, `eval.jsonl`, golden journal, `AutoApply`, option consts | `pua_rules::RuleClassifier` + `pua_explain` for journals |
| opengrok-server (optional, spec §7.2) | same delivery data if it adopts; `pua-jev` if moved | `RuleClassifier`; own Jev wire shapes next to `JevDoor` |
| open-ai-gateway (spec §7.3) | `pua-gateway` source and tests | `pua_text::normalize` for fences and confusables; the rest is its own code |
| buwiz-forms (spec §7.4) | `pua-bir` + `pua-lexicon::ocr` + `pua-ocr` source and tests | `pua_text` (Ñ-safe fold, offset map); `pua_lexicon` for catalog longest-match |
| tool routing (q12, no consumer yet) | nothing to take | `pua_core::CandidateSet` + overlap evidence |

## 8. FMM relationship

FMM (BCSC's deterministic geometric processing) is a **conceptual inspiration**, not a blueprint:
high-dimensional representation, resonance and structural matching suggested the pipeline shape.
PUA is its own architecture. It combines symbolic rules (`pua-rules`), lexical evidence
(`pua-lexicon`), graph structure (`pua-graph`), hyperdimensional vectors and a resonator
(`pua-hdc`), integer scoring, and explicit abstention (`pua-core`). PUA does not reproduce FMM's
algorithms. It is not a generative model. No crate or type should be renamed to FMM or HGRA
vocabulary unless it implements that thing (spec §1 non-goals already say "not a clone of BCSC's
FMM" and "no crate is named `hgra`").

## 9. Invariants the refactor must not weaken

| Invariant | Enforced today by |
|---|---|
| same input + data + profile → byte-identical `Decision` | `crates/pua-core/tests/props.rs` (`decide_is_deterministic`), `packs/pua-steer/tests/props.rs` (`determinism`), golden replay in `tests/eval_gate.rs` |
| candidate order never changes the answer | `crates/pua-core/tests/props.rs` (`candidate_permutation_is_invariant`), `packs/pua-toolbox/tests/props.rs` |
| exact tie / low margin → abstain | `crates/pua-core/src/decide.rs` tests + `abstain_iff_below_threshold_or_margin` (relies on every profile margin ≥ 1) |
| low confidence → abstain | same |
| no hidden randomness, clocks, env, unordered maps | `clippy.toml` disallowed types/methods; `float_arithmetic` denied |
| no network in the decision path | `deny.toml` + `scripts/architecture.txt` bans |
| declared invariances exact (`==`) | `crates/pua-text/tests/props.rs`, per-pack generator-sequence proptests |

Every refactor step must keep these green. Pack proptests move with the code into consumers, and
generic copies stay in the engine for `RuleClassifier` and `CandidateSet`.

## 10. Proposed refactor sequence, and what is blocked

Additive steps (safe, each a small PR, tests green, no deletion):

1. `pua-core`: shared gate, `Answer::abstain`, `CandidateSet`, neutral confusable text.
2. `pua-graph`: string → `u64` label helper.
3. `pua-rules`: `RuleClassifier`. Rewire `pua-steer` onto it. The golden journal must replay byte-identical.
4. `pua-core` + `pua-lexicon`: `CandidateSet` + overlap evidence. Rewire `pua-toolbox` onto it with identical scores.
5. Engine fixtures: a labelled copy of the delivery data, eval and golden under `crates/pua-rules/tests/fixtures/`. Repoint `autosteer_ask` bench.
6. Docs: fix every row of the DOCUMENTATION FIX table; README and spec rewritten around "how vs what"; new ADR 0010 for this boundary.

**Blocked: destructive steps awaiting Uriah's approval:**

| # | Step | Exact change |
|---|---|---|
| B1 | Delete `packs/pua-steer` | after NativeChat has the data + eval (handoff PR there) |
| B2 | Delete `packs/pua-gateway` | after an open-ai-gateway handoff PR |
| B3 | Delete `packs/pua-bir`, `packs/pua-ocr`, `crates/pua-lexicon/src/ocr.rs` | after a buwiz-forms handoff PR |
| B4 | Delete `packs/pua-toolbox` | after step 4 makes it a thin wrapper |
| B5 | Move or keep `crates/pua-jev` | **owner decision** (recommend: move to opengrok-server) |
| B6 | Remove `Pack` trait, `HdcMode`, `CandidatePick`/`RankedCandidates::pick` | public API removal (pre-1.0, no tags, no consumers) |
| B7 | Remove `packs/` from `Cargo.toml` members, `scripts/architecture.txt`, `.github/CODEOWNERS`; drop `gateway_shape_mixed` bench and `jev_reply_parse` fuzz target if B2/B5 | follows B1–B5 |
| B8 | Open handoff PRs in nativechat, open-ai-gateway, buwiz-forms (and opengrok-server if B5) | writes to other repos |

Questions for Uriah:

1. **`pua-jev`**: move to opengrok-server (PUA becomes 100 % float-free), or keep as PUA's escalation adapter?
2. **Handoff targets**: confirm NativeChat, open-ai-gateway and buwiz-forms as the homes in §7, or park the pack code on an archive branch in this repo instead.
3. **`pua-lexicon::ocr`**: move with BIR (default), or generalize into a data-driven confusion table now?
4. **HDC wiring**: keep `pua-hdc` as an unwired, documented capability until a consumer brings a codebook (recommended), or wire an optional codebook stage into `RuleClassifier` now?
5. **Custom thresholds**: presets only (recommended for now), or a validated `Thresholds` input?

## 11. Phase 2 follow-up (2026-10-05)

Uriah approved Phase 2 on 2026-10-05 with the defaults from §10: packs go to their consumers,
`pua-jev` goes to opengrok-server, HDC stays unwired, and only the three profile presets remain.
Every handoff target accepted a PR, so nothing was parked on an `archive/packs-*` branch.

### What was done

| Step | PR | Result |
|---|---|---|
| §10 step 3 + 5 | [#21](https://github.com/hexuria/pua/pull/21) (merged `6aef318`) | `pua-rules::RuleClassifier` (`ClassifierSpec`, `OnConfusable`) is the shared pipeline. The `delivery` fixture and its golden journal in `crates/pua-rules/tests/fixtures/delivery/` replay byte-identical to the old `pua-steer` journal. |
| §10 steps 1, 2, 4 + B6 | [#22](https://github.com/hexuria/pua/pull/22) (merged `e1635f1`) | `pua-core`: `CandidateSet`, one private gate in `decide`, `abstain()` so every abstain carries all options ranked, neutral confusable text. Removed `rank_candidates`, `RankedCandidates`, `CandidatePick`, `HdcMode`, `Thresholds::hdc` and the `Pack` trait. Added `pua-lexicon::overlap` and `pua-graph::label_of`. Toolbox fix: `chosen_id` is `None` on abstain. `DataVersion` changed on every golden row, because the profile table lost its HDC column. |
| B8 handoffs | nativechat#194, open-ai-gateway#149, buwiz-forms#67, opengrok-server#370 (drafts) | Consumer-owned crates depend on PUA by git rev `e1635f1`. See the migration table below. |
| B1–B5, B7 + §10 step 6 | [#23](https://github.com/hexuria/pua/pull/23) | Removed `packs/` (all five) and `crates/pua-jev`. Removed `pua_lexicon::ocr`, the `jev_reply_parse` fuzz target, `docs/eval/autosteer.md` and the autosteer goldens. Cleaned the workspace members, CODEOWNERS, `scripts/architecture.txt`, `scripts/mutants.sh` and repo rules. The `autosteer_ask` bench became `classifier_decide`, and `gateway_shape_mixed` was dropped. Engine docs now use consumer-neutral wording. README and spec were rewritten around "PUA owns *how*". Added ADR 0010. |

### Crates

- **Retained (engine):** `pua-core`, `pua-text`, `pua-lexicon`, `pua-rules`, `pua-hdc` (with
  `resonator`), `pua-graph`, `pua-explain`, plus `benches/pua-benches` and `fuzz/`.
- **Removed:** `pua-steer`, `pua-gateway`, `pua-bir`, `pua-ocr`, `pua-toolbox`, `pua-jev`.
  `pua-toolbox` had no consumer. Its generic parts already live in the engine (`CandidateSet`,
  `overlap`, `label_of`), so it was deleted rather than moved.

### Consumer migrations

| From | To | PR | Verified |
|---|---|---|---|
| `pua-steer` | hexuria/nativechat `crates/autosteer` (`nativechat-autosteer`; `PackError` → `AutosteerError`; `target-selection` dropped) | [#194](https://github.com/hexuria/nativechat/pull/194) | 16 unit, 2 eval/golden, 3 proptests and 1 doctest pass. The golden replays byte-identical and `EVAL.md` regenerates with the same body. clippy, fmt and `cargo deny` pass. |
| `pua-gateway` | hexuria/open-ai-gateway `crates/oag-shape` (depends only on `pua-text`; tag `oag-shape/1`) | [#149](https://github.com/hexuria/open-ai-gateway/pull/149) | 6 unit, 3 proptests and 1 doctest pass. clippy, fmt and deny pass. |
| `pua-bir` + `pua_lexicon::ocr` | hexuria/buwiz-forms `crates/bir-suggest` (`ocr_digits.rs`) | [#67](https://github.com/hexuria/buwiz-forms/pull/67) | The full test suite, clippy and fmt pass. Its CI runs `--workspace`, so it covers the new crates. |
| `pua-ocr` | hexuria/buwiz-forms `crates/bir-cor-extract` | [#67](https://github.com/hexuria/buwiz-forms/pull/67) | Same as the row above. |
| `pua-jev` | hexuria/opengrok-server `crates/opengrok-jev` | [#370](https://github.com/hexuria/opengrok-server/pull/370) | 15 unit, 7 proptests and 1 doctest pass. The wire shapes were re-checked against `routes.rs`. clippy, fmt, `check-architecture.sh` and `crate-size.sh` pass. |

### Deviations from the plan

- **Toolbox was not rewired onto a `RuleClassifier`.** In #22 it moved to `pua-text` tokens with
  `overlap` and `CandidateSet`, keeping identical scores. In #23 it was deleted (B4), because
  no consumer needs it.
- **`opengrok-jev` is a standalone workspace.** opengrok-server pins Rust 1.95 in its toolchain file
  and in `ci.yml`, but PUA's `rust-version` is 1.99. The crate therefore carries its own `[workspace]`
  and a 1.99.0 `rust-toolchain.toml`, and the root `Cargo.toml` excludes it.
- **The fixture keeps domain tag `pua-steer/1`.** This lets the engine journal and NativeChat's
  journal stay byte-identical at the same PUA rev. The tag is fixture data, not a crate name.
- **Handoffs pin a git rev, not a tag.** No PUA tag has been cut yet.
- **ADRs 0008 and 0009 stay in PUA** for history. Their status lines say "moved with the code", and
  each consumer crate carries a copy of its rule.

### Invariants after the split (§9)

| Invariant | Engine enforcement now |
|---|---|
| Determinism | `pua-core/tests/props.rs::decide_is_deterministic`, `pua-rules/tests/classifier.rs::decide_is_deterministic`, golden replay `golden_journal_replays_byte_identically` |
| Candidate order | `pua-core/tests/props.rs::candidate_permutation_is_invariant` (`CandidateSet`) |
| Tie / low margin / low confidence → abstain | `abstain_iff_below_threshold_or_margin`, `ties_rank_the_lower_index_first`, `decide.rs` boundary tests |
| No floats, clocks, env, unordered maps, network | `clippy.toml`, `float_arithmetic` deny (no exception left, now that `pua-jev` is gone), `deny.toml`, `scripts/architecture.txt` |
| Declared invariances | `pua-text`, `pua-rules` and `pua-lexicon` proptests. Each pack's generator-sequence proptests moved with it. |

Test count: 235 at the Phase 1 baseline and 250 after #22. After #23 there are 165 in PUA,
because the pack tests now run in the consumer repos.

### Remaining debt

1. **Workflows still mention packs.** The `bench.yml` and `mutants-diff.yml` path filters list
   `packs/**`, `mutants-diff.yml` diffs `crates packs`, and the nightly unsafe grep scans `packs`. These references are harmless now, because the directory is gone. The
   box token lacks `workflow` scope (ADR 0001), so the cleanup has to be pushed from the Mac.
2. **No PUA tag yet.** Cut `v0.1.0` once #23 is on `main`, then move the consumer pins from
   `rev = "e1635f1"` to the tag. The engine API changed between the pinned rev and #23 (`ocr`
   is gone), but no consumer imports the removed module.
3. **The handoff PRs are drafts awaiting the owners.** Each one adds a crate but does not yet wire
   it into the app (NativeChat `OnSend::Auto`, the gateway request path, the buwiz COR flow, the
   opengrok `JevDoor`).
4. **NativeChat CI does not test the new crate.** CI tests only `-p nativechat`, and adding
   `nativechat-autosteer` needs a workflow edit from the Mac. The lockfile also bumps
   `serde_spanned` and `unicode-segmentation`.
5. **opengrok-server toolchain.** Bumping it to 1.99 would let `opengrok-jev` join the root
   workspace.
6. **buwiz-forms overlap.** The existing `cor_ocr` already has its own longest-exact matching and a
   `FormRecord`. Dedupe it with `bir-cor-extract` when the flow is wired.
7. **HDC is unwired** (owner decision). `pua-hdc` and `resonator` stay as documented capabilities
   until a consumer brings a codebook.
8. **Bench baseline.** `classifier_decide` is a new bench name, so its first bench-gate run has no
   baseline to compare against.
9. **Custom thresholds** stay out of scope. There are three `Profile` presets only.
