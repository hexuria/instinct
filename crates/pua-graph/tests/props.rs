//! WL is a function of the graph, not of node numbering (spec §4.8; plan T8).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)]

use proptest::prelude::*;
use pua_graph::{Edge, LabeledGraph, NodeId, Rounds, wl_refine};

type Spec = (Vec<u64>, Vec<(usize, usize, Option<u64>, bool)>);

fn spec() -> impl Strategy<Value = Spec> {
    (1usize..10).prop_flat_map(|n| {
        (
            proptest::collection::vec(0u64..3, n),
            proptest::collection::vec(
                (0..n, 0..n, proptest::option::of(0u64..3), any::<bool>()),
                0..20,
            ),
        )
    })
}

/// Builds the graph with node `i` placed at position `perm[i]` and edges in `edge_order`.
fn build(s: &Spec, perm: &[usize], edge_order: &[usize]) -> (LabeledGraph, Vec<NodeId>) {
    let n = s.0.len();
    let mut inv = vec![0; n];
    for (i, &p) in perm.iter().enumerate() {
        inv[p] = i;
    }
    let mut g = LabeledGraph::new();
    let mut ids: Vec<Option<NodeId>> = vec![None; n];
    for &orig in &inv {
        ids[orig] = Some(g.add_node(s.0[orig]).unwrap());
    }
    let ids: Vec<NodeId> = ids.into_iter().map(Option::unwrap).collect();
    for &k in edge_order {
        let (a, b, label, directed) = s.1[k];
        let mut e = if directed {
            Edge::directed()
        } else {
            Edge::undirected()
        };
        if let Some(l) = label {
            e = e.labeled(l);
        }
        g.add_edge(ids[a], ids[b], e).unwrap();
    }
    (g, ids)
}

proptest! {
    #[test]
    fn relabeling_invariance(
        s in spec(),
        seed in any::<u64>(),
        rounds in prop_oneof![Just(Rounds::ToStability), (0u8..5).prop_map(Rounds::Fixed)],
    ) {
        let n = s.0.len();
        let ident: Vec<usize> = (0..n).collect();
        let edges: Vec<usize> = (0..s.1.len()).collect();
        // Deterministic permutation and edge order from the seed.
        let mut perm = ident.clone();
        let mut x = seed | 1;
        for i in (1..n).rev() {
            x ^= x << 13; x ^= x >> 7; x ^= x << 17;
            perm.swap(i, usize::try_from(x % (i as u64 + 1)).unwrap());
        }
        let mut order = edges.clone();
        for i in (1..order.len()).rev() {
            x ^= x << 13; x ^= x >> 7; x ^= x << 17;
            order.swap(i, usize::try_from(x % (i as u64 + 1)).unwrap());
        }
        let (g1, ids1) = build(&s, &ident, &edges);
        let (g2, ids2) = build(&s, &perm, &order);
        let (r1, r2) = (wl_refine(&g1, rounds), wl_refine(&g2, rounds));
        prop_assert_eq!(r1.fingerprint(), r2.fingerprint());
        prop_assert_eq!(r1.rounds(), r2.rounds());
        // Per-node colours are equivariant: the same original node gets the same colour.
        for i in 0..n {
            prop_assert_eq!(r1.colour(ids1[i]), r2.colour(ids2[i]));
        }
        #[cfg(feature = "spd-wl")]
        prop_assert_eq!(
            pua_graph::spd_wl_refine(&g1, rounds).fingerprint(),
            pua_graph::spd_wl_refine(&g2, rounds).fingerprint()
        );
    }

    #[test]
    fn refinement_only_splits_classes(s in spec(), h in 0u8..5) {
        let (g, _) = build(&s, &(0..s.0.len()).collect::<Vec<_>>(), &(0..s.1.len()).collect::<Vec<_>>());
        let a = wl_refine(&g, Rounds::Fixed(h));
        let b = wl_refine(&g, Rounds::Fixed(h + 1));
        prop_assert!(b.classes() >= a.classes());
        let stable = wl_refine(&g, Rounds::ToStability);
        prop_assert!(stable.rounds() as usize <= g.node_count());
    }
}
