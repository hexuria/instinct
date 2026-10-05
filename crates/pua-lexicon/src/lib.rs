//! `pua-lexicon`: closed-vocabulary matching over canonical tokens (spec §4.3).
//!
//! A [`Lexicon`] is built from a [`LexiconSpec`] once and validated: every term must already be
//! in canonical form under the pack's [`NormalizeConfig`], made of 1 to 3 free word tokens.
//! [`Lexicon::lookup`] then scans a [`Normalized`] text left to right and reports, per position,
//! the first that applies of:
//!
//! 1. **Exact**: the longest term over up to 3 adjacent free tokens of one sentence (token
//!    boundary by construction, never a prefix). Tokens are words, so separators between them
//!    are not compared: `"wait, no"` matches the term `"wait no"`; a protected span between
//!    them breaks adjacency.
//! 2. **Substring**: only for entries that opted in (refused under 5 chars), inside one token.
//! 3. **Repaired**: SymSpell-style typo repair against single-token terms. Distance 1 for terms
//!    of ≤ 4 chars, 2 otherwise; no repair of tokens under 4 chars or of guard words; ties
//!    broken by QWERTY adjacency, then transposition, then lexical order; each edit costs a fixed
//!    penalty.
//!
//! Tokens inside protected spans are never matched or repaired. A confusable token (mixed
//! script or homoglyph) never produces a hit: when its ASCII skeleton is a term it is reported
//! as a [`ConfusableFlag`] so the pack can lower confidence (spec §4.2).
//!
//! [`overlap`] is the open-vocabulary companion: the share of a candidate text's words that occur
//! in a query (used to rank free-text candidates, e.g. tool descriptions).
//!
//! Hits come out in text order and the result is independent of the order entries were listed
//! in the spec (entries are sorted by term at build time).
//!
//! ```
//! use pua_lexicon::{EntrySpec, Lexicon, LexiconSpec, MatchKind, Repair};
//! use pua_text::{NormalizeConfig, normalize};
//!
//! let spec = LexiconSpec {
//!     entries: vec![EntrySpec::new("never mind", "interrupt"), EntrySpec::new("cancel", "interrupt")],
//!     guards: vec![],
//!     repair: Repair::Typos { penalty_per_edit: pua_core::Confidence::new(150)? },
//! };
//! let lex = Lexicon::new(&spec, NormalizeConfig::default())?;
//! let text = normalize("Never  mind, CANCLE it", NormalizeConfig::default())?;
//! let found = lex.lookup(&text)?;
//! let hits: Vec<_> = found.hits().iter().map(|h| (lex.term(h.entry()), h.kind())).collect();
//! assert_eq!(hits[0], ("never mind", MatchKind::Exact));
//! assert_eq!(hits[1].0, "cancel");
//! assert!(matches!(hits[1].1, MatchKind::Repaired { edits: 1, .. }));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

pub mod ocr;
mod overlap;
mod repair;

pub use overlap::{MIN_OVERLAP_CHARS, overlap};

use core::fmt;
use core::ops::Range;
use std::collections::{BTreeMap, BTreeSet};

use pua_core::{Confidence, Span};
use pua_text::{Confusable, NormalizeConfig, Normalized, Token, normalize};

/// Most tokens a term may span (spec §4.3 "up to 3 adjacent tokens").
pub const MAX_TERM_TOKENS: usize = 3;
/// Shortest term (in chars) that may opt into substring matching.
pub const MIN_SUBSTRING_CHARS: usize = 5;
/// Shortest text token (in chars) that typo repair will touch.
pub const MIN_REPAIR_CHARS: usize = 4;

/// One vocabulary entry as written in pack data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EntrySpec {
    /// The term, already canonical (e.g. `"never mind"`).
    pub term: String,
    /// Opaque pack-defined tag the hit carries (e.g. `"interrupt"`, `"rdo:043"`).
    pub tag: String,
    /// Opt into substring matching inside a single token (refused under 5 chars).
    #[cfg_attr(feature = "serde", serde(default))]
    pub substring: bool,
}

impl EntrySpec {
    /// An exact-match entry.
    pub fn new(term: &str, tag: &str) -> Self {
        Self {
            term: term.to_owned(),
            tag: tag.to_owned(),
            substring: false,
        }
    }
}

/// Typo-repair policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum Repair {
    /// Exact and substring matching only.
    Off,
    /// SymSpell-style repair; each edit costs `penalty_per_edit`.
    Typos {
        /// Fixed cost per edit.
        penalty_per_edit: Confidence,
    },
}

/// A whole vocabulary as written in pack data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct LexiconSpec {
    /// Entries, in any order.
    pub entries: Vec<EntrySpec>,
    /// Real words that must never be repaired into a term (e.g. `"top"` near `"stop"`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub guards: Vec<String>,
    /// Repair policy.
    pub repair: Repair,
}

/// Why a [`LexiconSpec`] was refused. Indices point into `LexiconSpec::entries`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexiconError {
    /// No entries at all.
    NoEntries,
    /// An entry's term is empty.
    EmptyTerm {
        /// Entry index.
        index: usize,
    },
    /// An entry's tag is empty.
    EmptyTag {
        /// Entry index.
        index: usize,
    },
    /// The term is not in canonical form; `expected` is what it canonicalizes to (tokens joined
    /// by one space).
    NonCanonicalTerm {
        /// Entry index.
        index: usize,
        /// The canonical spelling to use instead.
        expected: String,
    },
    /// The term has no word tokens, or a token falls in a protected span (code, number, URL…),
    /// or a token is confusable. Such a term could never match.
    UnmatchableTerm {
        /// Entry index.
        index: usize,
    },
    /// The term spans more than [`MAX_TERM_TOKENS`] tokens.
    TooManyTokens {
        /// Entry index.
        index: usize,
        /// Its token count.
        tokens: usize,
    },
    /// Substring matching was requested for a term under [`MIN_SUBSTRING_CHARS`] chars.
    SubstringTooShort {
        /// Entry index.
        index: usize,
    },
    /// Substring matching was requested for a multi-token term.
    SubstringMultiToken {
        /// Entry index.
        index: usize,
    },
    /// Two entries share a term.
    DuplicateTerm {
        /// The second entry's index.
        index: usize,
        /// The term.
        term: String,
    },
    /// A guard word is not a single canonical free token.
    NonCanonicalGuard {
        /// Guard index.
        index: usize,
    },
    /// A guard word is listed twice.
    DuplicateGuard {
        /// The second occurrence's index.
        index: usize,
    },
    /// A guard word is also a term (it would both match and be protected from matching).
    GuardIsTerm {
        /// Guard index.
        index: usize,
    },
}

impl fmt::Display for LexiconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEntries => f.write_str("lexicon has no entries"),
            Self::EmptyTerm { index } => write!(f, "entry {index}: empty term"),
            Self::EmptyTag { index } => write!(f, "entry {index}: empty tag"),
            Self::NonCanonicalTerm { index, expected } => {
                write!(
                    f,
                    "entry {index}: term is not canonical, write {expected:?}"
                )
            }
            Self::UnmatchableTerm { index } => write!(
                f,
                "entry {index}: term has no free, non-confusable word tokens and could never match"
            ),
            Self::TooManyTokens { index, tokens } => write!(
                f,
                "entry {index}: term has {tokens} tokens, at most {MAX_TERM_TOKENS} allowed"
            ),
            Self::SubstringTooShort { index } => write!(
                f,
                "entry {index}: substring matching needs at least {MIN_SUBSTRING_CHARS} chars"
            ),
            Self::SubstringMultiToken { index } => {
                write!(f, "entry {index}: substring matching is single-token only")
            }
            Self::DuplicateTerm { index, term } => {
                write!(f, "entry {index}: duplicate term {term:?}")
            }
            Self::NonCanonicalGuard { index } => {
                write!(f, "guard {index}: not a single canonical free token")
            }
            Self::DuplicateGuard { index } => write!(f, "guard {index}: duplicate"),
            Self::GuardIsTerm { index } => write!(f, "guard {index}: is also a term"),
        }
    }
}

impl std::error::Error for LexiconError {}

/// Why a lookup was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// The text was canonicalized with a different config than the lexicon's terms, so the
    /// comparison would be meaningless.
    ConfigMismatch {
        /// The lexicon's config.
        lexicon: NormalizeConfig,
        /// The text's config.
        text: NormalizeConfig,
    },
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigMismatch { lexicon, text } => write!(
                f,
                "text normalized with {} but lexicon expects {}",
                text.fingerprint(),
                lexicon.fingerprint()
            ),
        }
    }
}

impl std::error::Error for LookupError {}

/// Index of an entry in a built [`Lexicon`] (entries are sorted by term).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(u32);

impl EntryId {
    /// The raw index.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// How a hit matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchKind {
    /// Exact token-boundary match.
    Exact,
    /// The term occurs inside a longer token (opt-in entries only).
    Substring,
    /// Typo repair.
    Repaired {
        /// Edit distance (1 or 2).
        edits: u8,
        /// `edits × penalty_per_edit`.
        penalty: Confidence,
    },
}

/// A lexicon hit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hit {
    entry: EntryId,
    tokens: (u32, u32),
    original: Span,
    kind: MatchKind,
}

impl Hit {
    /// The entry that matched.
    pub fn entry(&self) -> EntryId {
        self.entry
    }
    /// Token indices (into [`Normalized::tokens`]) covered by the hit.
    pub fn tokens(&self) -> Range<usize> {
        self.tokens.0 as usize..self.tokens.1 as usize
    }
    /// Span of the hit in the original text.
    pub fn original(&self) -> Span {
        self.original
    }
    /// How it matched.
    pub fn kind(&self) -> MatchKind {
        self.kind
    }
}

/// A confusable token whose ASCII skeleton is a term. Never a hit; evidence for lowering
/// confidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfusableFlag {
    entry: EntryId,
    token: u32,
    original: Span,
    kind: Confusable,
}

impl ConfusableFlag {
    /// The term the skeleton spells.
    pub fn entry(&self) -> EntryId {
        self.entry
    }
    /// Token index.
    pub fn token(&self) -> usize {
        self.token as usize
    }
    /// Span in the original text.
    pub fn original(&self) -> Span {
        self.original
    }
    /// Why the token is confusable.
    pub fn kind(&self) -> Confusable {
        self.kind
    }
}

/// Result of [`Lexicon::lookup`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Lookup {
    hits: Vec<Hit>,
    flags: Vec<ConfusableFlag>,
}

impl Lookup {
    /// Hits in text order; they never overlap.
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    /// Confusable flags in text order.
    pub fn flags(&self) -> &[ConfusableFlag] {
        &self.flags
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    term: Box<str>,
    tag: Box<str>,
    chars: Box<[char]>,
    tokens: u8,
    substring: bool,
}

/// A validated, immutable vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexicon {
    config: NormalizeConfig,
    entries: Vec<Entry>,
    exact: BTreeMap<Box<str>, EntryId>,
    substring: Vec<EntryId>,
    guards: BTreeSet<Box<str>>,
    deletes: BTreeMap<Box<str>, Vec<EntryId>>,
    repair: Repair,
    max_tokens: usize,
    longest_single: usize,
}

const fn max_edits(term_chars: usize) -> u16 {
    if term_chars <= 4 { 1 } else { 2 }
}

fn id(i: usize) -> EntryId {
    // Entry count is bounded by memory far below u32::MAX in practice; saturate defensively.
    EntryId(u32::try_from(i).unwrap_or(u32::MAX))
}

fn idx(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

fn make_hit(toks: &[Token], i: usize, n: usize, entry: EntryId, kind: MatchKind) -> Hit {
    Hit {
        entry,
        tokens: (idx(i), idx(i + n)),
        original: toks[i].original().cover(toks[i + n - 1].original()),
        kind,
    }
}

/// Canonical token texts of `s`, or `None` if any token is protected or confusable.
fn canonical_tokens(s: &str, config: NormalizeConfig) -> Option<(String, Vec<String>)> {
    let n = normalize(s, config).ok()?;
    let mut toks = Vec::new();
    for t in n.tokens() {
        if !t.is_free() || t.confusable().is_some() {
            return None;
        }
        toks.push(n.token_text(t).to_owned());
    }
    Some((n.canonical().to_owned(), toks))
}

impl Lexicon {
    /// Validates `spec` against `config` (the config the pack normalizes text with).
    ///
    /// # Errors
    /// The first [`LexiconError`] found, entries checked in spec order, then guards.
    pub fn new(spec: &LexiconSpec, config: NormalizeConfig) -> Result<Self, LexiconError> {
        if spec.entries.is_empty() {
            return Err(LexiconError::NoEntries);
        }
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        let mut staged: Vec<Entry> = Vec::with_capacity(spec.entries.len());
        for (index, e) in spec.entries.iter().enumerate() {
            if e.term.is_empty() {
                return Err(LexiconError::EmptyTerm { index });
            }
            if e.tag.is_empty() {
                return Err(LexiconError::EmptyTag { index });
            }
            let Some((_, toks)) = canonical_tokens(&e.term, config) else {
                return Err(LexiconError::UnmatchableTerm { index });
            };
            if toks.is_empty() {
                return Err(LexiconError::UnmatchableTerm { index });
            }
            let expected = toks.join(" ");
            if expected != e.term {
                return Err(LexiconError::NonCanonicalTerm { index, expected });
            }
            if toks.len() > MAX_TERM_TOKENS {
                return Err(LexiconError::TooManyTokens {
                    index,
                    tokens: toks.len(),
                });
            }
            let chars: Box<[char]> = e.term.chars().collect();
            if e.substring && toks.len() > 1 {
                return Err(LexiconError::SubstringMultiToken { index });
            }
            if e.substring && chars.len() < MIN_SUBSTRING_CHARS {
                return Err(LexiconError::SubstringTooShort { index });
            }
            if seen.insert(e.term.as_str(), index).is_some() {
                return Err(LexiconError::DuplicateTerm {
                    index,
                    term: e.term.clone(),
                });
            }
            staged.push(Entry {
                term: e.term.as_str().into(),
                tag: e.tag.as_str().into(),
                chars,
                tokens: u8::try_from(toks.len()).unwrap_or(u8::MAX),
                substring: e.substring,
            });
        }
        let mut guards = BTreeSet::new();
        for (index, g) in spec.guards.iter().enumerate() {
            match canonical_tokens(g, config) {
                Some((canon, toks)) if toks.len() == 1 && canon == *g => {}
                _ => return Err(LexiconError::NonCanonicalGuard { index }),
            }
            if seen.contains_key(g.as_str()) {
                return Err(LexiconError::GuardIsTerm { index });
            }
            if !guards.insert(Box::<str>::from(g.as_str())) {
                return Err(LexiconError::DuplicateGuard { index });
            }
        }
        staged.sort_by(|a, b| a.term.cmp(&b.term));

        let mut exact = BTreeMap::new();
        let mut substring = Vec::new();
        let mut deletes: BTreeMap<Box<str>, Vec<EntryId>> = BTreeMap::new();
        let mut max_tokens = 1;
        let mut longest_single = 0;
        for (i, e) in staged.iter().enumerate() {
            exact.insert(e.term.clone(), id(i));
            max_tokens = max_tokens.max(usize::from(e.tokens));
            if e.substring {
                substring.push(id(i));
            }
            if e.tokens == 1 && matches!(spec.repair, Repair::Typos { .. }) {
                longest_single = longest_single.max(e.chars.len());
                for d in repair::deletes(&e.chars, usize::from(max_edits(e.chars.len()))) {
                    deletes.entry(d.into_boxed_str()).or_default().push(id(i));
                }
            }
        }
        Ok(Self {
            config,
            entries: staged,
            exact,
            substring,
            guards,
            deletes,
            repair: spec.repair,
            max_tokens,
            longest_single,
        })
    }

    /// The config terms were validated against.
    pub fn config(&self) -> NormalizeConfig {
        self.config
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Always `false`: a built lexicon has at least one entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up an entry by its exact canonical term.
    pub fn find(&self, term: &str) -> Option<EntryId> {
        self.exact.get(term).copied()
    }

    /// The entry's term. Returns `""` for an id from another lexicon that is out of range.
    pub fn term(&self, id: EntryId) -> &str {
        self.entries.get(id.0 as usize).map_or("", |e| &e.term)
    }

    /// The entry's tag. Returns `""` for an id from another lexicon that is out of range.
    pub fn tag(&self, id: EntryId) -> &str {
        self.entries.get(id.0 as usize).map_or("", |e| &e.tag)
    }

    /// Canonical bytes of the vocabulary for `DataVersion`: config fingerprint, sorted entries
    /// (term, tag, substring flag), sorted guards and the repair policy.
    pub fn fingerprint(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut put = |b: &[u8]| {
            out.extend_from_slice(&(b.len() as u64).to_le_bytes());
            out.extend_from_slice(b);
        };
        put(b"pua-lexicon-v1");
        put(self.config.fingerprint().as_bytes());
        for e in &self.entries {
            put(e.term.as_bytes());
            put(e.tag.as_bytes());
            put(if e.substring { b"sub" } else { b"tok" });
        }
        put(b"guards");
        for g in &self.guards {
            put(g.as_bytes());
        }
        match self.repair {
            Repair::Off => put(b"repair=off"),
            Repair::Typos { penalty_per_edit } => {
                put(format!("repair=typos;penalty={}", penalty_per_edit.get()).as_bytes());
            }
        }
        out
    }

    /// Scans `text` for vocabulary hits (see the crate docs for the order of match kinds).
    ///
    /// # Errors
    /// [`LookupError::ConfigMismatch`] when `text` was normalized with a different config.
    pub fn lookup(&self, text: &Normalized<'_>) -> Result<Lookup, LookupError> {
        if text.config() != self.config {
            return Err(LookupError::ConfigMismatch {
                lexicon: self.config,
                text: text.config(),
            });
        }
        let toks = text.tokens();
        let mut out = Lookup::default();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            if !t.is_free() {
                i += 1;
                continue;
            }
            if let Some(kind) = t.confusable() {
                if let Some(entry) = t.ascii_skeleton().and_then(|s| self.find(s)) {
                    out.flags.push(ConfusableFlag {
                        entry,
                        token: idx(i),
                        original: t.original(),
                        kind,
                    });
                }
                i += 1;
                continue;
            }
            if let Some((entry, n)) = self.longest_exact(text, toks, i) {
                out.hits.push(make_hit(toks, i, n, entry, MatchKind::Exact));
                i += n;
                continue;
            }
            let word = text.token_text(t);
            if let Some(entry) = self.substring_hit(word) {
                out.hits
                    .push(make_hit(toks, i, 1, entry, MatchKind::Substring));
            } else if let Some((entry, kind)) = self.repair_hit(word) {
                out.hits.push(make_hit(toks, i, 1, entry, kind));
            }
            i += 1;
        }
        Ok(out)
    }

    fn longest_exact(
        &self,
        text: &Normalized<'_>,
        toks: &[Token],
        i: usize,
    ) -> Option<(EntryId, usize)> {
        let sentence = toks[i].sentence();
        let mut key = String::new();
        let mut best = None;
        for (n, t) in toks[i..].iter().take(self.max_tokens).enumerate() {
            if !t.is_free() || t.confusable().is_some() || t.sentence() != sentence {
                break;
            }
            if n > 0 {
                key.push(' ');
            }
            key.push_str(text.token_text(t));
            if let Some(&e) = self.exact.get(key.as_str()) {
                best = Some((e, n + 1));
            }
        }
        best
    }

    fn substring_hit(&self, word: &str) -> Option<EntryId> {
        // Longest term wins, then lexical order (entries are sorted, so the first of a length).
        let mut best: Option<EntryId> = None;
        for &e in &self.substring {
            let term = &self.entries[e.0 as usize].term;
            if word.len() > term.len()
                && word.contains(&**term)
                && best.is_none_or(|b| self.entries[b.0 as usize].term.len() < term.len())
            {
                best = Some(e);
            }
        }
        best
    }

    fn repair_hit(&self, word: &str) -> Option<(EntryId, MatchKind)> {
        let Repair::Typos { penalty_per_edit } = self.repair else {
            return None;
        };
        if self.guards.contains(word) {
            return None;
        }
        let q: Vec<char> = word.chars().collect();
        // More than 2 chars beyond longest_single, no term is within distance 2: bounds the
        // work on pathological long tokens.
        if q.len() < MIN_REPAIR_CHARS || q.len().saturating_sub(2) > self.longest_single {
            return None;
        }
        let mut candidates = BTreeSet::new();
        for d in repair::deletes(&q, 2) {
            if let Some(ids) = self.deletes.get(d.as_str()) {
                candidates.extend(ids.iter().copied());
            }
        }
        // Ids follow term order, so the least (cost, id) breaks remaining ties lexically. Cost 0
        // is an exact term, which the exact stage owns.
        let best = candidates
            .into_iter()
            .filter_map(|e| {
                let entry = &self.entries[e.0 as usize];
                repair::cost(&q, &entry.chars, max_edits(entry.chars.len()))
                    .filter(|c| c.0 != 0)
                    .map(|c| (c, e))
            })
            .min();
        best.map(|(c, e)| {
            let edits = u8::try_from(c.0).unwrap_or(u8::MAX);
            let penalty = Confidence::saturating(
                i32::from(c.0).saturating_mul(i32::from(penalty_per_edit.get())),
            );
            (e, MatchKind::Repaired { edits, penalty })
        })
    }
}

#[cfg(test)]
mod tests;
