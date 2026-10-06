//! Drive-modulated arbitration (spec §4.10).

use core::fmt;

use crate::{
    Answer, Confidence, Label, Millis, OptionIndex, Profile, Question, QuestionError, Scores,
    StageKind, TrailRecord, decide,
};

/// Maximum number of drives in one arbitration.
pub const MAX_DRIVES: usize = 16;

/// Folded into a consumer's `DataVersion` when it uses [`arbitrate`].
pub const INSTINCT_TAG: &str = "instinct-arbitrate-v1;urge=max_d(D*W/1000)*S/1000;dominant=lowest-index;persist=min_margin-if-raw>=min_confidence";

/// Index of a drive inside a [`Drives`] set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DriveIndex(u8);

impl DriveIndex {
    /// The raw index.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Builds an index. Range-checking happens against a concrete [`Drives`].
    pub const fn new(index: u8) -> Self {
        Self(index)
    }
}

/// One named drive with its current level (0..=1000).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Drive {
    name: Label,
    level: Confidence,
}

impl Drive {
    /// The drive name.
    pub fn name(&self) -> &Label {
        &self.name
    }

    /// The current drive level.
    pub const fn level(&self) -> Confidence {
        self.level
    }
}

/// An ordered, non-empty set of uniquely named drives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Drives(Box<[Drive]>);

impl Drives {
    /// Validates an ordered set of drives.
    ///
    /// # Errors
    /// [`InstinctError::NoDrives`], [`InstinctError::TooManyDrives`],
    /// [`InstinctError::DuplicateDrive`] or [`InstinctError::BadDriveName`].
    pub fn new<S: AsRef<str>>(drives: &[(S, Confidence)]) -> Result<Self, InstinctError> {
        if drives.is_empty() {
            return Err(InstinctError::NoDrives);
        }
        if drives.len() > MAX_DRIVES {
            return Err(InstinctError::TooManyDrives(drives.len()));
        }

        let mut validated = Vec::with_capacity(drives.len());
        for (name, level) in drives {
            let name = Label::new(name.as_ref()).map_err(InstinctError::BadDriveName)?;
            if validated.iter().any(|drive: &Drive| drive.name == name) {
                return Err(InstinctError::DuplicateDrive(name.as_str().into()));
            }
            validated.push(Drive {
                name,
                level: *level,
            });
        }
        Ok(Self(validated.into_boxed_slice()))
    }

    /// Number of drives.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty. A valid `Drives` is never empty.
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The drive at `index`, if in range.
    pub fn get(&self, index: DriveIndex) -> Option<&Drive> {
        self.0.get(usize::from(index.get()))
    }

    /// Finds a drive by its exact, case-sensitive name.
    pub fn index_of(&self, name: &str) -> Option<DriveIndex> {
        self.0
            .iter()
            .position(|drive| drive.name.as_str() == name)
            .and_then(|index| u8::try_from(index).ok())
            .map(DriveIndex)
    }

    /// Drives in their declared order.
    pub fn drives(&self) -> &[Drive] {
        &self.0
    }
}

/// Drive-by-option affinity weights for one question and one drive count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affinity<'q> {
    question: &'q Question,
    drives: usize,
    weights: Box<[Confidence]>,
}

impl<'q> Affinity<'q> {
    /// Creates a zero-filled affinity table for `question` and `drives`.
    pub fn new(question: &'q Question, drives: &Drives) -> Self {
        Self {
            question,
            drives: drives.len(),
            weights: vec![Confidence::ZERO; drives.len() * usize::from(question.arity())]
                .into_boxed_slice(),
        }
    }

    /// Sets the affinity of one drive for one option.
    ///
    /// # Errors
    /// [`InstinctError::DriveOutOfRange`] or [`InstinctError::OptionOutOfRange`].
    pub fn set(
        &mut self,
        drive: DriveIndex,
        option: OptionIndex,
        weight: Confidence,
    ) -> Result<(), InstinctError> {
        let drive_index = usize::from(drive.get());
        if drive_index >= self.drives {
            return Err(InstinctError::DriveOutOfRange {
                index: drive,
                len: self.drives,
            });
        }
        let arity = self.question.arity();
        if option.get() >= arity {
            return Err(InstinctError::OptionOutOfRange {
                index: option,
                arity,
            });
        }
        let offset = drive_index * usize::from(arity) + usize::from(option.get());
        if let Some(slot) = self.weights.get_mut(offset) {
            *slot = weight;
            Ok(())
        } else {
            Err(InstinctError::OptionOutOfRange {
                index: option,
                arity,
            })
        }
    }

    /// The affinity of one drive for one option, if both indices are in range.
    pub fn get(&self, drive: DriveIndex, option: OptionIndex) -> Option<Confidence> {
        let drive_index = usize::from(drive.get());
        let arity = self.question.arity();
        if drive_index >= self.drives || option.get() >= arity {
            return None;
        }
        let offset = drive_index * usize::from(arity) + usize::from(option.get());
        self.weights.get(offset).copied()
    }
}

/// An error validating or applying instinct arbitration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstinctError {
    /// At least one drive is required.
    NoDrives,
    /// More than [`MAX_DRIVES`] drives were supplied.
    TooManyDrives(usize),
    /// A drive name was repeated.
    DuplicateDrive(Box<str>),
    /// A drive name failed [`Label`] validation.
    BadDriveName(QuestionError),
    /// A drive index is not in the supplied set.
    DriveOutOfRange {
        /// Offending drive index.
        index: DriveIndex,
        /// Number of drives in the set.
        len: usize,
    },
    /// An option index is not in the question.
    OptionOutOfRange {
        /// Offending option index.
        index: OptionIndex,
        /// The question's arity.
        arity: u16,
    },
    /// Evidence and affinity refer to different questions.
    QuestionMismatch,
    /// The affinity table was built for a different number of drives.
    DriveCountMismatch {
        /// Number of drives in the affinity table.
        affinity: usize,
        /// Number of supplied drives.
        drives: usize,
    },
}

impl fmt::Display for InstinctError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDrives => f.write_str("at least one drive is required"),
            Self::TooManyDrives(n) => {
                write!(f, "too many drives: {n} (max {MAX_DRIVES})")
            }
            Self::DuplicateDrive(name) => write!(f, "duplicate drive name {name:?}"),
            Self::BadDriveName(error) => write!(f, "invalid drive name: {error}"),
            Self::DriveOutOfRange { index, len } => {
                write!(
                    f,
                    "drive index {} out of range for a drive set of length {len}",
                    index.get()
                )
            }
            Self::OptionOutOfRange { index, arity } => {
                write!(
                    f,
                    "option {index} out of range for a question with {arity} options"
                )
            }
            Self::QuestionMismatch => {
                f.write_str("evidence and affinity belong to different questions")
            }
            Self::DriveCountMismatch { affinity, drives } => {
                write!(
                    f,
                    "affinity drive count {affinity} does not match supplied drive count {drives}"
                )
            }
        }
    }
}

impl std::error::Error for InstinctError {}

/// The outcome of one arbitration. `answer` is always returned by [`decide`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arbitration<'q> {
    answer: Answer,
    urges: Scores<'q>,
    dominant: Box<[Option<DriveIndex>]>,
    persisted: Option<OptionIndex>,
    persisted_bonus: Option<Confidence>,
}

impl<'q> Arbitration<'q> {
    /// The answer produced by the profile's threshold-and-margin gate.
    pub fn answer(&self) -> &Answer {
        &self.answer
    }

    /// Raw urges, before persistence.
    pub fn urges(&self) -> &Scores<'q> {
        &self.urges
    }

    /// The dominant drive for `option`, if it has a non-zero drive pull.
    pub fn dominant(&self, option: OptionIndex) -> Option<DriveIndex> {
        self.dominant
            .get(usize::from(option.get()))
            .copied()
            .flatten()
    }

    /// The dominant drive of the chosen option, or `None` on abstain.
    pub fn winner_drive(&self) -> Option<DriveIndex> {
        self.answer
            .chosen()
            .and_then(|option| self.dominant(option))
    }

    /// The incumbent option whose raw urge received a persistence bonus, if any.
    pub fn persisted(&self) -> Option<OptionIndex> {
        self.persisted
    }

    /// Builds explanation records for non-zero urges and any persistence bonus.
    ///
    /// Pass the same drives used by [`arbitrate`].
    pub fn trail_records(&self, drives: &Drives) -> Vec<TrailRecord> {
        let mut records = Vec::new();
        let question = self.urges.question();
        for raw_index in 0..question.arity() {
            let option = OptionIndex::new(raw_index);
            let Some(urge) = self.urges.get(option) else {
                continue;
            };
            if urge == Confidence::ZERO {
                continue;
            }
            let Some(dominant) = self.dominant(option) else {
                continue;
            };
            let Some(drive) = drives.get(dominant) else {
                continue;
            };
            let Some(label) = option_label(question, option) else {
                continue;
            };
            records.push(
                TrailRecord::new(
                    StageKind::Instinct,
                    format!("{label} urge {urge} dominant {}", drive.name().as_str()),
                )
                .millis(Millis::saturating(i32::from(urge.get()))),
            );
        }

        if let (Some(option), Some(bonus)) = (self.persisted, self.persisted_bonus)
            && let Some(label) = option_label(question, option)
        {
            records.push(
                TrailRecord::new(StageKind::Instinct, format!("persist {label} +{bonus}"))
                    .millis(Millis::saturating(i32::from(bonus.get()))),
            );
        }
        records
    }

    /// Discards the arbitration details and returns its answer.
    pub fn into_answer(self) -> Answer {
        self.answer
    }
}

fn option_label(question: &Question, option: OptionIndex) -> Option<&str> {
    if let Some(options) = question.options() {
        return options.get(option).map(Label::as_str);
    }
    match option.get() {
        0 => Some("no"),
        1 => Some("yes"),
        _ => None,
    }
}

/// Combines evidence with drive levels and affinities, then applies one profile gate.
///
/// # Errors
/// [`InstinctError`] when the questions or drive counts mismatch, or an index is out of range.
pub fn arbitrate<'q>(
    evidence: &Scores<'q>,
    drives: &Drives,
    affinity: &Affinity<'q>,
    incumbent: Option<OptionIndex>,
    profile: Profile,
) -> Result<Arbitration<'q>, InstinctError> {
    let question = evidence.question();
    if *question != *affinity.question {
        return Err(InstinctError::QuestionMismatch);
    }
    if affinity.drives != drives.len() {
        return Err(InstinctError::DriveCountMismatch {
            affinity: affinity.drives,
            drives: drives.len(),
        });
    }
    if let Some(index) = incumbent
        && evidence.get(index).is_none()
    {
        return Err(InstinctError::OptionOutOfRange {
            index,
            arity: question.arity(),
        });
    }

    let mut urges = Scores::new(question);
    let mut dominant = Vec::with_capacity(usize::from(question.arity()));
    for raw_option in 0..question.arity() {
        let option = OptionIndex::new(raw_option);
        let mut best_pull = 0_i32;
        let mut best_drive = None;
        for (raw_drive, drive) in drives.0.iter().enumerate() {
            let drive_index = DriveIndex::new(u8::try_from(raw_drive).unwrap_or(u8::MAX));
            let weight = affinity
                .get(drive_index, option)
                .unwrap_or(Confidence::ZERO);
            let pull = i32::from(drive.level.get()) * i32::from(weight.get()) / 1000;
            if pull > best_pull {
                best_pull = pull;
                best_drive = Some(drive_index);
            }
        }
        let evidence_score = evidence.get(option).unwrap_or(Confidence::ZERO);
        let urge = Confidence::saturating(best_pull * i32::from(evidence_score.get()) / 1000);
        urges
            .set(option, urge)
            .map_err(|_| InstinctError::OptionOutOfRange {
                index: option,
                arity: question.arity(),
            })?;
        dominant.push(best_drive);
    }

    let mut persisted_scores = urges.clone();
    let mut persisted = None;
    let mut persisted_bonus = None;
    if let Some(index) = incumbent
        && let Some(raw_urge) = urges.get(index)
        && raw_urge >= profile.thresholds().min_confidence
    {
        let thresholds = profile.thresholds();
        let persisted_urge = raw_urge.saturating_add(thresholds.min_margin);
        persisted_scores.set(index, persisted_urge).map_err(|_| {
            InstinctError::OptionOutOfRange {
                index,
                arity: question.arity(),
            }
        })?;
        persisted = Some(index);
        persisted_bonus = Some(Confidence::saturating(
            i32::from(persisted_urge.get()) - i32::from(raw_urge.get()),
        ));
    }

    Ok(Arbitration {
        answer: decide(&persisted_scores, profile),
        urges,
        dominant: dominant.into_boxed_slice(),
        persisted,
        persisted_bonus,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use proptest::prelude::*;

    fn confidence(value: i16) -> Confidence {
        Confidence::new(value).unwrap()
    }

    fn option(index: u16) -> OptionIndex {
        OptionIndex::new(index)
    }

    fn question(labels: &[&str]) -> Question {
        Question::choice("q", labels).unwrap()
    }

    fn setup<'q>(
        question: &'q Question,
        levels: &[i16],
        weights: &[i16],
        evidence: &[i16],
    ) -> (Drives, Affinity<'q>, Scores<'q>) {
        assert_eq!(levels.len(), 3);
        assert_eq!(weights.len(), 9);
        assert_eq!(evidence.len(), usize::from(question.arity()));
        let names = ["goal", "threat", "caution"];
        let values: Vec<_> = names
            .iter()
            .zip(levels)
            .map(|(name, level)| (*name, confidence(*level)))
            .collect();
        let drives = Drives::new(&values).unwrap();
        let mut affinity = Affinity::new(question, &drives);
        for drive in 0..3 {
            for option in 0..usize::from(question.arity()) {
                affinity
                    .set(
                        DriveIndex::new(u8::try_from(drive).unwrap()),
                        OptionIndex::new(u16::try_from(option).unwrap()),
                        confidence(weights[drive * 3 + option]),
                    )
                    .unwrap();
            }
        }
        let mut scores = Scores::new(question);
        for (index, score) in evidence.iter().enumerate() {
            scores
                .set(
                    OptionIndex::new(u16::try_from(index).unwrap()),
                    confidence(*score),
                )
                .unwrap();
        }
        (drives, affinity, scores)
    }

    fn raw_urges(levels: &[i16], weights: &[i16], evidence: &[i16]) -> Vec<Confidence> {
        let question = question(&["a", "b", "c"]);
        let (drives, affinity, scores) = setup(&question, levels, weights, evidence);
        arbitrate(&scores, &drives, &affinity, None, Profile::Standard)
            .unwrap()
            .urges()
            .values()
            .to_vec()
    }

    fn profile() -> impl Strategy<Value = Profile> {
        prop_oneof![
            Just(Profile::Fast),
            Just(Profile::Standard),
            Just(Profile::Deep)
        ]
    }

    #[test]
    fn dominant_drive_wins_the_modal_example() {
        let question = question(&["accept", "checkout", "wait"]);
        let levels = [800, 950, 300];
        let weights = [0, 1000, 0, 1000, 0, 0, 0, 0, 800];
        let evidence = [900, 950, 400];
        let (drives, affinity, scores) = setup(&question, &levels, &weights, &evidence);
        let standard = arbitrate(&scores, &drives, &affinity, None, Profile::Standard).unwrap();
        assert_eq!(
            standard.urges().values(),
            &[confidence(855), confidence(760), confidence(96)]
        );
        assert_eq!(standard.dominant(option(0)), Some(DriveIndex::new(1)));
        assert_eq!(standard.dominant(option(1)), Some(DriveIndex::new(0)));
        assert_eq!(standard.dominant(option(2)), Some(DriveIndex::new(2)));
        for (profile, margin, min) in [
            (Profile::Standard, 95, 150),
            (Profile::Deep, 95, 100),
            (Profile::Fast, 95, 200),
        ] {
            assert!(matches!(
                arbitrate(&scores, &drives, &affinity, None, profile)
                    .unwrap()
                    .answer(),
                Answer::Abstain {
                    why: crate::AbstainReason::LowMargin {
                        margin: actual_margin,
                        min: actual_min,
                    },
                    ..
                } if *actual_margin == confidence(margin) && *actual_min == confidence(min)
            ));
        }

        let levels = [800, 1000, 300];
        let (drives, affinity, scores) = setup(&question, &levels, &weights, &evidence);
        let deep = arbitrate(&scores, &drives, &affinity, None, Profile::Deep).unwrap();
        assert_eq!(
            deep.urges().values(),
            &[confidence(900), confidence(760), confidence(96)]
        );
        assert_eq!(deep.answer().chosen(), Some(option(0)));
        assert_eq!(deep.winner_drive(), Some(DriveIndex::new(1)));
        assert!(matches!(
            arbitrate(&scores, &drives, &affinity, None, Profile::Standard)
                .unwrap()
                .answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowMargin {
                    margin,
                    min
                },
                ..
            } if *margin == confidence(140) && *min == confidence(150)
        ));
    }

    #[test]
    fn zero_evidence_gives_zero_urge() {
        let question = question(&["a", "b"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(1000))
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), Confidence::ZERO).unwrap();
        let result = arbitrate(&evidence, &drives, &affinity, None, Profile::Standard).unwrap();
        assert_eq!(result.urges().get(option(0)), Some(Confidence::ZERO));
        assert_eq!(result.dominant(option(0)), Some(DriveIndex::new(0)));
        assert!(matches!(
            result.answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowConfidence { top, min },
                ..
            } if *top == Confidence::ZERO && *min == confidence(750)
        ));
    }

    #[test]
    fn zero_drives_give_no_dominant() {
        let question = question(&["a", "b", "c"]);
        let levels = [0, 0, 0];
        let weights = [1000; 9];
        let evidence = [1000; 3];
        let (drives, affinity, scores) = setup(&question, &levels, &weights, &evidence);
        let result = arbitrate(&scores, &drives, &affinity, None, Profile::Standard).unwrap();
        assert_eq!(result.dominant(option(0)), None);
        assert_eq!(result.dominant(option(1)), None);
        assert!(matches!(
            result.answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowConfidence { top, min },
                ranked,
            } if *top == Confidence::ZERO
                && *min == confidence(750)
                && ranked.entries().len() == 3
        ));
    }

    #[test]
    fn dominant_tie_picks_lowest_drive_index() {
        let question = question(&["a", "b", "c"]);
        let levels = [500, 500, 0];
        let weights = [1000, 0, 0, 1000, 0, 0, 1000, 1000, 1000];
        let evidence = [1000; 3];
        let (drives, affinity, scores) = setup(&question, &levels, &weights, &evidence);
        let result = arbitrate(&scores, &drives, &affinity, None, Profile::Standard).unwrap();
        assert_eq!(result.dominant(option(0)), Some(DriveIndex::new(0)));
    }

    #[test]
    fn exact_urge_tie_freezes() {
        let question = question(&["a", "b"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(800))
            .unwrap();
        affinity
            .set(DriveIndex::new(0), option(1), confidence(800))
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), confidence(1000)).unwrap();
        evidence.set(option(1), confidence(1000)).unwrap();
        let result = arbitrate(&evidence, &drives, &affinity, None, Profile::Standard).unwrap();
        assert!(matches!(
            result.answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowMargin { margin, min },
                ranked,
            } if *margin == Confidence::ZERO
                && *min == confidence(150)
                && ranked.entries().len() == 2
        ));
    }

    #[test]
    fn incumbent_keeps_control_on_raw_tie() {
        let question = question(&["a", "b"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(800))
            .unwrap();
        affinity
            .set(DriveIndex::new(0), option(1), confidence(800))
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), confidence(1000)).unwrap();
        evidence.set(option(1), confidence(1000)).unwrap();
        let result = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(1)),
            Profile::Standard,
        )
        .unwrap();
        assert_eq!(result.persisted(), Some(option(1)));
        assert_eq!(result.answer().chosen(), Some(option(1)));
    }

    #[test]
    fn weak_incumbent_gets_no_bonus() {
        let question = question(&["a", "b"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(700))
            .unwrap();
        affinity
            .set(DriveIndex::new(0), option(1), confidence(600))
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), confidence(1000)).unwrap();
        evidence.set(option(1), confidence(1000)).unwrap();
        let result = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(0)),
            Profile::Standard,
        )
        .unwrap();
        assert_eq!(result.persisted(), None);
        assert!(matches!(
            result.answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowConfidence { top, min },
                ..
            } if *top == confidence(700) && *min == confidence(750)
        ));
    }

    #[test]
    fn challenger_beyond_twice_margin_takes_over() {
        let question = question(&["incumbent", "challenger"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(760))
            .unwrap();
        affinity
            .set(DriveIndex::new(0), option(1), Confidence::MAX)
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), Confidence::MAX).unwrap();
        evidence.set(option(1), Confidence::MAX).unwrap();
        let standard = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(0)),
            Profile::Standard,
        )
        .unwrap();
        assert_eq!(standard.persisted(), Some(option(0)));
        assert!(matches!(
            standard.answer(),
            Answer::Abstain {
                why: crate::AbstainReason::LowMargin { margin, min },
                ..
            } if *margin == confidence(90) && *min == confidence(150)
        ));

        affinity
            .set(DriveIndex::new(0), option(0), confidence(750))
            .unwrap();
        let deep = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(0)),
            Profile::Deep,
        )
        .unwrap();
        assert_eq!(deep.answer().chosen(), Some(option(1)));
        assert_eq!(deep.persisted(), Some(option(0)));
    }

    #[test]
    fn saturated_incumbent_bonus_is_capped() {
        let question = question(&["incumbent", "challenger"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        affinity
            .set(DriveIndex::new(0), option(0), confidence(950))
            .unwrap();
        affinity
            .set(DriveIndex::new(0), option(1), confidence(850))
            .unwrap();
        let mut evidence = Scores::new(&question);
        evidence.set(option(0), Confidence::MAX).unwrap();
        evidence.set(option(1), Confidence::MAX).unwrap();
        let result = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(0)),
            Profile::Standard,
        )
        .unwrap();
        assert_eq!(result.persisted(), Some(option(0)));
        assert!(matches!(
            result.answer(),
            Answer::Choice {
                option: chosen,
                confidence,
                ..
            } if *chosen == option(0) && *confidence == Confidence::MAX
        ));
        assert_eq!(
            result.trail_records(&drives).last().unwrap().text(),
            "persist incumbent +50"
        );
    }

    #[test]
    fn errors_have_stable_variants_and_display() {
        let empty: [(&str, Confidence); 0] = [];
        let err = Drives::new(&empty).unwrap_err();
        assert_eq!(err, InstinctError::NoDrives);
        assert_eq!(err.to_string(), "at least one drive is required");

        let too_many: Vec<_> = (0..=MAX_DRIVES)
            .map(|index| (format!("drive-{index}"), confidence(0)))
            .collect();
        let err = Drives::new(&too_many).unwrap_err();
        assert_eq!(err, InstinctError::TooManyDrives(17));
        assert_eq!(err.to_string(), "too many drives: 17 (max 16)");
        let max_drives: Vec<_> = (0..MAX_DRIVES)
            .map(|index| (format!("drive-{index}"), confidence(0)))
            .collect();
        assert_eq!(Drives::new(&max_drives).unwrap().len(), MAX_DRIVES);

        let err = Drives::new(&[("goal", confidence(0)), ("goal", confidence(1))]).unwrap_err();
        assert_eq!(err, InstinctError::DuplicateDrive("goal".into()));
        assert_eq!(err.to_string(), "duplicate drive name \"goal\"");

        let err = Drives::new(&[("  ", confidence(0))]).unwrap_err();
        assert_eq!(err, InstinctError::BadDriveName(QuestionError::EmptyLabel));
        assert_eq!(err.to_string(), "invalid drive name: label is empty");

        let question = question(&["a", "b", "c"]);
        let drives = Drives::new(&[("goal", confidence(1))]).unwrap();
        let mut affinity = Affinity::new(&question, &drives);
        let err = affinity
            .set(DriveIndex::new(3), option(0), confidence(1))
            .unwrap_err();
        assert_eq!(
            err,
            InstinctError::DriveOutOfRange {
                index: DriveIndex::new(3),
                len: 1,
            }
        );
        assert_eq!(
            err.to_string(),
            "drive index 3 out of range for a drive set of length 1"
        );
        let err = affinity
            .set(DriveIndex::new(0), option(3), confidence(1))
            .unwrap_err();
        assert_eq!(
            err,
            InstinctError::OptionOutOfRange {
                index: option(3),
                arity: 3,
            }
        );
        assert_eq!(
            err.to_string(),
            "option #3 out of range for a question with 3 options"
        );
    }

    #[test]
    fn arbitration_rejects_mismatched_inputs_and_incumbent() {
        let first = question(&["a", "b"]);
        let other = question(&["a", "different"]);
        let drives = Drives::new(&[("goal", confidence(1000))]).unwrap();
        let affinity = Affinity::new(&first, &drives);
        let two_drives =
            Drives::new(&[("goal", confidence(1)), ("threat", confidence(2))]).unwrap();
        let evidence = Scores::new(&other);
        let err = arbitrate(
            &evidence,
            &two_drives,
            &affinity,
            Some(option(2)),
            Profile::Standard,
        )
        .unwrap_err();
        assert_eq!(err, InstinctError::QuestionMismatch);
        assert_eq!(
            err.to_string(),
            "evidence and affinity belong to different questions"
        );

        let equal_but_distinct = first.clone();
        let evidence = Scores::new(&equal_but_distinct);
        assert!(arbitrate(&evidence, &drives, &affinity, None, Profile::Standard).is_ok());

        let evidence = Scores::new(&first);
        let err = arbitrate(
            &evidence,
            &two_drives,
            &affinity,
            Some(option(2)),
            Profile::Standard,
        )
        .unwrap_err();
        assert_eq!(
            err,
            InstinctError::DriveCountMismatch {
                affinity: 1,
                drives: 2,
            }
        );
        assert_eq!(
            err.to_string(),
            "affinity drive count 1 does not match supplied drive count 2"
        );

        let err = arbitrate(
            &evidence,
            &drives,
            &affinity,
            Some(option(2)),
            Profile::Standard,
        )
        .unwrap_err();
        assert_eq!(
            err,
            InstinctError::OptionOutOfRange {
                index: option(2),
                arity: 2,
            }
        );
        assert_eq!(
            err.to_string(),
            "option #2 out of range for a question with 2 options"
        );
    }

    #[test]
    fn drive_and_affinity_accessors_are_read_only() {
        let question = question(&["a", "b"]);
        let drives = Drives::new(&[("goal", confidence(700))]).unwrap();
        let drive_index = DriveIndex::new(0);
        assert_eq!(drives.get(drive_index).unwrap().name().as_str(), "goal");
        assert_eq!(drives.get(drive_index).unwrap().level(), confidence(700));
        assert!(drives.get(DriveIndex::new(1)).is_none());
        assert_eq!(drives.len(), 1);
        assert!(!drives.is_empty());

        let mut affinity = Affinity::new(&question, &drives);
        assert_eq!(affinity.get(drive_index, option(0)), Some(Confidence::ZERO));
        affinity
            .set(drive_index, option(0), confidence(500))
            .unwrap();
        assert_eq!(affinity.get(drive_index, option(0)), Some(confidence(500)));
        assert_eq!(affinity.get(drive_index, option(2)), None);
    }

    #[test]
    fn drives_index_of_matches_exact_names() {
        let drives =
            Drives::new(&[("goal", confidence(700)), ("threat", confidence(800))]).unwrap();
        assert_eq!(drives.index_of("goal"), Some(DriveIndex::new(0)));
        assert_eq!(drives.index_of("threat"), Some(DriveIndex::new(1)));
        assert_eq!(drives.index_of("Goal"), None);
        assert_eq!(drives.index_of("missing"), None);
    }

    #[test]
    fn drives_accessor_preserves_declared_order() {
        let drives =
            Drives::new(&[("goal", confidence(700)), ("threat", confidence(800))]).unwrap();
        assert_eq!(
            drives
                .drives()
                .iter()
                .map(|drive| drive.name().as_str())
                .collect::<Vec<_>>(),
            vec!["goal", "threat"]
        );
    }

    #[test]
    fn trail_records_name_the_dominant_drive() {
        let question = question(&["accept", "checkout", "wait"]);
        let levels = [800, 1000, 300];
        let weights = [0, 1000, 0, 1000, 0, 0, 0, 0, 800];
        let evidence = [900, 950, 400];
        let (drives, affinity, scores) = setup(&question, &levels, &weights, &evidence);
        let result =
            arbitrate(&scores, &drives, &affinity, Some(option(0)), Profile::Deep).unwrap();
        let records = result.trail_records(&drives);
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].stage(), StageKind::Instinct);
        assert_eq!(records[0].text(), "accept urge 900 dominant threat");
        assert_eq!(records[0].contribution(), Millis::new(900).unwrap());
        assert_eq!(records[1].text(), "checkout urge 760 dominant goal");
        assert_eq!(records[1].contribution(), Millis::new(760).unwrap());
        assert_eq!(records[2].text(), "wait urge 96 dominant caution");
        assert_eq!(records[2].contribution(), Millis::new(96).unwrap());
        assert_eq!(records[3].text(), "persist accept +100");
        assert_eq!(records[3].contribution(), Millis::new(100).unwrap());
    }

    #[test]
    fn instinct_tag_is_frozen() {
        assert_eq!(
            INSTINCT_TAG,
            "instinct-arbitrate-v1;urge=max_d(D*W/1000)*S/1000;dominant=lowest-index;persist=min_margin-if-raw>=min_confidence"
        );
    }

    proptest! {
        #[test]
        fn arbitration_is_deterministic(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
            incumbent in prop::option::of(0u16..3),
            profile in profile(),
        ) {
            let question = question(&["a", "b", "c"]);
            let (drives, affinity, evidence) = setup(&question, &levels, &weights, &evidence_values);
            let first = arbitrate(&evidence, &drives, &affinity, incumbent.map(option), profile).unwrap();
            let second = arbitrate(&evidence, &drives, &affinity, incumbent.map(option), profile).unwrap();
            prop_assert_eq!(first, second);
        }

        #[test]
        fn without_incumbent_answer_is_the_single_decide_gate(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
            profile in profile(),
        ) {
            let question = question(&["a", "b", "c"]);
            let (drives, affinity, evidence) = setup(&question, &levels, &weights, &evidence_values);
            let result = arbitrate(&evidence, &drives, &affinity, None, profile).unwrap();
            prop_assert_eq!(result.answer(), &decide(result.urges(), profile));
        }

        #[test]
        fn raising_a_drive_affinity_or_evidence_never_lowers_urge(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
            drive_index in 0usize..3,
            option_index in 0usize..3,
            increase in 0i16..=1000,
        ) {
            let before = raw_urges(&levels, &weights, &evidence_values);

            let mut raised_levels = levels.clone();
            raised_levels[drive_index] = (raised_levels[drive_index] + increase).min(1000);
            let after = raw_urges(&raised_levels, &weights, &evidence_values);
            prop_assert!(after.iter().zip(&before).all(|(new, old)| new >= old));

            let mut raised_weights = weights.clone();
            let weight_index = drive_index * 3 + option_index;
            raised_weights[weight_index] = (raised_weights[weight_index] + increase).min(1000);
            let after = raw_urges(&levels, &raised_weights, &evidence_values);
            prop_assert!(after.iter().zip(&before).all(|(new, old)| new >= old));

            let mut raised_evidence = evidence_values.clone();
            raised_evidence[option_index] = (raised_evidence[option_index] + increase).min(1000);
            let after = raw_urges(&levels, &weights, &raised_evidence);
            prop_assert!(after.iter().zip(&before).all(|(new, old)| new >= old));
        }

        #[test]
        fn urges_are_bounded_by_evidence_and_best_pull(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
        ) {
            let urges = raw_urges(&levels, &weights, &evidence_values);
            for option_index in 0..3 {
                let best_pull = (0..3)
                    .map(|drive_index| {
                        i32::from(levels[drive_index])
                            * i32::from(weights[drive_index * 3 + option_index])
                            / 1000
                    })
                    .max()
                    .unwrap();
                prop_assert!(i32::from(urges[option_index].get()) <= i32::from(evidence_values[option_index]));
                prop_assert!(i32::from(urges[option_index].get()) <= best_pull);
            }
        }

        #[test]
        fn zero_evidence_is_vacuum(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
        ) {
            let urges = raw_urges(&levels, &weights, &evidence_values);
            for (score, urge) in evidence_values.iter().zip(urges) {
                if *score == 0 {
                    prop_assert_eq!(urge, Confidence::ZERO);
                }
            }
        }

        #[test]
        fn answer_shape_is_bounded_and_abstain_ranks_every_option(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
            incumbent in prop::option::of(0u16..3),
            profile in profile(),
        ) {
            let question = question(&["a", "b", "c"]);
            let (drives, affinity, evidence) = setup(&question, &levels, &weights, &evidence_values);
            let answer = arbitrate(&evidence, &drives, &affinity, incumbent.map(option), profile).unwrap().into_answer();
            if let Some(chosen) = answer.chosen() {
                prop_assert!(chosen.get() < question.arity());
            }
            if let Answer::Abstain { ranked, .. } = answer {
                prop_assert_eq!(ranked.entries().len(), usize::from(question.arity()));
                let mut indices: Vec<_> =
                    ranked.entries().iter().map(|(index, _)| index.get()).collect();
                indices.sort_unstable();
                prop_assert_eq!(indices, (0..question.arity()).collect::<Vec<_>>());
            }
        }

        #[test]
        fn persistence_only_changes_the_incumbent_score(
            levels in prop::collection::vec(0i16..=1000, 3..4),
            weights in prop::collection::vec(0i16..=1000, 9..10),
            evidence_values in prop::collection::vec(0i16..=1000, 3..4),
            incumbent in 0u16..3,
            profile in profile(),
        ) {
            let question = question(&["a", "b", "c"]);
            let (drives, affinity, evidence) = setup(&question, &levels, &weights, &evidence_values);
            let result = arbitrate(&evidence, &drives, &affinity, Some(option(incumbent)), profile).unwrap();
            let mut expected = result.urges().clone();
            let index = option(incumbent);
            if let Some(raw) = expected.get(index)
                && raw >= profile.thresholds().min_confidence
            {
                expected.set(index, raw.saturating_add(profile.thresholds().min_margin)).unwrap();
            }
            prop_assert_eq!(result.answer(), &decide(&expected, profile));
        }
    }
}
