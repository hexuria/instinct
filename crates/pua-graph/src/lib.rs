//! `pua-graph`: labeled graphs and Weisfeiler-Leman colour refinement (spec §4.8).
//!
//! - [`LabeledGraph`]: a `u32` node arena; nodes carry a `u64` label; edges are undirected or
//!   directed and optionally labeled. Multi-edges and self-loops are allowed (the neighbour
//!   multiset counts them).
//! - [`wl_refine`]: 1-WL. `c⁽ᵗ⁺¹⁾(v) = blake3(tag ‖ c⁽ᵗ⁾(v) ‖ sorted neighbour tuples)` where a
//!   tuple is `(edge label, direction, c⁽ᵗ⁾(u))`. Folding edge label and direction into the
//!   tuple is the standard extension; the book defines node-labeled undirected 1-WL only.
//! - [`Rounds::Fixed`] (default 3, portable) or [`Rounds::ToStability`] (≤ n rounds, stops
//!   when the colour partition stops refining).
//! - [`Refinement::fingerprint`]: blake3 over tag, rounds and the sorted final colours.
//! - [`label_of`]: a stable `u64` label from domain key bytes.
//!
//! **Semantics.** A different fingerprint means the structures differ (modulo a 256-bit hash
//! collision). The same fingerprint means **WL-equivalent, not isomorphic**: `C₆` and two
//! disjoint triangles collide, regular graphs of equal degree collide, and 1-WL cannot see cut
//! edges. Use a fingerprint as a lookup or grouping key, never as proof of identity. The
//! `spd-wl` feature adds shortest-path-distance WL, which closes the cut-edge blind spot.
//!
//! ```
//! use pua_graph::{Edge, LabeledGraph, Rounds, wl_refine};
//!
//! let cycle = |n: u32| {
//!     let mut g = LabeledGraph::new();
//!     let v: Vec<_> = (0..n).map(|_| g.add_node(0)).collect::<Result<_, _>>()?;
//!     for i in 0..n as usize {
//!         g.add_edge(v[i], v[(i + 1) % n as usize], Edge::undirected())?;
//!     }
//!     Ok::<_, pua_graph::GraphError>(g)
//! };
//! let mut two_triangles = cycle(3)?;
//! let more = (0..3).map(|_| two_triangles.add_node(0)).collect::<Result<Vec<_>, _>>()?;
//! for i in 0..3 {
//!     two_triangles.add_edge(more[i], more[(i + 1) % 3], Edge::undirected())?;
//! }
//! let fp = |g: &LabeledGraph| wl_refine(g, Rounds::default()).fingerprint();
//! // WL-equivalent, not isomorphic: the documented collision.
//! assert_eq!(fp(&cycle(6)?), fp(&two_triangles));
//! assert_ne!(fp(&cycle(6)?), fp(&cycle(5)?));
//! # Ok::<(), pua_graph::GraphError>(())
//! ```
#![forbid(unsafe_code)]

use core::fmt;

/// Tag folded into every colour and fingerprint (spec §4.8); part of `DataVersion`.
pub const WL_TAG: &str = "wl-v1";
/// Default fixed number of rounds (owner decision q15: a fixed `h = 3` for portable fingerprints).
pub const DEFAULT_ROUNDS: u8 = 3;

/// A node handle into one [`LabeledGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// Index in insertion order.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Edge direction and optional label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    label: Option<u64>,
    directed: bool,
}

impl Edge {
    /// Unlabeled, undirected.
    pub const fn undirected() -> Self {
        Self {
            label: None,
            directed: false,
        }
    }
    /// Unlabeled, directed `from → to`.
    pub const fn directed() -> Self {
        Self {
            label: None,
            directed: true,
        }
    }
    /// The same edge with a label.
    #[must_use]
    pub const fn labeled(self, label: u64) -> Self {
        Self {
            label: Some(label),
            directed: self.directed,
        }
    }
}

/// Why a graph operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphError {
    /// A node id from another graph, or past the end.
    UnknownNode {
        /// The id's index.
        index: usize,
    },
    /// The arena is full (`u32::MAX` nodes).
    TooManyNodes,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode { index } => write!(f, "unknown node {index}"),
            Self::TooManyNodes => f.write_str("graph is full (u32::MAX nodes)"),
        }
    }
}

impl std::error::Error for GraphError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Dir {
    Undirected = 0,
    Out = 1,
    In = 2,
}

/// A node-labeled graph with optionally directed, optionally labeled edges.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct LabeledGraph {
    labels: Vec<u64>,
    /// Per node: (neighbour, edge label, direction as seen from this node).
    adj: Vec<Vec<(u32, Option<u64>, Dir)>>,
    edges: usize,
}

impl LabeledGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node with `label`.
    ///
    /// # Errors
    /// [`GraphError::TooManyNodes`] when the arena is full.
    pub fn add_node(&mut self, label: u64) -> Result<NodeId, GraphError> {
        let id = u32::try_from(self.labels.len()).map_err(|_| GraphError::TooManyNodes)?;
        if id == u32::MAX {
            return Err(GraphError::TooManyNodes);
        }
        self.labels.push(label);
        self.adj.push(Vec::new());
        Ok(NodeId(id))
    }

    /// Adds an edge (`from → to` when directed).
    ///
    /// # Errors
    /// [`GraphError::UnknownNode`] if either endpoint is not in this graph.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, edge: Edge) -> Result<(), GraphError> {
        for n in [from, to] {
            if n.index() >= self.labels.len() {
                return Err(GraphError::UnknownNode { index: n.index() });
            }
        }
        if edge.directed {
            self.adj[from.index()].push((to.0, edge.label, Dir::Out));
            self.adj[to.index()].push((from.0, edge.label, Dir::In));
        } else {
            self.adj[from.index()].push((to.0, edge.label, Dir::Undirected));
            if from != to {
                self.adj[to.index()].push((from.0, edge.label, Dir::Undirected));
            }
        }
        self.edges += 1;
        Ok(())
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.labels.len()
    }

    /// Number of edges added.
    pub fn edge_count(&self) -> usize {
        self.edges
    }

    /// A node's label.
    pub fn label(&self, n: NodeId) -> Option<u64> {
        self.labels.get(n.index()).copied()
    }
}

/// How many refinement rounds to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rounds {
    /// Exactly `h` rounds: a portable fingerprint (`h` is folded into it).
    Fixed(u8),
    /// Until the colour partition stops refining (at most `n` rounds).
    ToStability,
}

impl Default for Rounds {
    fn default() -> Self {
        Self::Fixed(DEFAULT_ROUNDS)
    }
}

/// A node colour (256-bit).
pub type Colour = [u8; 32];

/// Result of a refinement: per-node colours and the rounds actually run.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Refinement {
    colours: Vec<Colour>,
    rounds: u32,
    mode: &'static str,
    requested: Rounds,
}

impl Refinement {
    /// Final colour of `n`, so explain can say which node differs.
    pub fn colour(&self, n: NodeId) -> Option<&Colour> {
        self.colours.get(n.index())
    }

    /// All final colours in node order.
    pub fn colours(&self) -> &[Colour] {
        &self.colours
    }

    /// Rounds actually run.
    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    /// Number of distinct final colours.
    pub fn classes(&self) -> usize {
        distinct(&self.colours)
    }

    /// blake3 over the mode tag (`wl-v1` or `spd-wl-v1`), the requested rounds, the rounds run,
    /// the node count and the sorted final colours. Independent of node numbering.
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut sorted = self.colours.clone();
        sorted.sort_unstable();
        let mut h = blake3::Hasher::new();
        h.update(self.mode.as_bytes());
        match self.requested {
            Rounds::Fixed(r) => h.update(&[0, r]),
            Rounds::ToStability => h.update(&[1, 0]),
        };
        h.update(&self.rounds.to_le_bytes());
        h.update(&(sorted.len() as u64).to_le_bytes());
        for c in &sorted {
            h.update(c);
        }
        *h.finalize().as_bytes()
    }

    /// [`Refinement::fingerprint`] as lowercase hex.
    pub fn fingerprint_hex(&self) -> String {
        use core::fmt::Write as _;
        self.fingerprint()
            .iter()
            .fold(String::with_capacity(64), |mut s, b| {
                let _ = write!(s, "{b:02x}");
                s
            })
    }
}

fn distinct(colours: &[Colour]) -> usize {
    let mut v: Vec<&Colour> = colours.iter().collect();
    v.sort_unstable();
    v.dedup();
    v.len()
}

fn initial(tag: &str, labels: &[u64]) -> Vec<Colour> {
    labels
        .iter()
        .map(|l| {
            let mut h = blake3::Hasher::new();
            h.update(tag.as_bytes());
            h.update(b"init");
            h.update(&l.to_le_bytes());
            *h.finalize().as_bytes()
        })
        .collect()
}

/// Runs refinement rounds with `step` until `rounds` says stop.
fn refine_with(
    g: &LabeledGraph,
    rounds: Rounds,
    mode: &'static str,
    mut step: impl FnMut(&[Colour]) -> Vec<Colour>,
) -> Refinement {
    let mut colours = initial(mode, &g.labels);
    let mut run = 0u32;
    match rounds {
        Rounds::Fixed(h) => {
            for _ in 0..h {
                colours = step(&colours);
                run += 1;
            }
        }
        Rounds::ToStability => {
            let mut classes = distinct(&colours);
            for _ in 0..g.node_count() {
                let next = step(&colours);
                run += 1;
                let next_classes = distinct(&next);
                colours = next;
                if next_classes == classes {
                    break;
                }
                classes = next_classes;
            }
        }
    }
    Refinement {
        colours,
        rounds: run,
        mode,
        requested: rounds,
    }
}

/// A stable `u64` node or edge label from a sequence of byte parts: the first 8 bytes
/// (little-endian) of `blake3(tag ‖ len ‖ part ‖ len ‖ part …)`. Length-prefixing keeps
/// `["ab", "c"]` and `["a", "bc"]` apart. Use it to turn domain keys (tool ids, field names)
/// into labels without each consumer inventing its own hash.
///
/// ```
/// use pua_graph::label_of;
/// assert_eq!(label_of(&[b"fs.read"]), label_of(&[b"fs.read"]));
/// assert_ne!(label_of(&[b"ab", b"c"]), label_of(&[b"a", b"bc"]));
/// ```
pub fn label_of(parts: &[&[u8]]) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"pua-graph/label/1");
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let d = h.finalize();
    let mut out = [0u8; 8];
    out.copy_from_slice(&d.as_bytes()[..8]);
    u64::from_le_bytes(out)
}

/// 1-WL colour refinement (spec §4.8).
pub fn wl_refine(g: &LabeledGraph, rounds: Rounds) -> Refinement {
    refine_with(g, rounds, WL_TAG, |colours| {
        g.adj
            .iter()
            .enumerate()
            .map(|(v, nbrs)| {
                let mut tuples: Vec<(u8, u64, u8, &Colour)> = nbrs
                    .iter()
                    .map(|&(u, label, dir)| {
                        (
                            u8::from(label.is_some()),
                            label.unwrap_or(0),
                            dir as u8,
                            &colours[u as usize],
                        )
                    })
                    .collect();
                tuples.sort_unstable();
                let mut h = blake3::Hasher::new();
                h.update(WL_TAG.as_bytes());
                h.update(&colours[v]);
                h.update(&(tuples.len() as u64).to_le_bytes());
                for (has, label, dir, c) in tuples {
                    h.update(&[has]);
                    h.update(&label.to_le_bytes());
                    h.update(&[dir]);
                    h.update(c);
                }
                *h.finalize().as_bytes()
            })
            .collect()
    })
}

/// Tag for shortest-path-distance WL.
#[cfg(feature = "spd-wl")]
pub const SPD_WL_TAG: &str = "spd-wl-v1";

/// Shortest-path-distance WL (GD-WL with BFS distances over the underlying undirected graph):
/// `c⁽ᵗ⁺¹⁾(v) = blake3(tag ‖ c⁽ᵗ⁾(v) ‖ sorted {(dist(v, u), c⁽ᵗ⁾(u)) : u ≠ v})`, with
/// unreachable nodes at distance `u32::MAX`. `O(n·(n+m))` per round; integer only.
#[cfg(feature = "spd-wl")]
pub fn spd_wl_refine(g: &LabeledGraph, rounds: Rounds) -> Refinement {
    let n = g.node_count();
    let dist: Vec<Vec<u32>> = (0..n).map(|s| bfs(g, s)).collect();
    refine_with(g, rounds, SPD_WL_TAG, |colours| {
        (0..n)
            .map(|v| {
                let mut pairs: Vec<(u32, &Colour)> = (0..n)
                    .filter(|&u| u != v)
                    .map(|u| (dist[v][u], &colours[u]))
                    .collect();
                pairs.sort_unstable();
                let mut h = blake3::Hasher::new();
                h.update(SPD_WL_TAG.as_bytes());
                h.update(&colours[v]);
                h.update(&(pairs.len() as u64).to_le_bytes());
                for (d, c) in pairs {
                    h.update(&d.to_le_bytes());
                    h.update(c);
                }
                *h.finalize().as_bytes()
            })
            .collect()
    })
}

#[cfg(feature = "spd-wl")]
fn bfs(g: &LabeledGraph, s: usize) -> Vec<u32> {
    let mut d = vec![u32::MAX; g.node_count()];
    let mut queue = std::collections::VecDeque::new();
    d[s] = 0;
    queue.push_back(s);
    while let Some(v) = queue.pop_front() {
        for &(u, _, _) in &g.adj[v] {
            let u = u as usize;
            if d[u] == u32::MAX {
                d[u] = d[v] + 1;
                queue.push_back(u);
            }
        }
    }
    d
}

#[cfg(test)]
mod tests;
