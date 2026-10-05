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
| 3 | [#4](https://github.com/hexuria/pua/pull/4) | `explain` | `main` | T3 pua-explain (replay) | open, CI green |
| 4 | [#5](https://github.com/hexuria/pua/pull/5) | `text` | `explain` | T4 pua-text, ADR 0006, fuzz crate + `normalize` target | open |
| 5 | [#6](https://github.com/hexuria/pua/pull/6) | `lexicon` | `text` | T5 pua-lexicon, fuzz `lexicon_lookup` | open |
| 6 | [#7](https://github.com/hexuria/pua/pull/7) | `rules` | `lexicon` | T6 pua-rules, ADR 0005, fuzz `rules_match` | open |
| 7 | #8 | `hdc` | `rules` | T7 pua-hdc, docs/hdc-capacity.md | open |

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

## Next

- T8 pua-graph, T9 pua-jev, then packs T10-T14.

## Known constraints

- The box's GitHub token lacks the `workflow` scope; workflow file changes are pushed from the
  owner's Mac (ADR 0001). Everything else is pushed from the box.

## CI status

- PR gate (`ci.yml`): green on #1 (all 7 jobs + ci-ok). Bench gate and mutants-diff ran and
  skipped cleanly (no benches / no Rust diff yet).
