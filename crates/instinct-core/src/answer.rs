//! Answers, the ranked list and abstain reasons.

use core::fmt;

use crate::{Confidence, OptionIndex};

/// Why Instinct declined to answer. An abstain always carries the ranked options so a consumer can show
/// chips or escalate the same question (spec §4.6, §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
#[non_exhaustive]
pub enum AbstainReason {
    /// The best score is below the profile's minimum confidence.
    LowConfidence {
        /// Best score.
        top: Confidence,
        /// Profile minimum.
        min: Confidence,
    },
    /// The gap between the best and second-best score is below the profile's minimum margin.
    LowMargin {
        /// Top-2 margin.
        margin: Confidence,
        /// Profile minimum.
        min: Confidence,
    },
    /// There was nothing to choose from.
    NoCandidates,
    /// An iterative stage (resonator) did not reach a fixed point within its cap.
    NotConverged,
    /// A confusable (homoglyph / mixed-script) token matched a guarded term.
    Confusable,
}

impl fmt::Display for AbstainReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowConfidence { top, min } => write!(f, "low confidence {top} < {min}"),
            Self::LowMargin { margin, min } => write!(f, "low margin {margin} < {min}"),
            Self::NoCandidates => f.write_str("no candidates"),
            Self::NotConverged => f.write_str("not converged"),
            Self::Confusable => f.write_str("confusable token matched a guarded term"),
        }
    }
}

/// Options ranked by confidence descending, then index ascending (spec §5 rule 4). Only
/// constructed by [`crate::decide`] (or validated deserialization), so it is always sorted and
/// free of duplicate indices.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        try_from = "Vec<(OptionIndex, Confidence)>",
        into = "Vec<(OptionIndex, Confidence)>"
    )
)]
pub struct Ranked(Box<[(OptionIndex, Confidence)]>);

impl Ranked {
    pub(crate) fn from_sorted(v: Vec<(OptionIndex, Confidence)>) -> Self {
        debug_assert!(is_canonical(&v), "Ranked must be sorted and unique");
        Self(v.into_boxed_slice())
    }

    /// The ranked entries.
    pub fn entries(&self) -> &[(OptionIndex, Confidence)] {
        &self.0
    }

    /// The best entry.
    pub fn top(&self) -> Option<(OptionIndex, Confidence)> {
        self.0.first().copied()
    }
}

fn is_canonical(v: &[(OptionIndex, Confidence)]) -> bool {
    let sorted = v
        .windows(2)
        .all(|w| (w[0].1 > w[1].1) || (w[0].1 == w[1].1 && w[0].0 < w[1].0));
    let mut idx: Vec<OptionIndex> = v.iter().map(|e| e.0).collect();
    idx.sort_unstable();
    sorted && idx.windows(2).all(|w| w[0] != w[1])
}

impl TryFrom<Vec<(OptionIndex, Confidence)>> for Ranked {
    type Error = &'static str;
    fn try_from(v: Vec<(OptionIndex, Confidence)>) -> Result<Self, Self::Error> {
        if is_canonical(&v) {
            Ok(Self(v.into_boxed_slice()))
        } else {
            Err("ranked list must be sorted by confidence desc, index asc, with unique indices")
        }
    }
}

impl From<Ranked> for Vec<(OptionIndex, Confidence)> {
    fn from(r: Ranked) -> Self {
        r.0.into_vec()
    }
}

/// Instinct's answer to a [`crate::Question`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "answer", rename_all = "snake_case"))]
pub enum Answer {
    /// Yes or no.
    Noul {
        /// The answer.
        yes: bool,
        /// Confidence of the chosen side.
        confidence: Confidence,
    },
    /// One option.
    Choice {
        /// Chosen option.
        option: OptionIndex,
        /// Its confidence.
        confidence: Confidence,
        /// All options, ranked.
        ranked: Ranked,
    },
    /// One rubric level.
    Score {
        /// Chosen level.
        level: OptionIndex,
        /// Its confidence.
        confidence: Confidence,
    },
    /// Not sure. Consumers treat this as "use today's default" or escalate (spec §8).
    Abstain {
        /// Why.
        why: AbstainReason,
        /// All options, ranked.
        ranked: Ranked,
    },
}

impl Answer {
    /// Whether this is an abstain.
    pub fn is_abstain(&self) -> bool {
        matches!(self, Self::Abstain { .. })
    }

    /// The chosen option or level index, if any (Noul maps to 0 = no, 1 = yes).
    pub fn chosen(&self) -> Option<OptionIndex> {
        match self {
            Self::Noul { yes, .. } => Some(OptionIndex::new(u16::from(*yes))),
            Self::Choice { option, .. } => Some(*option),
            Self::Score { level, .. } => Some(*level),
            Self::Abstain { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(v: u16) -> OptionIndex {
        OptionIndex::new(v)
    }

    fn c(v: i16) -> Confidence {
        Confidence::new(v).unwrap()
    }

    #[test]
    fn ranked_try_from_rejects_non_canonical_orders() {
        let err = "ranked list must be sorted by confidence desc, index asc, with unique indices";
        // A duplicated index is not canonical even when the confidences sort.
        assert_eq!(
            Ranked::try_from(vec![(i(0), c(500)), (i(0), c(400))]),
            Err(err)
        );
        // Equal confidences must rank the lower index first.
        assert_eq!(
            Ranked::try_from(vec![(i(1), c(500)), (i(0), c(500))]),
            Err(err)
        );
        // Ascending confidence is not canonical.
        assert_eq!(
            Ranked::try_from(vec![(i(0), c(400)), (i(1), c(500))]),
            Err(err)
        );
        // The same pairs in canonical order pass; an empty list is canonical.
        assert!(Ranked::try_from(vec![(i(0), c(500)), (i(1), c(400))]).is_ok());
        assert!(Ranked::try_from(vec![(i(0), c(500)), (i(1), c(500))]).is_ok());
        assert!(Ranked::try_from(vec![]).is_ok());
    }
}
