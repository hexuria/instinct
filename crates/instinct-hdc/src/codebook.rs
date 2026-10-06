//! Item memory: `(id, vector)` sorted by id; cleanup and iterative top-k decode.

use core::fmt;

use instinct_core::Millis;

use crate::acc::Accumulator;
use crate::hv::{Dim, Hv};

/// Why a codebook was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodebookError {
    /// No entries.
    Empty,
    /// An id is empty.
    EmptyId {
        /// Entry index as given.
        index: usize,
    },
    /// Two entries share an id.
    DuplicateId {
        /// The id.
        id: String,
    },
}

impl fmt::Display for CodebookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("codebook has no entries"),
            Self::EmptyId { index } => write!(f, "codebook entry {index}: empty id"),
            Self::DuplicateId { id } => write!(f, "codebook: duplicate id {id:?}"),
        }
    }
}

impl std::error::Error for CodebookError {}

/// The nearest entry and how clearly it won.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nearest {
    /// Index into the (sorted) codebook.
    pub index: usize,
    /// Similarity to the query.
    pub similarity: Millis,
    /// Gap to the runner-up (`similarity` itself, as a gap to `−1000`, for a single entry is
    /// not meaningful, so a 1-entry codebook reports the full `2000` clamped to `1000`).
    pub margin: Millis,
}

/// A validated item memory, sorted by id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Codebook<D: Dim> {
    ids: Box<[Box<str>]>,
    vecs: Box<[Hv<D>]>,
}

impl<D: Dim> Codebook<D> {
    /// Builds from entries in any order.
    ///
    /// # Errors
    /// [`CodebookError::Empty`], [`CodebookError::EmptyId`] or [`CodebookError::DuplicateId`].
    pub fn new(entries: impl IntoIterator<Item = (String, Hv<D>)>) -> Result<Self, CodebookError> {
        let mut v: Vec<(String, Hv<D>)> = entries.into_iter().collect();
        if v.is_empty() {
            return Err(CodebookError::Empty);
        }
        if let Some(index) = v.iter().position(|(id, _)| id.is_empty()) {
            return Err(CodebookError::EmptyId { index });
        }
        v.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(w) = v.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(CodebookError::DuplicateId { id: w[0].0.clone() });
        }
        let (ids, vecs): (Vec<Box<str>>, Vec<Hv<D>>) = v
            .into_iter()
            .map(|(id, h)| (id.into_boxed_str(), h))
            .unzip();
        Ok(Self {
            ids: ids.into_boxed_slice(),
            vecs: vecs.into_boxed_slice(),
        })
    }

    /// Number of entries (≥ 1).
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Always `false`.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Id at `index` (`""` when out of range).
    pub fn id(&self, index: usize) -> &str {
        self.ids.get(index).map_or("", |s| s)
    }

    /// Vector at `index`.
    pub fn vector(&self, index: usize) -> Option<&Hv<D>> {
        self.vecs.get(index)
    }

    /// Index of `id`.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.ids.binary_search_by(|x| (**x).cmp(id)).ok()
    }

    /// All vectors in id order.
    pub fn vectors(&self) -> &[Hv<D>] {
        &self.vecs
    }

    /// Nearest entry by similarity, ties to the lower id.
    pub fn nearest(&self, q: &Hv<D>) -> Nearest {
        best_two(self.vecs.iter().map(|v| i64::from(q.similarity(v).get())))
    }

    /// Cleanup: the nearest entry if its similarity is at least `floor`.
    pub fn cleanup(&self, q: &Hv<D>, floor: Millis) -> Option<Nearest> {
        let n = self.nearest(q);
        (n.similarity >= floor).then_some(n)
    }

    /// Iterative top-`k` decode of a bundle from its accumulator (spec §4.5): find the entry
    /// with the largest normalized dot (`dot · 1000 / Σ|sums|`, ties to the lower id), subtract
    /// its estimated contribution (`round(dot / D)` times the entry), repeat. Stops early when
    /// the best score drops below `floor`. Each entry is returned at most once.
    pub fn decode(&self, acc: &Accumulator<D>, k: usize, floor: Millis) -> Vec<(usize, Millis)> {
        let mut acc = acc.clone();
        let mut out: Vec<(usize, Millis)> = Vec::new();
        let d = i64::from(D::BITS);
        for _ in 0..k.min(self.len()) {
            let l1 = acc.l1().max(1);
            let mut best: Option<(i64, usize, i64)> = None;
            for (i, v) in self.vecs.iter().enumerate() {
                if out.iter().any(|(j, _)| *j == i) {
                    continue;
                }
                let dot = acc.dot(v);
                let score = dot * 1000 / l1;
                if best.is_none_or(|(s, _, _)| score > s) {
                    best = Some((score, i, dot));
                }
            }
            let Some((score, i, dot)) = best else { break };
            let sim = Millis::saturating(i32::try_from(score).unwrap_or(i32::MIN));
            if sim < floor {
                break;
            }
            out.push((i, sim));
            // Round half away from zero.
            let est = if dot >= 0 {
                (2 * dot + d) / (2 * d)
            } else {
                (2 * dot - d) / (2 * d)
            };
            acc.add(&self.vecs[i], -i32::try_from(est).unwrap_or(0));
        }
        out
    }
}

/// Best and runner-up of integer scores; ties go to the lower index.
pub(crate) fn best_two(scores: impl Iterator<Item = i64>) -> Nearest {
    let mut best: Option<(usize, i64)> = None;
    let mut second: Option<i64> = None;
    for (i, s) in scores.enumerate() {
        match best {
            Some((_, b)) if s <= b => {
                if second.is_none_or(|x| s > x) {
                    second = Some(s);
                }
            }
            _ => {
                second = best.map(|(_, b)| b);
                best = Some((i, s));
            }
        }
    }
    let (index, top) = best.unwrap_or((0, 0));
    let margin = top - second.unwrap_or(-1000);
    Nearest {
        index,
        similarity: Millis::saturating(i32::try_from(top).unwrap_or(0)),
        margin: Millis::saturating(i32::try_from(margin).unwrap_or(i32::MAX)),
    }
}
