# `delivery` fixture (example data, synthetic)

A realistic, labelled data set the engine is regression-tested against. It is **example data**:
the meaning of the options (`queue`, `steer`, `interrupt`) belongs to the consumer that ships it
(NativeChat's autosteer module). PUA only checks that `RuleClassifier` decides it deterministically.

| File | What |
|---|---|
| `lexicon.toml` | closed vocabulary (hand-made, synthetic) |
| `rules.toml` | cue rules; classes in option order, class 0 = safe default |
| `eval.jsonl` | 221 hand-labelled synthetic messages |
| `journal.jsonl` | golden replay journal (`pua-explain` schema 1), domain tag `pua-steer/1` |

Regenerate the journal only for an intended behaviour change, and attach the diff to the PR:

```sh
cargo test -p pua-rules --test classifier -- --ignored write_golden_journal
```
