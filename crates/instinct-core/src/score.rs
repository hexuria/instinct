//! Integer scores. Two newtypes on purpose (ADR 0002): a similarity may be negative, a
//! confidence may not, and the type system keeps them apart.

use core::fmt;

/// Errors from constructing a score out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScoreError {
    /// A similarity outside −1000..=1000.
    SimilarityOutOfRange(i32),
    /// A confidence outside 0..=1000.
    ConfidenceOutOfRange(i32),
}

impl fmt::Display for ScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SimilarityOutOfRange(v) => write!(f, "similarity {v} outside -1000..=1000"),
            Self::ConfidenceOutOfRange(v) => write!(f, "confidence {v} outside 0..=1000"),
        }
    }
}

impl std::error::Error for ScoreError {}

/// A similarity on a −1000..=1000 scale (e.g. hypervector similarity). Not a probability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "i16", into = "i16"))]
pub struct Millis(i16);

impl Millis {
    /// −1000.
    pub const MIN: Self = Self(-1000);
    /// 0.
    pub const ZERO: Self = Self(0);
    /// 1000.
    pub const MAX: Self = Self(1000);

    /// Validates `v` against −1000..=1000.
    ///
    /// # Errors
    /// [`ScoreError::SimilarityOutOfRange`] when `v` is outside the range.
    pub const fn new(v: i16) -> Result<Self, ScoreError> {
        if v < -1000 || v > 1000 {
            return Err(ScoreError::SimilarityOutOfRange(v as i32));
        }
        Ok(Self(v))
    }

    /// Clamps any `i32` into −1000..=1000.
    pub const fn saturating(v: i32) -> Self {
        let c = if v < -1000 {
            -1000
        } else if v > 1000 {
            1000
        } else {
            v
        };
        #[allow(clippy::cast_possible_truncation)] // c is within −1000..=1000
        Self(c as i16)
    }

    /// The raw value.
    pub const fn get(self) -> i16 {
        self.0
    }

    /// Negative similarity becomes zero confidence; positive similarity maps 1:1.
    pub const fn to_confidence(self) -> Confidence {
        Confidence::saturating(self.0 as i32)
    }
}

impl TryFrom<i16> for Millis {
    type Error = ScoreError;
    fn try_from(v: i16) -> Result<Self, ScoreError> {
        Self::new(v)
    }
}

impl From<Millis> for i16 {
    fn from(m: Millis) -> i16 {
        m.0
    }
}

impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A confidence on a 0..=1000 scale. Deterministic and comparable within one data version and
/// `DataVersion`; **not** a calibrated probability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "i16", into = "i16"))]
pub struct Confidence(i16);

impl Confidence {
    /// 0.
    pub const ZERO: Self = Self(0);
    /// 1000.
    pub const MAX: Self = Self(1000);

    /// Validates `v` against 0..=1000.
    ///
    /// # Errors
    /// [`ScoreError::ConfidenceOutOfRange`] when `v` is outside the range.
    pub const fn new(v: i16) -> Result<Self, ScoreError> {
        if v < 0 || v > 1000 {
            return Err(ScoreError::ConfidenceOutOfRange(v as i32));
        }
        Ok(Self(v))
    }

    /// Clamps any `i32` into 0..=1000.
    pub const fn saturating(v: i32) -> Self {
        let c = if v < 0 {
            0
        } else if v > 1000 {
            1000
        } else {
            v
        };
        #[allow(clippy::cast_possible_truncation)] // c is within 0..=1000
        Self(c as i16)
    }

    /// The raw value.
    pub const fn get(self) -> i16 {
        self.0
    }

    /// `self + other`, clamped to 1000.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self::saturating(self.0 as i32 + other.0 as i32)
    }

    /// `self - other`, clamped to 0.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self::saturating(self.0 as i32 - other.0 as i32)
    }

    /// Integer halving (rounds toward zero), used by the question damper (spec §4.4).
    #[must_use]
    pub const fn halved(self) -> Self {
        Self(self.0 / 2)
    }
}

impl TryFrom<i16> for Confidence {
    type Error = ScoreError;
    fn try_from(v: i16) -> Result<Self, ScoreError> {
        Self::new(v)
    }
}

impl From<Confidence> for i16 {
    fn from(c: Confidence) -> i16 {
        c.0
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_bounds_are_exact() {
        assert_eq!(Millis::new(-1000), Ok(Millis::MIN));
        assert_eq!(Millis::new(1000), Ok(Millis::MAX));
        assert_eq!(
            Millis::new(1001),
            Err(ScoreError::SimilarityOutOfRange(1001))
        );
        assert_eq!(
            Millis::new(-1001),
            Err(ScoreError::SimilarityOutOfRange(-1001))
        );
        assert_eq!(Millis::saturating(i32::MAX), Millis::MAX);
        assert_eq!(Millis::saturating(i32::MIN), Millis::MIN);
        assert_eq!(Millis::saturating(-7).get(), -7);
    }

    #[test]
    fn confidence_bounds_are_exact() {
        assert_eq!(Confidence::new(0), Ok(Confidence::ZERO));
        assert_eq!(Confidence::new(1000), Ok(Confidence::MAX));
        assert_eq!(
            Confidence::new(-1),
            Err(ScoreError::ConfidenceOutOfRange(-1))
        );
        assert_eq!(
            Confidence::new(1001),
            Err(ScoreError::ConfidenceOutOfRange(1001))
        );
        assert_eq!(Confidence::saturating(5000), Confidence::MAX);
        assert_eq!(Confidence::saturating(-5), Confidence::ZERO);
    }

    #[test]
    fn confidence_arithmetic_saturates() {
        let c = |v| Confidence::new(v).unwrap();
        assert_eq!(c(900).saturating_add(c(300)), Confidence::MAX);
        assert_eq!(c(100).saturating_sub(c(300)), Confidence::ZERO);
        assert_eq!(c(301).halved(), c(150));
        assert_eq!(Millis::new(-400).unwrap().to_confidence(), Confidence::ZERO);
        assert_eq!(Millis::new(400).unwrap().to_confidence(), c(400));
    }

    #[test]
    fn errors_display() {
        assert_eq!(
            ScoreError::ConfidenceOutOfRange(-1).to_string(),
            "confidence -1 outside 0..=1000"
        );
        assert_eq!(
            ScoreError::SimilarityOutOfRange(2000).to_string(),
            "similarity 2000 outside -1000..=1000"
        );
    }
}
