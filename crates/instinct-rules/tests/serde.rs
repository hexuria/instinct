//! Pack data shape: a `RuleSetSpec` deserializes from TOML (spec §4.4 example) and is validated
//! by `RuleSet::new`.
#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use instinct_rules::{DEFAULT_NEGATION_WINDOW, RuleSet, RuleSetSpec, RulesError};
use instinct_text::NormalizeConfig;

const DATA: &str = r#"
negators = ["don't", "do not"]

[[classes]]
name = "interrupt"
scorer = "max"

[[rules]]
id = "interrupt.stop.v1"
class = "interrupt"
pattern = "stop"
weight_millis = 700
forbids = ["negation"]
version = 1
"#;

#[test]
fn toml_into_a_validated_rule_set() {
    let spec: RuleSetSpec = toml::from_str(DATA).unwrap();
    assert_eq!(spec.negation_window, DEFAULT_NEGATION_WINDOW);
    let rs = RuleSet::new(&spec, NormalizeConfig::default()).unwrap();
    assert_eq!(rs.class_count(), 1);
}

#[test]
fn bad_data_is_refused() {
    assert!(toml::from_str::<RuleSetSpec>(&DATA.replace("\"max\"", "\"mean\"")).is_err());
    assert!(toml::from_str::<RuleSetSpec>(&DATA.replace("forbids", "forbid")).is_err());
    assert!(toml::from_str::<RuleSetSpec>(&DATA.replace("version = 1", "")).is_err());
    let regexy: RuleSetSpec = toml::from_str(&DATA.replace("\"stop\"", "\"st(o|0)p\"")).unwrap();
    assert_eq!(
        RuleSet::new(&regexy, NormalizeConfig::default()).unwrap_err(),
        RulesError::NonCanonicalLiteral {
            id: "interrupt.stop.v1".into(),
            literal: "st(o|0)p".into()
        }
    );
    // The spec example's `protected_span` context is enforced by construction, so naming it
    // in data is refused rather than silently ignored.
    let legacy: RuleSetSpec =
        toml::from_str(&DATA.replace("[\"negation\"]", "[\"negation\", \"protected_span\"]"))
            .unwrap();
    assert_eq!(
        RuleSet::new(&legacy, NormalizeConfig::default()).unwrap_err(),
        RulesError::UnknownContext {
            id: "interrupt.stop.v1".into(),
            context: "protected_span".into()
        }
    );
}
