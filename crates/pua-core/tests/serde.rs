//! Serde round trips and validation on the way in (deserialization cannot bypass invariants).
#![cfg(feature = "serde")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)] // tests may unwrap (AGENTS.md rule 4)

use pua_core::{
    Confidence, DataVersion, Decision, Millis, OptionIndex, Profile, Question, Scores, Span,
    StageKind, Trail, TrailRecord, decide,
};

#[test]
fn decision_round_trips_byte_identically() {
    let q = Question::choice("delivery", &["queue", "steer", "interrupt"]).unwrap();
    let mut s = Scores::new(&q);
    s.set(OptionIndex::new(1), Confidence::new(800).unwrap())
        .unwrap();
    let mut trail = Trail::new();
    trail.push(
        TrailRecord::new(StageKind::Rules, "steer cue")
            .span(Span::new(0, 7).unwrap())
            .millis(Millis::new(800).unwrap()),
    );
    let d = Decision::new(
        decide(&s, Profile::Standard),
        Profile::Standard,
        DataVersion::builder("t").finish(),
        trail,
    );
    let a = serde_json::to_string(&d).unwrap();
    let back: Decision = serde_json::from_str(&a).unwrap();
    assert_eq!(back, d);
    assert_eq!(serde_json::to_string(&back).unwrap(), a);
}

#[test]
fn invalid_values_are_rejected_on_deserialize() {
    assert!(serde_json::from_str::<Confidence>("1001").is_err());
    assert!(serde_json::from_str::<Confidence>("-1").is_err());
    assert!(serde_json::from_str::<Millis>("-1001").is_err());
    assert!(serde_json::from_str::<Span>(r#"{"start":5,"end":2}"#).is_err());
    assert!(serde_json::from_str::<DataVersion>(r#""abc""#).is_err());
    assert!(
        serde_json::from_str::<Question>(r#"{"shape":"choice","name":"q","options":["a","a"]}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<Question>(r#"{"shape":"choice","name":"q","options":["a"]}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<Question>(r#"{"shape":"noul","name":""}"#).is_err());
    // Unsorted ranked list inside an answer.
    let bad = r#"{"answer":"abstain","why":{"kind":"no_candidates"},"ranked":[[0,1],[1,5]]}"#;
    assert!(serde_json::from_str::<pua_core::Answer>(bad).is_err());
    let good = r#"{"answer":"abstain","why":{"kind":"no_candidates"},"ranked":[[1,5],[0,1]]}"#;
    assert!(serde_json::from_str::<pua_core::Answer>(good).is_ok());
}
