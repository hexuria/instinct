use super::*;
use pua_lexicon::{EntrySpec, LexiconSpec, Repair};

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

fn class(name: &str, scorer: ScorerKind) -> ClassSpec {
    ClassSpec {
        name: name.into(),
        scorer,
    }
}

fn base() -> RuleSetSpec {
    RuleSetSpec {
        classes: vec![
            class("interrupt", ScorerKind::Max),
            class("steer", ScorerKind::Max),
            class("queue", ScorerKind::Sum),
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
            rule("interrupt.never_mind.v1", "interrupt", "never mind", 750),
            rule("steer.stop_gerund.v1", "steer", "stop {gerund}", 650),
            rule("steer.switch_to.v1", "steer", "switch to {word}", 600),
            rule("steer.instead.v1", "steer", "instead", 500),
            rule("queue.also.v1", "queue", "also", 300),
            rule("queue.after_that.v1", "queue", "after that", 400),
        ],
    }
}

fn set(spec: &RuleSetSpec) -> RuleSet {
    RuleSet::new(spec, NormalizeConfig::default()).unwrap()
}

fn err(spec: &RuleSetSpec) -> RulesError {
    RuleSet::new(spec, NormalizeConfig::default()).unwrap_err()
}

fn scores(rs: &RuleSet, text: &str) -> Vec<i16> {
    let n = normalize(text, rs.config()).unwrap();
    rs.score(&n, &Repairs::none())
        .unwrap()
        .scores()
        .iter()
        .map(|c| c.get())
        .collect()
}

fn with(f: impl FnOnce(&mut RuleSetSpec)) -> RuleSetSpec {
    let mut s = base();
    f(&mut s);
    s
}

#[test]
fn class_and_set_errors_are_exact() {
    assert_eq!(err(&with(|s| s.classes.clear())), RulesError::NoClasses);
    assert_eq!(
        err(&with(|s| s.classes[1].name = String::new())),
        RulesError::EmptyClassName { index: 1 }
    );
    assert_eq!(
        err(&with(|s| s.classes[2].name = "steer".into())),
        RulesError::DuplicateClass {
            name: "steer".into()
        }
    );
    assert_eq!(err(&with(|s| s.rules.clear())), RulesError::NoRules);
    assert_eq!(
        err(&with(|s| s.negation_window = 0)),
        RulesError::NegationWindowOutOfRange { window: 0 }
    );
    assert_eq!(
        err(&with(|s| s.negation_window = 9)),
        RulesError::NegationWindowOutOfRange { window: 9 }
    );
    assert!(RuleSet::new(&with(|s| s.negation_window = 8), NormalizeConfig::default()).is_ok());
    assert!(RuleSet::new(&with(|s| s.negation_window = 1), NormalizeConfig::default()).is_ok());
}

#[test]
fn negator_errors_are_exact() {
    for bad in ["", "Don't", "do  not", "a b c d", "`no`", "!!"] {
        assert_eq!(
            err(&with(|s| s.negators.push(bad.into()))),
            RulesError::NonCanonicalNegator { index: 4 },
            "{bad:?}"
        );
    }
    assert_eq!(
        err(&with(|s| s.negators.push("never".into()))),
        RulesError::DuplicateNegator { index: 4 }
    );
}

#[test]
fn rule_errors_are_exact() {
    let id = || "r".to_owned();
    let one = |r: RuleSpec| with(move |s| s.rules = vec![r]);
    assert_eq!(
        err(&one(rule("", "steer", "x", 1))),
        RulesError::EmptyRuleId { index: 0 }
    );
    assert_eq!(
        err(&with(|s| s.rules.push(rule(
            "queue.also.v1",
            "queue",
            "x",
            1
        )))),
        RulesError::DuplicateRuleId {
            id: "queue.also.v1".into()
        }
    );
    assert_eq!(
        err(&one(rule("r", "nope", "x", 1))),
        RulesError::UnknownClass {
            id: id(),
            class: "nope".into()
        }
    );
    for w in [0, -1, 1001] {
        assert_eq!(
            err(&one(rule("r", "steer", "x", w))),
            RulesError::WeightOutOfRange {
                id: id(),
                weight: w
            }
        );
    }
    assert!(
        RuleSet::new(
            &one(rule("r", "steer", "x", 1000)),
            NormalizeConfig::default()
        )
        .is_ok()
    );
    assert!(RuleSet::new(&one(rule("r", "steer", "x", 1)), NormalizeConfig::default()).is_ok());
    let mut r = rule("r", "steer", "x", 1);
    r.version = 0;
    assert_eq!(err(&one(r)), RulesError::ZeroVersion { id: id() });
    assert_eq!(
        err(&one(rule("r", "steer", "", 1))),
        RulesError::EmptyPattern { id: id() }
    );
    assert_eq!(
        err(&one(rule("r", "steer", "a b c d e f g", 1))),
        RulesError::PatternTooLong { id: id(), items: 7 }
    );
    assert!(
        RuleSet::new(
            &one(rule("r", "steer", "a b c d e f", 1)),
            NormalizeConfig::default()
        )
        .is_ok()
    );
    assert_eq!(
        err(&one(rule("r", "steer", "{word} {gerund}", 1))),
        RulesError::NoLiteral { id: id() }
    );
    for slot in ["{verb}", "{word", "word}", "{}"] {
        assert_eq!(
            err(&one(rule("r", "steer", &format!("stop {slot}"), 1))),
            RulesError::UnknownSlot {
                id: id(),
                slot: slot.into()
            }
        );
    }
    for lit in [
        "Stop",
        "sto+p",
        "st*p",
        "",
        "v1.2",
        "\u{455}top",
        "a,b",
        "(stop)",
    ] {
        assert_eq!(
            err(&one(rule("r", "steer", &format!("x {lit}"), 1))),
            RulesError::NonCanonicalLiteral {
                id: id(),
                literal: lit.into()
            },
            "{lit:?}"
        );
    }
    let mut r = rule("r", "steer", "x", 1);
    r.requires = vec!["urgent".into()];
    assert_eq!(
        err(&one(r)),
        RulesError::UnknownContext {
            id: id(),
            context: "urgent".into()
        }
    );
    let mut r = rule("r", "steer", "x", 1);
    r.forbids = vec!["protected_span".into()];
    assert_eq!(
        err(&one(r)),
        RulesError::UnknownContext {
            id: id(),
            context: "protected_span".into()
        }
    );
    let mut r = rule("r", "steer", "x", 1);
    r.requires = vec!["negation".into()];
    assert_eq!(
        err(&one(r)),
        RulesError::ContextConflict {
            id: id(),
            context: "negation".into()
        }
    );
    let mut r = rule("r", "steer", "x", 1);
    r.requires = vec!["question".into()];
    r.forbids = vec!["question".into()];
    assert_eq!(
        err(&one(r)),
        RulesError::ContextConflict {
            id: id(),
            context: "question".into()
        }
    );
    let mut r = rule("r", "steer", "x", 1);
    r.forbids = vec!["negation".into(), "negation".into()];
    assert_eq!(
        err(&one(r)),
        RulesError::DuplicateContext {
            id: id(),
            context: "negation".into()
        }
    );
}

#[test]
fn error_messages_name_the_problem() {
    let id = "r".to_owned();
    let cases = [
        (RulesError::NoClasses, "rule set declares no classes"),
        (
            RulesError::EmptyClassName { index: 2 },
            "class 2: empty name",
        ),
        (
            RulesError::DuplicateClass { name: "a".into() },
            "duplicate class \"a\"",
        ),
        (RulesError::NoRules, "rule set has no rules"),
        (RulesError::EmptyRuleId { index: 1 }, "rule 1: empty id"),
        (
            RulesError::DuplicateRuleId { id: id.clone() },
            "duplicate rule id \"r\"",
        ),
        (
            RulesError::UnknownClass {
                id: id.clone(),
                class: "c".into(),
            },
            "rule \"r\": unknown class \"c\"",
        ),
        (
            RulesError::WeightOutOfRange {
                id: id.clone(),
                weight: 0,
            },
            "rule \"r\": weight 0 outside 1..=1000",
        ),
        (
            RulesError::ZeroVersion { id: id.clone() },
            "rule \"r\": version must be >= 1",
        ),
        (
            RulesError::EmptyPattern { id: id.clone() },
            "rule \"r\": empty pattern",
        ),
        (
            RulesError::PatternTooLong {
                id: id.clone(),
                items: 7,
            },
            "rule \"r\": pattern has 7 items, at most 6 allowed",
        ),
        (
            RulesError::NoLiteral { id: id.clone() },
            "rule \"r\": pattern needs a literal token",
        ),
        (
            RulesError::UnknownSlot {
                id: id.clone(),
                slot: "{x}".into(),
            },
            "rule \"r\": unknown slot \"{x}\" (use {word} or {gerund})",
        ),
        (
            RulesError::NonCanonicalLiteral {
                id: id.clone(),
                literal: "A".into(),
            },
            "rule \"r\": literal \"A\" is not one canonical word token",
        ),
        (
            RulesError::UnknownContext {
                id: id.clone(),
                context: "x".into(),
            },
            "rule \"r\": unknown context \"x\" (use negation or question)",
        ),
        (
            RulesError::ContextConflict {
                id: id.clone(),
                context: "question".into(),
            },
            "rule \"r\": context \"question\" is both required and forbidden",
        ),
        (
            RulesError::DuplicateContext {
                id,
                context: "question".into(),
            },
            "rule \"r\": context \"question\" listed twice",
        ),
        (
            RulesError::NonCanonicalNegator { index: 0 },
            "negator 0: not 1..=3 canonical word tokens",
        ),
        (
            RulesError::DuplicateNegator { index: 0 },
            "negator 0: duplicate",
        ),
        (
            RulesError::NegationWindowOutOfRange { window: 9 },
            "negation window 9 outside 1..=8",
        ),
    ];
    for (e, msg) in cases {
        assert_eq!(e.to_string(), msg);
    }
}

#[test]
fn score_refuses_a_different_config() {
    let rs = set(&base());
    let other = NormalizeConfig {
        fold: pua_text::Fold::AsciiLower,
        ..NormalizeConfig::default()
    };
    let n = normalize("stop", other).unwrap();
    let e = rs.score(&n, &Repairs::none()).unwrap_err();
    assert_eq!(
        e,
        ScoreError::ConfigMismatch {
            rules: NormalizeConfig::default(),
            text: other
        }
    );
    assert!(e.to_string().contains("fold=ascii"));
}

#[test]
fn token_boundary_and_protected_spans() {
    let rs = set(&base());
    assert_eq!(scores(&rs, "STOP"), [700, 0, 0]);
    assert_eq!(scores(&rs, "stopwatch nonstop stops"), [0, 0, 0]);
    assert_eq!(scores(&rs, "the `stop` command is broken"), [0, 0, 0]);
    assert_eq!(
        scores(&rs, "\"stop\" 'x' /bin/stop http://a/stop"),
        [0, 0, 0]
    );
    assert_eq!(scores(&rs, "```\nstop\n```"), [0, 0, 0]);
    assert_eq!(
        scores(&rs, "\u{455}top"),
        [0, 0, 0],
        "confusables never fire"
    );
}

#[test]
fn negation_window_and_sentences() {
    let rs = set(&base());
    assert_eq!(scores(&rs, "don't stop"), [0, 0, 0]);
    assert_eq!(scores(&rs, "do not stop"), [0, 0, 0]);
    assert_eq!(scores(&rs, "no need to stop"), [0, 0, 0]);
    assert_eq!(scores(&rs, "never stop"), [0, 0, 0]);
    // Negator exactly at the window edge (3 tokens before) still counts...
    assert_eq!(scores(&rs, "don't a b stop"), [0, 0, 0]);
    // ...one further does not.
    assert_eq!(scores(&rs, "don't a b c stop"), [700, 0, 0]);
    // Different sentence: not negated.
    assert_eq!(scores(&rs, "don't. stop"), [700, 0, 0]);
    // A negator after the cue doesn't negate it.
    assert_eq!(scores(&rs, "stop, don't"), [700, 0, 0]);
    // A rule that doesn't forbid negation isn't cancelled.
    let mut s = base();
    s.rules[0].forbids.clear();
    assert_eq!(scores(&set(&s), "don't stop"), [700, 0, 0]);
    // A rule that requires negation counts only when negated.
    s.rules[0].requires = vec!["negation".into()];
    assert_eq!(scores(&set(&s), "don't stop"), [700, 0, 0]);
    assert_eq!(scores(&set(&s), "stop"), [0, 0, 0]);
}

#[test]
fn negation_window_size_is_respected() {
    let mut s = base();
    s.negation_window = 1;
    let rs = set(&s);
    assert_eq!(scores(&rs, "don't stop"), [0, 0, 0]);
    assert_eq!(scores(&rs, "don't x stop"), [700, 0, 0]);
    // A 3-token negator needs a window of at least 3.
    assert_eq!(scores(&rs, "no need to stop"), [700, 0, 0]);
}

#[test]
fn question_damper() {
    let rs = set(&base());
    assert_eq!(scores(&rs, "should I stop?"), [350, 0, 0]);
    assert_eq!(scores(&rs, "should I stop? now"), [350, 0, 0]);
    assert_eq!(scores(&rs, "stop. is it ok?"), [700, 0, 0]);
    // A `?` inside a protected span is not a question mark.
    assert_eq!(scores(&rs, "stop `x?`"), [700, 0, 0]);
    assert_eq!(scores(&rs, "stop http://a/b?c=1"), [700, 0, 0]);
    // requires = question: counted only in questions, not halved.
    let mut s = base();
    s.rules[0].requires = vec!["question".into()];
    assert_eq!(scores(&set(&s), "stop?"), [700, 0, 0]);
    assert_eq!(scores(&set(&s), "stop"), [0, 0, 0]);
    // forbids = question: never in questions.
    let mut s = base();
    s.rules[0].forbids.push("question".into());
    assert_eq!(scores(&set(&s), "stop?"), [0, 0, 0]);
    assert_eq!(scores(&set(&s), "stop"), [700, 0, 0]);
}

#[test]
fn specificity_gives_object_scope() {
    let rs = set(&base());
    assert_eq!(scores(&rs, "stop using postgres"), [0, 650, 0]);
    // A short token ending in -ing is not a gerund.
    assert_eq!(scores(&rs, "stop ring"), [700, 0, 0]);
    assert_eq!(
        scores(&rs, "stop thing"),
        [0, 650, 0],
        "gerund is a surface heuristic"
    );
    // Token order matters: "X using stop" is not the steer pattern.
    assert_eq!(scores(&rs, "postgres using stop"), [700, 0, 0]);
    // Negating the specific match does not resurrect the general one.
    assert_eq!(scores(&rs, "don't stop using it"), [0, 0, 0]);
    // Slots need a free token.
    assert_eq!(scores(&rs, "switch to `main`"), [0, 0, 0]);
    assert_eq!(scores(&rs, "switch to main"), [0, 600, 0]);
}

#[test]
fn conflicting_classes_are_kept_and_scorers_combine() {
    let rs = set(&base());
    assert_eq!(scores(&rs, "stop. instead do x"), [700, 500, 0]);
    // Max: strongest cue; Sum: saturating sum.
    assert_eq!(scores(&rs, "stop never mind"), [750, 0, 0]);
    assert_eq!(scores(&rs, "also after that also"), [0, 0, 1000]);
    assert_eq!(scores(&rs, "also after that"), [0, 0, 700]);
}

#[test]
fn repairs_match_as_their_term_with_penalty() {
    let rs = set(&base());
    let lex = Lexicon::new(
        &LexiconSpec {
            entries: vec![EntrySpec::new("stop", "x"), EntrySpec::new("instead", "x")],
            guards: vec![],
            repair: Repair::Typos {
                penalty_per_edit: Confidence::new(150).unwrap(),
            },
        },
        NormalizeConfig::default(),
    )
    .unwrap();
    let n = normalize("pls stpo", rs.config()).unwrap();
    let rep = Repairs::from_lookup(&lex, &lex.lookup(&n).unwrap());
    let out = rs.score(&n, &rep).unwrap();
    assert_eq!(out.scores()[0].get(), 550);
    assert_eq!(out.matches()[0].repair_penalty().get(), 150);
    // Without repairs, nothing.
    assert_eq!(
        rs.score(&n, &Repairs::none()).unwrap().scores()[0],
        Confidence::ZERO
    );
    // Negators are matched literally, never through repairs.
    let n = normalize("dont stpo", rs.config()).unwrap();
    let rep = Repairs::from_lookup(&lex, &lex.lookup(&n).unwrap());
    assert_eq!(rs.score(&n, &rep).unwrap().scores()[0].get(), 550);
}

#[test]
fn matches_trail_and_accessors() {
    let rs = set(&base());
    let text = "Stop using pg. stop?";
    let n = normalize(text, rs.config()).unwrap();
    let out = rs.score(&n, &Repairs::none()).unwrap();
    let m: Vec<_> = out
        .matches()
        .iter()
        .map(|m| {
            (
                rs.rule_id(m.rule()),
                m.status(),
                m.original().slice(text).unwrap(),
                m.effective().get(),
                m.question(),
                m.tokens(),
            )
        })
        .collect();
    assert_eq!(
        m,
        [
            (
                "interrupt.stop.v1",
                MatchStatus::Suppressed,
                "Stop",
                0,
                false,
                0..1
            ),
            (
                "steer.stop_gerund.v1",
                MatchStatus::Counted,
                "Stop using",
                650,
                false,
                0..2
            ),
            (
                "interrupt.stop.v1",
                MatchStatus::Counted,
                "stop",
                350,
                true,
                3..4
            ),
        ]
    );
    let interrupt = rs.class_id("interrupt").unwrap();
    assert_eq!(out.score(interrupt).get(), 350);
    assert_eq!(out.score(ClassId(9)), Confidence::ZERO);
    assert_eq!(rs.class_name(interrupt), "interrupt");
    assert_eq!(rs.class_name(ClassId(9)), "");
    assert_eq!(rs.scorer(ClassId(2)), Some(ScorerKind::Sum));
    assert_eq!(rs.scorer(ClassId(9)), None);
    assert_eq!(rs.class_count(), 3);
    assert_eq!(rs.class_id("nope"), None);
    assert_eq!(rs.rule_id(RuleRef(99)), "");
    assert_eq!(out.matches()[0].class(), interrupt);
    let trail = rs.trail(&out);
    assert_eq!(trail.len(), 3);
    assert_eq!(trail[0].text(), "suppressed interrupt");
    assert_eq!(trail[2].text(), "counted interrupt (question)");
    assert_eq!(trail[1].rule_id(), Some("steer.stop_gerund.v1"));
    assert_eq!(trail[1].contribution().get(), 650);
    assert_eq!(trail[1].scorer_kind(), Some(ScorerKind::Max));
    assert_eq!(trail[1].span_ref().unwrap().slice(text), Some("Stop using"));
    assert_eq!(MatchStatus::Negated.name(), "negated");
    assert_eq!(MatchStatus::ContextUnmet.name(), "context-unmet");
    assert_eq!(MatchStatus::Counted.name(), "counted");
    assert_eq!(ClassId(2).index(), 2);
}

#[test]
fn fingerprint_tracks_every_field_and_ignores_order() {
    let a = set(&base());
    let shuffled = set(&with(|s| {
        s.rules.reverse();
        s.negators.reverse();
    }));
    assert_eq!(a, shuffled);
    assert_eq!(a.fingerprint(), shuffled.fingerprint());
    let changes: [fn(&mut RuleSetSpec); 9] = [
        |s| s.rules[0].weight_millis = 701,
        |s| s.rules[0].version = 2,
        |s| s.rules[0].pattern = "halt".into(),
        |s| s.rules[0].class = "steer".into(),
        |s| s.rules[0].forbids.clear(),
        |s| s.rules[0].requires.push("question".into()),
        |s| {
            s.negators.pop();
        },
        |s| s.negation_window = 4,
        |s| s.classes[2].scorer = ScorerKind::Max,
    ];
    for (i, f) in changes.iter().enumerate() {
        assert_ne!(set(&with(f)).fingerprint(), a.fingerprint(), "change {i}");
    }
    let slots = set(&with(|s| s.rules[3].pattern = "switch to {gerund}".into()));
    assert_ne!(slots.fingerprint(), a.fingerprint());
}
