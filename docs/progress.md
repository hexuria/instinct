# Progress

Live status of the build-out in docs/plan.md. Updated with every PR so work can resume.

## Stack (stacked PRs, merged bottom-up with squash; `main` stays linear)

Each PR branches off the previous PR's branch and targets it. After a lower PR merges, the next PR
is retargeted to `main` (via `gh api -X PATCH .../pulls/N -f base=main`, since `gh pr edit` trips
over the classic-projects deprecation) and rebased with `git rebase --onto origin/main <old-base>`.

| # | PR | Branch | Base | Scope (plan task) | Status |
|---|---|---|---|---|---|
| 0 | [#1](https://github.com/hexuria/pua/pull/1) | `ci/foundation` | `main` | T0 CI foundation | merged (d3a9e32) |
| 1 | [#2](https://github.com/hexuria/pua/pull/2) | `plan` | `main` | T1 plan + review, ADR 0001 | open |
| 2 | #3 | `core` | `plan` | T2 pua-core | open |

## Done

- T0: CI gate, nightly, bench gate, release skeleton, cargo-deny, architecture check, Dependabot,
  CODEOWNERS, AGENTS/CONTRIBUTING, PR template. First CI run green on #1.

## Next

- T1 merge, T2 pua-core, then T3 pua-explain and T4 pua-text.

## Known constraints

- The box's GitHub token lacks the `workflow` scope; workflow file changes are pushed from the
  owner's Mac (ADR 0001). Everything else is pushed from the box.

## CI status

- PR gate (`ci.yml`): green on #1 (all 7 jobs + ci-ok). Bench gate and mutants-diff ran and
  skipped cleanly (no benches / no Rust diff yet).
