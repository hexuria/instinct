# ADR 0003: trail types in `pua-core`; spec's Stage/CandidateGen/Scorer traits deferred

Status: accepted (2026-10-05)

## Context

Spec §3 places the trail in `pua-explain`, but §4.1's `Pack::ask` (in `pua-core`) returns
"Answer + Trail", and §3.1 makes `pua-explain` depend on `pua-core`. A type cannot live downstream of
the trait that returns it. §4.1 also sketches `Stage`, `CandidateGen`, `Scorer` and `Decider` traits.

## Decision

- `Trail`, `TrailRecord`, `StageKind`, `ScorerKind` and `Decision` live in `pua-core`.
  `pua-explain` owns replay records, canonical serialization, diffs and rendering.
- `Pack` is kept (every pack implements it). `decide` is a function, not a `Decider` trait object.
- `Stage`, `CandidateGen`, `Scorer` are **not** added yet. Each stage crate exposes concrete
  functions; packs compose them. A trait is added when a second implementation of the same seam
  exists (e.g. an external pre-pass, delivery-advisor spec §4.1 `PrePass`).

## Alternatives discarded

- Trail in `pua-explain` with `pua-core` depending on it: inverts the spec's dependency direction.
- Implementing all sketched traits now: speculative public surface (skill §7, "public surface area
  is a liability"); a trait-object pipeline would also hide the stage order that the trail records.

## Downsides accepted

- Packs wire stages by hand (a few lines each). If more packs appear, revisit.
