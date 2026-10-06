//! Pack data shape: a `LexiconSpec` deserializes from TOML and is validated by `Lexicon::new`.
#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use instinct_lexicon::{Lexicon, LexiconError, LexiconSpec, Repair};
use instinct_text::NormalizeConfig;

const DATA: &str = r#"
guards = ["top"]
repair = { mode = "typos", penalty_per_edit = 150 }

[[entries]]
term = "stop"
tag = "interrupt"

[[entries]]
term = "oversteer"
tag = "steer"
substring = true
"#;

#[test]
fn toml_round_trip_into_a_validated_lexicon() {
    let spec: LexiconSpec = toml::from_str(DATA).unwrap();
    assert_eq!(spec.entries.len(), 2);
    assert!(spec.entries[1].substring);
    assert!(
        matches!(spec.repair, Repair::Typos { penalty_per_edit } if penalty_per_edit.get() == 150)
    );
    let lex = Lexicon::new(&spec, NormalizeConfig::default()).unwrap();
    assert_eq!(lex.tag(lex.find("stop").unwrap()), "interrupt");
}

#[test]
fn bad_data_is_refused_with_the_exact_reason() {
    // Out-of-range penalty fails at parse time (Confidence is validated on the way in).
    assert!(toml::from_str::<LexiconSpec>(&DATA.replace("150", "1001")).is_err());
    // Unknown fields are refused, so a typo in pack data can't be silently ignored.
    assert!(toml::from_str::<LexiconSpec>(&DATA.replace("substring", "substr")).is_err());
    assert!(toml::from_str::<LexiconSpec>(&DATA.replace("\"typos\"", "\"fuzzy\"")).is_err());
    let off: LexiconSpec =
        toml::from_str("repair = { mode = \"off\" }\n[[entries]]\nterm = \"Stop\"\ntag = \"x\"\n")
            .unwrap();
    assert_eq!(off.repair, Repair::Off);
    assert_eq!(
        Lexicon::new(&off, NormalizeConfig::default()).unwrap_err(),
        LexiconError::NonCanonicalTerm {
            index: 0,
            expected: "stop".into()
        }
    );
}
