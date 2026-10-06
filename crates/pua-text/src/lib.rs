//! `pua-text`: the canonicalization stage (spec §4.2, §5.2).
//!
//! [`normalize`] turns raw text into a [`Normalized`] view:
//!
//! 1. **NFC** everywhere (canonical equivalence is never a "repair").
//! 2. **Protected spans** are detected on the NFC text: code, URLs, paths, quoted text, numbers.
//!    Inside them nothing else changes: no fold, no whitespace collapse.
//! 3. Outside protected spans: the chosen [`Fold`] (`UnicodeLower` keeps `Ñ` as a letter:
//!    `PEÑA` → `peña`, never `pe a`), whitespace runs collapse to one space (leading and trailing
//!    dropped), and optionally runs of one repeated ASCII punctuation mark collapse.
//! 4. An **offset map**: every canonical byte range maps back to an original byte range, so every
//!    span PUA reports is in original coordinates ([`Normalized::to_original`]).
//! 5. **Tokens** (Unicode word boundaries, with their zone and sentence) and **confusable** flags.
//!
//! All declared invariances are enforced here, once; later stages only see the canonical form
//! (spec §5.2 "canonicalize first").
//!
//! ```
//! use pua_text::{normalize, NormalizeConfig};
//! let n = normalize("  PEÑA   TRADING  ", NormalizeConfig::default())?;
//! assert_eq!(n.canonical(), "peña trading");
//! let t = &n.tokens()[0];
//! assert_eq!(t.original().slice(n.original()), Some("PEÑA"));
//! # Ok::<(), pua_text::TextError>(())
//! ```
#![forbid(unsafe_code)]

mod nfc;
mod protect;

use core::fmt;
use core::ops::Range;

use pua_core::Span;
use unicode_security::MixedScript;
use unicode_segmentation::UnicodeSegmentation as _;

use crate::nfc::{Seg, map_range};
pub use crate::protect::ProtectedKind;

/// Largest accepted input (16 MiB). Larger inputs are refused rather than truncated.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Identifies the canonicalization algorithm and its Unicode data; consumers fold it into
/// `DataVersion`. Kept in sync with `Cargo.lock` by a test.
pub const ALGORITHM_TAG: &str =
    "pua-text-v1;unicode-normalization=0.1.25;unicode-segmentation=1.13.3;unicode-security=0.1.2";

/// Errors from [`normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextError {
    /// Input longer than [`MAX_INPUT_BYTES`].
    TooLong {
        /// Input length in bytes.
        len: usize,
        /// The limit.
        max: usize,
    },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { len, max } => write!(f, "input is {len} bytes (max {max})"),
        }
    }
}

impl std::error::Error for TextError {}

/// Case folding applied outside protected spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Fold {
    /// ASCII `A-Z` → `a-z` only (what opengrok `intent.rs` does today).
    AsciiLower,
    /// `char::to_lowercase` for every character. `Ñ` stays a letter (`ñ`). The Unicode version is
    /// the toolchain's (`char::UNICODE_VERSION`, recorded in `DataVersion`).
    #[default]
    UnicodeLower,
}

impl Fold {
    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::AsciiLower => "ascii_lower",
            Self::UnicodeLower => "unicode_lower",
        }
    }
}

/// What to do with runs of one repeated ASCII punctuation mark outside protected spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PunctRuns {
    /// Keep them (`!!` stays `!!`).
    #[default]
    Keep,
    /// Collapse to one (`!!!` → `!`). For consumers whose punctuation runs carry no meaning.
    Collapse,
}

/// Canonicalization settings, chosen per consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NormalizeConfig {
    /// Case fold.
    pub fold: Fold,
    /// Punctuation runs.
    pub punct_runs: PunctRuns,
}

impl NormalizeConfig {
    /// Stable bytes for `DataVersion`.
    pub fn fingerprint(&self) -> String {
        let p = match self.punct_runs {
            PunctRuns::Keep => "keep",
            PunctRuns::Collapse => "collapse",
        };
        format!("{ALGORITHM_TAG};fold={};punct={p}", self.fold.name())
    }
}

/// Whether a token is free text or inside a protected span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    /// Free text: may be matched and repaired.
    Free,
    /// Inside a protected span: never repaired, never matched as a cue.
    Protected(ProtectedKind),
}

/// Why a token looks confusable (spec §4.2 "Confusables").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Confusable {
    /// Characters from more than one script (UTS #39 resolved script set is empty), e.g. Cyrillic
    /// `ѕ` + Latin `top`.
    MixedScript,
    /// Single script, but its UTS #39 skeleton is plain ASCII different from the token.
    Homoglyph,
}

/// A word token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    canon: (u32, u32),
    original: Span,
    zone: Zone,
    sentence: u32,
    confusable: Option<Confusable>,
    skeleton: Option<Box<str>>,
}

impl Token {
    /// Byte range in the canonical text.
    pub fn canonical_range(&self) -> Range<usize> {
        self.canon.0 as usize..self.canon.1 as usize
    }
    /// Span in the original text.
    pub fn original(&self) -> Span {
        self.original
    }
    /// Free or protected.
    pub fn zone(&self) -> Zone {
        self.zone
    }
    /// Whether the token is free text.
    pub fn is_free(&self) -> bool {
        self.zone == Zone::Free
    }
    /// Index of the sentence containing the token.
    pub fn sentence(&self) -> u32 {
        self.sentence
    }
    /// Confusable flag, if any.
    pub fn confusable(&self) -> Option<Confusable> {
        self.confusable
    }
    /// The ASCII skeleton when the token is confusable and its skeleton is plain ASCII
    /// (e.g. `ѕtop` → `stop`). Never used to fire a cue; only to flag one.
    pub fn ascii_skeleton(&self) -> Option<&str> {
        self.skeleton.as_deref()
    }
}

/// A protected span in both coordinate systems.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Protected {
    kind: ProtectedKind,
    canon: (u32, u32),
    original: Span,
}

impl Protected {
    /// Kind.
    pub fn kind(&self) -> ProtectedKind {
        self.kind
    }
    /// Canonical byte range.
    pub fn canonical_range(&self) -> Range<usize> {
        self.canon.0 as usize..self.canon.1 as usize
    }
    /// Original span.
    pub fn original(&self) -> Span {
        self.original
    }
}

/// The canonical view of one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized<'a> {
    original: &'a str,
    config: NormalizeConfig,
    canonical: String,
    segs: Vec<Seg>,
    protected: Vec<Protected>,
    tokens: Vec<Token>,
    sentences: u32,
}

impl<'a> Normalized<'a> {
    /// The original input.
    pub fn original(&self) -> &'a str {
        self.original
    }
    /// The canonical text.
    pub fn canonical(&self) -> &str {
        &self.canonical
    }
    /// The config used.
    pub fn config(&self) -> NormalizeConfig {
        self.config
    }
    /// Protected spans in order.
    pub fn protected(&self) -> &[Protected] {
        &self.protected
    }
    /// Tokens in order.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
    /// Number of sentences.
    pub fn sentence_count(&self) -> u32 {
        self.sentences
    }
    /// The canonical text of a token.
    pub fn token_text(&self, t: &Token) -> &str {
        self.canonical.get(t.canonical_range()).unwrap_or("")
    }
    /// Maps a canonical byte range to the original span covering it. Ranges are clamped to the
    /// canonical text; the result always lies on UTF-8 boundaries of the original. Where NFC
    /// rewrote a combining sequence, the span widens to that whole sequence (ADR 0006).
    pub fn to_original(&self, canon: Range<usize>) -> Span {
        let len = u32_of(self.canonical.len());
        let a = u32_of(canon.start).min(len);
        let b = u32_of(canon.end).min(len).max(a);
        let (s, e) = map_range(&self.segs, a, b, u32_of(self.original.len()));
        Span::new(s, e).unwrap_or_else(|_| Span::new(s, s).unwrap_or_else(|_| empty_span()))
    }
}

fn empty_span() -> Span {
    Span::from_range(0..0).unwrap_or_else(|_| empty_span())
}

fn u32_of(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

struct Builder {
    out: String,
    segs: Vec<Seg>,
}

impl Builder {
    /// Appends `s`, mapped to the NFC range `src`.
    fn push(&mut self, s: &str, src: Range<usize>) {
        if s.is_empty() {
            return;
        }
        self.segs.push(Seg {
            out_start: u32_of(self.out.len()),
            src_start: u32_of(src.start),
            src_end: u32_of(src.end),
        });
        self.out.push_str(s);
    }
    /// Extends the last segment's source to `end` (a collapsed run absorbed into it).
    fn extend_last(&mut self, end: usize) {
        if let Some(last) = self.segs.last_mut() {
            last.src_end = last.src_end.max(u32_of(end));
        }
    }
}

/// The input-size gate, separate so the inclusive bound is testable without a 16 MiB input.
fn check_len(len: usize) -> Result<(), TextError> {
    if len > MAX_INPUT_BYTES {
        return Err(TextError::TooLong {
            len,
            max: MAX_INPUT_BYTES,
        });
    }
    Ok(())
}

/// Canonicalizes `text` (see the crate docs).
///
/// # Errors
/// [`TextError::TooLong`] when `text` exceeds [`MAX_INPUT_BYTES`].
#[allow(clippy::many_single_char_names)] // s/b/i/r/w: the scanner's conventional names
pub fn normalize(text: &str, config: NormalizeConfig) -> Result<Normalized<'_>, TextError> {
    check_len(text.len())?;
    let nfc = nfc::nfc_with_map(text);
    let spans = protect::detect(&nfc.text);
    let s = nfc.text.as_str();

    let mut b = Builder {
        out: String::with_capacity(s.len()),
        segs: Vec::with_capacity(s.len()),
    };
    let mut protected_canon: Vec<(Range<usize>, ProtectedKind, Range<usize>)> = Vec::new();
    let mut pending_space: Option<Range<usize>> = None;
    let mut last_punct: Option<char> = None;
    let mut span_iter = spans.iter().peekable();
    let mut i = 0usize;
    let mut buf = [0u8; 4];
    while i < s.len() {
        if let Some((r, kind)) = span_iter.peek().filter(|(r, _)| r.start == i).copied() {
            if let Some(ws) = pending_space.take().filter(|_| !b.out.is_empty()) {
                b.push(" ", ws);
            }
            let start = b.out.len();
            for (j, ch) in s[r.clone()].char_indices() {
                let at = r.start + j;
                b.push(ch.encode_utf8(&mut buf), at..at + ch.len_utf8());
            }
            protected_canon.push((start..b.out.len(), *kind, r.clone()));
            last_punct = None;
            i = r.end;
            span_iter.next();
            continue;
        }
        let Some(ch) = s[i..].chars().next() else {
            break;
        };
        let w = ch.len_utf8();
        if ch.is_whitespace() {
            pending_space = Some(pending_space.map_or(i..i + w, |p| p.start..i + w));
            last_punct = None;
            i += w;
            continue;
        }
        if let Some(ws) = pending_space.take().filter(|_| !b.out.is_empty()) {
            b.push(" ", ws);
        }
        if config.punct_runs == PunctRuns::Collapse
            && ch.is_ascii_punctuation()
            && last_punct == Some(ch)
        {
            b.extend_last(i + w);
            i += w;
            continue;
        }
        last_punct = ch.is_ascii_punctuation().then_some(ch);
        match config.fold {
            Fold::AsciiLower => b.push(ch.to_ascii_lowercase().encode_utf8(&mut buf), i..i + w),
            Fold::UnicodeLower => {
                let lowered: String = ch.to_lowercase().collect();
                b.push(&lowered, i..i + w);
            }
        }
        i += w;
    }

    // Compose canonical→NFC with NFC→original.
    let orig_len = u32_of(text.len());
    let segs: Vec<Seg> = b
        .segs
        .iter()
        .map(|sg| {
            let (a, e) = map_range(&nfc.segs, sg.src_start, sg.src_end, orig_len);
            Seg {
                out_start: sg.out_start,
                src_start: a,
                src_end: e,
            }
        })
        .collect();
    let mut n = Normalized {
        original: text,
        config,
        canonical: b.out,
        segs,
        protected: Vec::new(),
        tokens: Vec::new(),
        sentences: 0,
    };
    n.protected = protected_canon
        .into_iter()
        .map(|(c, kind, _)| Protected {
            kind,
            canon: (u32_of(c.start), u32_of(c.end)),
            original: n.to_original(c),
        })
        .collect();
    tokenize(&mut n);
    Ok(n)
}

fn tokenize(n: &mut Normalized<'_>) {
    let canon = n.canonical.as_str();
    let sentences = sentence_starts(canon);
    n.sentences = u32_of(sentences.len());
    let mut tokens = Vec::new();
    for (start, word) in canon.split_word_bound_indices() {
        if !word.chars().any(char::is_alphanumeric) {
            continue;
        }
        let end = start + word.len();
        let zone = n
            .protected
            .iter()
            .find(|p| (p.canon.0 as usize) < end && start < p.canon.1 as usize)
            .map_or(Zone::Free, |p| Zone::Protected(p.kind));
        let sentence = u32_of(sentences.partition_point(|s| *s <= start).saturating_sub(1));
        let (confusable, skeleton) = confusable_of(word);
        tokens.push(Token {
            canon: (u32_of(start), u32_of(end)),
            original: n.to_original(start..end),
            zone,
            sentence,
            confusable,
            skeleton,
        });
    }
    n.tokens = tokens;
}

/// Sentence starts in canonical text: a new sentence begins after a run of `.`, `!` or `?`
/// followed by a space. Case-free by construction (UAX #29 sentence rules look at letter case,
/// which would break case invariance on folded text). Abbreviations such as `e.g.` split a
/// sentence; that only narrows negation windows, never widens them.
fn sentence_starts(canon: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let bytes = canon.as_bytes();
    for (i, w) in bytes.windows(2).enumerate() {
        if matches!(w[0], b'.' | b'!' | b'?') && w[1] == b' ' {
            starts.push(i + 2);
        }
    }
    starts
}

fn confusable_of(word: &str) -> (Option<Confusable>, Option<Box<str>>) {
    if word.is_ascii() {
        return (None, None);
    }
    let sk: String = unicode_security::skeleton(word).collect();
    let ascii_skeleton = (sk.is_ascii() && sk != word).then(|| sk.into_boxed_str());
    if !word.is_single_script() {
        return (Some(Confusable::MixedScript), ascii_skeleton);
    }
    match ascii_skeleton {
        Some(s) => (Some(Confusable::Homoglyph), Some(s)),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(s: &str) -> String {
        normalize(s, NormalizeConfig::default())
            .unwrap()
            .canonical()
            .to_owned()
    }

    #[test]
    fn too_long_is_exact() {
        let big = "a".repeat(MAX_INPUT_BYTES + 1);
        assert_eq!(
            normalize(&big, NormalizeConfig::default()),
            Err(TextError::TooLong {
                len: MAX_INPUT_BYTES + 1,
                max: MAX_INPUT_BYTES
            })
        );
        assert_eq!(
            TextError::TooLong { len: 2, max: 1 }.to_string(),
            "input is 2 bytes (max 1)"
        );
    }

    #[test]
    fn size_gate_is_inclusive_and_pinned() {
        assert_eq!(MAX_INPUT_BYTES, 16_777_216);
        assert_eq!(check_len(MAX_INPUT_BYTES), Ok(()));
        assert_eq!(
            check_len(MAX_INPUT_BYTES + 1),
            Err(TextError::TooLong {
                len: MAX_INPUT_BYTES + 1,
                max: MAX_INPUT_BYTES
            })
        );
    }

    #[test]
    fn collapsed_whitespace_maps_to_the_whole_run() {
        let n = normalize("ab  \t  cd", NormalizeConfig::default()).unwrap();
        assert_eq!(n.canonical(), "ab cd");
        assert_eq!(n.to_original(2..3), Span::new(2, 7).unwrap());
        let n = normalize("x \u{3000}\u{a0}y", NormalizeConfig::default()).unwrap();
        assert_eq!(n.canonical(), "x y");
        assert_eq!(n.to_original(1..2), Span::new(1, 7).unwrap());
        // A single wide whitespace char: the run is exactly that char.
        let n = normalize("ab\u{3000}cd", NormalizeConfig::default()).unwrap();
        assert_eq!(n.to_original(2..3), Span::new(2, 5).unwrap());
    }

    #[test]
    fn empty_ranges_map_to_segment_edges() {
        // "e\u{301}x" -> "éx": é is 2 canonical bytes over 3 original bytes, one segment.
        let n = normalize("e\u{301}x", NormalizeConfig::default()).unwrap();
        assert_eq!(n.canonical(), "éx");
        let at = |k: usize| n.to_original(k..k);
        assert_eq!(at(0), Span::new(0, 0).unwrap()); // at a segment start: its source start
        assert_eq!(at(1), Span::new(3, 3).unwrap()); // inside a segment: its source end
        assert_eq!(at(2), Span::new(3, 3).unwrap());
        assert_eq!(at(3), Span::new(4, 4).unwrap()); // end of text
    }

    #[test]
    fn tokens_touching_a_protected_span_stay_free() {
        let n = normalize("stop`x`now", NormalizeConfig::default()).unwrap();
        let zones: Vec<(&str, Zone)> = n
            .tokens()
            .iter()
            .map(|t| (n.token_text(t), t.zone()))
            .collect();
        assert_eq!(
            zones,
            [
                ("stop", Zone::Free),
                ("x", Zone::Protected(ProtectedKind::InlineCode)),
                ("now", Zone::Free)
            ]
        );
    }

    #[test]
    fn unicode_fixtures() {
        assert_eq!(canon("PEÑA"), "peña");
        assert_eq!(canon("Ñiño"), "ñiño");
        assert_eq!(canon("PEN\u{303}A"), "peña"); // combining tilde → precomposed
        assert_ne!(canon("PEÑA"), canon("PENA"));
        let ascii = NormalizeConfig {
            fold: Fold::AsciiLower,
            ..NormalizeConfig::default()
        };
        assert_eq!(normalize("PEÑA", ascii).unwrap().canonical(), "peÑa");
    }

    #[test]
    fn whitespace_and_protected() {
        assert_eq!(canon("  Stop   NOW \t"), "stop now");
        assert_eq!(canon("Run  `Stop  It`   Now"), "run `Stop  It` now");
        assert_eq!(canon("a\n\n b"), "a b");
        assert_eq!(canon(""), "");
        assert_eq!(canon("   "), "");
    }

    #[test]
    fn punct_runs() {
        let c = NormalizeConfig {
            punct_runs: PunctRuns::Collapse,
            ..NormalizeConfig::default()
        };
        let n = normalize("Stop!!! now?? `a!!`", c).unwrap();
        assert_eq!(n.canonical(), "stop! now? `a!!`");
        assert_eq!(canon("stop!!"), "stop!!");
        // The collapsed run maps back to the whole original run.
        assert_eq!(n.to_original(4..5).slice(n.original()), Some("!!!"));
        assert!(
            c.fingerprint()
                .ends_with("fold=unicode_lower;punct=collapse")
        );
    }

    #[test]
    fn offsets_point_into_the_original() {
        // The three COR cases of A §5.3.
        for raw in [
            "  TIN: 123-456-789-000",
            "PEÑA TRADING  TRADE NAME: PEÑA STORE",
            "TIN:   1",
        ] {
            let n = normalize(raw, NormalizeConfig::default()).unwrap();
            for t in n.tokens() {
                let orig = t.original().slice(raw).unwrap();
                assert_eq!(orig.to_lowercase(), n.token_text(t), "{raw:?}");
            }
        }
        let n = normalize("PEÑA TRADING  TRADE NAME: X", NormalizeConfig::default()).unwrap();
        let pos = n.canonical().find("name").unwrap();
        assert_eq!(
            n.to_original(pos..pos + 4).slice(n.original()),
            Some("NAME")
        );
        // Clamping and empty ranges.
        assert_eq!(n.to_original(999..1000).len(), 0);
        assert_eq!(n.to_original(3..3).len(), 0);
    }

    #[test]
    fn tokens_zones_and_sentences() {
        let n = normalize(
            "Stop it. Then run `stop` here? Yes",
            NormalizeConfig::default(),
        )
        .unwrap();
        let words: Vec<(&str, bool, u32)> = n
            .tokens()
            .iter()
            .map(|t| (n.token_text(t), t.is_free(), t.sentence()))
            .collect();
        assert_eq!(
            words,
            vec![
                ("stop", true, 0),
                ("it", true, 0),
                ("then", true, 1),
                ("run", true, 1),
                ("stop", false, 1),
                ("here", true, 1),
                ("yes", true, 2)
            ]
        );
        assert_eq!(n.sentence_count(), 3);
        assert_eq!(
            n.tokens()[4].zone(),
            Zone::Protected(ProtectedKind::InlineCode)
        );
        assert_eq!(
            n.protected()[0].original().slice(n.original()),
            Some("`stop`")
        );
        assert_eq!(n.protected()[0].kind(), ProtectedKind::InlineCode);
        assert_eq!(&n.canonical()[n.protected()[0].canonical_range()], "`stop`");
    }

    #[test]
    fn confusables() {
        let n = normalize("\u{455}top please", NormalizeConfig::default()).unwrap(); // Cyrillic ѕ
        let t = &n.tokens()[0];
        assert_eq!(t.confusable(), Some(Confusable::MixedScript));
        assert_eq!(t.ascii_skeleton(), Some("stop"));
        assert_eq!(n.tokens()[1].confusable(), None);
        let n = normalize("PEÑA café", NormalizeConfig::default()).unwrap();
        assert!(n.tokens().iter().all(|t| t.confusable().is_none()));
    }

    #[test]
    fn algorithm_tag_matches_lockfile() {
        let lock = include_str!("../../../Cargo.lock");
        for (name, ver) in [
            ("unicode-normalization", "0.1.25"),
            ("unicode-segmentation", "1.13.3"),
            ("unicode-security", "0.1.2"),
        ] {
            let needle = format!("name = \"{name}\"\nversion = \"{ver}\"");
            assert!(
                lock.contains(&needle),
                "{name} is not {ver} in Cargo.lock: update ALGORITHM_TAG (changes DataVersion)"
            );
        }
        assert_eq!(Fold::AsciiLower.name(), "ascii_lower");
    }

    #[test]
    fn sentence_boundaries_are_terminal_punct_then_canonical_space() {
        let count = |s: &str| {
            normalize(s, NormalizeConfig::default())
                .unwrap()
                .sentence_count()
        };
        // Terminal punct inside a word does not break a sentence.
        assert_eq!(count("a!b c"), 1);
        assert_eq!(count("a.b c"), 1);
        assert_eq!(count("a. b"), 2);
        assert_eq!(count("a! b"), 2);
        assert_eq!(count("a? b"), 2);
        // Any whitespace breaks: canonical whitespace is already a single space.
        assert_eq!(count("a.\tb"), 2);
        assert_eq!(count("a.\n\nb"), 2);
        // Terminal punct at the end of input is a single sentence.
        assert_eq!(count("a."), 1);
        // Abbreviations are not detected: `e.g.` splits like any other `. `.
        assert_eq!(count("e.g. case"), 2);
    }

    #[test]
    fn ligatures_flag_as_homoglyph_with_ascii_skeleton() {
        // U+FB02 'ﬂ' folds into "flag": a lookalike that is not mixed-script.
        let n = normalize("the \u{fb02}ag is up", NormalizeConfig::default()).unwrap();
        let t = &n.tokens()[1];
        assert_eq!(n.token_text(t), "\u{fb02}ag");
        assert_eq!(t.confusable(), Some(Confusable::Homoglyph));
        assert_eq!(t.ascii_skeleton(), Some("flag"));
    }
}
