//! The one place an option wins (spec §4.6, §5 rule 4).

use core::fmt;

use crate::answer::Ranked;
use crate::{AbstainReason, Answer, Confidence, OptionIndex, Profile, Question};

/// Errors from writing into [`Scores`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScoresError {
    /// The index is not an option of the question these scores belong to.
    IndexOutOfRange {
        /// Offending index.
        index: OptionIndex,
        /// The question's arity.
        arity: u16,
    },
}

impl fmt::Display for ScoresError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexOutOfRange { index, arity } => {
                write!(
                    f,
                    "option {index} out of range for a question with {arity} options"
                )
            }
        }
    }
}

impl std::error::Error for ScoresError {}

/// One confidence per option of a specific question. Borrowing the question makes it impossible
/// to decide one question with another question's scores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scores<'q> {
    question: &'q Question,
    values: Box<[Confidence]>,
}

impl<'q> Scores<'q> {
    /// All-zero scores for `question`.
    pub fn new(question: &'q Question) -> Self {
        Self {
            question,
            values: vec![Confidence::ZERO; usize::from(question.arity())].into(),
        }
    }

    fn slot(&mut self, i: OptionIndex) -> Result<&mut Confidence, ScoresError> {
        let arity = self.question.arity();
        self.values
            .get_mut(usize::from(i.get()))
            .ok_or(ScoresError::IndexOutOfRange { index: i, arity })
    }

    /// Sets the score of option `i`.
    ///
    /// # Errors
    /// [`ScoresError::IndexOutOfRange`] when `i` is not an option of the question.
    pub fn set(&mut self, i: OptionIndex, c: Confidence) -> Result<(), ScoresError> {
        *self.slot(i)? = c;
        Ok(())
    }

    /// Raises the score of option `i` to at least `c` (a `Max` scorer).
    ///
    /// # Errors
    /// [`ScoresError::IndexOutOfRange`] when `i` is not an option of the question.
    pub fn raise_to(&mut self, i: OptionIndex, c: Confidence) -> Result<(), ScoresError> {
        let s = self.slot(i)?;
        *s = (*s).max(c);
        Ok(())
    }

    /// Adds `c` to option `i`, saturating at 1000 (a `Sum` scorer).
    ///
    /// # Errors
    /// [`ScoresError::IndexOutOfRange`] when `i` is not an option of the question.
    pub fn add(&mut self, i: OptionIndex, c: Confidence) -> Result<(), ScoresError> {
        let s = self.slot(i)?;
        *s = s.saturating_add(c);
        Ok(())
    }

    /// Subtracts `c` from option `i`, saturating at 0 (penalties).
    ///
    /// # Errors
    /// [`ScoresError::IndexOutOfRange`] when `i` is not an option of the question.
    pub fn sub(&mut self, i: OptionIndex, c: Confidence) -> Result<(), ScoresError> {
        let s = self.slot(i)?;
        *s = s.saturating_sub(c);
        Ok(())
    }

    /// The score of option `i`.
    pub fn get(&self, i: OptionIndex) -> Option<Confidence> {
        self.values.get(usize::from(i.get())).copied()
    }

    /// The question these scores belong to.
    pub fn question(&self) -> &'q Question {
        self.question
    }

    /// Scores in option order.
    pub fn values(&self) -> &[Confidence] {
        &self.values
    }
}

/// Options ranked by confidence descending, then index ascending (spec §5 rule 4).
fn rank(scores: &Scores<'_>) -> Vec<(OptionIndex, Confidence)> {
    let mut ranked: Vec<(OptionIndex, Confidence)> = scores
        .values
        .iter()
        .enumerate()
        .map(|(i, c)| (OptionIndex::new(u16::try_from(i).unwrap_or(u16::MAX)), *c))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
}

/// The one threshold-and-margin gate: `Some(reason)` when the best score must not win.
fn gate(top: Confidence, second: Confidence, profile: Profile) -> Option<AbstainReason> {
    let t = profile.thresholds();
    if top < t.min_confidence {
        return Some(AbstainReason::LowConfidence {
            top,
            min: t.min_confidence,
        });
    }
    let margin = top.saturating_sub(second);
    if margin < t.min_margin {
        return Some(AbstainReason::LowMargin {
            margin,
            min: t.min_margin,
        });
    }
    None
}

/// An [`Answer::Abstain`] for `why` that still carries **every** option, ranked from `scores`,
/// so a consumer can always show the options or escalate the same question (spec §4.6, §8).
/// Use it when a stage refuses before (or instead of) [`decide`].
pub fn abstain(scores: &Scores<'_>, why: AbstainReason) -> Answer {
    Answer::Abstain {
        why,
        ranked: Ranked::from_sorted(rank(scores)),
    }
}

/// Ranks options (confidence descending, then index ascending) and applies the profile:
/// below `min_confidence` or below `min_margin` the answer is [`Answer::Abstain`] carrying the
/// ranked list. At an exact tie the lower index ranks first (option 0 is the safe default); since
/// every profile's minimum margin is positive, an exact top-2 tie always abstains.
pub fn decide(scores: &Scores<'_>, profile: Profile) -> Answer {
    let ranked = rank(scores);
    let (top_i, top_c) = ranked[0];
    let second_c = ranked.get(1).map_or(Confidence::ZERO, |e| e.1);
    let ranked = Ranked::from_sorted(ranked);
    if let Some(why) = gate(top_c, second_c, profile) {
        return Answer::Abstain { why, ranked };
    }
    match scores.question {
        Question::Noul { .. } => Answer::Noul {
            yes: top_i.get() == 1,
            confidence: top_c,
        },
        Question::Choice { .. } => Answer::Choice {
            option: top_i,
            confidence: top_c,
            ranked,
        },
        Question::Score { .. } => Answer::Score {
            level: top_i,
            confidence: top_c,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(v: i16) -> Confidence {
        Confidence::new(v).unwrap()
    }
    fn i(v: u16) -> OptionIndex {
        OptionIndex::new(v)
    }

    #[test]
    fn index_out_of_range_is_exact() {
        let q = Question::choice("q", &["a", "b"]).unwrap();
        let mut s = Scores::new(&q);
        let e = ScoresError::IndexOutOfRange {
            index: i(2),
            arity: 2,
        };
        assert_eq!(s.set(i(2), c(1)), Err(e));
        assert_eq!(s.raise_to(i(2), c(1)), Err(e));
        assert_eq!(s.add(i(2), c(1)), Err(e));
        assert_eq!(s.sub(i(2), c(1)), Err(e));
        assert_eq!(s.get(i(2)), None);
        assert_eq!(
            e.to_string(),
            "option #2 out of range for a question with 2 options"
        );
    }

    #[test]
    fn score_ops() {
        let q = Question::choice("q", &["a", "b"]).unwrap();
        let mut s = Scores::new(&q);
        s.raise_to(i(1), c(300)).unwrap();
        s.raise_to(i(1), c(200)).unwrap();
        assert_eq!(s.get(i(1)), Some(c(300)));
        s.add(i(1), c(800)).unwrap();
        assert_eq!(s.get(i(1)), Some(Confidence::MAX));
        s.sub(i(1), c(100)).unwrap();
        assert_eq!(s.get(i(1)), Some(c(900)));
        s.set(i(0), c(5)).unwrap();
        assert_eq!(s.values(), &[c(5), c(900)]);
        assert_eq!(s.question(), &q);
    }

    #[test]
    fn choice_wins_above_thresholds() {
        let q = Question::choice("q", &["queue", "steer", "interrupt"]).unwrap();
        let mut s = Scores::new(&q);
        s.set(i(1), c(800)).unwrap();
        s.set(i(2), c(600)).unwrap();
        let a = decide(&s, Profile::Standard);
        let Answer::Choice {
            option,
            confidence,
            ranked,
        } = &a
        else {
            panic!("{a:?}")
        };
        assert_eq!((*option, *confidence), (i(1), c(800)));
        assert_eq!(
            ranked.entries(),
            &[(i(1), c(800)), (i(2), c(600)), (i(0), c(0))]
        );
        assert_eq!(a.chosen(), Some(i(1)));
        assert!(!a.is_abstain());
    }

    #[test]
    fn boundaries_are_inclusive_minimums() {
        let q = Question::choice("q", &["a", "b"]).unwrap();
        let mut s = Scores::new(&q);
        // Exactly min confidence and exactly min margin: answers.
        s.set(i(1), c(750)).unwrap();
        s.set(i(0), c(600)).unwrap();
        assert!(!decide(&s, Profile::Standard).is_abstain());
        // One below min confidence.
        s.set(i(1), c(749)).unwrap();
        s.set(i(0), c(0)).unwrap();
        assert_eq!(
            decide(&s, Profile::Standard),
            Answer::Abstain {
                why: AbstainReason::LowConfidence {
                    top: c(749),
                    min: c(750)
                },
                ranked: Ranked::from_sorted(vec![(i(1), c(749)), (i(0), c(0))]),
            }
        );
        // One below min margin.
        s.set(i(1), c(900)).unwrap();
        s.set(i(0), c(751)).unwrap();
        assert!(matches!(
            decide(&s, Profile::Standard),
            Answer::Abstain { why: AbstainReason::LowMargin { margin, min }, .. }
                if margin == c(149) && min == c(150)
        ));
    }

    #[test]
    fn exact_tie_ranks_option_zero_first_and_abstains() {
        let q = Question::choice("q", &["a", "b", "c"]).unwrap();
        let mut s = Scores::new(&q);
        s.set(i(2), c(900)).unwrap();
        s.set(i(0), c(900)).unwrap();
        let a = decide(&s, Profile::Deep);
        let Answer::Abstain { why, ranked } = a else {
            panic!()
        };
        assert_eq!(
            why,
            AbstainReason::LowMargin {
                margin: c(0),
                min: c(100)
            }
        );
        assert_eq!(ranked.top(), Some((i(0), c(900))));
    }

    #[test]
    fn noul_and_score_shapes() {
        let q = Question::noul("ok").unwrap();
        let mut s = Scores::new(&q);
        s.set(i(1), c(950)).unwrap();
        assert_eq!(
            decide(&s, Profile::Fast),
            Answer::Noul {
                yes: true,
                confidence: c(950)
            }
        );
        s.set(i(1), c(0)).unwrap();
        s.set(i(0), c(950)).unwrap();
        let a = decide(&s, Profile::Fast);
        assert_eq!(
            a,
            Answer::Noul {
                yes: false,
                confidence: c(950)
            }
        );
        assert_eq!(a.chosen(), Some(i(0)));
        let q = Question::score("lvl", &["lo", "hi"]).unwrap();
        let mut s = Scores::new(&q);
        s.set(i(1), c(800)).unwrap();
        let a = decide(&s, Profile::Standard);
        assert_eq!(
            a,
            Answer::Score {
                level: i(1),
                confidence: c(800)
            }
        );
        assert_eq!(a.chosen(), Some(i(1)));
    }

    #[test]
    fn abstain_carries_every_option_ranked() {
        let q = Question::choice("q", &["a", "b", "c"]).unwrap();
        let mut s = Scores::new(&q);
        s.set(i(2), c(300)).unwrap();
        assert_eq!(
            abstain(&s, AbstainReason::Confusable),
            Answer::Abstain {
                why: AbstainReason::Confusable,
                ranked: Ranked::from_sorted(vec![(i(2), c(300)), (i(0), c(0)), (i(1), c(0))]),
            }
        );
    }

    #[test]
    fn abstain_reason_display() {
        assert_eq!(
            AbstainReason::LowConfidence {
                top: c(1),
                min: c(2)
            }
            .to_string(),
            "low confidence 1 < 2"
        );
        assert_eq!(
            AbstainReason::LowMargin {
                margin: c(1),
                min: c(2)
            }
            .to_string(),
            "low margin 1 < 2"
        );
        assert_eq!(AbstainReason::NoCandidates.to_string(), "no candidates");
        assert_eq!(AbstainReason::NotConverged.to_string(), "not converged");
        assert_eq!(
            AbstainReason::Confusable.to_string(),
            "confusable token matched a guarded term"
        );
    }
}
