# Changelog

All notable changes to `hexuria/pua`. Tags are `vMAJOR.MINOR.PATCH` on `main` (spec §3.2). A change
to rule, lexicon or codebook data that alters output bumps MINOR and changes `DataVersion`; a change
to public types bumps MAJOR (MINOR while 0.x).

## [Unreleased]

### Added
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
