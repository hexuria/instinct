//! `pua-steer`: delivery advice for a chat message (spec §6.1).
//!
//! Question: `Choice{ name: "delivery", options: [queue, steer, interrupt] }` with option 0 the
//! safe default. Pipeline: canonicalize → lexicon → rules → decide. Interrupt answers always
//! carry [`AutoApply::Never`] (ADR 0008): a consumer that respects the type cannot auto-apply
//! an interrupt. Confusable control words never fire ([`pua_core::AbstainReason::Confusable`]).
//!
//! Target selection over live runs sits behind the off-by-default `target-selection` feature
//! (spec: waits for PRD Phase B).
//!
//! ```
//! use pua_core::{Answer, OptionIndex, Profile};
//! use pua_steer::{Advice, Autosteer, Input};
//!
//! let pack = Autosteer::load().expect("embedded data is valid");
//! let advice = pack.advise(&Input::message("stop the build"), Profile::Standard);
//! assert_eq!(advice.chosen_label(), Some("interrupt"));
//! assert_eq!(advice.auto_apply(), pua_steer::AutoApply::Never);
//! assert!(matches!(advice.decision().answer(), Answer::Choice { option, .. }
//!     if *option == OptionIndex::new(2)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod advice;
mod data;
mod input;

pub use advice::{Advice, AutoApply};
pub use input::{Input, LiveRun};

use core::fmt;

use pua_core::{DataVersion, Decision, Pack, Profile, Question};
use pua_lexicon::LexiconSpec;
use pua_rules::{ClassifierError, ClassifierSpec, OnConfusable, RuleClassifier, RuleSetSpec};
use pua_text::NormalizeConfig;

use crate::data::{LEXICON_TOML, RULES_TOML};

/// Pack algorithm tag folded into [`DataVersion`].
pub const ALGORITHM_TAG: &str = "pua-steer/1";

/// Option indices of the delivery question.
pub mod option {
    use pua_core::OptionIndex;
    /// Queue (safe default).
    pub const QUEUE: OptionIndex = OptionIndex::SAFE_DEFAULT;
    /// Steer.
    pub const STEER: OptionIndex = OptionIndex::new(1);
    /// Interrupt.
    pub const INTERRUPT: OptionIndex = OptionIndex::new(2);
}

/// Why the pack (or its embedded data) could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackError {
    /// Lexicon TOML failed to parse or validate.
    Lexicon(String),
    /// Rules TOML failed to parse or validate.
    Rules(String),
    /// The rule classes are not exactly `[queue, steer, interrupt]` in that order.
    ClassOrder(Vec<String>),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexicon(e) => write!(f, "lexicon: {e}"),
            Self::Rules(e) => write!(f, "rules: {e}"),
            Self::ClassOrder(names) => write!(
                f,
                "classes must be [queue, steer, interrupt], got {names:?}"
            ),
        }
    }
}

impl std::error::Error for PackError {}

/// The autosteer pack: embedded lexicon + rules answering the delivery question.
#[derive(Debug, Clone)]
pub struct Autosteer {
    classifier: RuleClassifier,
}

impl Autosteer {
    /// Loads the embedded pack data. Always succeeds for the committed TOML; returns an error
    /// only if that data is broken (caught by CI).
    ///
    /// # Errors
    /// [`PackError`] when the embedded TOML fails to parse or validate.
    pub fn load() -> Result<Self, PackError> {
        Self::from_toml(LEXICON_TOML, RULES_TOML)
    }

    /// Builds a pack from lexicon and rules TOML (for tests and the offline CLI).
    ///
    /// # Errors
    /// [`PackError`].
    pub fn from_toml(lexicon_toml: &str, rules_toml: &str) -> Result<Self, PackError> {
        let lexicon: LexiconSpec =
            toml::from_str(lexicon_toml).map_err(|e| PackError::Lexicon(e.to_string()))?;
        let rules: RuleSetSpec =
            toml::from_str(rules_toml).map_err(|e| PackError::Rules(e.to_string()))?;
        let spec = ClassifierSpec {
            domain: ALGORITHM_TAG.into(),
            question: "delivery".into(),
            normalize: NormalizeConfig {
                fold: pua_text::Fold::UnicodeLower,
                punct_runs: pua_text::PunctRuns::Collapse,
            },
            lexicon,
            rules,
            on_confusable: OnConfusable::Abstain,
        };
        let classifier = RuleClassifier::new(&spec).map_err(|e| match e {
            ClassifierError::Lexicon(e) => PackError::Lexicon(e.to_string()),
            ClassifierError::Rules(e) => PackError::Rules(e.to_string()),
            other => PackError::ClassOrder(vec![other.to_string()]),
        })?;
        // Classes must be exactly [queue, steer, interrupt] in that order: option indices
        // are hard-wired to those names.
        let labels: Vec<&str> = classifier
            .question()
            .options()
            .map(|o| o.labels().iter().map(pua_core::Label::as_str).collect())
            .unwrap_or_default();
        if labels != ["queue", "steer", "interrupt"] {
            return Err(PackError::ClassOrder(
                labels.into_iter().map(str::to_owned).collect(),
            ));
        }
        Ok(Self { classifier })
    }

    /// Consumer-facing ask: a [`Decision`] plus the typed [`AutoApply`] flag.
    pub fn advise(&self, input: &Input<'_>, profile: Profile) -> Advice {
        Advice::from_decision(self.ask(input, profile))
    }
}

impl Pack for Autosteer {
    type Input<'a> = Input<'a>;

    fn question(&self) -> &Question {
        self.classifier.question()
    }

    fn data_version(&self) -> DataVersion {
        self.classifier.data_version()
    }

    fn ask(&self, input: &Self::Input<'_>, profile: Profile) -> Decision {
        self.classifier.decide(input.text(), profile)
    }
}

/// Re-exports used by the eval / replay examples.
pub mod prelude {
    pub use crate::{ALGORITHM_TAG, Advice, AutoApply, Autosteer, Input, PackError, option};
    pub use pua_core::{Answer, Decision, OptionIndex, Pack, Profile};
}

#[cfg(test)]
mod tests;
