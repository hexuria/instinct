//! `pua-core`: the shared vocabulary of PUA, the Predictable Universal Advisor.
//!
//! - Scores: [`Millis`] (similarity, −1000..=1000) and [`Confidence`] (0..=1000). Integer only;
//!   they are deterministic scores, **not calibrated probabilities** (spec §5 rule 2, ADR 0002).
//! - [`Span`]: byte offsets into the **original** text.
//! - [`Question`] / [`Answer`]: Jev's three shapes (Noul, Choice, Score) plus `Abstain`.
//! - [`Profile`]: threshold tables (`fast`, `standard`, `deep`, spec §4.6).
//! - [`decide`] and [`rank_candidates`]: the only places a winner is picked, with the stable
//!   tie-break of spec §5 rule 4 (options are an ordered list, candidates are a set).
//! - [`Trail`], [`Decision`], [`DataVersion`] and the [`Pack`] trait.
//!
//! Same input + same [`DataVersion`] + same [`Profile`] gives a byte-identical [`Decision`].
//!
//! ```
//! use pua_core::{Answer, Confidence, OptionIndex, Profile, Question, Scores, decide};
//!
//! let q = Question::choice("delivery", &["queue", "steer", "interrupt"])?;
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
mod pack;
mod profile;
mod question;
mod score;
mod span;
mod trail;
mod version;

pub use answer::{AbstainReason, Answer, Ranked};
pub use candidate::{
    CandidateError, CandidateId, CandidatePick, MAX_ID_BYTES, RankedCandidates, rank_candidates,
};
pub use decide::{Scores, ScoresError, decide};
pub use pack::{Decision, Pack};
pub use profile::{HdcMode, Profile, Thresholds};
pub use question::{Label, MAX_LABEL_BYTES, OptionIndex, Options, Question, QuestionError};
pub use score::{Confidence, Millis, ScoreError};
pub use span::{Span, SpanError};
pub use trail::{ScorerKind, StageKind, Trail, TrailRecord};
pub use version::{DataVersion, DataVersionBuilder, DataVersionError};

/// The version of `pua-core`, folded into every [`DataVersion`] built with
/// [`DataVersion::builder`].
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
