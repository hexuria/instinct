# ADR 0009: Jev adapter trusts labels, converts floats once, and labels every fallback

- Status: accepted; **moved with the code** to hexuria/opengrok-server `crates/opengrok-jev`
  (ADR 0010). Instinct no longer has a float boundary at all.
- Date: 2026-10-05
- Task: T9 (`instinct-jev`)

## Context

Spec §8 sends the same `Question` to Jev when Instinct abstains and requires three things: an
answer outside the offered options is an error, Jev's four error kinds stay distinct, and a
fallback is never recorded as an answer. Spec §5 rule 2 allows a float into the decision path
only at this adapter. The wire shapes come from `opengrok-server
crates/opengrok-server/src/jev/routes.rs` (`AskedQuestion`, `AnsweredQuestion`, `noul_reading`,
`rung_of`). Instinct has no HTTP client and must not depend on `typesafe-sdk` (spec §3.1).

## Decision

- **One float boundary.** `confidence_from_unit` is the only float → `Confidence` conversion
  (round half away from zero; NaN/∞ → `NonFinite`, outside `[0, 1]` → `OutOfRange`).
  `clippy::float_arithmetic` stays denied workspace-wide and is allowed only in
  `instinct-jev/src/convert.rs`.
- **Labels, not rung numbers.** A score answer maps to a level by its `level` text, which must
  equal an offered level. The route does not pin whether rungs count from 0 or 1, so the
  number is only checked for finiteness. A missing `level` is refused (off-menu).
- **Off-menu guard everywhere.** A choice label, a probability key or a score level that was
  not offered is `ParseError::OffMenu`. Labels are compared exactly (no case or space folding).
- **No silent zero.** An offered label with no probability, or a `null` one (how the route sends
  NaN), is `MissingProbability`, never 0.
- **Noul drift check.** The noul answer uses the wire `yes` and `confidence`, and they must agree
  with `noul_reading(probabilities.yes)` (`YES_ABOVE = 0.5`, inclusive, as in the route). If the
  route's reading ever changes, replies are refused as `InconsistentNoul` instead of drifting
  silently.
- **Four kinds + labelled fallback.** `JevError::{Asked, Unreachable, TimedOut, Refused}`
  mirrors the route's error type (the timeout is held in whole milliseconds). `escalate` returns
  `Escalation::Answered` only for an on-menu reply that no pack veto rejected. Every other
  outcome is `Escalation::Fallback { why, answer }`, with `FallbackWhy::{Jev, Refused, Vetoed}`
  and a trail record `fallback: <why>`.

## Consequences

- Consumers convert their own transport results into `Result<&str, JevError>`. Each keeps its
  own SDK version, so there is no SDK version skew inside Instinct.
- A Jev deployment that returns scores with no legend cannot be used with score questions until
  it sends `level`. That is the safe failure.
- Gap: the shapes mirror one snapshot of `routes.rs`. Re-check that file before a consumer wires
  this adapter (plan T9, review report "Deferred").
