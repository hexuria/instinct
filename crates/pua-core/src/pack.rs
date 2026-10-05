//! The `Pack` trait and `Decision` (spec §4.1).

use crate::{Answer, DataVersion, Profile, Question, Trail};

/// An answer plus its trail, stamped with the pack's [`DataVersion`] and the profile used.
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
    /// The pack data version.
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

/// A pack: embedded data + an adapter that answers one fixed question.
///
/// Implementations must be pure: same `input` + same `profile` gives a byte-identical
/// [`Decision`] for a given [`Pack::data_version`].
pub trait Pack: Send + Sync {
    /// The pack's input (e.g. a chat message plus live runs).
    type Input<'a>
    where
        Self: 'a;

    /// The question this pack answers.
    fn question(&self) -> &Question;

    /// Digest of everything that can change an answer.
    fn data_version(&self) -> DataVersion;

    /// Answers the question for `input`.
    fn ask(&self, input: &Self::Input<'_>, profile: Profile) -> Decision;
}
