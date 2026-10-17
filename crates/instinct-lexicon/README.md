# instinct-lexicon

Closed-vocabulary matching for
[Instinct](https://github.com/hexuria/instinct), a small, domain-agnostic
**decision engine** in pure Rust: token-boundary exact and longest match,
opt-in substring, SymSpell typo repair with QWERTY tie-breaks, and
`overlap` — integer evidence for open-text candidates.

Deterministic: same lexicon data + same input gives the same matches and
scores. Data is consumer-owned (`LexiconSpec`, optional `serde` feature
for TOML/JSON specs); the crate ships no vocabulary of its own.

```rust
use instinct_lexicon::overlap;
use instinct_text::{normalize, NormalizeConfig};

let cfg = NormalizeConfig::default();
let q = normalize("please WRITE the file now", cfg)?;
let c = normalize("write a file to disk", cfg)?;
// candidate words: write, file, disk → 2 of 3 occur in the query.
assert_eq!(overlap(&q, &c)?.get(), 666);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Rust 1.99+, edition 2024, `#![forbid(unsafe_code)]`. See
[docs/spec.md](https://github.com/hexuria/instinct/blob/main/docs/spec.md)
for the contract.
