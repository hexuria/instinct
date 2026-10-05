# Verification log

How each owner in docs/plan.md "Verification architecture" was exercised, and what's left.
Updated per crate. The final summary is in docs/review-report.md.

## Mutation testing (cargo-mutants)

Run: `scripts/mutants.sh` (whole workspace, nightly) or `cargo mutants -p <crate>` locally. In
CI, `mutants-diff.yml` tests only the mutants in a PR's diff and is advisory: a survivor is
either killed by a new test or listed below as equivalent, with the reason.

### Equivalent mutants (accepted survivors)

| Location | Mutant | Why it is equivalent |
|---|---|---|
| `pua-core/src/score.rs` `Millis::saturating` | `<` → `<=` (lower clamp), `>` → `>=` (upper clamp) | At the boundary value the clamp returns the boundary itself (`-1000`/`1000`), the same value the unclamped branch returns. |
| `pua-core/src/score.rs` `Confidence::saturating` | same two boundary mutants | Same reason at `0`/`1000`. |
| `pua-core/src/answer.rs` `is_canonical` | `w[0].0 < w[1].0` → `<=` | Equal option indices are duplicates, which the duplicate check in the same function rejects anyway. |
| `pua-core/src/version.rs` `DataVersion::from_hex` | `(hi << 4) \| lo` → `^` | `hi << 4` and `lo` (< 16) have disjoint bits, so OR and XOR agree. |
| `pua-text` `normalize` | scan loop `i < s.len()` → `<=` | At `i == len` `s[i..].chars().next()` is `None` and the loop breaks; no protected span starts at `len` (spans are non-empty). |
| `pua-text` `nfc_with_map` | delete the `IsNormalized::Yes` arm | The quick check is only a shortcut: the fallback arm computes NFC and keeps it only if it differs, so an already-NFC chunk still maps char by char. |
| `pua-text` `nfc_with_map` | `i > chunk_start` → `>=` | Equality happens only at `i == 0`, where the flush of the empty chunk returns at once. |
| `pua-text` `covered` | → `false` | A shortcut before the overlap check: an open inside an earlier span either finds a close (the overlap check skips it) or finds none, and then no later open can find one either (every closer is at or after it). |
| `pua-lexicon` `Lexicon::is_empty` | → `false` | A built lexicon always has at least one entry (`NoEntries` is refused). |
| `pua-lexicon` `substring_hit` | `word.len() > term.len()` → `>=` | Equal length plus `contains` means `word == term`, which the exact stage already matched. |
| `pua-lexicon` `repair_hit` | `longest_single + 2` → `* 2` | Only a work bound: for a term of ≤ 2 chars no token of ≥ 4 chars is within distance 1 anyway, and for longer terms `*2` only admits tokens that fail the distance check. |
| `pua-lexicon` `repair_hit` | `c.0 > 0` → `>=` | Cost 0 means an exact single-token term, which the exact stage already matched. |
| `pua-lexicon` `repair_hit` | `(c, e) < best` → `<=` | Candidates are distinct ids visited once each, so the pair is never equal to `best`. |
| `pua-lexicon` `qwerty_adjacent` | `ra < rb` → `<=` | The `ra == rb` case returned earlier. |

### Survivors killed by new tests

| Crate | Mutants | Test |
|---|---|---|
| pua-core (local run, T2) | thresholds, conversions, accessors | `tests/props.rs::exact_boundaries_and_conversions` |
| pua-text (CI mutants-diff on #5, then local) | `MAX_INPUT_BYTES` arithmetic and the inclusive size gate; collapsed-whitespace offsets; empty-range mapping at segment edges; token/protected-span adjacency; `covered`/`overlaps` boundaries; bare `~/`, `./`, `../` paths | `src/lib.rs::tests::{size_gate_is_inclusive_and_pinned, collapsed_whitespace_maps_to_the_whole_run, empty_ranges_map_to_segment_edges, tokens_touching_a_protected_span_stay_free}`, `src/protect.rs::tests::{spans_before_after_and_touching_a_fence_survive, bare_relative_path_prefixes_are_paths}` |
| pua-lexicon (local run, T5) | `EntryId::get`, `Lexicon::config`, `ConfusableFlag::token`, protected token in a multi-token match, repair length bound, substring tie-breaks | `src/tests.rs::mutants_survivors_pinned`, `substring_tie_breaks` |
| pua-core (CI mutants-diff on #3) | `is_canonical` `>` → `>=`; `Span::contains` `&&` → `\|\|`; `Span::overlaps` `<` → `<=`; `Trail::is_empty` → `true` | same test (Ranked tie order, span relations); `trail::tests` |

Text mutants run (T4, local, after the tests above): 216 mutants, 0 missed (15 timeouts are
loop-step mutants that never terminate and count as detected).

Lexicon mutants run (T5, local): 199 mutants, 150 caught, 25 unviable, 11 timeouts (loop-step
mutants that never terminate; the test harness kills them, so they count as detected), 13 missed.
7 of the 13 were killed by `tests::mutants_survivors_pinned` and `tests::substring_tie_breaks`;
the other 6 are equivalent (table above).
