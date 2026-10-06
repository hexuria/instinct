//! Untrusted text against a fixed vocabulary: lookup never panics, hits are ordered, disjoint,
//! cover only free tokens and slice the original text.
#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use instinct_core::Confidence;
use instinct_lexicon::{EntrySpec, Lexicon, LexiconSpec, Repair};
use instinct_text::{NormalizeConfig, normalize};

fn lexicon() -> &'static Lexicon {
    static LEX: OnceLock<Lexicon> = OnceLock::new();
    LEX.get_or_init(|| {
        let mut entries: Vec<EntrySpec> = [
            "stop", "cancel", "never mind", "forget it", "halt", "abort", "switch to", "instead",
            "wait no", "after that", "also", "peña", "café",
        ]
        .iter()
        .map(|t| EntrySpec::new(t, "tag"))
        .collect();
        let mut sub = EntrySpec::new("oversteer", "steer");
        sub.substring = true;
        entries.push(sub);
        let spec = LexiconSpec {
            entries,
            guards: vec!["top".into(), "step".into()],
            repair: Repair::Typos { penalty_per_edit: Confidence::new(150).expect("in range") },
        };
        Lexicon::new(&spec, NormalizeConfig::default()).expect("valid fuzz lexicon")
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let lex = lexicon();
    let n = normalize(text, lex.config()).expect("below MAX_INPUT_BYTES");
    let found = lex.lookup(&n).expect("same config");
    let mut last = 0;
    for h in found.hits() {
        let r = h.tokens();
        assert!(r.start >= last && r.end > r.start && r.end <= n.tokens().len());
        last = r.end;
        assert!(n.tokens()[r].iter().all(|t| t.is_free()));
        assert!(h.original().slice(text).is_some());
    }
    for f in found.flags() {
        assert!(n.tokens()[f.token()].confusable().is_some());
    }
});
