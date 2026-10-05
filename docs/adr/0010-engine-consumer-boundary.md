# ADR 0010: PUA owns the HOW, consumers own the WHAT

- Status: accepted
- Date: 2026-10-05
- Source: [architecture audit](../architecture-audit.md) (Phase 1, approved by the owner); Phase 2
  executed it.

## Context

PUA shipped as seven engine crates plus five domain packs (`pua-steer`, `pua-gateway`, `pua-bir`,
`pua-ocr`, `pua-toolbox`) and a Jev wire adapter (`pua-jev`). The audit found four problems:

- The engine crates were already domain-agnostic in code.
- The one reusable pipeline (normalize → lexicon → rules → decide) was trapped in `pua-steer`.
- The candidate-set selector was trapped in `pua-toolbox`.
- Three packs made no decision at all, and no consumer depended on PUA.

Domain data and meaning were living in the wrong repo.

## Decision

- **PUA is a domain-agnostic decision engine.** It owns *how* a decision is made: canonicalization
  with an offset map, closed-vocabulary matching and typo repair, data-driven cue rules,
  hypervectors and the resonator, WL fingerprints, integer scores, the threshold-and-margin gate,
  abstention, trails, `DataVersion` and replay.
- **Consumers own *what* is decided.** That means questions and option meanings, lexicon/rules/codebook
  data, labelled evals, golden journals, product rules such as "interrupt never auto-applies"
  (ADR 0008), wire shapes and the Jev door (ADR 0009). The code lives in the consumer repo and
  depends on `pua-*` crates pinned by tag or `rev`.
- **Seven crates:** `pua-core`, `pua-text`, `pua-lexicon`, `pua-rules`, `pua-hdc`, `pua-graph` and
  `pua-explain`. The dependency edges are exactly those in `scripts/architecture.txt`. No `pua-*`
  crate may name a consumer concept or depend on a consumer crate.
- **Two generic entry points:**
  - `pua_rules::RuleClassifier` (data-driven text classifier: consumer supplies `ClassifierSpec`)
  - `pua_core::CandidateSet` + `pua_lexicon::overlap` (choose among a finite set of ids)
  
  Both go through the single gate in `pua_core::decide`. Every abstain carries all options ranked.
- **No floats anywhere in the engine.** The one f64 → `Confidence` conversion lives with the Jev
  adapter in opengrok-server.
- **No `Pack` trait.** A consumer that wants a trait defines it in its own repo. `Decision` stays in
  core as the journaled unit.
- **Profiles stay three presets** (`fast`, `standard`, `deep`). There is no custom thresholds API
  until a consumer needs one, and then only with `min_margin ≥ 1` so exact ties still abstain.
- **`pua-hdc` stays unwired.** It remains a documented capability until a consumer brings a codebook.
  `Profile` carries no HDC mode.
- **Engine fixtures are synthetic and labelled as such**
  (`crates/pua-rules/tests/fixtures/delivery/`). They are regression data for the engine, not a
  product.

## Consequences

- Handoffs, as draft PRs with code copied at `e1635f1` and engine crates pinned by rev:
  - hexuria/nativechat#194: autosteer
  - hexuria/open-ai-gateway#149: `oag-shape`
  - hexuria/buwiz-forms#67: `bir-suggest`, `bir-cor-extract` and the OCR digit table
  - hexuria/opengrok-server#370: `opengrok-jev`
  
  `pua-toolbox` had no consumer and was replaced by `CandidateSet` + `overlap`.
- A PUA change that alters output changes `DataVersion`. Each consumer regenerates its own golden
  journal when it bumps the pin. PUA's own goldens cover only its fixtures.
- Domain review happens in the consumer repo. PUA's CODEOWNERS covers engine crates and fixtures
  only.
- Re-adding a domain crate to this repo requires superseding this ADR.
