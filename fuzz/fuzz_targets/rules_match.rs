//! Untrusted text against a fixed rule set (plus lexicon repairs): scoring never panics,
//! scores stay in range, every match covers free tokens of one sentence and slices the text.
#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use pua_core::{Confidence, ScorerKind};
use pua_lexicon::{EntrySpec, Lexicon, LexiconSpec, Repair};
use pua_rules::{ClassSpec, Repairs, RuleSet, RuleSetSpec, RuleSpec};
use pua_text::{NormalizeConfig, normalize};

fn setup() -> &'static (Lexicon, RuleSet) {
    static S: OnceLock<(Lexicon, RuleSet)> = OnceLock::new();
    S.get_or_init(|| {
        let words = ["stop", "cancel", "instead", "also", "using"];
        let lex = Lexicon::new(
            &LexiconSpec {
                entries: words.iter().map(|w| EntrySpec::new(w, "x")).collect(),
                guards: vec!["top".into()],
                repair: Repair::Typos { penalty_per_edit: Confidence::new(150).expect("ok") },
            },
            NormalizeConfig::default(),
        )
        .expect("valid lexicon");
        let rule = |id: &str, class: &str, pattern: &str, w| RuleSpec {
            id: id.into(),
            class: class.into(),
            pattern: pattern.into(),
            weight_millis: w,
            requires: vec![],
            forbids: vec!["negation".into()],
            version: 1,
        };
        let rs = RuleSet::new(
            &RuleSetSpec {
                classes: vec![
                    ClassSpec { name: "interrupt".into(), scorer: ScorerKind::Max },
                    ClassSpec { name: "steer".into(), scorer: ScorerKind::Max },
                    ClassSpec { name: "queue".into(), scorer: ScorerKind::Sum },
                ],
                negators: vec!["don't".into(), "no need to".into()],
                negation_window: 3,
                rules: vec![
                    rule("i.stop", "interrupt", "stop", 700),
                    rule("i.cancel", "interrupt", "cancel", 720),
                    rule("s.stop_g", "steer", "stop {gerund}", 650),
                    rule("s.switch", "steer", "switch to {word}", 600),
                    rule("s.instead", "steer", "instead", 500),
                    rule("q.also", "queue", "also", 300),
                ],
            },
            NormalizeConfig::default(),
        )
        .expect("valid rules");
        (lex, rs)
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let (lex, rs) = setup();
    let n = normalize(text, rs.config()).expect("below MAX_INPUT_BYTES");
    let reps = Repairs::from_lookup(lex, &lex.lookup(&n).expect("same config"));
    let out = rs.score(&n, &reps).expect("same config");
    assert_eq!(out.scores().len(), 3);
    for m in out.matches() {
        let toks = &n.tokens()[m.tokens()];
        assert!(toks.iter().all(|t| t.is_free()));
        assert!(toks.iter().all(|t| t.sentence() == toks[0].sentence()));
        assert!(m.original().slice(text).is_some());
    }
    assert_eq!(rs.trail(&out).len(), out.matches().len());
});
