//! Untrusted Jev reply bodies: parsing never panics, and whatever is accepted obeys the guard
//! (the chosen option is offered, confidences are in range, the ranked list covers the menu).
#![no_main]

use libfuzzer_sys::fuzz_target;
use pua_core::{Answer, Question};
use pua_jev::{escalate, parse_reply};

fuzz_target!(|data: &[u8]| {
    let Ok(body) = std::str::from_utf8(data) else {
        return;
    };
    let questions = [
        Question::noul("done").expect("valid"),
        Question::choice("delivery", &["queue", "steer", "interrupt"]).expect("valid"),
        Question::score("effort", &["none", "a little", "most of it"]).expect("valid"),
    ];
    for q in &questions {
        match parse_reply(body, q) {
            Ok(Answer::Choice { option, confidence, ranked }) => {
                assert!(q.options().and_then(|o| o.get(option)).is_some());
                assert!(confidence.get() <= 1000);
                assert_eq!(ranked.entries().len(), usize::from(q.arity()));
            }
            Ok(Answer::Score { level, .. }) => {
                assert!(q.options().and_then(|o| o.get(level)).is_some());
            }
            Ok(Answer::Noul { .. }) => assert!(matches!(q, Question::Noul { .. })),
            Ok(other) => panic!("Jev never abstains: {other:?}"),
            Err(_) => {}
        }
        // The escalation wrapper labels every failure as a fallback, never as an answer.
        let e = escalate(q, Ok(body), |_| None, |_| Answer::Noul {
            yes: false,
            confidence: pua_core::Confidence::ZERO,
        });
        assert_eq!(e.is_fallback(), parse_reply(body, q).is_err());
    }
});
