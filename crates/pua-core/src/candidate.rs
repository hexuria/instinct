//! Candidates are a SET (spec §5 rule 4): ordered by score descending, then id ascending.
//! Input order never matters.

use core::fmt;

use crate::{AbstainReason, Confidence, Profile};

/// Maximum candidate id length in bytes.
pub const MAX_ID_BYTES: usize = 256;

/// Errors from building or ranking candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CandidateError {
    /// Empty id.
    EmptyId,
    /// Id longer than [`MAX_ID_BYTES`].
    IdTooLong(usize),
    /// The same id was given twice (a set has no duplicates).
    Duplicate(CandidateId),
}

impl fmt::Display for CandidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId => f.write_str("candidate id is empty"),
            Self::IdTooLong(n) => write!(f, "candidate id is {n} bytes (max {MAX_ID_BYTES})"),
            Self::Duplicate(id) => write!(f, "candidate {id} given twice"),
        }
    }
}

impl std::error::Error for CandidateError {}

/// A candidate id (run id, tool id, codebook id). Ids cross crate boundaries as strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct CandidateId(Box<str>);

impl CandidateId {
    /// Validates an id.
    ///
    /// # Errors
    /// [`CandidateError::EmptyId`] or [`CandidateError::IdTooLong`].
    pub fn new(s: &str) -> Result<Self, CandidateError> {
        if s.is_empty() {
            return Err(CandidateError::EmptyId);
        }
        if s.len() > MAX_ID_BYTES {
            return Err(CandidateError::IdTooLong(s.len()));
        }
        Ok(Self(s.into()))
    }

    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CandidateId {
    type Error = CandidateError;
    fn try_from(s: String) -> Result<Self, CandidateError> {
        Self::new(&s)
    }
}

impl From<CandidateId> for String {
    fn from(c: CandidateId) -> String {
        c.0.into()
    }
}

/// Candidates in canonical order: confidence descending, then id ascending.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RankedCandidates(Box<[(CandidateId, Confidence)]>);

/// Ranks a candidate set.
///
/// # Errors
/// [`CandidateError::Duplicate`] when an id appears twice.
pub fn rank_candidates(
    items: impl IntoIterator<Item = (CandidateId, Confidence)>,
) -> Result<RankedCandidates, CandidateError> {
    let mut v: Vec<(CandidateId, Confidence)> = items.into_iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(w) = v.windows(2).find(|w| w[0].0 == w[1].0) {
        return Err(CandidateError::Duplicate(w[0].0.clone()));
    }
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(RankedCandidates(v.into_boxed_slice()))
}

/// The outcome of picking one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "pick", rename_all = "snake_case"))]
pub enum CandidatePick {
    /// A candidate cleared both thresholds.
    Picked {
        /// The chosen id.
        id: CandidateId,
        /// Its confidence.
        confidence: Confidence,
        /// Gap to the runner-up (or to zero when alone).
        margin: Confidence,
    },
    /// Not sure; the ranked set is returned for chips or escalation.
    Abstain {
        /// Why.
        why: AbstainReason,
    },
}

impl RankedCandidates {
    /// The ranked entries.
    pub fn entries(&self) -> &[(CandidateId, Confidence)] {
        &self.0
    }

    /// Number of candidates.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no candidates.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Applies the profile thresholds (same rules as [`crate::decide`]).
    pub fn pick(&self, profile: Profile) -> CandidatePick {
        let t = profile.thresholds();
        let Some((top_id, top_c)) = self.0.first() else {
            return CandidatePick::Abstain {
                why: AbstainReason::NoCandidates,
            };
        };
        let second = self.0.get(1).map_or(Confidence::ZERO, |e| e.1);
        let margin = top_c.saturating_sub(second);
        if *top_c < t.min_confidence {
            return CandidatePick::Abstain {
                why: AbstainReason::LowConfidence {
                    top: *top_c,
                    min: t.min_confidence,
                },
            };
        }
        if margin < t.min_margin {
            return CandidatePick::Abstain {
                why: AbstainReason::LowMargin {
                    margin,
                    min: t.min_margin,
                },
            };
        }
        CandidatePick::Picked {
            id: top_id.clone(),
            confidence: *top_c,
            margin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> CandidateId {
        CandidateId::new(s).unwrap()
    }
    fn c(v: i16) -> Confidence {
        Confidence::new(v).unwrap()
    }

    #[test]
    fn id_errors_are_exact() {
        assert_eq!(CandidateId::new(""), Err(CandidateError::EmptyId));
        let long = "x".repeat(MAX_ID_BYTES + 1);
        assert_eq!(
            CandidateId::new(&long),
            Err(CandidateError::IdTooLong(MAX_ID_BYTES + 1))
        );
        assert_eq!(CandidateError::EmptyId.to_string(), "candidate id is empty");
        assert_eq!(
            CandidateError::Duplicate(id("a")).to_string(),
            "candidate a given twice"
        );
    }

    #[test]
    fn duplicate_is_an_error() {
        let r = rank_candidates([(id("a"), c(1)), (id("b"), c(2)), (id("a"), c(3))]);
        assert_eq!(r, Err(CandidateError::Duplicate(id("a"))));
    }

    #[test]
    fn order_is_score_desc_then_id_asc() {
        let r = rank_candidates([(id("b"), c(5)), (id("a"), c(5)), (id("c"), c(9))]).unwrap();
        let ids: Vec<&str> = r.entries().iter().map(|e| e.0.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(r.len(), 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn pick_rules() {
        let empty = rank_candidates([]).unwrap();
        assert_eq!(
            empty.pick(Profile::Deep),
            CandidatePick::Abstain {
                why: AbstainReason::NoCandidates
            }
        );
        let alone = rank_candidates([(id("a"), c(800))]).unwrap();
        assert_eq!(
            alone.pick(Profile::Standard),
            CandidatePick::Picked {
                id: id("a"),
                confidence: c(800),
                margin: c(800)
            }
        );
        let low = rank_candidates([(id("a"), c(700))]).unwrap();
        assert_eq!(
            low.pick(Profile::Standard),
            CandidatePick::Abstain {
                why: AbstainReason::LowConfidence {
                    top: c(700),
                    min: c(750)
                }
            }
        );
        let close = rank_candidates([(id("a"), c(900)), (id("b"), c(800))]).unwrap();
        assert_eq!(
            close.pick(Profile::Standard),
            CandidatePick::Abstain {
                why: AbstainReason::LowMargin {
                    margin: c(100),
                    min: c(150)
                }
            }
        );
    }
}
