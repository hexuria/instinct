//! `pua-pack-tool-selection`: choose a tool from candidates, or abstain (spec §6.5).
//!
//! Candidates are a **set**: scored after sorting by id ascending (id is the tie-break).
//! Namespaces are symbolic string prefixes of the tool id. A call-DAG WL fingerprint is
//! available for consumers that group by call structure.
//!
//! ```
//! use pua_core::{Answer, OptionIndex, Profile};
//! use pua_pack_tool_selection::{select, Candidate};
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
    AbstainReason, Answer, Confidence, DataVersion, Decision, OptionIndex, Profile, Question,
    Ranked, Scores, StageKind, Trail, TrailRecord, decide,
};
use pua_graph::{Edge, LabeledGraph, Rounds, wl_refine};
use pua_text::{NormalizeConfig, normalize};
use serde::{Deserialize, Serialize};

/// Algorithm tag.
pub const ALGORITHM_TAG: &str = "pua-pack-tool-selection/1";

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

/// Selects a tool for `message` from `candidates` (order-independent).
pub fn select(message: &str, candidates: &[Candidate], profile: Profile) -> Decision {
    let mut trail = Trail::new();
    let mut sorted: Vec<&Candidate> = candidates.iter().collect();
    sorted.sort_by_key(|c| c.id().as_str());

    let labels: Vec<&str> = sorted.iter().map(|c| c.id().as_str()).collect();
    let Ok(question) = Question::choice("tool", &labels) else {
        trail.push(TrailRecord::new(StageKind::Decide, "too few tools"));
        let ranked = Ranked::try_from(Vec::new()).unwrap_or_else(|_| unreachable!());
        return Decision::new(
            Answer::Abstain {
                why: AbstainReason::NoCandidates,
                ranked,
            },
            profile,
            version(candidates),
            trail,
        );
    };

    let config = NormalizeConfig::default();
    let canon = normalize(message, config)
        .map(|n| n.canonical().to_owned())
        .unwrap_or_default();

    let mut scores = Scores::new(&question);
    for (i, c) in sorted.iter().enumerate() {
        let desc = normalize(c.description(), config).map_or_else(
            |_| c.description().to_ascii_lowercase(),
            |n| n.canonical().to_owned(),
        );
        let score = overlap_score(&canon, c.id().as_str(), &desc);
        let idx = OptionIndex::new(u16::try_from(i).unwrap_or(u16::MAX));
        let _ = scores.set(idx, score);
        trail.push(
            TrailRecord::new(StageKind::Decide, format!("{} → {}", c.id(), score.get()))
                .millis(pua_core::Millis::saturating(i32::from(score.get()))),
        );
    }
    let answer = decide(&scores, profile);
    trail.push(TrailRecord::new(
        StageKind::Decide,
        match &answer {
            Answer::Choice { option, .. } => {
                format!("chose {}", sorted[option.get() as usize].id())
            }
            Answer::Abstain { why, .. } => format!("abstain: {why}"),
            other => format!("{other:?}"),
        },
    ));
    Decision::new(answer, profile, version(candidates), trail)
}

/// Resolves the chosen tool id from a decision (or the top-ranked id on abstain).
pub fn chosen_id<'a>(decision: &Decision, candidates: &'a [Candidate]) -> Option<&'a ToolId> {
    let mut sorted: Vec<&Candidate> = candidates.iter().collect();
    sorted.sort_by_key(|c| c.id().as_str());
    let idx = match decision.answer() {
        Answer::Choice { option, .. } => Some(*option),
        Answer::Abstain { ranked, .. } => ranked.top().map(|(i, _)| i),
        _ => None,
    }?;
    sorted.get(idx.get() as usize).map(|c| c.id())
}

fn is_token_sep(c: char) -> bool {
    !c.is_alphanumeric()
}

fn overlap_score(message: &str, tool_id: &str, description: &str) -> Confidence {
    const MIN_TOKEN: usize = 3;
    let msg: Vec<&str> = message
        .split(is_token_sep)
        .filter(|t| t.len() >= MIN_TOKEN)
        .collect();
    let mut desc: Vec<&str> = description
        .split(is_token_sep)
        .filter(|t| t.len() >= MIN_TOKEN)
        .collect();
    if let Some(leaf) = tool_id.rsplit(['.', '/', ':']).next()
        && leaf.len() >= MIN_TOKEN
        && desc.iter().all(|t| *t != leaf)
    {
        desc.push(leaf);
    }
    if desc.is_empty() {
        return Confidence::ZERO;
    }
    let hits = desc.iter().filter(|t| msg.iter().any(|m| m == *t)).count();
    let millis = hits.saturating_mul(1000) / desc.len();
    Confidence::saturating(i32::try_from(millis).unwrap_or(0))
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
        let mut h = blake3::Hasher::new();
        h.update(tool.as_str().as_bytes());
        h.update(&label.to_le_bytes());
        let digest = h.finalize();
        let lab = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap_or([0; 8]));
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
