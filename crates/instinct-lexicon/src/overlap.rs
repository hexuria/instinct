//! Token overlap between two normalized texts (an open-vocabulary companion to [`crate::Lexicon`]).

use std::collections::BTreeSet;

use instinct_core::Confidence;
use instinct_text::Normalized;

use crate::LookupError;

/// Shortest token (in chars) that counts toward [`overlap`].
pub const MIN_OVERLAP_CHARS: usize = 3;

/// Share of `candidate`'s distinct words that also occur in `query`, as `hits × 1000 / words`.
///
/// Only free tokens (outside protected spans) of at least [`MIN_OVERLAP_CHARS`] chars count, and
/// a confusable query token never matches (same policy as [`crate::Lexicon::lookup`]). Order of
/// tokens and repeats are irrelevant. A candidate with no countable word scores zero.
///
/// ```
/// use instinct_lexicon::overlap;
/// use instinct_text::{NormalizeConfig, normalize};
///
/// let cfg = NormalizeConfig::default();
/// let q = normalize("please WRITE the file now", cfg)?;
/// let c = normalize("write a file to disk", cfg)?;
/// // candidate words: write, file, disk → 2 of 3 occur in the query.
/// assert_eq!(overlap(&q, &c)?.get(), 666);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// [`LookupError::ConfigMismatch`] when the two texts were normalized with different configs
/// (`lexicon` is the candidate's config, `text` the query's).
pub fn overlap(
    query: &Normalized<'_>,
    candidate: &Normalized<'_>,
) -> Result<Confidence, LookupError> {
    if query.config() != candidate.config() {
        return Err(LookupError::ConfigMismatch {
            lexicon: candidate.config(),
            text: query.config(),
        });
    }
    let words = |n: &Normalized<'_>, allow_confusable: bool| -> BTreeSet<String> {
        n.tokens()
            .iter()
            .filter(|t| t.is_free() && (allow_confusable || t.confusable().is_none()))
            .map(|t| n.token_text(t))
            .filter(|w| w.chars().count() >= MIN_OVERLAP_CHARS)
            .map(str::to_owned)
            .collect()
    };
    let wanted = words(candidate, true);
    if wanted.is_empty() {
        return Ok(Confidence::ZERO);
    }
    let have = words(query, false);
    let hits = wanted.intersection(&have).count();
    let millis = hits.saturating_mul(1000) / wanted.len();
    Ok(Confidence::saturating(i32::try_from(millis).unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use instinct_text::{Fold, NormalizeConfig, normalize};

    fn ov(q: &str, c: &str) -> i16 {
        let cfg = NormalizeConfig::default();
        overlap(&normalize(q, cfg).unwrap(), &normalize(c, cfg).unwrap())
            .unwrap()
            .get()
    }

    #[test]
    fn counts_distinct_long_free_words() {
        assert_eq!(ov("alpha beta", "alpha beta"), 1000);
        assert_eq!(ov("alpha", "alpha alpha beta"), 500);
        assert_eq!(ov("beta alpha", "alpha beta gamma delta"), 500);
        // two-char words never count, on either side.
        assert_eq!(ov("ab cd", "ab cd"), 0);
        assert_eq!(ov("ab", "ab write"), 0);
        assert_eq!(ov("abc", "abc"), 1000);
        assert_eq!(ov("anything", ""), 0);
        assert_eq!(ov("", "alpha"), 0);
    }

    #[test]
    fn protected_and_confusable_tokens_do_not_count() {
        // A URL is protected: its inner words are not free.
        assert_eq!(ov("see https://write.example/file", "write file"), 0);
        // Cyrillic 'а' in "аlpha" makes the query token confusable.
        assert_eq!(ov("\u{430}lpha", "alpha"), 0);
    }

    #[test]
    fn confusable_candidate_words_dilute_the_denominator() {
        // The candidate's confusable words still count as "wanted" (they are real
        // text on the candidate side) but can never match the query's free words,
        // so a lookalike in the candidate lowers the score without blocking it.
        assert_eq!(ov("alpha beta", "alpha \u{430}eta"), 500);
        assert_eq!(ov("alpha beta", "\u{430}lpha \u{430}eta"), 0);
    }

    #[test]
    fn config_mismatch_is_an_error() {
        let a = NormalizeConfig::default();
        let b = NormalizeConfig {
            fold: Fold::AsciiLower,
            ..a
        };
        let r = overlap(&normalize("x", a).unwrap(), &normalize("x", b).unwrap());
        assert_eq!(
            r,
            Err(LookupError::ConfigMismatch {
                lexicon: b,
                text: a
            })
        );
    }
}
