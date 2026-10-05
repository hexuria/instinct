use super::*;

fn conf(v: i16) -> Confidence {
    Confidence::new(v).unwrap()
}

fn typos() -> Repair {
    Repair::Typos {
        penalty_per_edit: conf(150),
    }
}

fn spec(entries: &[(&str, &str)]) -> LexiconSpec {
    LexiconSpec {
        entries: entries.iter().map(|(t, g)| EntrySpec::new(t, g)).collect(),
        guards: vec![],
        repair: typos(),
    }
}

fn build(entries: &[(&str, &str)]) -> Lexicon {
    Lexicon::new(&spec(entries), NormalizeConfig::default()).unwrap()
}

fn err(s: &LexiconSpec) -> LexiconError {
    Lexicon::new(s, NormalizeConfig::default()).unwrap_err()
}

/// (term, matched original text, kind) per hit.
fn scan(lex: &Lexicon, text: &str) -> Vec<(String, String, MatchKind)> {
    let n = normalize(text, lex.config()).unwrap();
    lex.lookup(&n)
        .unwrap()
        .hits()
        .iter()
        .map(|h| {
            (
                lex.term(h.entry()).to_owned(),
                h.original().slice(text).unwrap().to_owned(),
                h.kind(),
            )
        })
        .collect()
}

fn rep(edits: u8) -> MatchKind {
    MatchKind::Repaired {
        edits,
        penalty: conf(150 * i16::from(edits)),
    }
}

#[test]
fn spec_errors_are_exact() {
    assert_eq!(err(&spec(&[])), LexiconError::NoEntries);
    assert_eq!(
        err(&spec(&[("", "x")])),
        LexiconError::EmptyTerm { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("stop", "")])),
        LexiconError::EmptyTag { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("stop", "a"), ("Stop", "a")])),
        LexiconError::NonCanonicalTerm {
            index: 1,
            expected: "stop".into()
        }
    );
    assert_eq!(
        err(&spec(&[("wait, no", "a")])),
        LexiconError::NonCanonicalTerm {
            index: 0,
            expected: "wait no".into()
        }
    );
    assert_eq!(
        err(&spec(&[("never  mind", "a")])),
        LexiconError::NonCanonicalTerm {
            index: 0,
            expected: "never mind".into()
        }
    );
    assert_eq!(
        err(&spec(&[("cafe\u{301}", "a")])),
        LexiconError::NonCanonicalTerm {
            index: 0,
            expected: "café".into()
        }
    );
    assert_eq!(
        err(&spec(&[("!!!", "a")])),
        LexiconError::UnmatchableTerm { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("v1.2", "a")])),
        LexiconError::UnmatchableTerm { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("`stop`", "a")])),
        LexiconError::UnmatchableTerm { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("\u{455}top", "a")])),
        LexiconError::UnmatchableTerm { index: 0 }
    );
    assert_eq!(
        err(&spec(&[("a b c d", "a")])),
        LexiconError::TooManyTokens {
            index: 0,
            tokens: 4
        }
    );
    assert_eq!(
        err(&spec(&[("stop", "a"), ("halt", "b"), ("stop", "c")])),
        LexiconError::DuplicateTerm {
            index: 2,
            term: "stop".into()
        }
    );
    let mut s = spec(&[("tin", "a")]);
    s.entries[0].substring = true;
    assert_eq!(err(&s), LexiconError::SubstringTooShort { index: 0 });
    let mut s = spec(&[("never mind", "a")]);
    s.entries[0].substring = true;
    assert_eq!(err(&s), LexiconError::SubstringMultiToken { index: 0 });
    let mut s = spec(&[("ab cd", "a")]);
    s.entries[0].substring = true;
    assert_eq!(err(&s), LexiconError::SubstringMultiToken { index: 0 });
}

#[test]
fn guard_errors_are_exact() {
    let mut s = spec(&[("stop", "a")]);
    s.guards = vec!["top".into(), "Top".into()];
    assert_eq!(err(&s), LexiconError::NonCanonicalGuard { index: 1 });
    s.guards = vec!["two words".into()];
    assert_eq!(err(&s), LexiconError::NonCanonicalGuard { index: 0 });
    s.guards = vec![String::new()];
    assert_eq!(err(&s), LexiconError::NonCanonicalGuard { index: 0 });
    s.guards = vec!["top".into(), "top".into()];
    assert_eq!(err(&s), LexiconError::DuplicateGuard { index: 1 });
    s.guards = vec!["stop".into()];
    assert_eq!(err(&s), LexiconError::GuardIsTerm { index: 0 });
}

#[test]
fn error_messages() {
    let cases: [(LexiconError, &str); 12] = [
        (LexiconError::NoEntries, "lexicon has no entries"),
        (LexiconError::EmptyTerm { index: 1 }, "entry 1: empty term"),
        (LexiconError::EmptyTag { index: 1 }, "entry 1: empty tag"),
        (
            LexiconError::NonCanonicalTerm {
                index: 0,
                expected: "stop".into(),
            },
            "entry 0: term is not canonical, write \"stop\"",
        ),
        (
            LexiconError::UnmatchableTerm { index: 0 },
            "entry 0: term has no free, non-confusable word tokens and could never match",
        ),
        (
            LexiconError::TooManyTokens {
                index: 0,
                tokens: 4,
            },
            "entry 0: term has 4 tokens, at most 3 allowed",
        ),
        (
            LexiconError::SubstringTooShort { index: 0 },
            "entry 0: substring matching needs at least 5 chars",
        ),
        (
            LexiconError::SubstringMultiToken { index: 0 },
            "entry 0: substring matching is single-token only",
        ),
        (
            LexiconError::DuplicateTerm {
                index: 2,
                term: "x".into(),
            },
            "entry 2: duplicate term \"x\"",
        ),
        (
            LexiconError::NonCanonicalGuard { index: 0 },
            "guard 0: not a single canonical free token",
        ),
        (
            LexiconError::DuplicateGuard { index: 0 },
            "guard 0: duplicate",
        ),
        (
            LexiconError::GuardIsTerm { index: 0 },
            "guard 0: is also a term",
        ),
    ];
    for (e, msg) in cases {
        assert_eq!(e.to_string(), msg);
    }
}

#[test]
fn lookup_refuses_a_different_config() {
    let lex = build(&[("stop", "a")]);
    let other = NormalizeConfig {
        punct_runs: pua_text::PunctRuns::Collapse,
        ..NormalizeConfig::default()
    };
    let n = normalize("stop", other).unwrap();
    let e = lex.lookup(&n).unwrap_err();
    assert_eq!(
        e,
        LookupError::ConfigMismatch {
            lexicon: NormalizeConfig::default(),
            text: other
        }
    );
    assert!(e.to_string().contains("punct=collapse"));
}

#[test]
fn exact_is_token_boundary_and_longest() {
    let lex = build(&[
        ("stop", "i"),
        ("never mind", "i"),
        ("never", "x"),
        ("a b c", "y"),
    ]);
    assert_eq!(
        scan(&lex, "Please STOP. Never mind!"),
        [
            ("stop".into(), "STOP".into(), MatchKind::Exact),
            ("never mind".into(), "Never mind".into(), MatchKind::Exact)
        ]
    );
    // Not a prefix, not inside a token.
    assert_eq!(scan(&lex, "stopwatch nonstop"), []);
    // Adjacent tokens across a comma still form the term; across a sentence they don't.
    assert_eq!(scan(&lex, "never, mind")[0].0, "never mind");
    assert_eq!(
        scan(&lex, "never. mind"),
        [("never".into(), "never".into(), MatchKind::Exact)]
    );
    // Three-token term, and falling back to a shorter one.
    assert_eq!(scan(&lex, "A b C")[0].0, "a b c");
    assert_eq!(scan(&lex, "never mindful")[0].0, "never");
}

#[test]
fn protected_tokens_never_match_or_repair() {
    let lex = build(&[("stop", "i"), ("never mind", "i")]);
    assert_eq!(
        scan(
            &lex,
            "`stop` \"stop\" ```\nstop\n``` http://x/stop /usr/stop"
        ),
        []
    );
    assert_eq!(scan(&lex, "never `x` mind"), []);
    assert_eq!(scan(&lex, "`stpo`"), []);
}

#[test]
fn confusables_flag_but_never_hit() {
    let lex = build(&[("stop", "i")]);
    let text = "\u{455}top now";
    let n = normalize(text, lex.config()).unwrap();
    let found = lex.lookup(&n).unwrap();
    assert_eq!(found.hits(), []);
    let f = &found.flags()[0];
    assert_eq!(lex.term(f.entry()), "stop");
    assert_eq!(f.token(), 0);
    assert_eq!(f.original().slice(text), Some("\u{455}top"));
    assert_eq!(f.kind(), Confusable::MixedScript);
    // Not repaired either, even one edit away from a term.
    assert_eq!(scan(&lex, "\u{455}tpo"), []);
}

#[test]
fn repair_distance_bounds() {
    let lex = build(&[("stop", "i"), ("cancel", "i"), ("halt", "i")]);
    // ≤ 4-char terms: distance 1 only.
    assert_eq!(scan(&lex, "stpo"), [("stop".into(), "stpo".into(), rep(1))]);
    assert_eq!(scan(&lex, "sotp")[0].2, rep(1));
    assert_eq!(scan(&lex, "xtpo"), []);
    // Longer terms: distance 2.
    assert_eq!(
        scan(&lex, "cancle"),
        [("cancel".into(), "cancle".into(), rep(1))]
    );
    assert_eq!(
        scan(&lex, "cnacle"),
        [("cancel".into(), "cnacle".into(), rep(2))]
    );
    assert_eq!(scan(&lex, "xxncle"), []);
    // Tokens under 4 chars are never repaired (here: 1 edit from "stop").
    assert_eq!(scan(&lex, "sto"), []);
    // Very long tokens are skipped outright.
    assert_eq!(scan(&lex, &"s".repeat(50_000)), []);
}

#[test]
fn guard_words_are_never_repaired() {
    let mut s = spec(&[("stop", "i"), ("halts", "i")]);
    s.guards = vec!["step".into(), "halls".into()];
    let lex = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_eq!(scan(&lex, "step halls"), []);
    assert_eq!(scan(&lex, "stoop")[0].0, "stop");
}

#[test]
fn repair_off_matches_exactly_only() {
    let mut s = spec(&[("stop", "i")]);
    s.repair = Repair::Off;
    let lex = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_eq!(scan(&lex, "stpo stop")[0].1, "stop");
    assert_eq!(scan(&lex, "stpo stop").len(), 1);
}

#[test]
fn repair_tie_breaks() {
    // d's QWERTY neighbours are s, e, r, f, c, x.
    let lex = build(&[("tesf", "adjacent"), ("tesp", "far")]);
    // "tesd" → tesf (d→f adjacent) vs tesp (d→p not adjacent): adjacency wins.
    assert_eq!(scan(&lex, "tesd")[0].0, "tesf");
    // Adjacency beats transposition.
    let lex = build(&[("abdc", "transposed"), ("abcf", "adjacent")]);
    // "abcd": abdc by transposition (cd↔dc), abcf by d→f (adjacent). Adjacent wins.
    assert_eq!(scan(&lex, "abcd")[0].0, "abcf");
    // Transposition beats a non-adjacent substitution.
    let lex = build(&[("abdc", "transposed"), ("abcp", "far")]);
    assert_eq!(scan(&lex, "abcd")[0].0, "abdc");
    // Full tie (two non-adjacent substitutions): lexical order.
    let lex = build(&[("abcz", "z"), ("abcp", "p")]);
    assert_eq!(scan(&lex, "abcm")[0].0, "abcp");
    // Fewer edits always beat tie-break preferences.
    let lex = build(&[("abcdef", "one"), ("abcfeg", "two")]);
    assert_eq!(scan(&lex, "abcdeg")[0].0, "abcdef");
}

#[test]
fn substring_is_opt_in_and_longest() {
    let mut s = spec(&[("steer", "s"), ("autosteer", "a"), ("stop", "i")]);
    s.entries[0].substring = true;
    s.entries[1].substring = true;
    s.repair = Repair::Off;
    let lex = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_eq!(
        scan(&lex, "oversteering"),
        [("steer".into(), "oversteering".into(), MatchKind::Substring)]
    );
    assert_eq!(scan(&lex, "myautosteerx")[0].0, "autosteer");
    // A whole-token occurrence is exact, not substring.
    assert_eq!(scan(&lex, "steer")[0].2, MatchKind::Exact);
    // "stop" did not opt in.
    assert_eq!(scan(&lex, "nonstop"), []);
}

#[test]
fn accessors_and_fingerprint() {
    let lex = build(&[("stop", "i"), ("cancel", "c")]);
    assert_eq!(lex.len(), 2);
    assert!(!lex.is_empty());
    let id = lex.find("stop").unwrap();
    assert_eq!(id.get(), 1, "entries are sorted by term");
    assert_eq!(lex.tag(id), "i");
    assert_eq!(lex.term(EntryId(99)), "");
    assert_eq!(lex.tag(EntryId(99)), "");
    assert_eq!(lex.find("nope"), None);
    let shuffled = build(&[("cancel", "c"), ("stop", "i")]);
    assert_eq!(lex, shuffled);
    assert_eq!(lex.fingerprint(), shuffled.fingerprint());
    assert_ne!(
        lex.fingerprint(),
        build(&[("stop", "x"), ("cancel", "c")]).fingerprint()
    );
    let mut s = spec(&[("stop", "i"), ("cancel", "c")]);
    s.repair = Repair::Off;
    let off = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_ne!(lex.fingerprint(), off.fingerprint());
    s.guards = vec!["step".into()];
    let guarded = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_ne!(off.fingerprint(), guarded.fingerprint());
    s.entries[1].substring = true;
    let sub = Lexicon::new(&s, NormalizeConfig::default()).unwrap();
    assert_ne!(sub.fingerprint(), guarded.fingerprint());
}

#[test]
fn hit_token_ranges() {
    let lex = build(&[("never mind", "i")]);
    let n = normalize("ok never mind", lex.config()).unwrap();
    let found = lex.lookup(&n).unwrap();
    assert_eq!(found.hits()[0].tokens(), 1..3);
}
