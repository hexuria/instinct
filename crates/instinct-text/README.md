# instinct-text

Canonicalization for [Instinct](https://github.com/hexuria/instinct), a
small, domain-agnostic **decision engine** in pure Rust: NFC
normalization, Ñ-preserving case folds, protected spans (code, URLs,
paths, quotes, numbers), tokens, sentences, confusable flags — each with
an offset map back to the original bytes.

Deterministic and allocation-bounded: same input gives the same tokens
and spans, so decisions built on them stay byte-identical.

```rust
use instinct_text::{normalize, NormalizeConfig};

let folded = normalize("  Ñandú  ", NormalizeConfig::default())?;
// `folded` keeps an offset map into the original byte string.
# Ok::<(), instinct_text::TextError>(())
```

Rust 1.99+, edition 2024, `#![forbid(unsafe_code)]`. See
[docs/spec.md](https://github.com/hexuria/instinct/blob/main/docs/spec.md)
for the contract.
