# SPEC: Instinct (`instinct-*` crates)

Status: **implemented**. Seven engine crates are on `main`, with no tag yet. This spec describes the
domain-agnostic engine after the Phase 2 split ([ADR 0010](adr/0010-engine-consumer-boundary.md),
[architecture audit](architecture-audit.md)).  
Repo: [`hexuria/instinct`](https://github.com/hexuria/instinct). Consumers pin by git tag or rev (§3.2).
Toolchain: Rust **1.99.0** (`rust-toolchain.toml`, owner rule), edition 2024,
`#![forbid(unsafe_code)]` in every crate, workspace lints (`unwrap_used`, `expect_used`,
`float_arithmetic` denied).  
History: the earlier version of this file specified domain packs inside this repo (`git show
677d78c:docs/spec.md`). Its pack sections moved to the consumers (§6).

**Name.** "Predictable" means **deterministic**: same input + same data + same profile →
byte-identical `Decision` (§5). Confidences are integer scores on a 0–1000 scale. They are **not**
calibrated probabilities.

---

## 1. Goal and non-goals

**Goal.** A small family of pure Rust crates that decide closed-world questions about text and
structure. Its properties:

- **predictable:** same input + same data gives byte-identical output; no sampling, clocks or RNG
- **explainable:** every answer carries its stage trail and spans in the original text
- **abstain-first:** below the profile's confidence or margin it says "not sure", with every option
  ranked
- **local:** CPU-only, microseconds to milliseconds, no network, no floats
- **domain-agnostic:** Instinct owns *how* a decision is made; the consumer owns *what* is decided (§2)

**Non-goals.**

- No LLM, GPU, gradient training, sampling, network or async in any Instinct crate.
- No domain data or meaning in this repo. That includes lexicons, rules, labels and codebooks for a
  product, option meanings, auto-apply policy and wire formats. The only data here is labelled
  synthetic **fixtures** for engine regression tests.
- Never overrides exact signals a consumer already has (reply-to, explicit target, a stop on a
  specific run id, an explicit tier). Consumers check those before asking Instinct.
- No learned canonicalization, symmetry discovery, spectral graph features or resistance distance.
- **Not a clone of BCSC's FMM.** Public descriptions of FMM (high-dimensional representation,
  resonance, structural matching) inspired the pipeline shape. FMM is a conceptual inspiration, not a
  blueprint: Instinct does not reproduce FMM's algorithms, makes no claim of equivalence and is not a
  generative model. Instinct's own combination is:
  - symbolic rules (`instinct-rules`)
  - lexical evidence (`instinct-lexicon`)
  - graph structure (`instinct-graph`)
  - hyperdimensional vectors and a resonator (`instinct-hdc`)
  - integer scoring and explicit abstention (`instinct-core`)
- Not hyper-use's `HgraMatcher`. hyper-use is credited as prior art for the HDC encoder and is not a
  dependency. No crate is named `hgra` or after FMM vocabulary.

**Position.** Instinct can serve as a local tier below a heavier decision model (TypeSafe's Jev), asking
the same three question shapes. Instinct is not "System One"; that is Jev's name.

## 2. Boundary: how vs what

| Instinct owns (how) | Consumers own (what) |
|---|---|
| canonicalization, offset map, protected spans, confusable detection (`instinct-text`) | which `NormalizeConfig` (fold, punctuation runs) fits their text |
| closed-vocabulary lookup, typo repair, word overlap (`instinct-lexicon`) | the vocabulary (`LexiconSpec`) and its tags |
| cue rules: negation, question damper, object scope, Max/Sum scorers (`instinct-rules`) | the rules (`RuleSetSpec`): classes, patterns, weights |
| `RuleClassifier`: normalize → lexicon → confusable policy → rules → decide | the question name, the class order (class 0 = safe default), the `DataVersion` domain tag |
| `CandidateSet` + the one decide gate (`instinct-core`) | the candidate ids and their evidence |
| hypervectors, codebook cleanup, resonator (`instinct-hdc`) | codebooks and what a factor means |
| WL fingerprints (`instinct-graph`) | which graph (form layout, call DAG) and its labels |
| `Trail`, `Decision`, `DataVersion`, replay and diff (`instinct-core`, `instinct-explain`) | journals, golden files, labelled evals, regression thresholds |
| three `Profile` presets | which preset a call site uses |
| — | product policy (e.g. "an interrupt is never auto-applied"), UI, wire shapes, escalation to Jev |

A consumer module is typically the following. Everything else is Instinct:

- a `ClassifierSpec` built from its own TOML
- a match on the chosen option label
- its policy type
- a journal of `Decision`s

## 3. Repo layout (`hexuria/instinct`)

```text
Cargo.toml            workspace: members, [workspace.package], [workspace.lints]
rust-toolchain.toml   channel = "1.99.0"
crates/
  instinct-core/      Question/Answer shapes, Millis, Confidence, Span, Scores + decide (one gate),
                 abstain, CandidateSet, Profile presets, Trail, Decision, DataVersion
  instinct-text/      NFC, folds (ASCII | Unicode, Ñ-preserving), offset map, protected spans,
                 token and sentence spans, confusable flags
  instinct-lexicon/   closed vocab: token-boundary exact and longest match (≤ 3 tokens), opt-in
                 substring (≥ 5 chars), SymSpell repair + QWERTY tie-break; overlap()
  instinct-rules/     token-keyed cue rules (ADR 0005); RuleClassifier
  instinct-hdc/       packed bipolar vectors, seeded encoder, bind/bundle/permute, codebook, resonator
  instinct-graph/     LabeledGraph, 1-WL (+ optional SPD-WL) fingerprints, label_of
  instinct-explain/   replay records (JSON lines), replay check, decision diff, trail rendering
crates/instinct-rules/tests/fixtures/delivery/   synthetic labelled fixture (engine regression only)
benches/         Callgrind Ir gate (docs/benchmarks.md)
fuzz/            cargo-fuzz targets
```

Data is parsed when a consumer builds a classifier (`toml` / `serde` behind each crate's `serde`
feature) and validated in the constructor. There is no `build.rs` codegen.

### 3.1 Dependency rules

No `instinct-*` crate depends on a consumer crate or names a consumer concept. Ids cross the boundary as
`&str` / `u64`. `scripts/architecture.txt` encodes the allowed edges and is checked in CI:

| Crate | May depend on |
|---|---|
| `instinct-core` | — |
| `instinct-text` | `instinct-core` |
| `instinct-lexicon` | `instinct-core`, `instinct-text` |
| `instinct-rules` | `instinct-core`, `instinct-text`, `instinct-lexicon` |
| `instinct-hdc` | `instinct-core` |
| `instinct-graph` | `instinct-core` |
| `instinct-explain` | `instinct-core` |

- Banned anywhere in any crate's closure: `tokio`, `axum`, `sqlx`, `reqwest`, `hyper`,
  `typesafe-sdk`, `opengrok-*`.
- External deps are permissive only (`deny.toml`): `unicode-normalization`, `unicode-segmentation`,
  `unicode-security`, `blake3` (pure), `serde`, `serde_json`, `toml`. Dev-only: `proptest`,
  `gungraun`.
- Each crate has a ceiling of 8,000 src lines (`scripts/crate-ceilings.txt`).
- Every crate builds alone with `--no-default-features --locked`. None needs a database or network.

### 3.2 How consumers pin it

Consumers take a **git dependency pinned by tag or `rev`**, never a branch. No tag exists yet. The
handoff PRs pin `rev = "e1635f12908deda99308f03222c4265043e0e749"`.

```toml
instinct-rules = { git = "https://github.com/hexuria/instinct", rev = "<full sha>", features = ["serde"] }
```

- **Tags.** `vMAJOR.MINOR.PATCH` on `main`, with `CHANGELOG.md`. Any change that alters an output
  changes `DataVersion` and needs a CHANGELOG line. A change to public types bumps MAJOR (MINOR while
  0.x).
- **Consumer goldens.** When a consumer bumps its pin, it regenerates its own golden journal and
  reviews the diff in the same PR.
- **Local development.** `[patch."https://github.com/hexuria/instinct"]` with `path = "../instinct/crates/..."`.
  Never commit the patch.
- **Toolchain.** `rust-version = "1.99"`. A consumer on an older toolchain cannot build Instinct. For
  example, opengrok-server pins 1.95, so its `opengrok-jev` crate is its own workspace until it bumps.
- **Two copies.** Every crate in one consumer build must pin the same rev. Otherwise Cargo builds two
  `instinct-core`s and the types don't unify.
- **Consumer gates.** A consumer's `deny.toml` must allow the git source
  `https://github.com/hexuria/instinct`.

## 4. Core pipeline

```text
text ─► instinct-text normalize (NFC, fold, protected spans, offset map, confusables)
     ─► instinct-lexicon lookup (exact / longest / substring / repaired hits; confusable flags)
     ─► instinct-rules score (cues, negation, damper, scope; Max or Sum per class)
     ─► [consumer-supplied evidence: overlap, hdc cleanup, graph fingerprints, via Scores]
     ─► instinct-core decide (profile thresholds, top-2 margin, abstain with ranked options)
     ─► Decision { answer, profile, data_version, trail }  ─► instinct-explain replay / diff
```

`RuleClassifier` runs the first, second, third and fifth stages from one `ClassifierSpec`.
`CandidateSet` lets any consumer run the decide stage over its own evidence.

### 4.1 Core types (`instinct-core`)

- `Millis(i16)`: similarity, −1000..=1000. `Confidence(i16)`: 0..=1000 (ADR 0002). Integers, not
  probabilities.
- `Span { start, end }`: byte offsets into the **original** text.
- `Question`: `Noul`, `Choice` (an **ordered** list of options where option 0 is the safe default),
  `Score` (ordered levels).
- `Answer`: `Noul`, `Choice { option, confidence, ranked }`, `Score`, and `Abstain { why, ranked }`.
  `ranked` always holds every option, confidence-descending then index-ascending.
- `Scores` holds one `Confidence` per option. `set`, `raise_to` (Max) and `add` (Sum) are the
  composition point for evidence.
- `decide(&Scores, Profile) -> Answer` is **the** gate (§4.6). `abstain(&Scores, why)` is for stages
  that refuse early (too-long input, confusable, config mismatch) and still return every option ranked.
- `CandidateSet` (§4.9).
- `Trail` / `TrailRecord` with `StageKind`, optional rule id, `ScorerKind`, span (original
  coordinates), millis and text. It lives in core (ADR 0003).
- `Decision = (Answer, Profile, DataVersion, Trail)`. This is the journaled unit.
- `DataVersion`: a domain-separated blake3 builder over named fields (§5 rule 5).

The spec once listed `Stage` / `CandidateGen` / `Scorer` / `Decider` traits and a `Pack` trait. None
exists. ADR 0003 deferred traits until there is a second implementation, and ADR 0010 dropped `Pack`.
Every call is sync and pure, and allocation is bounded by input length and data size.

### 4.2 Text with an offset map (`instinct-text`)

- **NFC first,** then a fold per `NormalizeConfig`:
  - `Fold::AsciiLower`
  - `Fold::UnicodeLower` (default): `Ñ` stays a letter, so `fold("PEÑA") == "peña"`, never `"pe a"`.
  
  `PunctRuns::{Keep, Collapse}` controls repeated punctuation.
- **Offset map.** Every canonical byte maps back to an original byte range, and every span Instinct  returns is in original coordinates (ADR 0006).
- **Protected spans.** Fenced and inline code, URLs, paths, quoted text, version numbers and decimals
  are never matched or repaired inside. The detector is equivariant: folding case or spacing outside
  a span neither creates nor destroys one.
- **Tokens and sentences** as byte ranges. A sentence break is `.`/`!`/`?` followed by a space, which
  keeps it case-free.
- **Confusables.** Mixed-script and homoglyph tokens are flagged with their ASCII skeleton. They never
  match a term or a rule literal.
- Over-long input (> 16 MiB) is refused, never truncated.

### 4.3 Lexicon (`instinct-lexicon`)

- **Exact** is the longest term over up to 3 adjacent free tokens of one sentence. It is
  token-boundary by construction and never matches by prefix.
- **Substring** is opt-in per entry and refused for terms under 5 chars.
- **Typo repair** is SymSpell-style against single-token terms:
  - distance 1 for terms of ≤ 4 chars, 2 otherwise
  - no repair of tokens under 4 chars, or of guard words
  - ties broken by QWERTY adjacency, then transposition, then lexical order
  - a fixed penalty per edit
- **Confusable flags** report a lookalike of a known term, so the caller can abstain.
- **`overlap(query, candidate)`** is the share of the candidate's distinct free words (≥ 3 chars) that
  occur in the query, as `hits × 1000 / words`. Confusable query tokens never match. Use it as
  open-text evidence for candidates.
- **Symbolic hierarchies** (code families, namespaces) use exact or longest matching, not HDC
  similarity.

### 4.4 Rules and `RuleClassifier` (`instinct-rules`)

- Rules are data (`RuleSetSpec`) made of literal canonical tokens plus `{word}` / `{gerund}` slots.
  There is **no regex** in rule data. Matching is keyed by first token (ADR 0005), not Aho-Corasick.
- Per sentence, the steps run in this order:
  1. Specificity: a match inside a longer match is suppressed, which gives object scope.
  2. Negation window.
  3. Question damper.
  4. Lexicon repairs: a repaired token matches as its term, minus the penalty.
- Each class declares `ScorerKind::Max` (critical-set property) or `Sum` (saturating), and the trail
  records it.
- **`RuleClassifier::new(&ClassifierSpec)`** validates the spec, in this order:
  1. lexicon
  2. rules
  3. question: the rule classes become the options, in declared order
  
  It then stamps `DataVersion = builder(domain)` over lexicon, rules, profile table and normalize
  config. `on_confusable` adds a field only when it is `Ignore`.
- **`decide(text, profile) -> Decision`** runs normalize → lookup → confusable policy
  (`OnConfusable::Abstain` by default) → rules → `instinct_core::decide`, and writes one trail record per
  stage. Every refusal returns `Abstain` with all options ranked.
- **Fixture:** `tests/fixtures/delivery/` (synthetic, labelled) replays a golden journal
  byte-identically, passes an eval false-positive gate and runs a determinism proptest.

### 4.5 HDC (`instinct-hdc`)

- Bipolar vectors are packed one bit per component. `D ∈ {1024, 2048, 4096}` is a type, so mixing
  dimensions does not compile.
- Encoder: an FNV-1a seed over `pua-hv1 ‖ namespace ‖ symbol ‖ version`, expanded by SplitMix64. The
  algorithm is credited to hyper-use and ported with Instinct's own tag.
- Algebra:
  - bind = XOR
  - permute = rotation (a deliberate order-encoding symmetry break)
  - bundle = `i32` sums with zero → +1
  - similarity in `Millis`
- Codebook: item memory sorted by id, with cleanup and iterative top-k decode from the accumulator.
- Resonator: two factors, at most 16 iterations, otherwise `NotConverged` (→
  `AbstainReason::NotConverged`). There is no randomness, and ties go to the lower id.
- Capacity is **measured**, not assumed (`docs/hdc-capacity.md`).
- **Not wired into a pipeline.** A consumer that brings a codebook combines a cleanup hit through
  `Scores::raise_to` and a `StageKind::Hdc` trail record. There is no `Evidence` trait until two
  sources must be combined generically by Instinct itself.

### 4.6 Decide

Profiles are three fixed threshold rows. They are not code paths, and there is no custom-thresholds
API (ADR 0010):

| Profile | min confidence | min top-2 margin |
|---|---|---|
| `fast` | 850 | 200 |
| `standard` (default) | 750 | 150 |
| `deep` | 650 | 100 |

Options are ranked by confidence descending, then index ascending. If the top score is below the
minimum confidence, or the top-2 margin is below the minimum margin, the answer is `Abstain` carrying
the ranked list. Every minimum margin is positive, so **an exact top-2 tie always abstains**. Near a
boundary Instinct returns the tied set instead of flipping. `Profile::table_bytes()` goes into every
`DataVersion`.

### 4.7 Explain (`instinct-explain`)

- `ReplayRecord` = `(schema, input hash, input, decision)`, serialized as one canonical JSON line
  (struct order, no maps, integers only).
- `ReplayRecord::check` re-runs a closure on the stored input and compares byte for byte.
  `diff(before, after)` reports changed answers and a moved `DataVersion`. `render_trail` produces
  text for humans.
- Max-scorer critical set: for `ScorerKind::Max`, edits outside the winning match and its negation
  window cannot change that class's score. Sum scorers make no such claim.

### 4.8 Graph fingerprints (`instinct-graph`)

- `LabeledGraph`: `u64` node labels, directed or undirected edges, optional edge labels.
- `wl_refine(g, Rounds)`:
  `c⁽ᵗ⁺¹⁾(v) = blake3(tag ‖ c⁽ᵗ⁾(v) ‖ sorted (edge label, direction, c⁽ᵗ⁾(u)))`.
  `Rounds` is `Fixed(3)` (default) or `ToStability`. Per-node colours are exposed.
- **Semantics.** A different fingerprint means the structures differ. The same fingerprint means
  **WL-equivalent, not isomorphic**: `C₆` and two triangles collide, as a documented test shows. A
  fingerprint is a grouping key, never proof of identity.
- `spd-wl` feature: shortest-path-distance WL, which closes the cut-edge blind spot. There are no
  spectral or resistance-distance features (they need floats or are not well defined).
- `label_of(parts)` gives a stable `u64` label from length-prefixed key bytes.

### 4.9 Candidate sets (`instinct-core`)

`CandidateSet::new(ids)` sorts ids ascending and rejects duplicates (`CandidateError::Duplicate`).
`question(name)` makes a `Choice` whose option *i* is the *i*-th id, so candidates go through the
same `decide` gate. Methods:

- `index_of(id)` / `id(option)` map between ids and options.
- `chosen(&Answer)` is **`None` on abstain**, so a consumer cannot act on an abstain by accident.

Exact ties rank the lower id first and abstain. Input order never matters (§5 rule 4).

## 5. Determinism and replay contract

1. Same `(input, DataVersion, profile)` gives a byte-identical `Decision`.
2. **No floats anywhere in the engine.** Scores are `i16`, accumulators `i32`.
   `clippy::float_arithmetic` is denied workspace-wide with no exception. The one Jev f64 →
   `Confidence` conversion lives in the consumer (opengrok-server `opengrok-jev`, ADR 0009).
3. No clocks, RNG, environment reads, thread-locals or `HashMap`/`HashSet`. `clippy.toml` disallows
   them, and the code uses `BTreeMap` or sorted `Vec`.
4. **Options vs candidates.**
   - The options of a `Choice` are an ordered list. At an exact tie the lower index ranks first, and
     the answer abstains. Away from ties the chosen index is **equivariant** under relabeling.
   - Candidates (ids in a `CandidateSet`, lexicon hits, codebook entries) are a **set**, sorted by id.
     Input order never matters.
5. `DataVersion` = blake3 over a domain tag plus named fields. It covers everything that can change an
   answer: data fingerprints, normalize config, Unicode version, seed tags, D, WL tag and rounds, and
   the profile table. It goes into every `Decision` and replay record.
   Historical hash and wire domain tags remain frozen; changing them changes `DataVersion` and golden replay bytes.
6. Consumers journal the `Decision` (via `instinct-explain`), not a second copy of their text.

### 5.1 Tests that enforce it

- **Determinism** proptests (core `decide`, `RuleClassifier`), across runs.
- **Candidate permutation invariance** (`CandidateSet` + `decide`).
- **Option relabeling equivariance** when the top-2 margin is non-zero, and abstain at exact ties.
- **Abstain iff** below confidence or margin. Every abstain carries all options.
- **Per-stage invariance:**
  - `normalize(x') == normalize(x)` for each declared move
  - protected spans are equivariant
  - the lexicon hit multiset is invariant
- **Span equivariance:** `fold(x'[span']) == fold(x[span])`.
- **hdc algebra:** bundle order invariance, bind self-inverse, similarity under shared bind/rotate.
- **graph:** relabeling invariance, the documented `C₆` vs 2×`C₃` collision, and `spd-wl` separating a
  cut-edge pair.
- **Unicode fixtures:** `PEÑA`, `Ñiño`, combining vs precomposed `Ñ`, Cyrillic `ѕtop`.
- **Golden replay** of the delivery fixture through `RuleClassifier` (byte-identical) and an eval
  false-positive gate.
- **Mutation testing:** `cargo mutants --in-diff` on every PR (advisory), and every engine crate nightly (`scripts/mutants.sh`).

### 5.2 Invariance contract

- An `Answer` must be **invariant** under the consumer's declared moves (for example case, spacing,
  NFC/NFD, punctuation runs, candidate order). A `Span` is **equivariant**: it moves with the text.
- **Canonicalize first.** Declared invariances are enforced in `instinct-text` (and by sorting, for sets).
  If `normalize(x') == normalize(x)` exactly, every later stage inherits the invariance. No later
  stage adds its own invariance logic.
- **Exact equality.** The path is integer-only, so tests use `==`. Any difference is a bug, not
  rounding.
- **Equivalence relations, not groups.** Most normalizations are not invertible, so the contract is
  "invariant under the equivalence generated by these moves".
- **Declare only what holds.** Inside protected spans nothing is a symmetry. Each consumer lists what
  is explicitly *not* a symmetry for its data.
- Internal choices (fold variant, seed tag, D, WL rounds) reach the output only through
  `DataVersion`.

### 5.3 Budgets

| Path | Bench (`benches/benches/paths.rs`) | Wall-clock target (informational) |
|---|---|---|
| `RuleClassifier::decide`, one message (delivery fixture) | `classifier_decide` | < 2 ms |
| text normalize, 32 KB input | `normalize_32kib` | < 1 ms |
| cleanup over 4,096 entries, D = 1024 | `cleanup_4096` | < 1 ms |
| resonator, \|A\| = \|B\| = 64, ≤ 16 iterations | `resonator_64x64` | < 5 ms |
| `wl_refine` fingerprint, 500 nodes / 2,000 edges, h = 3 | `wl_fingerprint_500` | < 1 ms |

The CI gate is Callgrind instruction count (gungraun): fail on more than **10 % Ir** growth against
the merge base (`docs/benchmarks.md`).

## 6. Former packs (moved to consumers)

Until Phase 2 this section specified five domain packs. Their code, data, tests and invariance
tables now live in the consumer repos:

| § | Was | Now |
|---|---|---|
| 6.1 | `instinct-steer`: delivery advice (queue / steer / interrupt), `AutoApply::Never` on interrupt | hexuria/nativechat `crates/autosteer` (#194) |
| 6.2 | `instinct-gateway`: request shape features | hexuria/open-ai-gateway `crates/oag-shape` (#149) |
| 6.3 | `instinct-bir`: TIN format repair, Ñ-safe names, catalog, layout fingerprint | hexuria/buwiz-forms `crates/bir-suggest` (#67) |
| 6.4 | `instinct-ocr`: COR label extraction | hexuria/buwiz-forms `crates/bir-cor-extract` (#67) |
| 6.5 | `instinct-toolbox`: tool choice | removed; use `CandidateSet` + `overlap` (§4.9) |

The old text is at `git show 677d78c:docs/spec.md`. The engine keeps one synthetic copy of the
delivery data as a regression fixture (§4.4).

### 6.6 hdc invariances

- Bundle is invariant under input order.
- Similarity is invariant under binding both arguments with the same key, and under rotating both by
  the same shift.
- XOR-bind is self-inverse.
- Codebook cleanup is invariant under entry order (sorted by id).

## 7. Integrating Instinct in a consumer

1. Check exact signals first. Instinct is not consulted when the answer is already known.
2. Build the classifier once (`RuleClassifier::new`) from your data. Fail CI if the data doesn't load.
3. Call `decide(text, profile)`. Map the chosen option label to your action. Treat `Abstain` as
   "today's default" or show the ranked options.
4. Journal the `Decision` with `instinct-explain` and keep a golden journal plus a labelled eval in your
   repo.
5. Put product policy in your types (e.g. NativeChat's `AutoApply`), not in Instinct.

## 8. Escalation to a heavier tier (no silent fallback)

Escalation is consumer code. Instinct's part is that an abstain carries the same `Question` and every
option ranked:

- **Same shape both ways.** The consumer sends Instinct's `Question` to Jev and parses the reply back into
  an `Answer` (opengrok-server `opengrok-jev`: `render_question`, `parse_reply`, one float
  conversion).
- **Guard.** An answer outside the offered options is an error. Consumer vetoes still apply.
- **No silent fallback.** Jev's error kinds stay distinct. A fallback is the caller's declared one and
  is recorded in the trail as a fallback (`StageKind::Escalation`), never as an answer.

## 9. Evaluation

- **Engine:**
  - the delivery fixture eval (false-positive gate) and golden replay in `instinct-rules`
  - the HDC capacity table (`docs/hdc-capacity.md`)
  - WL counterexample tests
  
  No product accuracy claim is made here.
- **Consumers** own their labelled sets and thresholds. Examples: NativeChat's interrupt
  false-positive = 0 gate, and buwiz-forms' "never emits valid" and `PEÑA ≠ PENA` tests.

## 10. Status

- Phase 1 (audit) and Phase 2 (split) are done on `main`; see `docs/progress.md`.
- Consumer handoffs are open as draft PRs (§6).
- The first tag will be cut when a consumer adopts a pin.

## 11. Open questions

1. Release: tags + `rev` pins only, or also crates.io at 1.0?
2. Jev input: send Instinct's canonical text (exact invariance, lost cues) or the raw text? This is a
   consumer decision, but it affects replay.
3. When a consumer brings a codebook, should `RuleClassifier` grow an optional HDC stage? The answer
   needs a second evidence source before any trait.
4. Custom thresholds: only when a consumer needs them, validated with `min_margin ≥ 1`.

## 12. Reviewer checklist

- [ ] No `instinct-*` crate depends on a consumer crate or names a consumer concept. No tokio, axum, sqlx,
      reqwest, hyper or typesafe-sdk.
- [ ] `scripts/architecture.txt` and crate ceilings are updated in the same change as the crates.
- [ ] `rust-toolchain.toml` stays on 1.99.0, and `rust-version` stays ≤ 1.99.
- [ ] No floats, clocks, RNG or unordered maps. `DataVersion` is in every output.
- [ ] Every declared invariance is enforced in `instinct-text` or by sorting, and tests use exact `==`.
- [ ] Permutation tests are scoped to candidates. Option relabeling is tested as equivariance away
      from ties.
- [ ] Every returned span indexes the original text. `PEÑA` never becomes `PE A`.
- [ ] No substring cue under 5 chars, and no regex in rule data.
- [ ] Every abstain carries all options ranked. `CandidateSet::chosen` is `None` on abstain.
- [ ] The same WL fingerprint is never treated as identity.
- [ ] Domain data appears only as labelled synthetic fixtures under `tests/fixtures/`.
