//! Rule data as written in consumer data files, and its validation errors.

use core::fmt;

use pua_core::ScorerKind;

/// Most items (literals + slots) in one pattern.
pub const MAX_PATTERN_ITEMS: usize = 6;
/// Most tokens in one negator phrase.
pub const MAX_NEGATOR_TOKENS: usize = 3;
/// Largest negation window, in tokens.
pub const MAX_NEGATION_WINDOW: u8 = 8;
/// Default negation window (spec §4.4).
pub const DEFAULT_NEGATION_WINDOW: u8 = 3;

/// A cue class and how its rule scores combine.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ClassSpec {
    /// Class name, e.g. `"interrupt"`.
    pub name: String,
    /// `max` (critical set) or `sum`.
    pub scorer: ScorerKind,
}

/// One rule as written in consumer data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RuleSpec {
    /// Unique id, e.g. `"interrupt.stop.v1"`.
    pub id: String,
    /// Name of a declared class.
    pub class: String,
    /// Canonical literal tokens and slots separated by single spaces, e.g. `"stop {gerund}"`.
    /// Slots: `{word}` (any free token), `{gerund}` (a free token of ≥ 5 chars ending in
    /// `ing`). No regex.
    pub pattern: String,
    /// Weight, 1..=1000.
    pub weight_millis: i16,
    /// Contexts that must hold for the rule to count.
    #[cfg_attr(feature = "serde", serde(default))]
    pub requires: Vec<String>,
    /// Contexts that cancel the rule.
    #[cfg_attr(feature = "serde", serde(default))]
    pub forbids: Vec<String>,
    /// Rule version, ≥ 1. Part of `DataVersion`.
    pub version: u32,
}

/// A whole rule set as written in consumer data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RuleSetSpec {
    /// Classes in output order.
    pub classes: Vec<ClassSpec>,
    /// Negator phrases (1..=3 canonical tokens), e.g. `"don't"`, `"no need to"`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub negators: Vec<String>,
    /// Tokens before a cue searched for a negator (1..=8, default 3).
    #[cfg_attr(feature = "serde", serde(default = "default_window"))]
    pub negation_window: u8,
    /// The rules.
    pub rules: Vec<RuleSpec>,
}

#[cfg(feature = "serde")]
const fn default_window() -> u8 {
    DEFAULT_NEGATION_WINDOW
}

/// Why a [`RuleSetSpec`] was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulesError {
    /// No classes declared.
    NoClasses,
    /// A class name is empty.
    EmptyClassName {
        /// Class index.
        index: usize,
    },
    /// Two classes share a name.
    DuplicateClass {
        /// The name.
        name: String,
    },
    /// No rules.
    NoRules,
    /// A rule id is empty.
    EmptyRuleId {
        /// Rule index.
        index: usize,
    },
    /// Two rules share an id.
    DuplicateRuleId {
        /// The id.
        id: String,
    },
    /// A rule names a class that was not declared.
    UnknownClass {
        /// Rule id.
        id: String,
        /// The class it named.
        class: String,
    },
    /// Weight outside 1..=1000.
    WeightOutOfRange {
        /// Rule id.
        id: String,
        /// The weight.
        weight: i16,
    },
    /// Version 0.
    ZeroVersion {
        /// Rule id.
        id: String,
    },
    /// The pattern is empty.
    EmptyPattern {
        /// Rule id.
        id: String,
    },
    /// The pattern has more than [`MAX_PATTERN_ITEMS`] items.
    PatternTooLong {
        /// Rule id.
        id: String,
        /// Item count.
        items: usize,
    },
    /// The pattern is only slots.
    NoLiteral {
        /// Rule id.
        id: String,
    },
    /// A `{slot}` other than `{word}` / `{gerund}`.
    UnknownSlot {
        /// Rule id.
        id: String,
        /// The slot text.
        slot: String,
    },
    /// A literal is not one canonical free token (this is also how regex syntax is refused:
    /// `sto+p` is not a word token).
    NonCanonicalLiteral {
        /// Rule id.
        id: String,
        /// The literal as written.
        literal: String,
    },
    /// A context name other than `negation` / `question`.
    UnknownContext {
        /// Rule id.
        id: String,
        /// The context.
        context: String,
    },
    /// The same context is both required and forbidden (the rule could never count).
    ContextConflict {
        /// Rule id.
        id: String,
        /// The context.
        context: String,
    },
    /// A context is listed twice in one list.
    DuplicateContext {
        /// Rule id.
        id: String,
        /// The context.
        context: String,
    },
    /// A negator is not 1..=3 canonical free tokens joined by single spaces.
    NonCanonicalNegator {
        /// Negator index.
        index: usize,
    },
    /// A negator is listed twice.
    DuplicateNegator {
        /// Negator index of the second occurrence.
        index: usize,
    },
    /// Negation window outside 1..=8.
    NegationWindowOutOfRange {
        /// The window.
        window: u8,
    },
}

impl fmt::Display for RulesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoClasses => f.write_str("rule set declares no classes"),
            Self::EmptyClassName { index } => write!(f, "class {index}: empty name"),
            Self::DuplicateClass { name } => write!(f, "duplicate class {name:?}"),
            Self::NoRules => f.write_str("rule set has no rules"),
            Self::EmptyRuleId { index } => write!(f, "rule {index}: empty id"),
            Self::DuplicateRuleId { id } => write!(f, "duplicate rule id {id:?}"),
            Self::UnknownClass { id, class } => {
                write!(f, "rule {id:?}: unknown class {class:?}")
            }
            Self::WeightOutOfRange { id, weight } => {
                write!(f, "rule {id:?}: weight {weight} outside 1..=1000")
            }
            Self::ZeroVersion { id } => write!(f, "rule {id:?}: version must be >= 1"),
            Self::EmptyPattern { id } => write!(f, "rule {id:?}: empty pattern"),
            Self::PatternTooLong { id, items } => write!(
                f,
                "rule {id:?}: pattern has {items} items, at most {MAX_PATTERN_ITEMS} allowed"
            ),
            Self::NoLiteral { id } => write!(f, "rule {id:?}: pattern needs a literal token"),
            Self::UnknownSlot { id, slot } => write!(
                f,
                "rule {id:?}: unknown slot {slot:?} (use {{word}} or {{gerund}})"
            ),
            Self::NonCanonicalLiteral { id, literal } => write!(
                f,
                "rule {id:?}: literal {literal:?} is not one canonical word token"
            ),
            Self::UnknownContext { id, context } => write!(
                f,
                "rule {id:?}: unknown context {context:?} (use negation or question)"
            ),
            Self::ContextConflict { id, context } => write!(
                f,
                "rule {id:?}: context {context:?} is both required and forbidden"
            ),
            Self::DuplicateContext { id, context } => {
                write!(f, "rule {id:?}: context {context:?} listed twice")
            }
            Self::NonCanonicalNegator { index } => write!(
                f,
                "negator {index}: not 1..={MAX_NEGATOR_TOKENS} canonical word tokens"
            ),
            Self::DuplicateNegator { index } => write!(f, "negator {index}: duplicate"),
            Self::NegationWindowOutOfRange { window } => write!(
                f,
                "negation window {window} outside 1..={MAX_NEGATION_WINDOW}"
            ),
        }
    }
}

impl std::error::Error for RulesError {}
