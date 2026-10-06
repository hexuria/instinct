# Changelog

All notable changes to `hexuria/instinct` (named `hexuria/pua` before the rename). Tags are `vMAJOR.MINOR.PATCH` on `main` (spec §3.2). A change
to rule, lexicon or codebook data that alters output bumps MINOR and changes `DataVersion`; a change
to public types bumps MAJOR (MINOR while 0.x).

## [Unreleased]

### Changed
- Renamed PUA → Instinct: crates `pua-*` → `instinct-*`, repo hexuria/pua → hexuria/instinct. Hash/wire domain tags (`pua-data-version-v1`, `pua-hv1`, …) are frozen and unchanged, so DataVersions and golden journals are byte-identical.

### Added
- Instinct arbitration in `instinct-core`: consumer-owned drives and affinities, max-dominance urge,
  thresholded incumbent persistence, and the existing single decide gate (ADR 0011).
- CI foundation: PR gate, nightly tier, mutants-diff, bench gate, release skeleton, cargo-deny,
  architecture check, Dependabot, CODEOWNERS.
- `pua-core`: bounded scores, spans, `DataVersion`, questions/answers, `decide`, candidates,
  profiles, trail, `Pack` trait.
- `pua-explain`: replay records (schema 1), JSON lines, decision diff, trail rendering.
- `pua-text`: canonicalization with original-span mapping (ADR 0006); cargo-fuzz crate.
- `pua-lexicon`: closed-vocabulary matching with typo repair and the OCR digit table.
- `pua-rules`: data-driven cue rules with negation, question damper and object scope (ADR 0005).
- `pua-hdc`: packed hypervectors, seeded encoder, codebook decode, resonator; measured capacity table.
- `pua-graph`: 1-WL fingerprints with per-node colours; optional `spd-wl`.
- `pua-jev`: Jev wire shapes, one float boundary, off-menu guard, labelled escalation fallback.
- `pua-steer`: delivery advice (queue/steer/interrupt) with typed AutoApply::Never.
- `pua-rules::RuleClassifier`: the normalize → lexicon → rules → decide pipeline as a
  data-driven engine type (was private to `pua-steer`).
- `pua-core::CandidateSet` (sorted, unique candidate ids as a `Choice`) and `pua_core::abstain`.
- `pua-lexicon::overlap` (word-overlap score between two normalized texts) and
  `pua-graph::label_of` (stable `u64` labels from key bytes).
- ADR 0010: engine / consumer boundary (PUA owns *how*, consumers own *what*).

### Changed
- Every `Answer::Abstain` from `pua-rules::RuleClassifier` carries all options (unscored ones in
  index order) instead of an empty list; the confusable trail line reads
  "abstain: confusable token matched a guarded term".
- `Profile::table_bytes` no longer encodes an HDC mode, so every `DataVersion` changes.
- `pua-toolbox::chosen_id` returns `None` on abstain (it used to return the top-ranked id).

### Removed
- `pua_core::{rank_candidates, RankedCandidates, CandidatePick}` (use `CandidateSet` + `decide`),
  `HdcMode` / `Thresholds::hdc` (never read), and the `Pack` trait (no generic consumer).
- Domain packs and `pua-jev` moved to their consumers (ADR 0010; they depend on PUA by git rev):
  `pua-steer` → hexuria/nativechat `crates/autosteer`
  ([#194](https://github.com/hexuria/nativechat/pull/194));
  `pua-gateway` → hexuria/open-ai-gateway `crates/oag-shape`
  ([#149](https://github.com/hexuria/open-ai-gateway/pull/149));
  `pua-bir` + `pua-ocr` + the OCR digit table (`pua_lexicon::ocr`) → hexuria/buwiz-forms
  `crates/bir-suggest` / `crates/bir-cor-extract`
  ([#67](https://github.com/hexuria/buwiz-forms/pull/67));
  `pua-jev` → hexuria/opengrok-server `crates/opengrok-jev`
  ([#370](https://github.com/hexuria/opengrok-server/pull/370));
  `pua-toolbox` deleted (no consumer; the generic parts are `CandidateSet`, `overlap`, `label_of`).
  The `jev_reply_parse` fuzz target, `docs/eval/autosteer.md`, the autosteer goldens and the
  `gateway_shape_mixed` bench went with them; `autosteer_ask` became `classifier_decide`.
