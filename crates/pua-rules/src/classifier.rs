//! [`RuleClassifier`]: the generic text → lexicon → rules → decide pipeline.
//!
//! PUA owns **how** the decision is made; the consumer owns **what** it means. The consumer
//! supplies a [`ClassifierSpec`]: a question name, the canonicalization config, a closed
//! vocabulary and a rule set. The rule classes, in declared order, become the options of a
//! `Question::Choice`; class 0 is the safe default (spec §5 rule 4). Nothing in this module
//! knows what any option means.
//!
//! Pipeline, all pure and integer-only:
//!
//! 1. [`normalize`] (refuses over-long input → abstain);
//! 2. [`Lexicon::lookup`] (exact, substring, repaired hits; confusable flags);
//! 3. confusable policy ([`OnConfusable`]);
//! 4. [`RuleSet::score`] with the lexicon's repairs;
//! 5. class scores → [`Scores`] → [`decide`] (profile thresholds and top-2 margin).
//!
//! Every stage appends to the [`Trail`]; the [`Decision`] is stamped with a [`DataVersion`] over
//! the lexicon, the rules, the profile table and the normalize config.

use core::fmt;

use pua_core::{
    AbstainReason, Answer, Confidence, DataVersion, Decision, Millis, OptionIndex, Profile,
    Question, QuestionError, Ranked, Scores, StageKind, Trail, TrailRecord, decide,
};
use pua_lexicon::{Lexicon, LexiconError, LexiconSpec, MatchKind};
use pua_text::{NormalizeConfig, normalize};

use crate::{ClassId, Repairs, RuleSet, RuleSetSpec, RulesError};

/// What to do when a token is a lookalike (homoglyph / mixed script) of a lexicon term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OnConfusable {
    /// Abstain with [`AbstainReason::Confusable`]: a lookalike never fires a cue.
    #[default]
    Abstain,
    /// Record the flags in the trail and carry on (the confusable token still never matches).
    Ignore,
}

/// Everything a consumer provides to build a [`RuleClassifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifierSpec {
    /// `DataVersion` domain tag, chosen by the consumer (e.g. `"nativechat-autosteer/1"`).
    pub domain: String,
    /// Question name (the consumer's word, e.g. `"delivery"`).
    pub question: String,
    /// Canonicalization settings; the lexicon and rules are validated against them.
    pub normalize: NormalizeConfig,
    /// Closed vocabulary.
    pub lexicon: LexiconSpec,
    /// Rule set. Its classes, in declared order, are the options; class 0 is the safe default.
    pub rules: RuleSetSpec,
    /// Confusable policy.
    pub on_confusable: OnConfusable,
}

/// Why a [`ClassifierSpec`] was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClassifierError {
    /// The lexicon failed validation.
    Lexicon(LexiconError),
    /// The rule set failed validation.
    Rules(RulesError),
    /// The classes do not form a valid `Choice` (fewer than 2, duplicate or bad label).
    Question(QuestionError),
}

impl fmt::Display for ClassifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexicon(e) => write!(f, "lexicon: {e}"),
            Self::Rules(e) => write!(f, "rules: {e}"),
            Self::Question(e) => write!(f, "question: {e}"),
        }
    }
}

impl std::error::Error for ClassifierError {}

/// A validated, immutable text classifier over a finite option set.
#[derive(Debug, Clone)]
pub struct RuleClassifier {
    question: Question,
    lexicon: Lexicon,
    rules: RuleSet,
    config: NormalizeConfig,
    on_confusable: OnConfusable,
    data_version: DataVersion,
}

impl RuleClassifier {
    /// Validates `spec` and compiles it.
    ///
    /// # Errors
    /// The first [`ClassifierError`]: lexicon, then rules, then the question built from the
    /// rule classes.
    pub fn new(spec: &ClassifierSpec) -> Result<Self, ClassifierError> {
        let config = spec.normalize;
        let lexicon = Lexicon::new(&spec.lexicon, config).map_err(ClassifierError::Lexicon)?;
        let rules = RuleSet::new(&spec.rules, config).map_err(ClassifierError::Rules)?;
        let labels: Vec<&str> = (0..rules.class_count())
            .map(|i| rules.class_name(ClassId(u16::try_from(i).unwrap_or(u16::MAX))))
            .collect();
        let question =
            Question::choice(&spec.question, &labels).map_err(ClassifierError::Question)?;
        let mut builder = DataVersion::builder(&spec.domain)
            .field("lexicon", &lexicon.fingerprint())
            .field("rules", &rules.fingerprint())
            .field("profile_table", &Profile::table_bytes())
            .field("normalize", config.fingerprint().as_bytes());
        // `Abstain` is the original layout, so its digest carries no extra field; any other
        // policy changes answers and therefore the version.
        if spec.on_confusable == OnConfusable::Ignore {
            builder = builder.field("on_confusable", b"ignore");
        }
        Ok(Self {
            question,
            lexicon,
            rules,
            config,
            on_confusable: spec.on_confusable,
            data_version: builder.finish(),
        })
    }

    /// The question (options = rule classes in declared order).
    pub fn question(&self) -> &Question {
        &self.question
    }

    /// Digest of everything that can change an answer.
    pub fn data_version(&self) -> DataVersion {
        self.data_version
    }

    /// The canonicalization settings.
    pub fn config(&self) -> NormalizeConfig {
        self.config
    }

    /// The compiled lexicon.
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// The compiled rule set.
    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }

    /// The label of option `i`, if it exists.
    pub fn label(&self, i: OptionIndex) -> Option<&str> {
        self.question
            .options()
            .and_then(|o| o.get(i))
            .map(pua_core::Label::as_str)
    }

    fn refuse(&self, why: AbstainReason, profile: Profile, trail: Trail) -> Decision {
        Decision::new(
            Answer::Abstain {
                why,
                ranked: empty_ranked(),
            },
            profile,
            self.data_version,
            trail,
        )
    }

    /// Decides `text`. Pure: same text + same profile gives a byte-identical [`Decision`].
    pub fn decide(&self, text: &str, profile: Profile) -> Decision {
        let mut trail = Trail::new();
        let Ok(normalized) = normalize(text, self.config) else {
            // Over-long input: refuse rather than truncate (spec §5: no silent loss).
            trail.push(TrailRecord::new(
                StageKind::Normalize,
                "input refused (too long)",
            ));
            return self.refuse(AbstainReason::NoCandidates, profile, trail);
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
            .millis(Millis::ZERO),
        );

        let Ok(lookup) = self.lexicon.lookup(&normalized) else {
            trail.push(TrailRecord::new(StageKind::Lexicon, "config mismatch"));
            return self.refuse(AbstainReason::NoCandidates, profile, trail);
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
                rec = rec.millis(Millis::saturating(-i32::from(penalty.get())));
            }
            trail.push(rec);
        }
        if !lookup.flags().is_empty() {
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
            if self.on_confusable == OnConfusable::Abstain {
                trail.push(TrailRecord::new(
                    StageKind::Decide,
                    "abstain: confusable control word",
                ));
                return self.refuse(AbstainReason::Confusable, profile, trail);
            }
        }

        let repairs = Repairs::from_lookup(&self.lexicon, &lookup);
        let Ok(scores) = self.rules.score(&normalized, &repairs) else {
            trail.push(TrailRecord::new(StageKind::Rules, "config mismatch"));
            return self.refuse(AbstainReason::NoCandidates, profile, trail);
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
                    self.label(*option).unwrap_or("?"),
                    confidence.get()
                ),
                Answer::Abstain { why, .. } => format!("abstain: {why}"),
                other => format!("{other:?}"),
            },
        ));
        Decision::new(answer, profile, self.data_version, trail)
    }
}

fn empty_ranked() -> Ranked {
    Ranked::try_from(Vec::<(OptionIndex, Confidence)>::new()).unwrap_or_else(|_| unreachable!())
}
