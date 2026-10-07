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
    /// The input was refused before scoring (e.g. over the size limit); never truncated.
    InputTooLong,
    /// A stage refused because the text was canonicalized with a different config than the
    /// compiled data (lexicon or rules).
    ConfigMismatch,
}

impl fmt::Display for AbstainReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LowConfidence { top, min } => write!(f, "low confidence {top} < {min}"),
            Self::LowMargin { margin, min } => write!(f, "low margin {margin} < {min}"),
            Self::NoCandidates => f.write_str("no candidates"),
            Self::NotConverged => f.write_str("not converged"),
            Self::Confusable => f.write_str("confusable token matched a guarded term"),
            Self::InputTooLong => f.write_str("input refused (too long)"),
            Self::ConfigMismatch => f.write_str("config mismatch"),
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

/// Why a [`Ranked`] list was refused (deserialization boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RankedError {
    /// An empty ranked list: an abstain always carries every option.
    Empty,
    /// Entries are not ordered by confidence descending, then index ascending.
    Unsorted,
    /// An option index appears twice.
    DuplicateIndex {
        /// The repeated index.
        index: OptionIndex,
    },
}

impl fmt::Display for RankedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("ranked list is empty"),
            Self::Unsorted => {
                f.write_str("ranked list must be sorted by confidence desc, then index asc")
            }
            Self::DuplicateIndex { index } => {
                write!(f, "ranked list repeats option index {index}")
            }
        }
    }
}

impl std::error::Error for RankedError {}

impl TryFrom<Vec<(OptionIndex, Confidence)>> for Ranked {
    type Error = RankedError;
    fn try_from(v: Vec<(OptionIndex, Confidence)>) -> Result<Self, Self::Error> {
        if v.is_empty() {
            return Err(RankedError::Empty);
        }
        let mut idx: Vec<OptionIndex> = v.iter().map(|e| e.0).collect();
        idx.sort_unstable();
        if let Some(w) = idx.windows(2).find(|w| w[0] == w[1]) {
            return Err(RankedError::DuplicateIndex { index: w[0] });
        }
        let mut canon = v.clone();
        canon.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if canon != v {
            return Err(RankedError::Unsorted);
        }
        Ok(Self(v.into_boxed_slice()))
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
        // A duplicated index is not canonical even when the confidences sort.
        assert_eq!(
            Ranked::try_from(vec![(i(0), c(500)), (i(0), c(400))]),
            Err(RankedError::DuplicateIndex { index: i(0) })
        );
        // Equal confidences must rank the lower index first.
        assert_eq!(
            Ranked::try_from(vec![(i(1), c(500)), (i(0), c(500))]),
            Err(RankedError::Unsorted)
        );
        // Ascending confidence is not canonical.
        assert_eq!(
            Ranked::try_from(vec![(i(0), c(400)), (i(1), c(500))]),
            Err(RankedError::Unsorted)
        );
        // An empty ranked list is refused: an abstain always carries every option.
        assert_eq!(Ranked::try_from(vec![]), Err(RankedError::Empty));
        // The same pairs in canonical order pass.
        assert!(Ranked::try_from(vec![(i(0), c(500)), (i(1), c(400))]).is_ok());
        assert!(Ranked::try_from(vec![(i(0), c(500)), (i(1), c(500))]).is_ok());
        assert_eq!(RankedError::Empty.to_string(), "ranked list is empty");
        assert_eq!(
            RankedError::Unsorted.to_string(),
            "ranked list must be sorted by confidence desc, then index asc"
        );
        assert_eq!(
            RankedError::DuplicateIndex { index: i(2) }.to_string(),
            "ranked list repeats option index #2"
        );
    }
}
