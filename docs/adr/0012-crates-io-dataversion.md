# ADR 0012: crates.io releases hash the crate version into DataVersion

- Status: accepted
- Date: 2026-10-17
- Task: gpui-agent M0 (publish `instinct-core`, `instinct-text`, `instinct-lexicon`)

## Context

`DataVersion` already hashes the crate version alongside the domain tag, data, seeds, profile table
and Unicode version, so a decision is always attributable to an exact build. Until 0.1.0 the only
consumption mode was a git rev pin, where the workspace version stayed `0.0.0` — the rev itself was
the identity. Publishing `instinct-core`, `instinct-text` and `instinct-lexicon` at `0.1.0` makes the
version field real, and a crates.io build of the same tree therefore reports a **different**
`DataVersion` than a git-rev build.

That is intended, not a bug: `DataVersion` answers "which exact artifact decided this", and a
published 0.1.0 artifact *is* a different artifact than `rev <sha>` of the same source. What
consumers must not do is mix the two in one comparison set (for example, a golden journal recorded
under a git-rev dep cannot be replayed byte-identically under `=0.1.0`).

## Decision

Publish the three crates at 0.1.0 and record here that:

1. `DataVersion` differs between crates.io builds and git-rev builds of identical source, because the
   crate version is hashed.
2. Consumers pick **one** consumption mode per dataset (journal, golden, eval). Switching modes means
   re-recording the reference, not expecting equality.
3. Within a mode, the determinism guarantee is unchanged: same input + same data + same profile gives
   a byte-identical `Decision`.

## Alternatives considered

- **Exclude the crate version from `DataVersion`.** Rejected: it would make a published artifact and
  a dirty git checkout indistinguishable, which defeats the point of the hash.
- **Publish every crate.** Rejected for now: rules, hdc, graph and explain still move; only the
  stable types a policy consumer needs are versioned.
