//! `instinct-core`: the shared vocabulary of Instinct.
//!
//! - Scores: [`Millis`] (similarity, −1000..=1000) and [`Confidence`] (0..=1000). Integer only;
//!   they are deterministic scores, **not calibrated probabilities** (spec §5 rule 2, ADR 0002).
//! - [`Span`]: byte offsets into the **original** text.
//! - [`Question`] / [`Answer`]: Jev's three shapes (Noul, Choice, Score) plus `Abstain`.
//! - [`Profile`]: threshold tables (`fast`, `standard`, `deep`, spec §4.6).
//! - [`decide`]: the only place a winner is picked (one threshold-and-margin gate), with the
//!   stable tie-break of spec §5 rule 4; [`abstain`] for stages that refuse early.
//! - [`CandidateSet`]: a finite candidate set (sorted, unique ids) as a `Choice`.
//! - [`arbitrate`]: consumer-owned drive levels and affinities with max-dominance urge,
//!   persistence and the same profile gate.
//! - [`Trail`], [`Decision`] and [`DataVersion`].
//!
//! Same input + same [`DataVersion`] + same [`Profile`] gives a byte-identical [`Decision`].
//!
//! ```
//! use instinct_core::{Answer, Confidence, OptionIndex, Profile, Question, Scores, decide};
//!
//! let q = Question::choice("route", &["keep", "move", "stop"])?;
//! let mut scores = Scores::new(&q);
//! scores.set(OptionIndex::new(1), Confidence::new(820)?)?;
//! match decide(&scores, Profile::Standard) {
//!     Answer::Choice { option, .. } => assert_eq!(option, OptionIndex::new(1)),
//!     other => panic!("unexpected {other:?}"),
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Instinct arbitration keeps near-ties frozen while allowing a sufficiently supported incumbent
//! to persist. In the modal example, threat at 1000 makes "accept" win under the deep profile:
//!
//! ```
//! use instinct_core::{
//!     Affinity, Answer, Confidence, DriveIndex, Drives, OptionIndex, Profile, Question, Scores,
//!     arbitrate,
//! };
//!
//! let question = Question::choice("route", &["accept", "checkout", "wait"])?;
//! let drives = Drives::new(&[
//!     ("goal", Confidence::new(800)?),
//!     ("threat", Confidence::new(1000)?),
//!     ("caution", Confidence::new(300)?),
//! ])?;
//! let mut affinity = Affinity::new(&question, &drives);
//! affinity.set(DriveIndex::new(0), OptionIndex::new(1), Confidence::MAX)?;
//! affinity.set(DriveIndex::new(1), OptionIndex::new(0), Confidence::MAX)?;
//! affinity.set(DriveIndex::new(2), OptionIndex::new(2), Confidence::new(800)?)?;
//! let mut evidence = Scores::new(&question);
//! evidence.set(OptionIndex::new(0), Confidence::new(900)?)?;
//! evidence.set(OptionIndex::new(1), Confidence::new(950)?)?;
//! evidence.set(OptionIndex::new(2), Confidence::new(400)?)?;
//! let result = arbitrate(&evidence, &drives, &affinity, None, Profile::Deep)?;
//! assert!(matches!(
//!     result.answer(),
//!     Answer::Choice { option, .. } if *option == OptionIndex::new(0)
//! ));
//! assert_eq!(result.winner_drive(), Some(DriveIndex::new(1)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod answer;
mod candidate;
mod decide;
mod decision;
mod instinct;
mod profile;
mod question;
mod score;
mod span;
mod trail;
mod version;

pub use answer::{AbstainReason, Answer, Ranked, RankedError};
pub use candidate::{CandidateError, CandidateId, CandidateSet, MAX_ID_BYTES};
pub use decide::{Scores, ScoresError, abstain, decide};
pub use decision::Decision;
pub use instinct::{
    Affinity, Arbitration, Drive, DriveIndex, Drives, INSTINCT_TAG, InstinctError, MAX_DRIVES,
    arbitrate,
};
pub use profile::{Profile, Thresholds};
pub use question::{Label, MAX_LABEL_BYTES, OptionIndex, Options, Question, QuestionError};
pub use score::{Confidence, Millis, ScoreError};
pub use span::{Span, SpanError};
pub use trail::{ScorerKind, StageKind, Trail, TrailRecord};
pub use version::{DataVersion, DataVersionBuilder, DataVersionError};

/// The version of `instinct-core`, folded into every [`DataVersion`] built with
/// [`DataVersion::builder`].
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
