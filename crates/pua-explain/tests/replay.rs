//! Replay records: golden bytes, round trips, tamper detection, divergence diffs.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may unwrap (AGENTS.md rule 4)

use proptest::prelude::*;
use pua_core::{
    Answer, Confidence, DataVersion, Decision, Millis, OptionIndex, Profile, Question, ScorerKind,
    Scores, Span, StageKind, Trail, TrailRecord, decide,
};
use pua_explain::{ExplainError, ReplayCheck, ReplayRecord, diff, render_trail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Input {
    message: String,
    runs: Vec<String>,
}

fn toy_pack(input: &Input, version_tag: &str) -> Decision {
    let q = Question::choice("delivery", &["queue", "steer", "interrupt"]).unwrap();
    let mut s = Scores::new(&q);
    let mut trail = Trail::new();
    if let Some(pos) = input.message.find("stop") {
        let span = Span::from_range(pos..pos + 4).unwrap();
        s.set(OptionIndex::new(2), Confidence::new(800).unwrap())
            .unwrap();
        trail.push(
            TrailRecord::new(StageKind::Rules, "cue 'stop'")
                .rule("interrupt.stop.v1")
                .scorer(ScorerKind::Max)
                .span(span)
                .millis(Millis::new(800).unwrap()),
        );
    }
    let answer = decide(&s, Profile::Standard);
    trail.push(TrailRecord::new(
        StageKind::Decide,
        format!("{answer:?}").chars().take(20).collect::<String>(),
    ));
    Decision::new(
        answer,
        Profile::Standard,
        DataVersion::builder(version_tag).finish(),
        trail,
    )
}

fn input() -> Input {
    Input {
        message: "please stop now".into(),
        runs: vec!["run_a".into()],
    }
}

#[test]
fn golden_line_is_byte_stable() {
    let i = input();
    let line = ReplayRecord::new(i.clone(), toy_pack(&i, "toy-v1"))
        .unwrap()
        .to_json_line()
        .unwrap();
    let golden = include_str!("fixtures/replay_v1.jsonl").trim_end();
    assert_eq!(
        line, golden,
        "replay framing changed; if deliberate, bump REPLAY_SCHEMA and update the fixture"
    );
    let back: ReplayRecord<Input> = ReplayRecord::from_json_line(golden).unwrap();
    assert_eq!(back.to_json_line().unwrap(), golden);
    assert_eq!(back.input(), &i);
}

#[test]
fn check_identical_and_diverged() {
    let i = input();
    let rec = ReplayRecord::new(i.clone(), toy_pack(&i, "toy-v1")).unwrap();
    assert_eq!(
        rec.check(|x| toy_pack(x, "toy-v1")).unwrap(),
        ReplayCheck::Identical
    );
    let ReplayCheck::Diverged(d) = rec.check(|x| toy_pack(x, "toy-v2")).unwrap() else {
        panic!("data version moved, must diverge");
    };
    assert!(d.data_version.is_some());
    assert!(d.answer.is_none() && d.profile.is_none() && !d.trail_changed);
    assert!(!d.is_empty());
}

#[test]
fn diff_reports_answer_and_trail_changes() {
    let a = toy_pack(&input(), "v");
    let b = toy_pack(
        &Input {
            message: "carry on".into(),
            runs: vec![],
        },
        "v",
    );
    let d = diff(&a, &b);
    assert!(d.answer.is_some() && d.trail_changed && d.data_version.is_none());
    assert!(diff(&a, &a).is_empty());
    let c = Decision::new(
        a.answer().clone(),
        Profile::Deep,
        a.data_version(),
        a.trail().clone(),
    );
    assert_eq!(
        diff(&a, &c).profile,
        Some((Profile::Standard, Profile::Deep))
    );
}

#[test]
fn tampering_is_detected_with_exact_errors() {
    let golden = include_str!("fixtures/replay_v1.jsonl").trim_end();
    let edited = golden.replace("please stop now", "please stop NOW");
    assert_eq!(
        ReplayRecord::<Input>::from_json_line(&edited),
        Err(ExplainError::InputHashMismatch)
    );
    let schema = golden.replacen("\"schema\":1", "\"schema\":2", 1);
    assert_eq!(
        ReplayRecord::<Input>::from_json_line(&schema),
        Err(ExplainError::SchemaMismatch {
            found: 2,
            expected: 1
        })
    );
    assert!(matches!(
        ReplayRecord::<Input>::from_json_line("{"),
        Err(ExplainError::Deserialize(_))
    ));
    let bad_conf = golden.replacen("800", "1800", 1);
    assert!(matches!(
        ReplayRecord::<Input>::from_json_line(&bad_conf),
        Err(ExplainError::Deserialize(_))
    ));
}

#[test]
fn render_is_stable() {
    let text = render_trail(toy_pack(&input(), "v").trail());
    let first = text.lines().next().unwrap();
    assert_eq!(
        first,
        " 1 rules      interrupt.stop.v1 @7..11 +800 [max] cue 'stop'"
    );
    assert_eq!(text.lines().count(), 2);
}

#[test]
fn record_accessors() {
    let i = input();
    let d = toy_pack(&i, "v");
    let rec = ReplayRecord::new(i, d.clone()).unwrap();
    assert_eq!(rec.decision(), &d);
    assert_eq!(rec.input_hash().to_string().len(), 64);
    assert!(matches!(d.answer(), Answer::Choice { .. }));
}

proptest! {
    #[test]
    fn round_trip_is_identity(msg in "\\PC{0,64}", runs in prop::collection::vec("[a-z_]{1,8}", 0..4)) {
        let i = Input { message: msg, runs };
        let rec = ReplayRecord::new(i.clone(), toy_pack(&i, "v")).unwrap();
        let line = rec.to_json_line().unwrap();
        let back = ReplayRecord::<Input>::from_json_line(&line).unwrap();
        prop_assert_eq!(&back, &rec);
        prop_assert_eq!(back.to_json_line().unwrap(), line);
        prop_assert_eq!(rec.check(|x| toy_pack(x, "v")).unwrap(), ReplayCheck::Identical);
    }
}
