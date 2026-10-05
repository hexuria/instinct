//! `Decision`: what a consumer journals (spec §4.1).

use crate::{Answer, DataVersion, Profile, Trail};

/// An answer plus its trail, stamped with the consumer's [`DataVersion`] and the profile used.
/// This is what consumers journal (spec §5 rule 6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Decision {
    answer: Answer,
    profile: Profile,
    data_version: DataVersion,
    trail: Trail,
}

impl Decision {
    /// Assembles a decision.
    pub fn new(answer: Answer, profile: Profile, data_version: DataVersion, trail: Trail) -> Self {
        Self {
            answer,
            profile,
            data_version,
            trail,
        }
    }
    /// The answer.
    pub fn answer(&self) -> &Answer {
        &self.answer
    }
    /// The profile used.
    pub fn profile(&self) -> Profile {
        self.profile
    }
    /// The data version.
    pub fn data_version(&self) -> DataVersion {
        self.data_version
    }
    /// The trail.
    pub fn trail(&self) -> &Trail {
        &self.trail
    }
    /// Splits into parts.
    pub fn into_parts(self) -> (Answer, Profile, DataVersion, Trail) {
        (self.answer, self.profile, self.data_version, self.trail)
    }
}
