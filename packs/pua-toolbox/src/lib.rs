//! `pua-toolbox`: choose a tool from candidates, or abstain (spec §6.5).
//!
//! Candidates are a **set** ([`CandidateSet`]): ids sorted ascending, unique (id is the
//! tie-break). Scores are word overlap ([`pua_lexicon::overlap`]).
//! Namespaces are symbolic string prefixes of the tool id. A call-DAG WL fingerprint is
//! available for consumers that group by call structure.
//!
//! ```
//! use pua_core::{Answer, OptionIndex, Profile};
//! use pua_toolbox::{select, Candidate};
//!
//! let tools = [
//!     Candidate::new("fs.read", "read a file from disk"),
//!     Candidate::new("fs.write", "write a file to disk"),
//! ];
//! let decision = select("please write the file now", &tools, Profile::Deep);
//! assert_eq!(decision.answer().chosen(), Some(OptionIndex::new(1)));
//! ```
#![forbid(unsafe_code)]

use pua_core::{
    AbstainReason, Answer, CandidateId, CandidateSet, Confidence, DataVersion, Decision,
    OptionIndex, Profile, Ranked, Scores, StageKind, Trail, TrailRecord, abstain, decide,
};
use pua_graph::{Edge, LabeledGraph, Rounds, label_of, wl_refine};
use pua_lexicon::overlap;
use pua_text::{NormalizeConfig, normalize};
use serde::{Deserialize, Serialize};

/// Algorithm tag.
pub const ALGORITHM_TAG: &str = "pua-toolbox/1";

/// Stable tool id (namespace is symbolic — compared as part of the string).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ToolId(Box<str>);

impl ToolId {
    /// Creates an id.
    pub fn new(id: impl Into<Box<str>>) -> Self {
        Self(id.into())
    }
    /// As str.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for ToolId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One tool candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Candidate {
    id: ToolId,
    description: Box<str>,
}

impl Candidate {
    /// Builds a candidate.
    pub fn new(id: impl Into<Box<str>>, description: impl Into<Box<str>>) -> Self {
        Self {
            id: ToolId::new(id),
            description: description.into(),
        }
    }
    /// Id.
    pub fn id(&self) -> &ToolId {
        &self.id
    }
    /// Description.
    pub fn description(&self) -> &str {
        &self.description
    }
}

fn refuse(why: &str, profile: Profile, candidates: &[Candidate]) -> Decision {
    let mut trail = Trail::new();
    trail.push(TrailRecord::new(StageKind::Decide, why));
    // No valid question exists here (fewer than 2 distinct, valid ids), so there are no
    // options to rank.
    let ranked = Ranked::try_from(Vec::new()).unwrap_or_else(|_| unreachable!());
    Decision::new(
        Answer::Abstain {
            why: AbstainReason::NoCandidates,
            ranked,
        },
        profile,
        version(candidates),
        trail,
    )
}

fn candidate_set(candidates: &[Candidate]) -> Result<CandidateSet, String> {
    let ids = candidates
        .iter()
        .map(|c| CandidateId::new(c.id().as_str()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    CandidateSet::new(ids).map_err(|e| e.to_string())
}

/// The text a candidate is scored by: its description plus the leaf of its id
/// (`fs.write` → `write`).
fn candidate_text(c: &Candidate) -> String {
    let leaf = c.id().as_str().rsplit(['.', '/', ':']).next().unwrap_or("");
    format!("{} {leaf}", c.description())
}

/// Selects a tool for `message` from `candidates` (order-independent).
///
/// Scores are [`pua_lexicon::overlap`] of the message with each candidate's text; the winner
/// goes through [`pua_core::decide`] over a [`CandidateSet`]. Invalid or duplicate ids, and
/// fewer than two tools, abstain with [`AbstainReason::NoCandidates`] and say why in the trail.
pub fn select(message: &str, candidates: &[Candidate], profile: Profile) -> Decision {
    let set = match candidate_set(candidates) {
        Ok(set) => set,
        Err(e) => return refuse(&e, profile, candidates),
    };
    let Ok(question) = set.question("tool") else {
        return refuse("too few tools", profile, candidates);
    };
    let mut trail = Trail::new();
    let config = NormalizeConfig::default();
    let mut scores = Scores::new(&question);
    let Ok(query) = normalize(message, config) else {
        trail.push(TrailRecord::new(
            StageKind::Normalize,
            "input refused (too long)",
        ));
        let answer = abstain(&scores, AbstainReason::NoCandidates);
        return Decision::new(answer, profile, version(candidates), trail);
    };
    for c in candidates {
        let text = candidate_text(c);
        let score = normalize(&text, config)
            .ok()
            .and_then(|n| overlap(&query, &n).ok())
            .unwrap_or(Confidence::ZERO);
        if let Some(idx) = set.index_of(c.id().as_str()) {
            let _ = scores.set(idx, score);
        }
    }
    for (id, score) in set.ids().iter().zip(scores_in_order(&set, &scores)) {
        trail.push(
            TrailRecord::new(StageKind::Decide, format!("{id} → {}", score.get()))
                .millis(pua_core::Millis::saturating(i32::from(score.get()))),
        );
    }
    let answer = decide(&scores, profile);
    trail.push(TrailRecord::new(
        StageKind::Decide,
        match (&answer, set.chosen(&answer)) {
            (_, Some(id)) => format!("chose {id}"),
            (Answer::Abstain { why, .. }, None) => format!("abstain: {why}"),
            (other, None) => format!("{other:?}"),
        },
    ));
    Decision::new(answer, profile, version(candidates), trail)
}

fn scores_in_order(set: &CandidateSet, scores: &Scores<'_>) -> Vec<Confidence> {
    (0..set.len())
        .map(|i| {
            scores
                .get(OptionIndex::new(u16::try_from(i).unwrap_or(u16::MAX)))
                .unwrap_or(Confidence::ZERO)
        })
        .collect()
}

/// Resolves the chosen tool id from a decision. `None` on abstain: an abstain must never be
/// acted on as if a tool had been picked (use the ranked list in the answer to show options).
pub fn chosen_id<'a>(decision: &Decision, candidates: &'a [Candidate]) -> Option<&'a ToolId> {
    let set = candidate_set(candidates).ok()?;
    let id = set.chosen(decision.answer())?;
    candidates
        .iter()
        .map(Candidate::id)
        .find(|t| t.as_str() == id.as_str())
}

fn version(candidates: &[Candidate]) -> DataVersion {
    let mut h = blake3::Hasher::new();
    h.update(ALGORITHM_TAG.as_bytes());
    let mut ids: Vec<_> = candidates.iter().map(|c| c.id().as_str()).collect();
    ids.sort_unstable();
    for id in ids {
        h.update(id.as_bytes());
        h.update(&[0]);
    }
    DataVersion::builder(ALGORITHM_TAG)
        .field("tools", h.finalize().as_bytes())
        .finish()
}

/// WL fingerprint of a tool-call DAG. Nodes are `(tool id, local label)`; edges are directed calls.
pub fn call_dag_fingerprint(nodes: &[(ToolId, u64)], edges: &[(usize, usize)]) -> [u8; 32] {
    let mut g = LabeledGraph::new();
    let mut ids = Vec::new();
    for (tool, label) in nodes {
        let lab = label_of(&[tool.as_str().as_bytes(), &label.to_le_bytes()]);
        if let Ok(id) = g.add_node(lab) {
            ids.push(id);
        }
    }
    for &(a, b) in edges {
        if let (Some(&from), Some(&to)) = (ids.get(a), ids.get(b)) {
            let _ = g.add_edge(from, to, Edge::directed());
        }
    }
    wl_refine(&g, Rounds::Fixed(3)).fingerprint()
}

#[cfg(test)]
mod tests;
