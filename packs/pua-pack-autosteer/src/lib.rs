//! `pua-pack-autosteer`: delivery advice for a chat message (spec §6.1).
//!
//! Question: `Choice{ name: "delivery", options: [queue, steer, interrupt] }` with option 0 the
//! safe default. Pipeline: canonicalize → lexicon → rules → decide. Interrupt answers always
//! carry [`AutoApply::Never`] (ADR 0008): a consumer that respects the type cannot auto-apply
//! an interrupt. Confusable control words never fire ([`AbstainReason::Confusable`]).
//!
//! Target selection over live runs sits behind the off-by-default `target-selection` feature
//! (spec: waits for PRD Phase B).
//!
//! ```
//! use pua_core::{Answer, OptionIndex, Profile};
//! use pua_pack_autosteer::{Advice, Autosteer, Input};
//!
//! let pack = Autosteer::load().expect("embedded data is valid");
//! let advice = pack.advise(&Input::message("stop the build"), Profile::Standard);
//! assert_eq!(advice.chosen_label(), Some("interrupt"));
//! assert_eq!(advice.auto_apply(), pua_pack_autosteer::AutoApply::Never);
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

use pua_core::{
    AbstainReason, Answer, Confidence, DataVersion, Decision, OptionIndex, Pack, Profile, Question,
    Ranked, Scores, StageKind, Trail, TrailRecord, decide,
};
use pua_lexicon::{Lexicon, LexiconSpec, MatchKind};
use pua_rules::{Repairs, RuleSet, RuleSetSpec};
use pua_text::{NormalizeConfig, normalize};

use crate::data::{LEXICON_TOML, RULES_TOML};

/// Pack algorithm tag folded into [`DataVersion`].
pub const ALGORITHM_TAG: &str = "pua-pack-autosteer/1";

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
    question: Question,
    lexicon: Lexicon,
    rules: RuleSet,
    config: NormalizeConfig,
    data_version: DataVersion,
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
        let config = NormalizeConfig {
            fold: pua_text::Fold::UnicodeLower,
            punct_runs: pua_text::PunctRuns::Collapse,
        };
        let lex_spec: LexiconSpec =
            toml::from_str(lexicon_toml).map_err(|e| PackError::Lexicon(e.to_string()))?;
        let lexicon =
            Lexicon::new(&lex_spec, config).map_err(|e| PackError::Lexicon(e.to_string()))?;
        let rule_spec: RuleSetSpec =
            toml::from_str(rules_toml).map_err(|e| PackError::Rules(e.to_string()))?;
        let rules =
            RuleSet::new(&rule_spec, config).map_err(|e| PackError::Rules(e.to_string()))?;
        // Classes must be exactly [queue, steer, interrupt] in that order: option indices
        // are hard-wired to those names.
        for (name, want) in [("queue", 0usize), ("steer", 1), ("interrupt", 2)] {
            match rules.class_id(name) {
                Some(c) if c.index() == want => {}
                _ => {
                    return Err(PackError::ClassOrder(vec![format!(
                        "want {name} at {want}, class_count={}",
                        rules.class_count()
                    )]));
                }
            }
        }
        if rules.class_count() != 3 {
            return Err(PackError::ClassOrder(vec![format!(
                "class_count={}",
                rules.class_count()
            )]));
        }
        let question = Question::choice("delivery", &["queue", "steer", "interrupt"])
            .map_err(|e| PackError::Rules(e.to_string()))?;
        let data_version = DataVersion::builder(ALGORITHM_TAG)
            .field("lexicon", &lexicon.fingerprint())
            .field("rules", &rules.fingerprint())
            .field("profile_table", &Profile::table_bytes())
            .field("normalize", config.fingerprint().as_bytes())
            .finish();
        Ok(Self {
            question,
            lexicon,
            rules,
            config,
            data_version,
        })
    }

    /// Consumer-facing ask: a [`Decision`] plus the typed [`AutoApply`] flag.
    pub fn advise(&self, input: &Input<'_>, profile: Profile) -> Advice {
        Advice::from_decision(self.ask(input, profile))
    }

    fn pipeline(&self, text: &str, profile: Profile) -> Decision {
        let mut trail = Trail::new();
        let Ok(normalized) = normalize(text, self.config) else {
            // Over-long input: abstain with no candidates. The pack refuses to answer rather
            // than truncate (spec §5: no silent loss).
            trail.push(TrailRecord::new(
                StageKind::Normalize,
                "input refused (too long)",
            ));
            return Decision::new(
                Answer::Abstain {
                    why: AbstainReason::NoCandidates,
                    ranked: empty_ranked(),
                },
                profile,
                self.data_version,
                trail,
            );
        };
        trail.push(
            TrailRecord::new(
                StageKind::Normalize,
                format!(
                    "nfc; {} protected; {} tokens; {} confusable",
                    normalized.protected().len(),
                    normalized.tokens().len(),
                    normalized
                        .tokens()
                        .iter()
                        .filter(|t| t.confusable().is_some())
                        .count()
                ),
            )
            .millis(pua_core::Millis::ZERO),
        );

        let Ok(lookup) = self.lexicon.lookup(&normalized) else {
            trail.push(TrailRecord::new(StageKind::Lexicon, "config mismatch"));
            return Decision::new(
                Answer::Abstain {
                    why: AbstainReason::NoCandidates,
                    ranked: empty_ranked(),
                },
                profile,
                self.data_version,
                trail,
            );
        };
        for h in lookup.hits() {
            let term = self.lexicon.term(h.entry());
            let text = match h.kind() {
                MatchKind::Exact => format!("exact '{term}'"),
                MatchKind::Substring => format!("substring '{term}'"),
                MatchKind::Repaired { edits, penalty } => {
                    format!(
                        "repair → '{term}' (edits {edits}, penalty {})",
                        penalty.get()
                    )
                }
            };
            let mut rec = TrailRecord::new(StageKind::Lexicon, text).span(h.original());
            if let MatchKind::Repaired { penalty, .. } = h.kind() {
                rec = rec.millis(pua_core::Millis::saturating(-i32::from(penalty.get())));
            }
            trail.push(rec);
        }
        if !lookup.flags().is_empty() {
            // Confusable control words never fire (plan T10).
            for f in lookup.flags() {
                trail.push(
                    TrailRecord::new(
                        StageKind::Lexicon,
                        format!(
                            "confusable {kind:?} ~ '{}'",
                            self.lexicon.term(f.entry()),
                            kind = f.kind(),
                        ),
                    )
                    .span(f.original()),
                );
            }
            trail.push(TrailRecord::new(
                StageKind::Decide,
                "abstain: confusable control word",
            ));
            return Decision::new(
                Answer::Abstain {
                    why: AbstainReason::Confusable,
                    ranked: empty_ranked(),
                },
                profile,
                self.data_version,
                trail,
            );
        }

        let repairs = Repairs::from_lookup(&self.lexicon, &lookup);
        let Ok(scores) = self.rules.score(&normalized, &repairs) else {
            trail.push(TrailRecord::new(StageKind::Rules, "config mismatch"));
            return Decision::new(
                Answer::Abstain {
                    why: AbstainReason::NoCandidates,
                    ranked: empty_ranked(),
                },
                profile,
                self.data_version,
                trail,
            );
        };
        for rec in self.rules.trail(&scores) {
            trail.push(rec);
        }

        let mut option_scores = Scores::new(&self.question);
        for (i, c) in scores.scores().iter().enumerate() {
            let _ = option_scores.set(OptionIndex::new(u16::try_from(i).unwrap_or(u16::MAX)), *c);
        }
        let answer = decide(&option_scores, profile);
        trail.push(TrailRecord::new(
            StageKind::Decide,
            match &answer {
                Answer::Choice {
                    option, confidence, ..
                } => format!(
                    "{} confidence {}",
                    self.question
                        .options()
                        .and_then(|o| o.get(*option))
                        .map_or("?", |l| l.as_str()),
                    confidence.get()
                ),
                Answer::Abstain { why, .. } => format!("abstain: {why}"),
                other => format!("{other:?}"),
            },
        ));
        Decision::new(answer, profile, self.data_version, trail)
    }
}

impl Pack for Autosteer {
    type Input<'a> = Input<'a>;

    fn question(&self) -> &Question {
        &self.question
    }

    fn data_version(&self) -> DataVersion {
        self.data_version
    }

    fn ask(&self, input: &Self::Input<'_>, profile: Profile) -> Decision {
        self.pipeline(input.text(), profile)
    }
}

fn empty_ranked() -> Ranked {
    #[allow(clippy::expect_used)]
    {
        Ranked::try_from(Vec::<(OptionIndex, Confidence)>::new()).expect("empty is canonical")
    }
}

/// Re-exports used by the eval / replay examples.
pub mod prelude {
    pub use crate::{ALGORITHM_TAG, Advice, AutoApply, Autosteer, Input, PackError, option};
    pub use pua_core::{Answer, Decision, OptionIndex, Pack, Profile};
}

#[cfg(test)]
mod tests;
