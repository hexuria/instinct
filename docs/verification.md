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
| `pua-lexicon` `qwerty_adjacent` | `ra < rb` → `<=` | The `ra == rb` case returned earlier. |
| `pua-rules` `Repairs::none` | → `Default::default()` | `none()` is defined as `Self::default()`. |
| `pua-hdc` `Codebook::is_empty` | → `false` | A built codebook always has an entry (`CodebookError::Empty`). |
| `pua-hdc` `Hv::permute` | `(lo << bs) \| (prev >> (64 - bs))` → `^` | The two shifted halves occupy disjoint bits. |
| `pua-hdc` `best_two` | runner-up `s > x` → `>=` | On equality the runner-up is overwritten with the same value. |

### Survivors killed by new tests

| Crate | Mutants | Test |
|---|---|---|
| pua-core (local run, T2) | thresholds, conversions, accessors | `tests/props.rs::exact_boundaries_and_conversions` |
| pua-text (CI mutants-diff on #5, then local) | `MAX_INPUT_BYTES` arithmetic and the inclusive size gate; collapsed-whitespace offsets; empty-range mapping at segment edges; token/protected-span adjacency; `covered`/`overlaps` boundaries; bare `~/`, `./`, `../` paths | `src/lib.rs::tests::{size_gate_is_inclusive_and_pinned, collapsed_whitespace_maps_to_the_whole_run, empty_ranges_map_to_segment_edges, tokens_touching_a_protected_span_stay_free}`, `src/protect.rs::tests::{spans_before_after_and_touching_a_fence_survive, bare_relative_path_prefixes_are_paths}` |
| pua-lexicon (local run, T5) | `EntryId::get`, `Lexicon::config`, `ConfusableFlag::token`, protected token in a multi-token match, repair length bound, substring tie-breaks | `src/tests.rs::mutants_survivors_pinned`, `substring_tie_breaks` |
| pua-rules (local run, T6) | `RuleSet::config`; negator overlapping the cue (`b + len <= start`); `?` offset check (`from + k`) | `src/tests.rs::mutants_survivors_pinned` |
| pua-hdc (local run, T7) | 19 in `Codebook::decode` (subtraction estimate, tie order, inclusive floor), `best_two` runner-up, `Encoder::version`, resonator fixed-point `&&` | `src/tests.rs::decode_subtracts_the_estimated_contribution`, `decode_ties_and_inclusive_floor`, `nearest_margin_is_top_minus_runner_up`, `resonator_iteration_counts_are_pinned`, `decode_estimate_is_exact_for_integer_weights` (both rounding branches; the earlier tests left residuals that score ±1000 by identity), `resonator_needs_both_estimates_unchanged` (single-entry `A`) |
| pua-graph (local run, T8) | `refine_with` fixed-round counter `+=` → `*=` | `src/tests.rs::rounds_and_stability` (`Fixed(7).rounds() == 7`) |
| pua-jev (local run, T9) | `AskedChoice::label`, `ConvertError` Display | `src/tests.rs::wire_choice_labels_and_convert_texts` |
| pua-steer (CI mutants-diff on #11, then local) | `from_toml` class-order gate; `Input::{as_str,live_runs}` | `src/tests.rs::{class_order_is_pinned,input_accessors}` |
| pua-core (CI mutants-diff on #3) | `is_canonical` `>` → `>=`; `Span::contains` `&&` → `\|\|`; `Span::overlaps` `<` → `<=`; `Trail::is_empty` → `true` | same test (Ranked tie order, span relations); `trail::tests` |

Text mutants run (T4, local, after the tests above): 216 mutants, 0 missed (15 timeouts are
loop-step mutants that never terminate and count as detected).

Lexicon mutants run (T5, local): 199 mutants, 150 caught, 25 unviable, 11 timeouts (loop-step
mutants that never terminate; the test harness kills them, so they count as detected), 13 missed.
7 of the 13 were killed by `tests::mutants_survivors_pinned` and `tests::substring_tie_breaks`;
the other 6 are equivalent. Three of those (`repair_hit`'s work bound `+ 2`, `c.0 > 0` and the
`(c, e) < best` scan) were later removed by rewriting the guards (`saturating_sub`, `!= 0`, `min()`)
after CI mutants-diff on #6 flagged them; the other three are excluded in `.cargo/mutants.toml`.

Rules mutants run (T6, local, after the survivor tests): 137 mutants, 116 caught, 21 unviable,
0 missed.

HDC mutants run (T7, local, after the tests above): 220 mutants, 198 caught, 22 unviable, 0 missed.

Graph mutants run (T8, local, `--all-features`): 51 mutants, 43 caught, 7 unviable, 1 timeout
(`bfs` `==` → `!=` never terminates), 0 missed.

Jev mutants run (T9, local): 46 mutants, 38 caught, 8 unviable, 0 missed.

Autosteer mutants run (T10, local, after CI survivors): 0 missed.


## Fuzz targets (T16)

| Target | Crate under test | Seeds |
|---|---|---|
| `normalize` | pua-text | `fuzz/seeds/normalize/` |
| `lexicon_lookup` | pua-lexicon | `fuzz/seeds/lexicon_lookup/` |
| `rules_match` | pua-rules | `fuzz/seeds/rules_match/` |
| `graph_from_bytes` | pua-graph | `fuzz/seeds/graph_from_bytes/` |
| `jev_reply_parse` | pua-jev | `fuzz/seeds/jev_reply_parse/` |

Nightly: `scripts/fuzz-smoke.sh` (dated `$NIGHTLY`). Crashes land in `fuzz/artifacts/<target>/`.
