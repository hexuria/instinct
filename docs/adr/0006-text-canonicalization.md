# ADR 0006: Text canonicalization: chunked NFC, span granularity, fold, sentences

- Status: accepted
- Date: 2026-10-05
- Task: T4 (`pua-text`)

## Context

Every text pack scores the canonical form of untrusted input and has to report evidence as
byte spans of the *original* text (spec §5). Whole-string NFC loses the mapping from canonical
bytes back to original bytes. Case folding, punctuation-run collapse and sentence splitting
each pick which differences count as the "same" message, and that choice is what the invariance
proptests check.

## Decision

1. **Chunked NFC with an offset map.** The input is split before every char whose first NFD
   char has canonical combining class 0 and `NFC_QC=Yes`. NFC cannot compose across such a
   boundary, so normalizing each chunk on its own gives the same result as whole-string NFC.
   A differential proptest (`nfc::tests`) checks this against `unicode-normalization`'s
   `nfc()` on adversarial strings full of combining marks.
2. **Span granularity.** Chunks NFC leaves unchanged map back char by char. Chunks NFC
   rewrites (for example `N` + U+0303 → `Ñ`, or a Hangul jamo sequence) map back as one unit,
   so an evidence span that touches part of a rewritten sequence widens to the whole
   combining sequence in the original. Spans never split a UTF-8 code point, and
   `Span::slice(original)` always succeeds (proptest plus fuzz target `normalize`).
3. **Fold.** `Fold::AsciiLower` lowercases ASCII only and declares *ASCII-case* invariance
   only. `Fold::UnicodeLower` uses `char::to_lowercase` (a simple lowercase mapping, not full
   case folding: `ß` stays `ß` and Turkish dotted/dotless I is not special-cased). Packs that
   need diacritic-sensitive labels (bir-fields, ocr-labels) never strip marks: `PEÑA` ≠
   `PENA`. Protected spans (code, quotes, URLs, paths, numbers) keep their exact bytes after
   NFC and are never folded.
4. **Case-free sentence splitter.** A new sentence starts after `.`, `!` or `?` followed by
   whitespace. The split never looks at case, so case toggling cannot move a sentence
   boundary. That keeps the rules engine's same-sentence negation window inside the case
   symmetry. Abbreviations ("e.g. stop") can over-split. That costs recall only, never a
   false cue, because the negation window just shrinks.
5. **Confusables.** Tokens that mix scripts, or whose ASCII skeleton differs from the token,
   carry `Confusable` and their skeleton. Lexicon and rules never let a skeleton hit fire a
   cue (spec §5.3). The skeleton is exposed for explanation only.
6. **Size cap.** Input over `MAX_INPUT_BYTES` (16 MiB) returns `TextError::TooLong`. The
   canonicalization never panics or truncates silently.

The algorithm identity (`ALGORITHM_TAG`, which includes the Unicode-crate versions; a test
keeps it in sync with `Cargo.lock`) and the config fingerprint feed `DataVersion`. Any
Unicode-data upgrade therefore changes every pack's data version and shows up in replay.

## Consequences

- Evidence spans in decomposed input can be wider than the matched cue (a whole combining
  sequence). Documented in rustdoc on `Normalized::to_original`.
- A dependency bump of `unicode-normalization`/`unicode-segmentation`/`unicode-security`
  is a data-version change, not a refactor, and needs a CHANGELOG entry.
- The proptest symmetries (case toggle, whitespace insertion, NFD/NFC, punctuation
  duplication under `Collapse`) are the executable form of this ADR.
