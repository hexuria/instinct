//! Candidates are a SET (spec §5 rule 4): sorted by id, unique. Input order never matters.

use core::fmt;

use crate::{Answer, OptionIndex, Question, QuestionError};

/// Maximum candidate id length in bytes.
pub const MAX_ID_BYTES: usize = 256;

/// Errors from building candidates.
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

/// A finite candidate set: ids sorted ascending and unique, so input order can never matter.
///
/// The set turns into a [`Question::Choice`] whose option `i` is the `i`-th id, which lets every
/// candidate decision go through the one gate in [`crate::decide`]: exact ties rank the lower
/// id first and, because every profile margin is positive, abstain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CandidateSet(Box<[CandidateId]>);

impl CandidateSet {
    /// Builds the set.
    ///
    /// # Errors
    /// [`CandidateError::Duplicate`] when an id appears twice.
    pub fn new(ids: impl IntoIterator<Item = CandidateId>) -> Result<Self, CandidateError> {
        let mut v: Vec<CandidateId> = ids.into_iter().collect();
        v.sort();
        if let Some(w) = v.windows(2).find(|w| w[0] == w[1]) {
            return Err(CandidateError::Duplicate(w[0].clone()));
        }
        Ok(Self(v.into_boxed_slice()))
    }

    /// The ids, ascending.
    pub fn ids(&self) -> &[CandidateId] {
        &self.0
    }

    /// Number of candidates.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A `Choice` named `name` whose options are the ids in ascending order.
    ///
    /// # Errors
    /// [`QuestionError`] when the set has fewer than 2 or too many candidates.
    pub fn question(&self, name: &str) -> Result<Question, QuestionError> {
        let labels: Vec<&str> = self.0.iter().map(CandidateId::as_str).collect();
        Question::choice(name, &labels)
    }

    /// The id behind option `i`.
    pub fn id(&self, i: OptionIndex) -> Option<&CandidateId> {
        self.0.get(usize::from(i.get()))
    }

    /// The option index of `id`.
    pub fn index_of(&self, id: &str) -> Option<OptionIndex> {
        self.0
            .binary_search_by(|c| c.as_str().cmp(id))
            .ok()
            .and_then(|i| u16::try_from(i).ok())
            .map(OptionIndex::new)
    }

    /// The chosen id. `None` for [`Answer::Abstain`] (and for Noul / Score answers), so a caller
    /// cannot act on an abstain by accident.
    pub fn chosen(&self, answer: &Answer) -> Option<&CandidateId> {
        match answer {
            Answer::Choice { option, .. } => self.id(*option),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> CandidateId {
        CandidateId::new(s).unwrap()
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
        let r = CandidateSet::new([id("a"), id("b"), id("a")]);
        assert_eq!(r, Err(CandidateError::Duplicate(id("a"))));
    }

    #[test]
    fn set_is_sorted_and_maps_both_ways() {
        let s = CandidateSet::new([id("fs.write"), id("fs.read"), id("git.log")]).unwrap();
        let ids: Vec<&str> = s.ids().iter().map(CandidateId::as_str).collect();
        assert_eq!(ids, ["fs.read", "fs.write", "git.log"]);
        assert_eq!((s.len(), s.is_empty()), (3, false));
        assert_eq!(s.index_of("fs.write"), Some(OptionIndex::new(1)));
        assert_eq!(s.index_of("nope"), None);
        assert_eq!(s.id(OptionIndex::new(2)), Some(&id("git.log")));
        assert_eq!(s.id(OptionIndex::new(3)), None);
        let q = s.question("tool").unwrap();
        assert_eq!(q.arity(), 3);
        assert_eq!(q.options().unwrap().labels()[0].as_str(), "fs.read");
        let empty = CandidateSet::new([]).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.question("t"), Err(QuestionError::TooFewOptions(0)));
    }

    #[test]
    fn index_of_is_unaddressable_past_u16_max() {
        // A set may hold more than 65536 ids, but an OptionIndex can only name the
        // first 65536 — ids past that sort into the set yet cannot be addressed.
        let ids: Vec<CandidateId> = (0..=usize::from(u16::MAX))
            .map(|i| id(&format!("id_{i:05}")))
            .collect();
        let s = CandidateSet::new(ids).unwrap();
        assert_eq!(s.len(), usize::from(u16::MAX) + 1);
        assert_eq!(s.index_of("id_00000"), Some(OptionIndex::new(0)));
        assert_eq!(s.index_of("id_65535"), Some(OptionIndex::new(u16::MAX)));
        // The last id is a member but has no representable index.
        assert_eq!(s.index_of("id_65536"), None);
        assert_eq!(
            s.question("q"),
            Err(QuestionError::TooManyOptions(usize::from(u16::MAX) + 1))
        );
    }

    #[test]
    fn chosen_is_none_on_abstain() {
        use crate::{AbstainReason, Confidence, Profile, Scores, abstain, decide};
        let s = CandidateSet::new([id("a"), id("b")]).unwrap();
        let q = s.question("t").unwrap();
        let mut sc = Scores::new(&q);
        sc.set(OptionIndex::new(1), Confidence::new(900).unwrap())
            .unwrap();
        assert_eq!(s.chosen(&decide(&sc, Profile::Standard)), Some(&id("b")));
        sc.set(OptionIndex::new(0), Confidence::new(850).unwrap())
            .unwrap();
        let a = decide(&sc, Profile::Standard);
        assert!(a.is_abstain());
        assert_eq!(s.chosen(&a), None);
        assert_eq!(s.chosen(&abstain(&sc, AbstainReason::NoCandidates)), None);
        let n = Question::noul("n").unwrap();
        let mut ns = Scores::new(&n);
        ns.set(OptionIndex::new(1), Confidence::MAX).unwrap();
        assert_eq!(s.chosen(&decide(&ns, Profile::Fast)), None);
    }
}
