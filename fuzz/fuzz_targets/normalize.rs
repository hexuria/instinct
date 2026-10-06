//! Untrusted text → canonicalization must never panic, and every span it returns must index the
//! original text on UTF-8 boundaries.
#![no_main]

use libfuzzer_sys::fuzz_target;
use instinct_text::{Fold, NormalizeConfig, PunctRuns, normalize};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    for fold in [Fold::AsciiLower, Fold::UnicodeLower] {
        for punct_runs in [PunctRuns::Keep, PunctRuns::Collapse] {
            let cfg = NormalizeConfig { fold, punct_runs };
            let n = normalize(text, cfg).expect("fuzz inputs are far below MAX_INPUT_BYTES");
            for t in n.tokens() {
                assert!(t.original().slice(text).is_some());
            }
            for p in n.protected() {
                assert!(p.original().slice(text).is_some());
            }
            let len = n.canonical().len();
            assert!(n.to_original(0..len).slice(text).is_some());
            // Idempotence: the canonical text is a fixed point of free-text normalization only
            // when it has no protected spans (protected text keeps its case), so check that case.
            if n.protected().is_empty() {
                let again = normalize(n.canonical(), cfg).expect("shorter than input");
                if again.protected().is_empty() {
                    assert_eq!(again.canonical(), n.canonical());
                }
            }
        }
    }
});
