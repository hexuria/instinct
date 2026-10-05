//! Validated question shapes (spec §4.1): Noul, Choice, Score.

use core::fmt;

/// Maximum length of a label in bytes.
pub const MAX_LABEL_BYTES: usize = 256;

/// Errors from building a [`Question`] or [`Label`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum QuestionError {
    /// An empty or whitespace-only label.
    EmptyLabel,
    /// A label longer than [`MAX_LABEL_BYTES`].
    LabelTooLong(usize),
    /// A label with leading or trailing whitespace.
    UntrimmedLabel(String),
    /// Fewer than two options or levels.
    TooFewOptions(usize),
    /// More than `u16::MAX` options or levels.
    TooManyOptions(usize),
    /// The same option or level label appears twice.
    DuplicateOption(String),
}

impl fmt::Display for QuestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLabel => f.write_str("label is empty"),
            Self::LabelTooLong(n) => write!(f, "label is {n} bytes (max {MAX_LABEL_BYTES})"),
            Self::UntrimmedLabel(l) => write!(f, "label {l:?} has surrounding whitespace"),
            Self::TooFewOptions(n) => write!(f, "{n} options given, at least 2 required"),
            Self::TooManyOptions(n) => write!(f, "{n} options given, at most 65535 allowed"),
            Self::DuplicateOption(l) => write!(f, "option {l:?} appears more than once"),
        }
    }
}

impl std::error::Error for QuestionError {}

/// A non-empty, trimmed label of at most [`MAX_LABEL_BYTES`] bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct Label(Box<str>);

impl Label {
    /// Validates a label.
    ///
    /// # Errors
    /// [`QuestionError::EmptyLabel`], [`QuestionError::LabelTooLong`] or
    /// [`QuestionError::UntrimmedLabel`].
    pub fn new(s: &str) -> Result<Self, QuestionError> {
        if s.trim().is_empty() {
            return Err(QuestionError::EmptyLabel);
        }
        if s.len() > MAX_LABEL_BYTES {
            return Err(QuestionError::LabelTooLong(s.len()));
        }
        if s.trim() != s {
            return Err(QuestionError::UntrimmedLabel(s.to_owned()));
        }
        Ok(Self(s.into()))
    }

    /// The label text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Label {
    type Error = QuestionError;
    fn try_from(s: String) -> Result<Self, QuestionError> {
        Self::new(&s)
    }
}

impl From<Label> for String {
    fn from(l: Label) -> String {
        l.0.into()
    }
}

/// An index into the options (or levels) of one question. Only valid for the question that
/// produced it; [`Options::get`] returns `None` for an index from elsewhere that is out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct OptionIndex(u16);

impl OptionIndex {
    /// The first option: the safe default of every Choice (spec §5 rule 4).
    pub const SAFE_DEFAULT: Self = Self(0);

    /// The raw index.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Builds an index. Range-checking happens against a concrete [`Options`].
    pub const fn new(i: u16) -> Self {
        Self(i)
    }
}

impl fmt::Display for OptionIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// An ordered list of 2..=65535 distinct labels. Order matters: index 0 is the safe default.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "Vec<Label>", into = "Vec<Label>"))]
pub struct Options(Box<[Label]>);

impl Options {
    /// Validates an ordered option list.
    ///
    /// # Errors
    /// Any label error, [`QuestionError::TooFewOptions`], [`QuestionError::TooManyOptions`] or
    /// [`QuestionError::DuplicateOption`].
    pub fn new<S: AsRef<str>>(labels: &[S]) -> Result<Self, QuestionError> {
        let labels = labels
            .iter()
            .map(|s| Label::new(s.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_labels(labels)
    }

    fn from_labels(labels: Vec<Label>) -> Result<Self, QuestionError> {
        if labels.len() < 2 {
            return Err(QuestionError::TooFewOptions(labels.len()));
        }
        if labels.len() > usize::from(u16::MAX) {
            return Err(QuestionError::TooManyOptions(labels.len()));
        }
        let mut sorted: Vec<&Label> = labels.iter().collect();
        sorted.sort();
        if let Some(w) = sorted.windows(2).find(|w| w[0] == w[1]) {
            return Err(QuestionError::DuplicateOption(w[0].as_str().to_owned()));
        }
        Ok(Self(labels.into_boxed_slice()))
    }

    /// Number of options (always ≥ 2).
    pub fn len(&self) -> u16 {
        // from_labels guarantees len <= u16::MAX.
        u16::try_from(self.0.len()).unwrap_or(u16::MAX)
    }

    /// Always `false`; present for API symmetry with slices.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The label at `i`, if in range.
    pub fn get(&self, i: OptionIndex) -> Option<&Label> {
        self.0.get(usize::from(i.0))
    }

    /// The index of `label`, if present.
    pub fn index_of(&self, label: &str) -> Option<OptionIndex> {
        self.0
            .iter()
            .position(|l| l.as_str() == label)
            .and_then(|p| u16::try_from(p).ok())
            .map(OptionIndex)
    }

    /// The labels in order.
    pub fn labels(&self) -> &[Label] {
        &self.0
    }

    /// All indices in order.
    pub fn indices(&self) -> impl Iterator<Item = OptionIndex> + '_ {
        (0..self.len()).map(OptionIndex)
    }
}

impl TryFrom<Vec<Label>> for Options {
    type Error = QuestionError;
    fn try_from(v: Vec<Label>) -> Result<Self, QuestionError> {
        Self::from_labels(v)
    }
}

impl From<Options> for Vec<Label> {
    fn from(o: Options) -> Self {
        o.0.into_vec()
    }
}

/// A question in one of Jev's three shapes. The same value is what PUA answers and what a
/// consumer escalates to Jev (spec §8).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "shape", rename_all = "snake_case"))]
pub enum Question {
    /// Yes/no. Scored as two implicit options: index 0 = no (safe default), index 1 = yes.
    Noul {
        /// Question name.
        name: Label,
    },
    /// One of an ordered list of options; option 0 is the safe default.
    Choice {
        /// Question name.
        name: Label,
        /// Ordered options.
        options: Options,
    },
    /// An ordered rubric of levels.
    Score {
        /// Question name.
        name: Label,
        /// Ordered levels, lowest first.
        levels: Options,
    },
}

impl Question {
    /// A yes/no question.
    ///
    /// # Errors
    /// Label errors for `name`.
    pub fn noul(name: &str) -> Result<Self, QuestionError> {
        Ok(Self::Noul {
            name: Label::new(name)?,
        })
    }

    /// A choice question.
    ///
    /// # Errors
    /// Label errors and option-list errors.
    pub fn choice<S: AsRef<str>>(name: &str, options: &[S]) -> Result<Self, QuestionError> {
        Ok(Self::Choice {
            name: Label::new(name)?,
            options: Options::new(options)?,
        })
    }

    /// A score (rubric) question.
    ///
    /// # Errors
    /// Label errors and level-list errors.
    pub fn score<S: AsRef<str>>(name: &str, levels: &[S]) -> Result<Self, QuestionError> {
        Ok(Self::Score {
            name: Label::new(name)?,
            levels: Options::new(levels)?,
        })
    }

    /// The question name.
    pub fn name(&self) -> &Label {
        match self {
            Self::Noul { name } | Self::Choice { name, .. } | Self::Score { name, .. } => name,
        }
    }

    /// Number of scorable slots: 2 for Noul, the option or level count otherwise.
    pub fn arity(&self) -> u16 {
        match self {
            Self::Noul { .. } => 2,
            Self::Choice { options, .. } => options.len(),
            Self::Score { levels, .. } => levels.len(),
        }
    }

    /// The options of a Choice or the levels of a Score.
    pub fn options(&self) -> Option<&Options> {
        match self {
            Self::Noul { .. } => None,
            Self::Choice { options, .. } => Some(options),
            Self::Score { levels, .. } => Some(levels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_errors_are_exact() {
        assert_eq!(Label::new(""), Err(QuestionError::EmptyLabel));
        assert_eq!(Label::new("   "), Err(QuestionError::EmptyLabel));
        assert_eq!(
            Label::new(" a"),
            Err(QuestionError::UntrimmedLabel(" a".into()))
        );
        let long = "x".repeat(MAX_LABEL_BYTES + 1);
        assert_eq!(
            Label::new(&long),
            Err(QuestionError::LabelTooLong(MAX_LABEL_BYTES + 1))
        );
        assert!(Label::new(&"x".repeat(MAX_LABEL_BYTES)).is_ok());
    }

    #[test]
    fn options_errors_are_exact() {
        assert_eq!(Options::new(&["a"]), Err(QuestionError::TooFewOptions(1)));
        assert_eq!(
            Options::new::<&str>(&[]),
            Err(QuestionError::TooFewOptions(0))
        );
        assert_eq!(
            Options::new(&["a", "b", "a"]),
            Err(QuestionError::DuplicateOption("a".into()))
        );
        assert_eq!(Options::new(&["a", ""]), Err(QuestionError::EmptyLabel));
        let many: Vec<String> = (0..=usize::from(u16::MAX)).map(|i| i.to_string()).collect();
        assert_eq!(
            Options::new(&many),
            Err(QuestionError::TooManyOptions(65536))
        );
    }

    #[test]
    fn question_accessors() {
        let q = Question::choice("delivery", &["queue", "steer", "interrupt"]).unwrap();
        assert_eq!(q.name().as_str(), "delivery");
        assert_eq!(q.arity(), 3);
        let o = q.options().unwrap();
        assert_eq!(o.index_of("steer"), Some(OptionIndex::new(1)));
        assert_eq!(o.index_of("nope"), None);
        assert_eq!(
            o.get(OptionIndex::new(2)).map(Label::as_str),
            Some("interrupt")
        );
        assert_eq!(o.get(OptionIndex::new(3)), None);
        assert_eq!(o.indices().count(), 3);
        assert!(!o.is_empty());
        let n = Question::noul("ok?").unwrap();
        assert_eq!((n.arity(), n.options()), (2, None));
        let s = Question::score("quality", &["low", "mid", "high"]).unwrap();
        assert_eq!(s.arity(), 3);
        assert_eq!(s.name().to_string(), "quality");
        assert_eq!(Question::noul(""), Err(QuestionError::EmptyLabel));
        assert_eq!(
            Question::score("s", &["x"]),
            Err(QuestionError::TooFewOptions(1))
        );
        assert_eq!(
            Question::choice(" s", &["x", "y"]),
            Err(QuestionError::UntrimmedLabel(" s".into()))
        );
    }

    #[test]
    fn error_display() {
        assert_eq!(
            QuestionError::TooFewOptions(1).to_string(),
            "1 options given, at least 2 required"
        );
        assert_eq!(
            QuestionError::DuplicateOption("a".into()).to_string(),
            "option \"a\" appears more than once"
        );
        assert_eq!(QuestionError::EmptyLabel.to_string(), "label is empty");
        assert_eq!(OptionIndex::new(4).to_string(), "#4");
    }
}
