//! Profiles are tables, not code paths (spec §4.6).

use crate::Confidence;

/// How far the HDC stage runs under a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HdcMode {
    /// No HDC stage (text, lexicon, rules only).
    Off,
    /// Codebook cleanup only.
    CleanupOnly,
    /// Cleanup plus the resonator.
    Resonator,
}

/// A decision profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Profile {
    /// Strict thresholds, no HDC.
    Fast,
    /// The default.
    #[default]
    Standard,
    /// Looser thresholds, resonator on.
    Deep,
}

/// The threshold row of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Thresholds {
    /// Minimum confidence of the best option.
    pub min_confidence: Confidence,
    /// Minimum top-2 margin.
    pub min_margin: Confidence,
    /// HDC stage mode.
    pub hdc: HdcMode,
}

const fn c(v: i16) -> Confidence {
    Confidence::saturating(v as i32)
}

impl Profile {
    /// All profiles, in table order.
    pub const ALL: [Self; 3] = [Self::Fast, Self::Standard, Self::Deep];

    /// The threshold row (spec §4.6 table).
    pub const fn thresholds(self) -> Thresholds {
        match self {
            Self::Fast => Thresholds {
                min_confidence: c(850),
                min_margin: c(200),
                hdc: HdcMode::Off,
            },
            Self::Standard => Thresholds {
                min_confidence: c(750),
                min_margin: c(150),
                hdc: HdcMode::CleanupOnly,
            },
            Self::Deep => Thresholds {
                min_confidence: c(650),
                min_margin: c(100),
                hdc: HdcMode::Resonator,
            },
        }
    }

    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }

    /// Canonical bytes of the whole profile table, folded into `DataVersion` by packs.
    pub fn table_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        for p in Self::ALL {
            let t = p.thresholds();
            out.extend_from_slice(p.name().as_bytes());
            out.push(0x1f);
            out.extend_from_slice(&t.min_confidence.get().to_le_bytes());
            out.extend_from_slice(&t.min_margin.get().to_le_bytes());
            out.push(match t.hdc {
                HdcMode::Off => 0,
                HdcMode::CleanupOnly => 1,
                HdcMode::Resonator => 2,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_spec() {
        let rows: Vec<_> = Profile::ALL
            .iter()
            .map(|p| {
                let t = p.thresholds();
                (p.name(), t.min_confidence.get(), t.min_margin.get(), t.hdc)
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("fast", 850, 200, HdcMode::Off),
                ("standard", 750, 150, HdcMode::CleanupOnly),
                ("deep", 650, 100, HdcMode::Resonator),
            ]
        );
        assert_eq!(Profile::default(), Profile::Standard);
        assert_eq!(
            Profile::table_bytes().len(),
            3 * (1 + 4 + 1) + "faststandarddeep".len()
        );
    }
}
