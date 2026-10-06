//! Byte spans into the ORIGINAL text (spec §4.1, §4.2).

use core::fmt;
use core::ops::Range;

/// Errors from building a [`Span`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpanError {
    /// `start > end`.
    Inverted {
        /// Start offset.
        start: u32,
        /// End offset.
        end: u32,
    },
    /// An offset does not fit in `u32` (inputs over 4 GiB are not supported).
    TooLarge,
}

impl fmt::Display for SpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inverted { start, end } => write!(f, "span start {start} > end {end}"),
            Self::TooLarge => f.write_str("span offset exceeds u32"),
        }
    }
}

impl std::error::Error for SpanError {}

/// A half-open byte range `start..end` in the original text. `start <= end` is guaranteed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "RawSpan", into = "RawSpan"))]
pub struct Span {
    start: u32,
    end: u32,
}

impl Span {
    /// Builds a span.
    ///
    /// # Errors
    /// [`SpanError::Inverted`] when `start > end`.
    pub const fn new(start: u32, end: u32) -> Result<Self, SpanError> {
        if start > end {
            return Err(SpanError::Inverted { start, end });
        }
        Ok(Self { start, end })
    }

    /// Builds a span from a `usize` range.
    ///
    /// # Errors
    /// [`SpanError::TooLarge`] when an offset exceeds `u32`, [`SpanError::Inverted`] when
    /// `start > end`.
    pub fn from_range(r: Range<usize>) -> Result<Self, SpanError> {
        let start = u32::try_from(r.start).map_err(|_| SpanError::TooLarge)?;
        let end = u32::try_from(r.end).map_err(|_| SpanError::TooLarge)?;
        Self::new(start, end)
    }

    /// Start offset (inclusive).
    pub const fn start(self) -> u32 {
        self.start
    }

    /// End offset (exclusive).
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Length in bytes.
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span is empty.
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The span as a `usize` range.
    pub const fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    /// The covered text, or `None` if the span is out of bounds or splits a UTF-8 character.
    pub fn slice(self, text: &str) -> Option<&str> {
        text.get(self.range())
    }

    /// Whether `other` lies entirely inside `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Whether the two spans share at least one byte.
    pub const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// The smallest span covering both.
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy)]
#[allow(unreachable_pub, dead_code)]
struct RawSpan {
    start: u32,
    end: u32,
}

impl TryFrom<RawSpan> for Span {
    type Error = SpanError;
    fn try_from(r: RawSpan) -> Result<Self, SpanError> {
        Self::new(r.start, r.end)
    }
}

impl From<Span> for RawSpan {
    fn from(s: Span) -> Self {
        Self {
            start: s.start,
            end: s.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_errors() {
        assert_eq!(
            Span::new(3, 2),
            Err(SpanError::Inverted { start: 3, end: 2 })
        );
        assert_eq!(
            Span::from_range(usize::MAX..usize::MAX),
            Err(SpanError::TooLarge)
        );
        assert_eq!(Span::from_range(0..usize::MAX), Err(SpanError::TooLarge));
        let s = Span::new(2, 5).unwrap();
        assert_eq!(
            (s.start(), s.end(), s.len(), s.is_empty()),
            (2, 5, 3, false)
        );
        assert!(Span::new(4, 4).unwrap().is_empty());
        assert_eq!(s.to_string(), "2..5");
        assert_eq!(SpanError::TooLarge.to_string(), "span offset exceeds u32");
        assert_eq!(
            SpanError::Inverted { start: 3, end: 2 }.to_string(),
            "span start 3 > end 2"
        );
    }

    #[test]
    fn slice_respects_char_boundaries() {
        let t = "PEÑA";
        assert_eq!(Span::new(0, 2).unwrap().slice(t), Some("PE"));
        assert_eq!(Span::new(2, 4).unwrap().slice(t), Some("Ñ"));
        assert_eq!(Span::new(2, 3).unwrap().slice(t), None);
        assert_eq!(Span::new(0, 99).unwrap().slice(t), None);
    }

    #[test]
    fn relations() {
        let a = Span::new(0, 10).unwrap();
        let b = Span::new(2, 4).unwrap();
        let c = Span::new(10, 12).unwrap();
        assert!(a.contains(b) && !b.contains(a));
        assert!(a.overlaps(b) && !a.overlaps(c));
        assert_eq!(b.cover(c), Span::new(2, 12).unwrap());
    }
}
