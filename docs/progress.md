# Progress

Live status of the build-out in docs/plan.md. Updated with every PR so work can resume.

## Stack (stacked PRs, merged bottom-up with squash; `main` stays linear)

Each PR branches off the previous PR's branch and targets it. After a lower PR merges, the next PR
is retargeted to `main` (via `gh api -X PATCH .../pulls/N -f base=main`, since `gh pr edit` trips
over the classic-projects deprecation) and rebased with `git rebase --onto origin/main <old-base>`.

| # | PR | Branch | Base | Scope (plan task) | Status |
|---|---|---|---|---|---|
| 0 | [#1](https://github.com/hexuria/pua/pull/1) | `ci/foundation` | `main` | T0 CI foundation | merged (d3a9e32) |
| 1 | [#2](https://github.com/hexuria/pua/pull/2) | `plan` | `main` | T1 plan + review, ADR 0001 | merged (6c08249) |
| 2 | [#3](https://github.com/hexuria/pua/pull/3) | `core` | `main` | T2 pua-core, ADRs 0002-0003 | merged (f7ddbf5) |
| 3 | [#4](https://github.com/hexuria/pua/pull/4) | `explain` | `main` | T3 pua-explain (replay) | merged (e406cd5) |
| 4 | [#5](https://github.com/hexuria/pua/pull/5) | `text` | `main` | T4 pua-text, ADR 0006, fuzz crate + `normalize` target | merged (53809a7) |
| 5 | [#6](https://github.com/hexuria/pua/pull/6) | `lexicon` | `main` | T5 pua-lexicon, fuzz `lexicon_lookup` | merged (bad3ace) |
| 6 | [#7](https://github.com/hexuria/pua/pull/7) | `rules` | `main` | T6 pua-rules, ADR 0005, fuzz `rules_match` | merged (71f056f) |
| 7 | [#8](https://github.com/hexuria/pua/pull/8) | `hdc` | `main` | T7 pua-hdc, docs/hdc-capacity.md | merged (7316e2e) |
| 8 | [#9](https://github.com/hexuria/pua/pull/9) | `graph` | `main` | T8 pua-graph (1-WL, spd-wl), fuzz `graph_from_bytes` | merged (2e9a0f4) |
| 9 | [#10](https://github.com/hexuria/pua/pull/10) | `jev` | `main` | T9 pua-jev, ADR 0009, fuzz `jev_reply_parse` | merged (3c6fd68) |
| 10 | [#11](https://github.com/hexuria/pua/pull/11) | `autosteer` | `main` | T10 pua-steer, ADR 0008, eval+goldens | merged (761365c) |
| 11 | [#12](https://github.com/hexuria/pua/pull/12) | `fix-autosteer-mutants` | `main` | T10 mutants survivors | merged (5e4732d) |
| 12 | [#13](https://github.com/hexuria/pua/pull/13) | `gateway` | `main` | T11 pua-gateway | merged (ecbe60a) |
| 13 | [#14](https://github.com/hexuria/pua/pull/14) | `bir` | `main` | T12 pua-bir | merged (3316f11) |
| 14 | [#15](https://github.com/hexuria/pua/pull/15) | `ocr-labels` | `main` | T13 pua-ocr | merged (87ec58f) |
| 15 | [#16](https://github.com/hexuria/pua/pull/16) | `tool-selection` | `main` | T14 pua-toolbox | merged (a097137) |
| 16 | [#17](https://github.com/hexuria/pua/pull/17) | `benches` | `main` | T15–T17 benches, fuzz docs, review-report | merged (727ab99) |


## Done

- T0: CI gate, nightly, bench gate, release skeleton, cargo-deny, architecture check, Dependabot,
  CODEOWNERS, AGENTS/CONTRIBUTING, PR template. First CI run green on #1.

- T1: plan with tasks T0-T17, verification-architecture owner table, ADR list, plan review.
- T2: pua-core (scores, spans, DataVersion, questions/answers, decide, candidates, profiles,
  trail, Pack trait). 36 tests incl. proptests; local mutants run, survivors killed by tests.
- T3: pua-explain (replay records with schema + input hash, JSON lines, diff, trail render).
- T4: pua-text (chunked NFC + offset map, protected spans, fold, punctuation runs, sentences,
  confusables). 20 tests incl. differential NFC proptest and move-sequence invariance; 20k-case
  local stress run green. Fuzz target `normalize`: 329,871 runs / 46 s, no crash.

- T5: pua-lexicon (validated vocabulary, longest exact, opt-in substring, SymSpell repair with
  QWERTY/transposition/lexical tie-break, guards, confusable flags, OCR digit table). 25 tests
  incl. 5 proptests (invariance, entry-order independence, SymSpell == brute force); 20k-case
  stress green. Fuzz `lexicon_lookup`: 306,060 runs / 41 s, no crash.

- T6: pua-rules (validated rule sets, token-keyed matching per ADR 0005, specificity
  suppression for object scope, negation window, question damper, repairs with penalty,
  Max/Sum). 21 tests incl. 5 proptests (invariance, protected cues inert, sentence locality,
  spec-order independence, no panics); 20k-case stress green. Fuzz `rules_match`: 307,525 runs
  / 41 s, no crash.

- T7: pua-hdc (typed dimensions, seeded encoder with pinned reference vectors, bind/permute/
  bundle, codebook nearest/cleanup/decode, resonator with `IterationCap` ≤ 16). 21 tests incl.
  6 proptests (spec §6.6 at D1024/2048/4096). Capacity table measured (docs/hdc-capacity.md):
  decode recall 100% up to k = 100 at every D and n ≤ 1024; sign recall drops to 64.8% at
  D1024, n = 1024, k = 200.

- T8: pua-graph (labeled graph arena, 1-WL with blake3 and sorted (edge label, direction,
  colour) tuples, `Rounds::Fixed(3)`/`ToStability`, fingerprint, per-node colours, `spd-wl`).
  9 tests incl. 2 proptests (relabeling + edge-order invariance, per-node equivariance, splits
  only). C₆ vs 2×C₃ and the bridge pair collide under 1-WL and separate under spd-wl. Fuzz
  `graph_from_bytes`: 20,989 runs / 41 s, no crash.

- T9: pua-jev (wire shapes mirroring opengrok-server `jev/routes.rs`, one float boundary in
  `convert.rs`, off-menu guard on labels/probability keys/score levels, noul drift check,
  `JevError` four kinds, `escalate` → `Escalation::{Answered, Fallback}` with `FallbackWhy`,
  ADR 0009). 16 unit tests + doctest + 6 proptests (exact millis round trip, monotone
  conversion, ranked order, menu-reorder equivariance, off-menu refusal, no panics). Mutants:
  46 tested, 0 missed. Fuzz `jev_reply_parse`: 921,757 runs / 41 s, no crash.

- Mutants-in-diff (CI) on #5/#6 flagged survivors: pua-text tests added (0 missed locally,
  4 equivalent excluded); pua-lexicon `repair_hit` guards rewritten so they have no
  equivalent mutants. Stack rebased on the fixes.

- T10: pua-steer (Choice delivery=[queue,steer,interrupt]; data/*.toml; text→lexicon→rules→decide;
  AutoApply::Never on interrupt; confusable control words → Abstain; 221 labelled messages;
  interrupt FP=0 at standard; keyword-baseline comparison in docs/eval/autosteer.md; golden
  journal + replay CLI; generator-sequence proptests). ADR 0008.

- T10: pua-steer merged (#11); mutants survivors killed (#12).

- T11: pua-gateway — integer `ShapeFeatures` only (no tier); §6.2 proptests.
- T12: pua-bir — TIN suggestions never emit "valid"; Ñ-names; sample catalog; WL layout.
- T13: pua-ocr — COR extraction via original spans; three A §5.3 cases; NFC/NFD free move.
- T14: pua-toolbox — Choice/Abstain over tools; call-DAG WL; permutation/case/space props.


- T11–T14 packs merged (#13–#16).
- T15: `benches/pua-benches` gungraun Ir paths + docs/benchmarks.md; bench-gate uses `--bench paths`.
- T16: fuzz targets/seeds/nightly smoke confirmed in docs/verification.md.
- T17: docs/review-report.md.

## Next

- T0–T17 complete on `main`. Pack crates renamed to `pua-steer` / `pua-gateway` / `pua-bir` / `pua-ocr` / `pua-toolbox` (this PR).
- Follow-ups: consumer adoption, Phase B target-selection.


## Known constraints

- The box's GitHub token lacks the `workflow` scope; workflow file changes are pushed from the
  owner's Mac (ADR 0001). Everything else is pushed from the box.

## CI status

- `mutants-in-diff` is advisory (`continue-on-error`). It shows "fail" whenever cargo-mutants
  exits 3, which means **timeouts only**: loop-step mutants that never terminate, which count as
  detected. A PR merges once `ci-ok` is green and the mutants log has no `MISSED` line.
- `docs/progress.md` is kept current at the **top** of the stack; lower branches carry the
  snapshot from when they were written, and main catches up as the stack merges.

- PR gate (`ci.yml`): green on #1 (all 7 jobs + ci-ok). Bench gate and mutants-diff ran and
  skipped cleanly (no benches / no Rust diff yet).
- Stack #5-#9: `ci-ok` green on every PR. A PR merges only when `mutants-in-diff` is green too,
  because a survivor in the diff means a test is missing.
