# instinct-core

Core types for [Instinct](https://github.com/hexuria/instinct), a small,
domain-agnostic **decision engine** in pure Rust: `Question` / `Answer`
(Noul, Choice, Score, Abstain), `Confidence` (integer millis, 0–1000),
`Span`, `Scores`, `CandidateSet`, `Profile` presets, `decide` (the one
threshold-and-margin gate), optional drive `arbitrate`, `Trail`,
`Decision` and `DataVersion`.

Same input + same data (`DataVersion`) + same profile gives a
byte-identical `Decision`: no randomness, clocks, environment reads or
unordered-map iteration in the decision path, and no floats. An exact tie
or a low margin always abstains — `CandidateSet::chosen` is `None` on
abstain, so an abstain can never be acted on by mistake.

```rust
use instinct_core::{CandidateId, CandidateSet};

let set = CandidateSet::new([
    CandidateId::new("save")?,
    CandidateId::new("discard")?,
])?;
let question = set.question("operation")?;
// Evidence scores come from the consumer (e.g. instinct-lexicon::overlap);
// `CandidateSet::chosen` is None when the gate abstains.
# Ok::<(), Box<dyn std::error::Error>>(())
```

Optional feature `serde` serializes the core types (validated on the way
in). Rust 1.99+, edition 2024, `#![forbid(unsafe_code)]`. See
[docs/spec.md](https://github.com/hexuria/instinct/blob/main/docs/spec.md)
for the contract.
