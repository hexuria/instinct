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
#![forbid(unsafe_code)]

mod answer;
mod candidate;
mod decide;
mod decision;
mod profile;
mod question;
mod score;
mod span;
mod trail;
mod version;

pub use answer::{AbstainReason, Answer, Ranked};
pub use candidate::{CandidateError, CandidateId, CandidateSet, MAX_ID_BYTES};
pub use decide::{Scores, ScoresError, abstain, decide};
pub use decision::Decision;
pub use profile::{Profile, Thresholds};
pub use question::{Label, MAX_LABEL_BYTES, OptionIndex, Options, Question, QuestionError};
pub use score::{Confidence, Millis, ScoreError};
pub use span::{Span, SpanError};
pub use trail::{ScorerKind, StageKind, Trail, TrailRecord};
pub use version::{DataVersion, DataVersionBuilder, DataVersionError};

/// The version of `instinct-core`, folded into every [`DataVersion`] built with
/// [`DataVersion::builder`].
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
