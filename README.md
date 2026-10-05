# PUA: Predictable Universal Advisor

PUA is a family of small, pure Rust crates that make closed-world text and structure decisions: queue,
steer or interrupt; which label; which field fix; which tool. They run locally on the CPU in
microseconds to milliseconds, with no network and no model. Every answer comes with an explanation,
and when PUA isn't sure it says so (**Abstain**) instead of guessing.

Each product uses PUA through a thin **pack** (data + adapter) on top of shared core crates.

## "Predictable" means deterministic

**Same input, same scores, no sampling.** Given the same input, the same pack data (`DataVersion`)
and the same profile, PUA returns a byte-identical decision every time:

- no randomness, clocks, environment reads or unordered-map iteration in the decision path
- integer scores (`Millis`, 0–1000), no floats
- hypervector seeds are fixed by pack data and a version tag; nothing is sampled at run time
- declared invariances (case, spacing, Unicode form, candidate order) are enforced by canonicalization
  and tested with exact equality

PUA confidences are deterministic scores, **not calibrated probabilities**.

## Relation to Jev

PUA is **tier 0, below Jev**. Jev (TypeSafe's "System One" decision model) remains the heavier tier
above it.

| Tier | What | Latency |
|---|---|---|
| 0 | **PUA**: local, deterministic, explainable, abstains first | µs–ms |
| 1 | **Jev**: network decision model | ~70–500 ms |
| 2 | Frontier LLM via open-ai-gateway | seconds |

PUA asks the same question shapes as Jev (Noul / Choice / Score). When PUA abstains, a consumer can
show the ranked options to a person or escalate the *same* question to Jev. A Jev answer outside the
offered options is refused, and there is no silent fallback. PUA is not "System One"; that name
belongs to Jev.

## Consumers

| Consumer | Use | Pack |
|---|---|---|
| **NativeChat** (first) | `OnSend::Auto`: suggest queue / steer / interrupt while a turn is running; interrupt is never automatic; off by default | `pua-steer` |
| **opengrok-server** (optional, flagged off) | advise route, queued-message annotation, workflow `match` step, reviewer pre-filter before Jev | `pua-steer`, core crates |
| **open-ai-gateway** | advisory integer shape features only; never maps wording to a tier | `pua-gateway` |
| **buwiz-forms** | COR OCR label extraction with an offset map (fixes the label-offset bug), Ñ-safe folding, field suggestions; never claims TIN validity | `pua-ocr`, `pua-bir` |

A `pua-toolbox` pack is also specified; its first consumer is still open.

Consumers pin PUA by **git tag or rev**, never a branch:

```toml
pua-steer = { git = "https://github.com/hexuria/pua", tag = "v0.1.0" }
```

No PUA crate depends on an opengrok crate, or on tokio, axum, sqlx, reqwest, hyper or typesafe-sdk.

## Layout

```text
crates/  pua-core  pua-text  pua-lexicon  pua-rules  pua-hdc  pua-graph  pua-explain  pua-jev
packs/   pua-steer  pua-gateway  pua-bir  pua-ocr
         pua-toolbox
docs/    spec.md
```

Rust **1.99.0** is pinned in `rust-toolchain.toml` (owner rule). Edition 2024, `unsafe_code` forbidden.

## Status

**Spec only.** The design is in [`docs/spec.md`](docs/spec.md) and is up for review. Every crate is an
empty stub, and nothing is tagged or released yet. Code lands in phases (spec §10), starting with the
core crates and an offline autosteer pack.

## License

MIT. See [LICENSE](LICENSE).
