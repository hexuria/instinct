# ADR 0005: Rules match on tokens, not with a byte-level Aho-Corasick automaton

- Status: accepted
- Date: 2026-10-05
- Task: T6 (`instinct-rules`)

## Context

Spec §4.4 says rule data is "compiled at build time into Aho-Corasick automata + small
matchers". The automaton finds literal byte strings anywhere in a text. Every Instinct cue has to
match on **token boundaries** (the "tin matched setting" lesson, `intent.rs:112-115`), skip
protected spans, stay inside one sentence, and support `{word}`/`{gerund}` slots. A byte
automaton would need all of those checks re-applied after every hit, and the boundary check is
exactly where the old intent code went wrong.

## Decision

- `RuleSet::new` compiles each pattern into a sequence of items (literal canonical token,
  `{word}`, `{gerund}`). It indexes rules by their **first literal token** in a sorted map
  (`BTreeMap`, deterministic iteration).
- `RuleSet::score` walks the token list once. For each free, non-confusable token it looks up
  the rules keyed by that token's text (or its lexicon repair) and verifies the remaining items
  in place. Token-boundary semantics hold by construction: a token either equals a literal or
  it doesn't.
- Work per token is bounded by the rules sharing that first literal, times the pattern length
  (≤ 6). Specificity suppression only looks at matches starting ≤ 6 tokens earlier, so total
  work is linear in the token count for a fixed rule set.
- No `aho-corasick` dependency.

## Consequences

- Patterns are token sequences only. A rule can't match part of a word. Opt-in substring
  matching belongs to the lexicon (≥ 5 chars, spec §4.3), not to rules.
- Regex syntax can't be written: a literal must be one canonical word token, so `sto+p` or
  `st(o|0)p` is refused with `RulesError::NonCanonicalLiteral` when the data loads.
- Adding a cue is still a data change (TOML), with no code change, as spec §4.4 wants.
- If rule sets ever grow to thousands of literals per first token, revisit with a measured
  benchmark (T15 bench gate). Today's packs have tens of rules.
