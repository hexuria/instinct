//! Property tests for rules (spec §4.4, §5.2; plan T6).
//!
//! - Scores are invariant under case, whitespace width and NFC/NFD (two renderings of one
//!   abstract message).
//! - A cue inside a protected span never changes scores (swap a protected cue for a protected
//!   non-cue of the same token count).
//! - Sentence locality, the critical-set property in its checkable form: every effect is
//!   sentence-local, so for `max` classes `score(A. B) = max(score(A), score(B))` and for
//!   `sum` classes it is the saturating sum. Edits in other sentences cannot move a `max`
//!   winner.
//! - Rule and negator order in the spec don't matter.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)]

use instinct_core::{Confidence, ScorerKind};
use instinct_rules::{ClassSpec, Repairs, RuleSet, RuleSetSpec, RuleSpec};
use instinct_text::{NormalizeConfig, normalize};
use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization as _;

fn rule(id: &str, class: &str, pattern: &str, w: i16) -> RuleSpec {
    RuleSpec {
        id: id.into(),
        class: class.into(),
        pattern: pattern.into(),
        weight_millis: w,
        requires: vec![],
        forbids: vec!["negation".into()],
        version: 1,
    }
}

fn spec() -> RuleSetSpec {
    RuleSetSpec {
        classes: vec![
            ClassSpec {
                name: "interrupt".into(),
                scorer: ScorerKind::Max,
            },
            ClassSpec {
                name: "steer".into(),
                scorer: ScorerKind::Max,
            },
            ClassSpec {
                name: "queue".into(),
                scorer: ScorerKind::Sum,
            },
        ],
        negators: vec![
            "don't".into(),
            "do not".into(),
            "no need to".into(),
            "never".into(),
        ],
        negation_window: 3,
        rules: vec![
            rule("interrupt.stop.v1", "interrupt", "stop", 700),
            rule("interrupt.cancel.v1", "interrupt", "cancel", 720),
            rule("interrupt.never_mind.v1", "interrupt", "never mind", 750),
            rule("steer.stop_gerund.v1", "steer", "stop {gerund}", 650),
            rule("steer.switch_to.v1", "steer", "switch to {word}", 600),
            rule("steer.instead.v1", "steer", "instead", 500),
            rule("steer.peña.v1", "steer", "peña", 450),
            rule("queue.also.v1", "queue", "also", 300),
            rule("queue.after_that.v1", "queue", "after that", 400),
        ],
    }
}

const WORDS: &[&str] = &[
    "stop",
    "cancel",
    "never",
    "mind",
    "using",
    "switch",
    "to",
    "main",
    "instead",
    "also",
    "after",
    "that",
    "don't",
    "do",
    "not",
    "no",
    "need",
    "please",
    "the",
    "build",
    "peña",
    "café",
    "running",
    "\u{455}top",
];

#[derive(Debug, Clone)]
enum Piece {
    Word(&'static str),
    /// A protected span holding one word: `cue` or `filler` depending on the variant.
    Protected,
}

fn message() -> impl Strategy<Value = Vec<(Piece, &'static str)>> {
    let piece = prop_oneof![
        6 => proptest::sample::select(WORDS).prop_map(Piece::Word),
        1 => Just(Piece::Protected),
    ];
    let sep = proptest::sample::select(&[" ", " ", " ", ", ", ". ", "? ", "! "][..]);
    proptest::collection::vec((piece, sep), 0..14)
}

fn render(msg: &[(Piece, &str)], style: &[u8], protected_word: &str) -> String {
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
            Piece::Protected => {
                out.push('`');
                out.push_str(protected_word);
                out.push('`');
            }
        }
        out.push_str(sep);
        out.push_str(&" ".repeat(usize::from(bits.next().unwrap_or(0) % 3)));
    }
    out
}

fn scores(rs: &RuleSet, text: &str) -> Vec<Confidence> {
    let n = normalize(text, rs.config()).unwrap();
    rs.score(&n, &Repairs::none()).unwrap().scores().to_vec()
}

fn rules() -> RuleSet {
    RuleSet::new(&spec(), NormalizeConfig::default()).unwrap()
}

proptest! {
    #[test]
    fn invariant_under_case_space_nfd(
        msg in message(),
        a in proptest::collection::vec(any::<u8>(), 1..64),
        b in proptest::collection::vec(any::<u8>(), 1..64),
    ) {
        let rs = rules();
        let x = render(&msg, &a, "stop");
        let y = render(&msg, &b, "stop");
        prop_assert_eq!(scores(&rs, &x), scores(&rs, &y), "{:?} vs {:?}", x, y);
    }

    #[test]
    fn protected_cues_never_change_scores(
        msg in message(),
        style in proptest::collection::vec(any::<u8>(), 1..64),
        cue in proptest::sample::select(&["stop", "cancel", "instead", "also"][..]),
    ) {
        let rs = rules();
        let with_cue = render(&msg, &style, cue);
        let filler = render(&msg, &style, "tops");
        prop_assert_eq!(scores(&rs, &with_cue), scores(&rs, &filler));
    }

    #[test]
    fn sentence_locality(
        a in message(),
        b in message(),
        sa in proptest::collection::vec(any::<u8>(), 1..32),
        sb in proptest::collection::vec(any::<u8>(), 1..32),
    ) {
        let rs = rules();
        let x = render(&a, &sa, "stop");
        let y = render(&b, &sb, "stop");
        // A full stop + space always ends a sentence; neither part can end in a `?` that
        // would carry over, because the `.` is the terminator here.
        let joined = format!("{x} . {y}");
        let (sx, sy, sj) = (scores(&rs, &format!("{x} .")), scores(&rs, &y), scores(&rs, &joined));
        for (k, scorer) in [ScorerKind::Max, ScorerKind::Max, ScorerKind::Sum].iter().enumerate() {
            let expect = match scorer {
                ScorerKind::Max => sx[k].max(sy[k]),
                ScorerKind::Sum => sx[k].saturating_add(sy[k]),
            };
            prop_assert_eq!(sj[k], expect, "class {} in {:?}", k, joined);
        }
    }

    #[test]
    fn spec_order_does_not_matter(
        msg in message(),
        style in proptest::collection::vec(any::<u8>(), 1..64),
        rule_order in Just((0..9).collect::<Vec<usize>>()).prop_shuffle(),
        neg_order in Just((0..4).collect::<Vec<usize>>()).prop_shuffle(),
    ) {
        let base = spec();
        let mut shuffled = base.clone();
        shuffled.rules = rule_order.iter().map(|&i| base.rules[i].clone()).collect();
        shuffled.negators = neg_order.iter().map(|&i| base.negators[i].clone()).collect();
        let a = RuleSet::new(&base, NormalizeConfig::default()).unwrap();
        let b = RuleSet::new(&shuffled, NormalizeConfig::default()).unwrap();
        prop_assert_eq!(&a, &b);
        let x = render(&msg, &style, "stop");
        let n = normalize(&x, a.config()).unwrap();
        prop_assert_eq!(a.score(&n, &Repairs::none()).unwrap(), b.score(&n, &Repairs::none()).unwrap());
    }

    #[test]
    fn never_panics_and_spans_slice(text in "\\PC{0,200}") {
        let rs = rules();
        let n = normalize(&text, rs.config()).unwrap();
        let out = rs.score(&n, &Repairs::none()).unwrap();
        for m in out.matches() {
            prop_assert!(m.original().slice(&text).is_some());
            prop_assert!(m.effective() <= Confidence::new(1000).unwrap());
        }
    }
}
