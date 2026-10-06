//! The explanation trail (spec §4.7). Types live here because every [`crate::Decision`] carries one;
//! `instinct-explain` owns replay records, diffs and rendering (ADR 0003).

use crate::{Millis, Span};

/// The pipeline stage that produced a trail record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum StageKind {
    /// Canonicalization (NFC, fold, protected spans).
    Normalize,
    /// Closed-vocabulary lookup and typo repair.
    Lexicon,
    /// Cue rules, negation and scope.
    Rules,
    /// WL fingerprints.
    Graph,
    /// Hypervector similarity, cleanup, resonator.
    Hdc,
    /// Thresholds, margin, abstain.
    Decide,
    /// Escalation or a caller-declared fallback (spec §8).
    Escalation,
}

/// How a scorer combines its cues (spec §4.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ScorerKind {
    /// Strongest cue wins. Has a critical set: inputs outside it cannot change the output.
    Max,
    /// Cues add up. No critical-set property.
    Sum,
}

impl ScorerKind {
    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Sum => "sum",
        }
    }
}

/// One step of the trail.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrailRecord {
    step: u16,
    stage: StageKind,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    rule_id: Option<Box<str>>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    scorer: Option<ScorerKind>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    span: Option<Span>,
    millis: Millis,
    text: Box<str>,
}

impl TrailRecord {
    /// A record for `stage` with a human-readable `text`. The step number is assigned by
    /// [`Trail::push`].
    pub fn new(stage: StageKind, text: impl Into<Box<str>>) -> Self {
        Self {
            step: 0,
            stage,
            rule_id: None,
            scorer: None,
            span: None,
            millis: Millis::ZERO,
            text: text.into(),
        }
    }

    /// Attaches the rule id.
    #[must_use]
    pub fn rule(mut self, id: impl Into<Box<str>>) -> Self {
        self.rule_id = Some(id.into());
        self
    }

    /// Attaches the scorer kind.
    #[must_use]
    pub fn scorer(mut self, k: ScorerKind) -> Self {
        self.scorer = Some(k);
        self
    }

    /// Attaches a span in original-text coordinates.
    #[must_use]
    pub fn span(mut self, s: Span) -> Self {
        self.span = Some(s);
        self
    }

    /// Attaches a signed contribution.
    #[must_use]
    pub fn millis(mut self, m: Millis) -> Self {
        self.millis = m;
        self
    }

    /// 1-based step number.
    pub fn step(&self) -> u16 {
        self.step
    }
    /// Stage.
    pub fn stage(&self) -> StageKind {
        self.stage
    }
    /// Rule id, if any.
    pub fn rule_id(&self) -> Option<&str> {
        self.rule_id.as_deref()
    }
    /// Scorer kind, if any.
    pub fn scorer_kind(&self) -> Option<ScorerKind> {
        self.scorer
    }
    /// Span in original coordinates, if any.
    pub fn span_ref(&self) -> Option<Span> {
        self.span
    }
    /// Signed contribution.
    pub fn contribution(&self) -> Millis {
        self.millis
    }
    /// Human-readable text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// An ordered list of trail records. Steps are numbered 1, 2, 3, … in push order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Trail {
    records: Vec<TrailRecord>,
}

impl Trail {
    /// An empty trail.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a record, assigning the next step number (saturating at `u16::MAX`).
    pub fn push(&mut self, mut record: TrailRecord) {
        record.step = u16::try_from(self.records.len() + 1).unwrap_or(u16::MAX);
        self.records.push(record);
    }

    /// The records in order.
    pub fn records(&self) -> &[TrailRecord] {
        &self.records
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the trail is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_numbered_in_push_order() {
        let mut t = Trail::new();
        assert!(t.is_empty());
        t.push(TrailRecord::new(StageKind::Normalize, "nfc ok"));
        t.push(
            TrailRecord::new(StageKind::Rules, "cue")
                .rule("stop.cue.v1")
                .scorer(ScorerKind::Max)
                .span(Span::new(0, 4).unwrap())
                .millis(Millis::new(700).unwrap()),
        );
        assert_eq!(t.len(), 2);
        assert!(!t.is_empty());
        let r = &t.records()[1];
        assert_eq!(r.step(), 2);
        assert_eq!(r.stage(), StageKind::Rules);
        assert_eq!(r.rule_id(), Some("stop.cue.v1"));
        assert_eq!(r.scorer_kind(), Some(ScorerKind::Max));
        assert_eq!(r.span_ref(), Some(Span::new(0, 4).unwrap()));
        assert_eq!(r.contribution().get(), 700);
        assert_eq!(r.text(), "cue");
        assert_eq!(t.records()[0].step(), 1);
        assert_eq!(t.records()[0].rule_id(), None);
        assert_eq!(ScorerKind::Sum.name(), "sum");
        assert_eq!(ScorerKind::Max.name(), "max");
    }

    #[test]
    fn step_numbers_saturate_at_u16_max() {
        let mut t = Trail::new();
        for _ in 0..usize::from(u16::MAX) + 2 {
            t.push(TrailRecord::new(StageKind::Normalize, "x"));
        }
        let rs = t.records();
        assert_eq!(rs.len(), usize::from(u16::MAX) + 2);
        // Steps are 1-based; every record past the first 65534 keeps step u16::MAX.
        assert_eq!(rs[usize::from(u16::MAX) - 1].step(), u16::MAX);
        assert_eq!(rs[usize::from(u16::MAX)].step(), u16::MAX);
        assert_eq!(rs[usize::from(u16::MAX) + 1].step(), u16::MAX);
    }
}
