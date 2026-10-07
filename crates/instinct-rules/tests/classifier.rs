#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
//! `RuleClassifier` against the labelled `delivery` fixture: golden replay, eval gate, errors,
//! confusable policy, determinism and option-order behaviour.

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use instinct_core::{AbstainReason, Answer, Profile, QuestionError};
use instinct_explain::{ReplayCheck, ReplayRecord};
use instinct_lexicon::{LexiconError, LexiconSpec};
use instinct_rules::{
    ClassifierError, ClassifierSpec, OnConfusable, RuleClassifier, RuleSetSpec, RulesError,
};
use instinct_text::{Fold, NormalizeConfig, PunctRuns};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

const DOMAIN: &str = "pua-steer/1";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/delivery")
        .join(name)
}

fn spec(on_confusable: OnConfusable) -> ClassifierSpec {
    let lexicon: LexiconSpec =
        toml::from_str(&fs::read_to_string(fixture("lexicon.toml")).unwrap()).unwrap();
    let rules: RuleSetSpec =
        toml::from_str(&fs::read_to_string(fixture("rules.toml")).unwrap()).unwrap();
    ClassifierSpec {
        domain: DOMAIN.into(),
        question: "delivery".into(),
        normalize: NormalizeConfig {
            fold: Fold::UnicodeLower,
            punct_runs: PunctRuns::Collapse,
        },
        lexicon,
        rules,
        on_confusable,
    }
}

fn classifier() -> &'static RuleClassifier {
    static C: OnceLock<RuleClassifier> = OnceLock::new();
    C.get_or_init(|| RuleClassifier::new(&spec(OnConfusable::Abstain)).unwrap())
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalInput {
    message: String,
    profile: String,
}

fn profile_of(s: &str) -> Profile {
    match s {
        "fast" => Profile::Fast,
        "deep" => Profile::Deep,
        _ => Profile::Standard,
    }
}

const SEEDS: [(&str, &str); 12] = [
    ("stop the build", "standard"),
    ("stpo now", "standard"),
    ("don't stop", "standard"),
    ("stop using postgres", "standard"),
    ("also add tests", "standard"),
    ("use rust instead", "standard"),
    ("run `stop` now", "standard"),
    ("\u{0455}top the build", "standard"),
    ("should I stop?", "standard"),
    ("hello there", "fast"),
    ("cancel the job", "deep"),
    ("STOP!!", "standard"),
];

#[test]
#[ignore = "writer: run explicitly to regenerate the golden journal"]
fn write_golden_journal() {
    let c = classifier();
    let mut out = String::new();
    for (msg, profile) in SEEDS {
        let input = JournalInput {
            message: msg.into(),
            profile: profile.into(),
        };
        let rec = ReplayRecord::new(input, c.decide(msg, profile_of(profile))).unwrap();
        out.push_str(&rec.to_json_line().unwrap());
        out.push('\n');
    }
    fs::write(fixture("journal.jsonl"), out).unwrap();
}

#[test]
fn golden_journal_replays_byte_identically() {
    let c = classifier();
    let body = fs::read_to_string(fixture("journal.jsonl")).unwrap();
    let mut n = 0;
    for line in body.lines().filter(|l| !l.is_empty()) {
        let record: ReplayRecord<JournalInput> = ReplayRecord::from_json_line(line).unwrap();
        let check = record
            .check(|i| c.decide(&i.message, profile_of(&i.profile)))
            .unwrap();
        assert!(
            matches!(check, ReplayCheck::Identical),
            "line {n}: {check:?}"
        );
        n += 1;
    }
    assert_eq!(n, SEEDS.len());
}

#[derive(Debug, Deserialize)]
struct Row {
    message: String,
    label: String,
}

#[test]
fn eval_fixture_has_no_false_positive_on_the_last_option() {
    // The last class of this fixture is the costly one; negated, protected and confusable cues
    // must never select it at `standard`.
    let c = classifier();
    let last = c.question().options().unwrap().len() - 1;
    let body = fs::read_to_string(fixture("eval.jsonl")).unwrap();
    let (mut n, mut fp) = (0u32, 0u32);
    for line in body.lines().filter(|l| !l.is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        n += 1;
        let d = c.decide(&row.message, Profile::Standard);
        if d.answer().chosen().map(instinct_core::OptionIndex::get) == Some(last)
            && row.label == "abstain_or_other"
        {
            fp += 1;
        }
    }
    assert!(n >= 200, "{n}");
    assert_eq!(fp, 0);
}

#[test]
fn options_are_the_rule_classes_in_order() {
    let c = classifier();
    let labels: Vec<_> = c
        .question()
        .options()
        .unwrap()
        .labels()
        .iter()
        .map(|l| l.as_str().to_owned())
        .collect();
    assert_eq!(labels, ["queue", "steer", "interrupt"]);
    assert_eq!(c.question().name().as_str(), "delivery");
    assert_eq!(
        c.label(instinct_core::OptionIndex::new(2)),
        Some("interrupt")
    );
    assert_eq!(c.label(instinct_core::OptionIndex::new(3)), None);
    assert_eq!(c.lexicon().config(), c.config());
    assert_eq!(c.rules().class_count(), 3);
}

#[test]
fn spec_errors_are_exact_and_ordered() {
    let mut s = spec(OnConfusable::Abstain);
    s.lexicon.entries.clear();
    s.rules.classes.clear();
    // Lexicon is validated first.
    assert_eq!(
        RuleClassifier::new(&s).unwrap_err(),
        ClassifierError::Lexicon(LexiconError::NoEntries)
    );
    let mut s = spec(OnConfusable::Abstain);
    s.rules.classes.clear();
    assert_eq!(
        RuleClassifier::new(&s).unwrap_err(),
        ClassifierError::Rules(RulesError::NoClasses)
    );
    // One class is not a Choice.
    let mut s = spec(OnConfusable::Abstain);
    s.rules.classes.truncate(1);
    s.rules.rules.retain(|r| r.class == "queue");
    let e = RuleClassifier::new(&s).unwrap_err();
    assert_eq!(
        e,
        ClassifierError::Question(QuestionError::TooFewOptions(1))
    );
    assert!(e.to_string().starts_with("question: "));
    assert!(
        ClassifierError::Rules(RulesError::NoClasses)
            .to_string()
            .starts_with("rules: ")
    );
    assert!(
        ClassifierError::Lexicon(LexiconError::NoEntries)
            .to_string()
            .starts_with("lexicon: ")
    );
}

#[test]
fn confusable_policy() {
    let msg = "\u{0455}top the build";
    let abstain = classifier().decide(msg, Profile::Standard);
    assert!(matches!(
        abstain.answer(),
        Answer::Abstain {
            why: AbstainReason::Confusable,
            ..
        }
    ));
    let ignore = RuleClassifier::new(&spec(OnConfusable::Ignore)).unwrap();
    assert_ne!(ignore.data_version(), classifier().data_version());
    let d = ignore.decide(msg, Profile::Standard);
    // The lookalike never fires a cue, so nothing reaches the threshold either way.
    assert!(!matches!(
        d.answer(),
        Answer::Abstain {
            why: AbstainReason::Confusable,
            ..
        }
    ));
    assert_ne!(
        d.answer().chosen().map(instinct_core::OptionIndex::get),
        Some(2)
    );
    assert!(
        d.trail()
            .records()
            .iter()
            .any(|r| r.text().starts_with("confusable"))
    );
    assert_eq!(OnConfusable::default(), OnConfusable::Abstain);
}

#[test]
fn over_long_input_is_refused_not_truncated() {
    let big = "a".repeat(instinct_text::MAX_INPUT_BYTES + 1);
    let d = classifier().decide(&big, Profile::Standard);
    assert!(matches!(
        d.answer(),
        Answer::Abstain {
            why: AbstainReason::InputTooLong,
            ..
        }
    ));
    assert_eq!(d.trail().records()[0].text(), "input refused (too long)");
}

#[test]
fn config_mismatch_is_impossible_by_construction() {
    // The lexicon and rules are compiled with the classifier's own config.
    let d = classifier().decide("stop", Profile::Standard);
    assert!(
        !d.trail()
            .records()
            .iter()
            .any(|r| r.text() == "config mismatch")
    );
    assert_eq!(d.data_version(), classifier().data_version());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn decide_is_deterministic(msg in "\\PC{0,80}", p in 0u8..3) {
        let profile = Profile::ALL[usize::from(p)];
        let a = classifier().decide(&msg, profile);
        let b = classifier().decide(&msg, profile);
        prop_assert_eq!(a, b);
    }
}
