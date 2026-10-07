//! NFC with an offset map, by chunking at stable starters (ADR 0006).
//!
//! A boundary is placed before every character with canonical combining class 0 and
//! `NFC_Quick_Check = Yes`: such a character never composes with what precedes it and is never
//! reordered, so normalizing chunks independently equals normalizing the whole string. This is
//! differential-tested against whole-string NFC (`tests/props.rs`).

use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

/// `nfc[out_start..next.out_start)` came from `orig[src_start..src_end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seg {
    pub(crate) out_start: u32,
    pub(crate) src_start: u32,
    pub(crate) src_end: u32,
    /// The output is byte-identical to the source range (a verbatim run), so positions inside
    /// map linearly instead of snapping to the segment's boundaries.
    pub(crate) linear: bool,
}

pub(crate) struct NfcOut {
    pub(crate) text: String,
    pub(crate) segs: Vec<Seg>,
}

/// A boundary before `c` is safe when the first character of `NFD(c)` is a starter (ccc 0) with
/// `NFC_Quick_Check = Yes`: nothing before it can compose with it or reorder across it.
/// (Using `c` itself would miss composition exclusions such as U+FB43, whose NFC is two chars.)
fn is_stable_starter(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    let mut first = None;
    unicode_normalization::char::decompose_canonical(c, |d| {
        if first.is_none() {
            first = Some(d);
        }
    });
    first.is_some_and(|f| {
        canonical_combining_class(f) == 0 && is_nfc_quick(core::iter::once(f)) == IsNormalized::Yes
    })
}

fn u32_of(n: usize) -> u32 {
    // Inputs are capped at MAX_INPUT_BYTES (< u32::MAX) before reaching here.
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub(crate) fn nfc_with_map(orig: &str) -> NfcOut {
    let mut text = String::with_capacity(orig.len());
    let mut segs = Vec::new();
    let mut chunk_start = 0usize;
    let flush = |start: usize, end: usize, text: &mut String, segs: &mut Vec<Seg>| {
        if start == end {
            return;
        }
        let chunk = &orig[start..end];
        let changed: Option<String> = match is_nfc_quick(chunk.chars()) {
            IsNormalized::Yes => None,
            _ => Some(chunk.nfc().collect::<String>()).filter(|n| n != chunk),
        };
        if changed.is_none() {
            // Already NFC: one verbatim run — interior positions map byte-for-byte.
            segs.push(Seg {
                out_start: u32_of(text.len()),
                src_start: u32_of(start),
                src_end: u32_of(end),
                linear: true,
            });
            text.push_str(chunk);
        } else {
            // NFC changed this combining sequence: map it as one unit.
            segs.push(Seg {
                out_start: u32_of(text.len()),
                src_start: u32_of(start),
                src_end: u32_of(end),
                linear: false,
            });
            text.push_str(changed.as_deref().unwrap_or(chunk));
        }
    };
    for (i, ch) in orig.char_indices() {
        if i > chunk_start && is_stable_starter(ch) {
            flush(chunk_start, i, &mut text, &mut segs);
            chunk_start = i;
        }
    }
    flush(chunk_start, orig.len(), &mut text, &mut segs);
    NfcOut { text, segs }
}

/// Maps `out` byte ranges back to `src` byte ranges through a segment list.
pub(crate) fn seg_index(segs: &[Seg], out_pos: u32) -> usize {
    // Last segment whose out_start <= out_pos.
    segs.partition_point(|s| s.out_start <= out_pos)
        .saturating_sub(1)
}

/// `(src_start, src_end)` covering `out[start..end)`. Empty ranges map to an empty source range.
///
/// `start`/`end` are snapped to char boundaries by the caller; a non-linear segment covers at
/// most one output char, so a boundary position inside it is always its start.
pub(crate) fn map_range(segs: &[Seg], start: u32, end: u32, src_len: u32) -> (u32, u32) {
    if segs.is_empty() {
        return (0, 0);
    }
    if start >= end {
        let i = seg_index(segs, start);
        let s = segs[i];
        let p = if s.linear {
            s.src_start
                .saturating_add(start.saturating_sub(s.out_start))
        } else {
            s.src_start
        };
        return (p.min(src_len), p.min(src_len));
    }
    let ia = seg_index(segs, start);
    let ib = seg_index(segs, end - 1);
    let sa = segs[ia];
    let sb = segs[ib];
    let a = if sa.linear {
        sa.src_start.saturating_add(start - sa.out_start)
    } else {
        sa.src_start
    };
    let b = if sb.linear {
        sb.src_start.saturating_add(end - sb.out_start)
    } else {
        sb.src_end
    };
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn alphabet() -> impl Strategy<Value = String> {
        let pieces = prop::sample::select(vec![
            "a", "N", "e", "\u{303}", "\u{301}", "\u{327}", "\u{31b}", "\u{323}", "Ñ", "ñ", "é",
            "ọ", "\u{1100}", "\u{1161}", "\u{11a8}", "가", "\u{b47}", "\u{b3e}", " ", "\t", "Å",
            "\u{212b}", "\u{344}", "\u{f73}", "\u{fb43}", "\u{2adc}", "\u{fb1d}", "ﬁ", "ς", "İ",
            "😀", "\u{200d}", "x",
        ]);
        prop::collection::vec(pieces, 0..40).prop_map(|v| v.concat())
    }

    proptest! {
        /// Differential test against the trusted oracle: whole-string NFC.
        #[test]
        fn chunked_nfc_equals_whole_string_nfc(s in alphabet()) {
            let out = nfc_with_map(&s);
            let oracle: String = s.nfc().collect();
            prop_assert_eq!(&out.text, &oracle);
            // Segments are monotone and cover the source in order.
            for w in out.segs.windows(2) {
                prop_assert!(w[0].out_start < w[1].out_start);
                prop_assert!(w[0].src_end <= w[1].src_start);
            }
        }

        #[test]
        fn arbitrary_strings_match_the_oracle(s in "\\PC{0,64}") {
            prop_assert_eq!(nfc_with_map(&s).text, s.nfc().collect::<String>());
        }
    }

    #[test]
    fn known_composition_cases() {
        for s in [
            "N\u{303}",
            "\u{1100}\u{1161}\u{11a8}",
            "\u{b47}\u{b3e}",
            "a\u{323}\u{302}",
            "\u{212b}",
        ] {
            assert_eq!(nfc_with_map(s).text, s.nfc().collect::<String>(), "{s:?}");
        }
        assert_eq!(nfc_with_map("").segs.len(), 0);
        assert_eq!(map_range(&[], 0, 0, 0), (0, 0));
    }
}
