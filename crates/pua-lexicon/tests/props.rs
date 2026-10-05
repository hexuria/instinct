//! Property tests for the lexicon (spec §4.3, §5.2; plan T5).
//!
//! - The hit multiset is invariant under the declared moves: letter case, whitespace width,
//!   NFC/NFD (generator-sequence style: one abstract message, two independent renderings).
//! - The result does not depend on the order entries are listed in the spec.
//! - The `SymSpell` index finds exactly what a brute-force scan over all terms finds.
//! - Nothing inside a protected span ever matches or is repaired.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names,
    clippy::needless_range_loop // the reference DP mirrors the textbook recurrence
)]

use proptest::prelude::*;
use pua_core::Confidence;
use pua_lexicon::{EntrySpec, Lexicon, LexiconSpec, MatchKind, Repair};
use pua_text::{NormalizeConfig, normalize};
use unicode_normalization::UnicodeNormalization as _;

const TERMS: &[(&str, &str)] = &[
    ("stop", "interrupt"),
    ("cancel", "interrupt"),
    ("never mind", "interrupt"),
    ("forget it", "interrupt"),
    ("halt", "interrupt"),
    ("switch to", "steer"),
    ("instead", "steer"),
    ("wait no", "steer"),
    ("after that", "queue"),
    ("also", "queue"),
    ("peña", "name"),
    ("café", "place"),
];

const NOISE: &[&str] = &[
    "top",
    "step",
    "pena",
    "cafe",
    "stpo",
    "cancle",
    "cnacel",
    "nevr",
    "mind",
    "the",
    "please",
    "now",
    "insted",
    "swicth",
    "aftr",
    "that",
    "it",
    "no",
    "wait",
    "\u{455}top",
    "halts",
];

#[derive(Debug, Clone)]
enum Piece {
    Word(String),
    Protected(String),
}

fn piece() -> impl Strategy<Value = Piece> {
    let term = proptest::sample::select(TERMS).prop_map(|(t, _)| Piece::Word(t.to_owned()));
    let noise = proptest::sample::select(NOISE).prop_map(|w| Piece::Word(w.to_owned()));
    let prot = proptest::sample::select(TERMS).prop_flat_map(|(t, _)| {
        proptest::sample::select(vec![
            format!("`{t}`"),
            format!("\"{t}\""),
            format!("http://x/{}", t.replace(' ', "-")),
        ])
        .prop_map(Piece::Protected)
    });
    prop_oneof![3 => term, 3 => noise, 1 => prot]
}

const SEPS: &[&str] = &[" ", ", ", ". ", "! ", "? ", " - "];

/// An abstract message: pieces with separators.
fn message() -> impl Strategy<Value = Vec<(Piece, &'static str)>> {
    proptest::collection::vec((piece(), proptest::sample::select(SEPS)), 0..12)
}

/// One concrete rendering: `style` bits pick case per char, extra whitespace per separator and
/// NFC vs NFD per word. Protected pieces are rendered verbatim.
fn render(msg: &[(Piece, &str)], style: &[u8]) -> String {
    let mut bits = style.iter().copied().cycle();
    let mut out = String::new();
    for (p, sep) in msg {
        match p {
            Piece::Word(w) => {
                let decompose = bits.next().unwrap_or(0) & 1 == 1;
                let mut word = String::new();
                for c in w.chars() {
                    let up = bits.next().unwrap_or(0) & 1 == 1;
                    let mut u = c.to_uppercase();
                    match (up, u.next(), u.next()) {
                        (true, Some(x), None) if x.to_lowercase().eq(c.to_lowercase()) => {
                            word.push(x);
                        }
                        _ => word.push(c),
                    }
                }
                if decompose {
                    out.extend(word.nfd());
                } else {
                    out.push_str(&word);
                }
            }
            Piece::Protected(s) => out.push_str(s),
        }
        // Whitespace width is a symmetry; the punctuation itself is kept.
        let extra = usize::from(bits.next().unwrap_or(0) % 3);
        out.push_str(sep);
        out.push_str(&" ".repeat(extra));
        if extra == 2 {
            out.push('\t');
        }
    }
    out
}

fn lexicon(order: &[usize], guards: bool) -> Lexicon {
    let spec = LexiconSpec {
        entries: order
            .iter()
            .map(|&i| EntrySpec::new(TERMS[i].0, TERMS[i].1))
            .collect(),
        guards: if guards {
            vec!["top".into(), "step".into(), "pena".into()]
        } else {
            vec![]
        },
        repair: Repair::Typos {
            penalty_per_edit: Confidence::new(150).unwrap(),
        },
    };
    Lexicon::new(&spec, NormalizeConfig::default()).unwrap()
}

type Observed = (
    Vec<(String, String, MatchKind, usize, usize)>,
    Vec<(String, usize)>,
);

/// Hits as (term, tag, kind, token range), plus flags as (term, token): independent of
/// entry ids.
fn observe(lex: &Lexicon, text: &str) -> Observed {
    let n = normalize(text, lex.config()).unwrap();
    let found = lex.lookup(&n).unwrap();
    let hits = found
        .hits()
        .iter()
        .map(|h| {
            assert!(h.original().slice(text).is_some());
            (
                lex.term(h.entry()).to_owned(),
                lex.tag(h.entry()).to_owned(),
                h.kind(),
                h.tokens().start,
                h.tokens().end,
            )
        })
        .collect();
    let flags = found
        .flags()
        .iter()
        .map(|f| (lex.term(f.entry()).to_owned(), f.token()))
        .collect();
    (hits, flags)
}

fn all() -> Vec<usize> {
    (0..TERMS.len()).collect()
}

proptest! {
    #[test]
    fn hits_invariant_under_case_space_and_nfd(
        msg in message(),
        a in proptest::collection::vec(any::<u8>(), 1..64),
        b in proptest::collection::vec(any::<u8>(), 1..64),
    ) {
        let lex = lexicon(&all(), true);
        let x = render(&msg, &a);
        let y = render(&msg, &b);
        prop_assert_eq!(observe(&lex, &x), observe(&lex, &y), "{:?} vs {:?}", x, y);
    }

    #[test]
    fn entry_order_does_not_matter(
        msg in message(),
        style in proptest::collection::vec(any::<u8>(), 1..64),
        order in Just(all()).prop_shuffle(),
    ) {
        let x = render(&msg, &style);
        let a = lexicon(&all(), true);
        let b = lexicon(&order, true);
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(a.fingerprint(), b.fingerprint());
        prop_assert_eq!(observe(&a, &x), observe(&b, &x));
    }

    #[test]
    fn hits_are_ordered_disjoint_and_in_bounds(msg in message(), style in proptest::collection::vec(any::<u8>(), 1..64)) {
        let lex = lexicon(&all(), false);
        let x = render(&msg, &style);
        let n = normalize(&x, lex.config()).unwrap();
        let found = lex.lookup(&n).unwrap();
        let mut last = 0;
        for h in found.hits() {
            prop_assert!(h.tokens().start >= last);
            prop_assert!(h.tokens().end > h.tokens().start);
            prop_assert!(h.tokens().end <= n.tokens().len());
            last = h.tokens().end;
            for t in &n.tokens()[h.tokens()] {
                prop_assert!(t.is_free());
                prop_assert!(h.original().contains(t.original()));
            }
        }
    }

    #[test]
    fn nothing_inside_protected_spans(msg in message(), style in proptest::collection::vec(any::<u8>(), 1..64)) {
        let lex = lexicon(&all(), false);
        let x = render(&msg, &style);
        for wrapped in [format!("```\n{x}\n```"), format!("```\n{x}")] {
            let (hits, flags) = observe(&lex, &wrapped);
            prop_assert!(hits.is_empty(), "{:?}", wrapped);
            prop_assert!(flags.is_empty());
        }
    }

    #[test]
    fn symspell_index_equals_brute_force(word in "[a-zñé]{0,9}") {
        let lex = lexicon(&all(), false);
        let n = normalize(&word, lex.config()).unwrap();
        let got = lex.lookup(&n).unwrap().hits().first().map(|h| (lex.term(h.entry()).to_owned(), h.kind()));
        prop_assert_eq!(got, brute(&word));
    }
}

/// Reference: scan every single-token term with a plain OSA distance and the documented
/// tie-break, no index.
fn brute(word: &str) -> Option<(String, MatchKind)> {
    if word.is_empty() {
        return None;
    }
    if let Some((t, _)) = TERMS.iter().find(|(t, _)| *t == word) {
        return Some(((*t).to_owned(), MatchKind::Exact));
    }
    let q: Vec<char> = word.chars().collect();
    if q.len() < 4 {
        return None;
    }
    let mut best: Option<((usize, usize, usize), &str)> = None;
    for (t, _) in TERMS.iter().filter(|(t, _)| !t.contains(' ')) {
        let tc: Vec<char> = t.chars().collect();
        let max = if tc.len() <= 4 { 1 } else { 2 };
        let c = osa(&q, &tc);
        if c.0 <= max && best.is_none_or(|b| (c, *t) < b) {
            best = Some((c, t));
        }
    }
    best.map(|(c, t)| {
        let edits = u8::try_from(c.0).unwrap();
        (
            t.to_owned(),
            MatchKind::Repaired {
                edits,
                penalty: Confidence::new(150 * i16::from(edits)).unwrap(),
            },
        )
    })
}

fn adjacent(a: char, b: char) -> bool {
    const ROWS: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
    let pos = |c: char| {
        ROWS.iter()
            .enumerate()
            .find_map(|(r, row)| row.find(c).map(|i| (r, i)))
    };
    match (pos(a), pos(b)) {
        (Some((r1, c1)), Some((r2, c2))) => {
            (r1 == r2 && c1.abs_diff(c2) == 1)
                || (r2 == r1 + 1 && (c2 == c1 || c2 + 1 == c1))
                || (r1 == r2 + 1 && (c1 == c2 || c1 + 1 == c2))
        }
        _ => false,
    }
}

/// (edits, keyboard misses, non-transpositions), minimized lexicographically.
fn osa(a: &[char], b: &[char]) -> (usize, usize, usize) {
    let add =
        |x: (usize, usize, usize), y: (usize, usize, usize)| (x.0 + y.0, x.1 + y.1, x.2 + y.2);
    let mut d = vec![vec![(0, 0, 0); b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = (i, i, i);
    }
    for j in 0..=b.len() {
        d[0][j] = (j, j, j);
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let sub = if a[i - 1] == b[j - 1] {
                (0, 0, 0)
            } else if adjacent(a[i - 1], b[j - 1]) {
                (1, 0, 1)
            } else {
                (1, 1, 1)
            };
            let mut m = add(d[i - 1][j - 1], sub)
                .min(add(d[i - 1][j], (1, 1, 1)))
                .min(add(d[i][j - 1], (1, 1, 1)));
            if i > 1
                && j > 1
                && a[i - 1] == b[j - 2]
                && a[i - 2] == b[j - 1]
                && a[i - 1] != b[j - 1]
            {
                m = m.min(add(d[i - 2][j - 2], (1, 1, 0)));
            }
            d[i][j] = m;
        }
    }
    d[a.len()][b.len()]
}
