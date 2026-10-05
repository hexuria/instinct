//! `pua-explain`: replay records, canonical JSON journals, decision diffs and trail rendering
//! (spec §4.7, §5 rule 6).
//!
//! A [`ReplayRecord`] is `(schema, input hash, input, decision)`. Serialized with
//! [`ReplayRecord::to_json_line`] it is one canonical JSON line: struct fields in declaration
//! order, no maps, integers only. Re-running the classifier on the stored input must reproduce the
//! stored line byte for byte ([`ReplayRecord::check`]); any difference is reported as a
//! [`DecisionDiff`], typically because `DataVersion` moved.
//!
//! Inputs must serialize deterministically: no `HashMap`, no floats. That is the caller's
//! contract; consumers use plain structs and sorted vectors.
#![forbid(unsafe_code)]

use core::fmt;
use core::fmt::Write as _;

use pua_core::{Answer, DataVersion, Decision, Profile, Trail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Replay record schema version. Bump on any change to the record layout.
pub const REPLAY_SCHEMA: u16 = 1;

/// Errors from building, reading or checking replay records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExplainError {
    /// The input or decision could not be serialized.
    Serialize(String),
    /// A journal line is not a valid record.
    Deserialize(String),
    /// The record was written with a different schema.
    SchemaMismatch {
        /// Schema in the record.
        found: u16,
        /// Schema this build reads.
        expected: u16,
    },
    /// The stored input hash does not match the stored input (corrupted or edited journal).
    InputHashMismatch,
}

impl fmt::Display for ExplainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::Deserialize(e) => write!(f, "deserialize: {e}"),
            Self::SchemaMismatch { found, expected } => {
                write!(f, "replay schema {found}, this build reads {expected}")
            }
            Self::InputHashMismatch => f.write_str("input hash does not match the stored input"),
        }
    }
}

impl std::error::Error for ExplainError {}

/// blake3 of the canonical JSON of an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct InputHash([u8; 32]);

impl InputHash {
    fn of_json(json: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"pua-replay-input-v1");
        h.update(json.as_bytes());
        Self(*h.finalize().as_bytes())
    }
}

impl fmt::Display for InputHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl TryFrom<String> for InputHash {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        // Reuse DataVersion's strict hex parser (64 lowercase hex chars).
        DataVersion::from_hex(&s)
            .map(|v| Self(*v.as_bytes()))
            .map_err(|e| e.to_string())
    }
}

impl From<InputHash> for String {
    fn from(h: InputHash) -> String {
        h.to_string()
    }
}

fn to_json<T: Serialize>(v: &T) -> Result<String, ExplainError> {
    serde_json::to_string(v).map_err(|e| ExplainError::Serialize(e.to_string()))
}

/// One journal row: everything needed to re-run a decision and compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRecord<I> {
    schema: u16,
    input_hash: InputHash,
    input: I,
    decision: Decision,
}

impl<I: Serialize> ReplayRecord<I> {
    /// Builds a record, hashing the canonical JSON of `input`.
    ///
    /// # Errors
    /// [`ExplainError::Serialize`] when `input` cannot be serialized.
    pub fn new(input: I, decision: Decision) -> Result<Self, ExplainError> {
        let input_hash = InputHash::of_json(&to_json(&input)?);
        Ok(Self {
            schema: REPLAY_SCHEMA,
            input_hash,
            input,
            decision,
        })
    }

    /// The canonical one-line JSON form (no trailing newline).
    ///
    /// # Errors
    /// [`ExplainError::Serialize`] when serialization fails.
    pub fn to_json_line(&self) -> Result<String, ExplainError> {
        to_json(self)
    }

    /// The stored input.
    pub fn input(&self) -> &I {
        &self.input
    }

    /// The stored decision.
    pub fn decision(&self) -> &Decision {
        &self.decision
    }

    /// The input hash.
    pub fn input_hash(&self) -> InputHash {
        self.input_hash
    }

    /// Re-runs `rerun` on the stored input and compares the result byte for byte.
    ///
    /// # Errors
    /// [`ExplainError::Serialize`] when a decision cannot be serialized.
    pub fn check<F>(&self, rerun: F) -> Result<ReplayCheck, ExplainError>
    where
        F: FnOnce(&I) -> Decision,
    {
        let actual = rerun(&self.input);
        let expected_json = to_json(&self.decision)?;
        let actual_json = to_json(&actual)?;
        if expected_json == actual_json {
            return Ok(ReplayCheck::Identical);
        }
        Ok(ReplayCheck::Diverged(Box::new(diff(
            &self.decision,
            &actual,
        ))))
    }
}

impl<I: Serialize + DeserializeOwned> ReplayRecord<I> {
    /// Parses one journal line and verifies schema and input hash.
    ///
    /// # Errors
    /// [`ExplainError::Deserialize`], [`ExplainError::SchemaMismatch`] or
    /// [`ExplainError::InputHashMismatch`].
    pub fn from_json_line(line: &str) -> Result<Self, ExplainError> {
        let r: Self =
            serde_json::from_str(line).map_err(|e| ExplainError::Deserialize(e.to_string()))?;
        if r.schema != REPLAY_SCHEMA {
            return Err(ExplainError::SchemaMismatch {
                found: r.schema,
                expected: REPLAY_SCHEMA,
            });
        }
        if InputHash::of_json(&to_json(&r.input)?) != r.input_hash {
            return Err(ExplainError::InputHashMismatch);
        }
        Ok(r)
    }
}

/// Result of re-running a stored record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayCheck {
    /// Byte-identical decision.
    Identical,
    /// The decision changed; see the diff.
    Diverged(Box<DecisionDiff>),
}

/// What changed between two decisions for the same input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecisionDiff {
    /// `(before, after)` when `DataVersion` moved.
    pub data_version: Option<(DataVersion, DataVersion)>,
    /// `(before, after)` when the profile differs.
    pub profile: Option<(Profile, Profile)>,
    /// `(before, after)` when the answer changed.
    pub answer: Option<(Answer, Answer)>,
    /// Whether the trail differs (wording, spans, contributions or order).
    pub trail_changed: bool,
}

impl DecisionDiff {
    /// Whether nothing differs.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Compares two decisions (spec §4.7 "diff of two replays").
pub fn diff(before: &Decision, after: &Decision) -> DecisionDiff {
    DecisionDiff {
        data_version: (before.data_version() != after.data_version())
            .then(|| (before.data_version(), after.data_version())),
        profile: (before.profile() != after.profile()).then(|| (before.profile(), after.profile())),
        answer: (before.answer() != after.answer())
            .then(|| (before.answer().clone(), after.answer().clone())),
        trail_changed: before.trail() != after.trail(),
    }
}

/// Renders a trail as aligned text, one record per line:
/// `step stage [rule] [@span] [±millis] [scorer] text`.
pub fn render_trail(trail: &Trail) -> String {
    let mut out = String::new();
    for r in trail.records() {
        let stage = format!("{:?}", r.stage()).to_lowercase();
        let _ = write!(out, "{:>2} {stage:<10}", r.step());
        if let Some(rule) = r.rule_id() {
            let _ = write!(out, " {rule}");
        }
        if let Some(span) = r.span_ref() {
            let _ = write!(out, " @{span}");
        }
        if r.contribution().get() != 0 {
            let _ = write!(out, " {:+}", r.contribution().get());
        }
        if let Some(k) = r.scorer_kind() {
            let _ = write!(out, " [{}]", k.name());
        }
        let _ = writeln!(out, " {}", r.text());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        assert_eq!(
            ExplainError::SchemaMismatch {
                found: 9,
                expected: 1
            }
            .to_string(),
            "replay schema 9, this build reads 1"
        );
        assert_eq!(
            ExplainError::InputHashMismatch.to_string(),
            "input hash does not match the stored input"
        );
        assert_eq!(
            ExplainError::Serialize("x".into()).to_string(),
            "serialize: x"
        );
        assert_eq!(
            ExplainError::Deserialize("y".into()).to_string(),
            "deserialize: y"
        );
    }
}
