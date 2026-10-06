//! Invariance contract tests for the canonicalization stage (spec §5.1, §5.2).
//!
//! Elementary moves (applied only outside protected spans unless stated):
//! - toggle the case of one letter whose lower and upper forms are single characters that fold
//!   to the same lowercase (so `ς`/`Σ` and `İ` are excluded: they are not case pairs under
//!   `char::to_lowercase`);
//! - insert a space next to an existing whitespace character;
//! - replace one character (anywhere) with its NFD decomposition (canonical equivalence);
//! - (collapse mode) duplicate one of `! ? . , ; :`.
//!
//! Random **sequences** of moves must leave the canonical text, the token texts and the protected
//! spans exactly equal (invariance is closed under composition, GDL Thm 3.106), while every
//! returned span still indexes the new original text and folds to the same canonical text
//! (span equivariance).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)] // tests may unwrap (AGENTS.md rule 4)

use instinct_text::{Fold, NormalizeConfig, Normalized, PunctRuns, Zone, normalize};
use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;

const PIECES: &[&str] = &[
    "stop",
    "Stop",
    "STOP",
    "don't",
    "never",
    "mind",
    "instead",
    "PEÑA",
    "Ñiño",
    "N\u{303}",
    "e\u{301}",
    "café",
    "\u{455}top",
    "ß",
    "İ",
    "ς",
    "Σ",
    "😀",
    " ",
    "  ",
    "\t",
    "\n",
    "!",
    "!!",
    "?",
    ".",
    ",",
    "`",
    "`stop`",
    "```",
    "\"",
    "\"quoted text\"",
    "http://Ex.com/Stop",
    "src/Stop.rs",
    "~/a",
    "v1.2",
    "3.14",
    "1.",
    "a",
    "B",
    "x",
    "가",
    "\u{1100}\u{1161}",
    "\u{3000}",
    "(",
    ")",
];

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PIECES), 0..24).prop_map(|v| v.concat())
}

fn config() -> impl Strategy<Value = NormalizeConfig> {
    (
        prop_oneof![Just(Fold::UnicodeLower), Just(Fold::AsciiLower)],
        prop_oneof![Just(PunctRuns::Keep), Just(PunctRuns::Collapse)],
    )
        .prop_map(|(fold, punct_runs)| NormalizeConfig { fold, punct_runs })
}

#[derive(Debug, Clone, Copy)]
enum Move {
    ToggleCase,
    InsertSpace,
    Nfd,
    DupPunct,
}

fn moves() -> impl Strategy<Value = Vec<(Move, usize)>> {
    prop::collection::vec(
        (
            prop_oneof![
                Just(Move::ToggleCase),
                Just(Move::InsertSpace),
                Just(Move::Nfd),
                Just(Move::DupPunct)
            ],
            any::<usize>(),
        ),
        1..8,
    )
}

fn in_protected(n: &Normalized<'_>, at: usize) -> bool {
    n.protected()
        .iter()
        .any(|p| p.original().range().contains(&at))
}

fn toggled(c: char, fold: Fold) -> Option<char> {
    let mut up = c.to_uppercase();
    let mut lo = c.to_lowercase();
    let (u, l) = (up.next()?, lo.next()?);
    if up.next().is_some() || lo.next().is_some() {
        return None;
    }
    let other = if c == u { l } else { u };
    if other == c {
        return None;
    }
    let same_fold = match fold {
        Fold::UnicodeLower => other.to_lowercase().eq(c.to_lowercase()),
        // AsciiLower only declares ASCII case invariance.
        Fold::AsciiLower => c.is_ascii_alphabetic() && other.is_ascii_alphabetic(),
    };
    // Changing NFC-ness would make this an NFC move, not a case move; keep them separate.
    let nfc_stable = other.to_string().nfc().eq(core::iter::once(other));
    (same_fold && nfc_stable).then_some(other)
}

/// Applies one move at a pseudo-random eligible position. Returns `None` if no position fits.
fn apply(x: &str, mv: Move, pick: usize, cfg: NormalizeConfig) -> Option<String> {
    let n = normalize(x, cfg).unwrap();
    let chars: Vec<(usize, char)> = x.char_indices().collect();
    let eligible: Vec<(usize, char)> = chars
        .iter()
        .copied()
        .filter(|(i, c)| match mv {
            Move::ToggleCase => {
                // Under AsciiLower a base letter followed by a combining mark is part of a
                // non-ASCII character after NFC, so it is not an ASCII case move.
                let next_is_mark = x[*i + c.len_utf8()..].chars().next().is_some_and(|m| {
                    unicode_normalization::char::canonical_combining_class(m) != 0
                });
                !in_protected(&n, *i)
                    && toggled(*c, cfg.fold).is_some()
                    && !(cfg.fold == Fold::AsciiLower && next_is_mark)
            }
            Move::InsertSpace => !in_protected(&n, *i) && c.is_whitespace(),
            Move::Nfd => c.to_string().nfd().count() > 1,
            Move::DupPunct => {
                cfg.punct_runs == PunctRuns::Collapse
                    && !in_protected(&n, *i)
                    && "!?.,;:".contains(*c)
            }
        })
        .collect();
    if eligible.is_empty() {
        return None;
    }
    let (i, c) = eligible[pick % eligible.len()];
    let w = c.len_utf8();
    let mut out = String::with_capacity(x.len() + 4);
    out.push_str(&x[..i]);
    match mv {
        Move::ToggleCase => out.push(toggled(c, cfg.fold)?),
        Move::InsertSpace => {
            out.push(c);
            out.push(' ');
        }
        Move::Nfd => out.extend(c.to_string().nfd()),
        Move::DupPunct => {
            out.push(c);
            out.push(c);
        }
    }
    out.push_str(&x[i + w..]);
    Some(out)
}

fn summary(n: &Normalized<'_>) -> (String, Vec<(String, Zone, u32)>, Vec<String>) {
    (
        n.canonical().to_owned(),
        n.tokens()
            .iter()
            .map(|t| (n.token_text(t).to_owned(), t.zone(), t.sentence()))
            .collect(),
        n.protected()
            .iter()
            .map(|p| n.canonical()[p.canonical_range()].to_owned())
            .collect(),
    )
}

/// The free-text transformation applied to a slice in isolation (no protection detection, no
/// trimming): NFC, whitespace runs → one space, fold, optional punctuation-run collapse.
fn fold_slice(s: &str, cfg: NormalizeConfig) -> String {
    let mut out = String::new();
    let mut prev_ws = false;
    let mut prev: Option<char> = None;
    for c in s.nfc() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
            prev = None;
            continue;
        }
        prev_ws = false;
        if cfg.punct_runs == PunctRuns::Collapse && c.is_ascii_punctuation() && prev == Some(c) {
            continue;
        }
        prev = Some(c);
        match cfg.fold {
            Fold::AsciiLower => out.push(c.to_ascii_lowercase()),
            Fold::UnicodeLower => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Every returned span indexes the original and maps back consistently.
fn check_spans(n: &Normalized<'_>) -> Result<(), TestCaseError> {
    let orig = n.original();
    for t in n.tokens() {
        let s = t.original().slice(orig);
        prop_assert!(
            s.is_some(),
            "token span {:?} not on char boundaries of {:?}",
            t.original(),
            orig
        );
        prop_assert!(!t.original().is_empty());
        if t.is_free() {
            // Span equivariance: the original text under the span folds to the token text
            // (`contains`: spans resolve to combining-sequence granularity, ADR 0006).
            let folded = fold_slice(s.unwrap(), n.config());
            prop_assert!(
                folded.contains(n.token_text(t)),
                "{:?} folded to {:?}, token {:?}",
                s,
                folded,
                n.token_text(t)
            );
        }
    }
    for p in n.protected() {
        let s = p.original().slice(orig).unwrap();
        // Spans resolve to combining-sequence granularity where NFC rewrote a sequence (ADR 0006),
        // so the NFC of the original slice contains the span text (equal in all ordinary text).
        let nfc: String = s.nfc().collect();
        prop_assert!(
            nfc.contains(&n.canonical()[p.canonical_range()]),
            "{:?} vs {:?}",
            nfc,
            &n.canonical()[p.canonical_range()]
        );
    }
    Ok(())
}

proptest! {

    #[test]
    fn move_sequences_leave_canonical_form_unchanged(x in text(), mv in moves(), cfg in config()) {
        let base = summary(&normalize(&x, cfg).unwrap());
        let mut cur = x.clone();
        for (m, pick) in mv {
            if let Some(next) = apply(&cur, m, pick, cfg) {
                cur = next;
            }
            let n = normalize(&cur, cfg).unwrap();
            prop_assert_eq!(&summary(&n), &base, "after {:?}: {:?} vs original {:?}", m, cur, x);
            check_spans(&n)?;
        }
    }

    #[test]
    fn spans_index_the_original(x in text(), cfg in config()) {
        let n = normalize(&x, cfg).unwrap();
        check_spans(&n)?;
    }

    #[test]
    fn never_panics_and_offsets_stay_in_bounds(x in "\\PC{0,128}", a in 0usize..200, b in 0usize..200, cfg in config()) {
        let n = normalize(&x, cfg).unwrap();
        let (lo, hi) = (a.min(b), a.max(b));
        let s = n.to_original(lo..hi);
        prop_assert!(s.slice(&x).is_some());
        check_spans(&n)?;
    }

    #[test]
    fn idempotent_outside_protected(x in text()) {
        // Canonical text re-normalized is a fixed point (canonical forms are canonical).
        let cfg = NormalizeConfig::default();
        let once = normalize(&x, cfg).unwrap().canonical().to_owned();
        let twice = normalize(&once, cfg).unwrap().canonical().to_owned();
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn deterministic(x in text(), cfg in config()) {
        let a = summary(&normalize(&x, cfg).unwrap());
        let x2 = x.clone();
        let b = std::thread::spawn(move || summary(&normalize(&x2, cfg).unwrap())).join().unwrap();
        prop_assert_eq!(a, b);
    }

    /// A cue word inside a protected span stays protected, wherever the span sits.
    #[test]
    fn protected_cues_are_never_free(prefix in text(), suffix in text()) {
        let x = format!("{prefix} `stop` {suffix}");
        let n = normalize(&x, NormalizeConfig::default()).unwrap();
        let inside: Vec<_> = n.tokens().iter().filter(|t| {
            let r = t.original().range();
            x[r].contains("stop") && n.protected().iter().any(|p| p.original().contains(t.original()))
        }).collect();
        for t in inside {
            prop_assert!(!t.is_free());
        }
    }
}
