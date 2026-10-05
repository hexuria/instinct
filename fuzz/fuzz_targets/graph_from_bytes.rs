//! Graphs built from arbitrary bytes: refinement never panics, stays within its round bound,
//! and the fingerprint does not depend on node insertion order (checked by rebuilding the graph
//! with nodes inserted in reverse).
#![no_main]

use libfuzzer_sys::fuzz_target;
use pua_graph::{Edge, LabeledGraph, NodeId, Rounds, spd_wl_refine, wl_refine};

fn build(n: usize, labels: &[u8], edges: &[[u8; 3]], reverse: bool) -> LabeledGraph {
    let mut g = LabeledGraph::new();
    let mut ids: Vec<Option<NodeId>> = vec![None; n];
    let order: Vec<usize> = if reverse { (0..n).rev().collect() } else { (0..n).collect() };
    for i in order {
        ids[i] = Some(g.add_node(u64::from(labels.get(i).copied().unwrap_or(0) % 4)).expect("small"));
    }
    for [a, b, kind] in edges {
        let (Some(Some(x)), Some(Some(y))) =
            (ids.get(usize::from(*a) % n), ids.get(usize::from(*b) % n))
        else {
            continue;
        };
        let mut e = if kind & 1 == 1 { Edge::directed() } else { Edge::undirected() };
        if kind & 2 == 2 {
            e = e.labeled(u64::from(kind >> 2));
        }
        g.add_edge(*x, *y, e).expect("same graph");
    }
    g
}

fuzz_target!(|data: &[u8]| {
    let Some((&n, rest)) = data.split_first() else { return };
    let n = usize::from(n % 24) + 1;
    let (labels, rest) = rest.split_at(rest.len().min(n));
    let edges: Vec<[u8; 3]> = rest.chunks_exact(3).take(64).map(|c| [c[0], c[1], c[2]]).collect();
    let g = build(n, labels, &edges, false);
    let r = build(n, labels, &edges, true);
    for rounds in [Rounds::Fixed(3), Rounds::ToStability] {
        let a = wl_refine(&g, rounds);
        assert!(a.rounds() as usize <= n.max(3));
        assert_eq!(a.fingerprint(), wl_refine(&r, rounds).fingerprint());
        assert_eq!(
            spd_wl_refine(&g, rounds).fingerprint(),
            spd_wl_refine(&r, rounds).fingerprint()
        );
    }
});
