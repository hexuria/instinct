# PUA: Predictable Universal Advisor

PUA is a small, domain-agnostic **decision engine** in pure Rust. Given a question with a closed set
of answers, it scores the options from text and structure and returns an answer with its trail. When
it isn't sure, it returns **Abstain** with every option ranked, instead of guessing. It runs locally
on the CPU in microseconds to milliseconds, with no network, no model and no floats.

**PUA owns the *how*. Consumers own the *what*** ([ADR 0010](docs/adr/0010-engine-consumer-boundary.md)):

- **PUA, the how:**
  - canonicalization with an offset map
  - closed-vocabulary matching with typo repair and a confusable guard
  - data-driven cue rules with negation and scope
  - hypervectors and a resonator
  - Weisfeiler-Leman graph fingerprints
  - integer scores and one threshold-and-margin gate
  - abstention, trails, `DataVersion` and replay
- **Consumers, the what:**
  - the question and what each option means
  - lexicon, rules and codebook data
  - labelled evals and golden journals
  - product rules (for example "an interrupt is never auto-applied")
  - wire formats and any escalation to a heavier model

## "Predictable" means deterministic

Same input + same data (`DataVersion`) + same profile gives a **byte-identical** `Decision`:

- No randomness, clocks, environment reads or unordered-map iteration in the decision path.
- Integer scores only (`Confidence` 0–1000, `Millis` −1000..=1000). The engine contains no floats.
- Hypervector seeds are fixed by data and a version tag. Nothing is sampled.
- Declared invariances (case, spacing, Unicode form, candidate order) are enforced by
  canonicalization and sorting, and tested with exact equality.
- An exact tie or a low margin always abstains.

Confidences are deterministic scores, **not calibrated probabilities**.

## Crates

| Crate | What it does |
|---|---|
| `pua-core` | `Question` / `Answer` (Noul, Choice, Score, Abstain), `Confidence`, `Span`, `Scores` + `decide` (the one gate), `abstain`, `CandidateSet`, `Profile` presets, `Trail`, `Decision`, `DataVersion` |
| `pua-text` | NFC + fold (`Ñ`-safe), offset map to original bytes, protected spans (code, URLs, paths, quotes, numbers), tokens, sentences, confusable flags |
| `pua-lexicon` | closed vocabulary: token-boundary exact and longest match, opt-in substring, SymSpell typo repair with QWERTY tie-breaks; `overlap` for open-text candidates |
| `pua-rules` | token-keyed cue rules with negation window, question damper and object scope; **`RuleClassifier`**, the full text → `Decision` pipeline from a `ClassifierSpec` |
| `pua-hdc` | packed bipolar hypervectors, seeded encoder, bind/bundle/permute, codebook cleanup, bounded resonator (not wired into a pipeline yet) |
| `pua-graph` | labelled graphs, 1-WL (optional SPD-WL) fingerprints with per-node colours, `label_of` |
| `pua-explain` | replay records (JSON lines), replay check, decision diff, trail rendering |

```rust
use pua_rules::{ClassifierSpec, OnConfusable, RuleClassifier};

let classifier = RuleClassifier::new(&ClassifierSpec {
    domain: "my-app-classifier/1".into(),   // folded into DataVersion
    question: "route".into(),               // your word, not PUA's
    normalize: Default::default(),
    lexicon: toml::from_str(LEXICON_TOML)?, // your data
    rules: toml::from_str(RULES_TOML)?,     // classes = options; class 0 is the safe default
    on_confusable: OnConfusable::Abstain,
})?;
let decision = classifier.decide(text, pua_core::Profile::Standard); // answer + trail + DataVersion
```

For choosing among a finite set of ids (tools, records, routes), use `pua_core::CandidateSet` with
`pua_lexicon::overlap` (or your own integer evidence). `CandidateSet::chosen` is `None` on abstain,
so an abstain can never be acted on by mistake.

## Consumers

No consumer depends on PUA in production yet. Handoff PRs (drafts) carry the domain code that used
to live here:

| Consumer | Takes | PR |
|---|---|---|
| NativeChat | delivery advice (queue / steer / interrupt) data, eval, golden, `AutoApply` | hexuria/nativechat#194 |
| open-ai-gateway | request shape features (`oag-shape`) | hexuria/open-ai-gateway#149 |
| buwiz-forms | BIR field suggestions, COR label extraction, OCR digit table | hexuria/buwiz-forms#67 |
| opengrok-server | Jev wire adapter, the float boundary (`opengrok-jev`) | hexuria/opengrok-server#370 |

Consumers pin PUA by **git tag or rev**, never a branch (no tag exists yet):

```toml
pua-rules = { git = "https://github.com/hexuria/pua", rev = "<full sha>", features = ["serde"] }
```

No PUA crate depends on a consumer crate, or on tokio, axum, sqlx, reqwest, hyper or typesafe-sdk
(`scripts/architecture.txt`, checked in CI).

## Inspiration, honestly

PUA's pipeline shape took ideas from public descriptions of BCSC's FMM: high-dimensional
representation, resonance and structural matching. FMM is a **conceptual inspiration, not a
blueprint**. PUA does not reproduce FMM's algorithms, makes no claim of equivalence to it, and is not
a generative model. Concretely, PUA combines:

- symbolic rules (`pua-rules`)
- lexical evidence (`pua-lexicon`)
- graph structure (`pua-graph`)
- hyperdimensional vectors with a resonator (`pua-hdc`; the encoder algorithm is credited to
  hyper-use)
- integer scoring and explicit abstention (`pua-core`)

No crate is named after FMM or HGRA vocabulary.

## Relation to Jev

PUA can sit as a local tier below a heavier decision model such as TypeSafe's Jev. It asks the same
three question shapes, and an abstain carries the ranked options, so a consumer can show them to a
person or escalate the *same* question. The escalation adapter is consumer code (opengrok-server
`opengrok-jev`). PUA is not "System One"; that name belongs to Jev.

## Layout and status

```text
crates/   pua-core  pua-text  pua-lexicon  pua-rules  pua-hdc  pua-graph  pua-explain
benches/  Callgrind Ir gate (docs/benchmarks.md)
fuzz/     cargo-fuzz targets (normalize, lexicon_lookup, rules_match, graph_from_bytes)
docs/     spec.md · adr/ · architecture-audit.md · progress.md
```

- The code is implemented and tested.
- Nothing is tagged or released yet. The first tag follows the consumer handoffs.
- Rust **1.99.0** is pinned in `rust-toolchain.toml`.
- Edition 2024, `unsafe_code` forbidden.
- See [`docs/spec.md`](docs/spec.md) for the contract and [`docs/progress.md`](docs/progress.md) for
  the state of work.

## License

MIT. See [LICENSE](LICENSE).
