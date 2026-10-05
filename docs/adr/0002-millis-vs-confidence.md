# ADR 0002: separate `Millis` (similarity) and `Confidence` newtypes

Status: accepted (2026-10-05)

## Context

Spec §4.1 sketches one `pub struct Millis(pub i16)` for both similarity (−1000..=1000) and
confidence (0..=1000), with a public field.

## Decision

Two newtypes with private fields and validated constructors: `Millis` (−1000..=1000) and
`Confidence` (0..=1000). `Millis::to_confidence` is the only bridge (negative → 0). Deserialization
goes through the same validation (`serde(try_from)`), so a journal cannot smuggle in 1001.

## Alternatives discarded

- One type with a public field (spec sketch): a negative confidence or a 5000 similarity would be
  expressible; arithmetic mistakes would be silent.
- A `u16` for confidence: mixing signed and unsigned arithmetic invites casts; i16 for both keeps
  conversions explicit and lossless.

## Downsides accepted

- A small deviation from the spec's type names; the spec's intent (integer millis, not a
  probability) is unchanged.
