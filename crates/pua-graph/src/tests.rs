#![allow(clippy::many_single_char_names)]

use super::*;

/// Undirected unlabeled graph from an edge list.
fn graph(n: u32, edges: &[(u32, u32)]) -> LabeledGraph {
    let mut g = LabeledGraph::new();
    let v: Vec<NodeId> = (0..n).map(|_| g.add_node(0).unwrap()).collect();
    for &(a, b) in edges {
        g.add_edge(v[a as usize], v[b as usize], Edge::undirected())
            .unwrap();
    }
    g
}

fn cycle(n: u32) -> Vec<(u32, u32)> {
    (0..n).map(|i| (i, (i + 1) % n)).collect()
}

fn fp(g: &LabeledGraph) -> [u8; 32] {
    wl_refine(g, Rounds::default()).fingerprint()
}

/// Two triangles joined by the bridge 2–3, and C₆ plus the chord 0–3: 1-WL equivalent
/// (same degree sequence and neighbour-degree multisets), not isomorphic (one has a cut edge).
fn bridge_pair() -> (LabeledGraph, LabeledGraph) {
    let triangles = graph(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
    let mut c6 = cycle(6);
    c6.push((0, 3));
    (triangles, graph(6, &c6))
}

#[test]
fn errors_are_exact() {
    let mut g = graph(2, &[]);
    let mut other = graph(5, &[]);
    let far = other.add_node(1).unwrap();
    assert_eq!(
        g.add_edge(NodeId(0), far, Edge::undirected()),
        Err(GraphError::UnknownNode { index: 5 })
    );
    assert_eq!(
        g.add_edge(far, NodeId(0), Edge::directed()),
        Err(GraphError::UnknownNode { index: 5 })
    );
    assert_eq!(g.edge_count(), 0);
    assert_eq!(
        GraphError::UnknownNode { index: 5 }.to_string(),
        "unknown node 5"
    );
    assert_eq!(
        GraphError::TooManyNodes.to_string(),
        "graph is full (u32::MAX nodes)"
    );
}

#[test]
fn documented_collisions_and_separations() {
    // C6 vs 2×C3: the textbook 1-WL collision (GDL Ex. 3.233).
    let mut two = cycle(3);
    two.extend(cycle(3).iter().map(|(a, b)| (a + 3, b + 3)));
    assert_eq!(fp(&graph(6, &cycle(6))), fp(&graph(6, &two)));
    // The bridge pair collides under 1-WL...
    let (t, c) = bridge_pair();
    assert_eq!(fp(&t), fp(&c));
    assert_eq!(
        wl_refine(&t, Rounds::ToStability).fingerprint(),
        wl_refine(&c, Rounds::ToStability).fingerprint()
    );
    // ...while different structures separate.
    assert_ne!(fp(&graph(3, &[(0, 1), (1, 2)])), fp(&graph(3, &cycle(3))));
    assert_ne!(fp(&graph(6, &cycle(6))), fp(&graph(5, &cycle(5))));
}

#[cfg(feature = "spd-wl")]
#[test]
fn spd_wl_separates_what_1wl_cannot() {
    let (t, c) = bridge_pair();
    let spd = |g: &LabeledGraph| spd_wl_refine(g, Rounds::default()).fingerprint();
    assert_ne!(spd(&t), spd(&c));
    let mut two = cycle(3);
    two.extend(cycle(3).iter().map(|(a, b)| (a + 3, b + 3)));
    assert_ne!(spd(&graph(6, &cycle(6))), spd(&graph(6, &two)));
    // Still equivariant: a relabeled copy matches.
    let relabeled = graph(6, &[(5, 4), (4, 3), (3, 5), (2, 1), (1, 0), (0, 2), (3, 2)]);
    assert_eq!(spd(&t), spd(&relabeled));
    // Different tag from plain WL.
    assert_ne!(spd(&t), fp(&t));
}

#[test]
fn labels_direction_and_edge_labels_matter() {
    let labeled = |a: u64, b: u64, e: Edge| {
        let mut g = LabeledGraph::new();
        let (x, y) = (g.add_node(a).unwrap(), g.add_node(b).unwrap());
        g.add_edge(x, y, e).unwrap();
        g
    };
    let base = fp(&labeled(1, 2, Edge::undirected()));
    assert_ne!(base, fp(&labeled(1, 3, Edge::undirected())));
    assert_ne!(base, fp(&labeled(1, 2, Edge::directed())));
    assert_ne!(
        fp(&labeled(1, 2, Edge::directed())),
        fp(&labeled(2, 1, Edge::directed()))
    );
    assert_eq!(
        fp(&labeled(1, 2, Edge::directed())),
        fp(&labeled(1, 2, Edge::directed()))
    );
    assert_ne!(base, fp(&labeled(1, 2, Edge::undirected().labeled(7))));
    assert_ne!(
        fp(&labeled(1, 2, Edge::undirected().labeled(7))),
        fp(&labeled(1, 2, Edge::undirected().labeled(8)))
    );
    // A label of 0 is not the same as no label.
    assert_ne!(base, fp(&labeled(1, 2, Edge::undirected().labeled(0))));
}

#[test]
fn rounds_and_stability() {
    let path = graph(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
    let r0 = wl_refine(&path, Rounds::Fixed(0));
    assert_eq!((r0.rounds(), r0.classes()), (0, 1));
    // A fixed request runs exactly that many rounds, even past stability.
    assert_eq!(wl_refine(&path, Rounds::Fixed(7)).rounds(), 7);
    let stable = wl_refine(&path, Rounds::ToStability);
    // P5 refines to {ends}, {next}, {middle}: 3 classes, stable after 3 rounds (2 to refine,
    // 1 to confirm).
    assert_eq!(stable.classes(), 3);
    assert_eq!(stable.rounds(), 3);
    // Different requested rounds give different fingerprints even when colours are stable.
    assert_ne!(
        wl_refine(&path, Rounds::Fixed(3)).fingerprint(),
        wl_refine(&path, Rounds::Fixed(4)).fingerprint()
    );
    assert_ne!(
        wl_refine(&path, Rounds::Fixed(3)).fingerprint(),
        stable.fingerprint()
    );
    let empty = wl_refine(&LabeledGraph::new(), Rounds::ToStability);
    assert_eq!((empty.rounds(), empty.classes()), (0, 0));
    assert_eq!(Rounds::default(), Rounds::Fixed(DEFAULT_ROUNDS));
}

#[test]
fn per_node_colours_and_accessors() {
    let mut g = graph(3, &[(0, 1), (1, 2)]);
    let r = wl_refine(&g, Rounds::default());
    assert_eq!(r.colour(NodeId(0)), r.colour(NodeId(2)));
    assert_ne!(r.colour(NodeId(0)), r.colour(NodeId(1)));
    assert_eq!(r.colour(NodeId(9)), None);
    assert_eq!(r.colours().len(), 3);
    assert_eq!(r.fingerprint_hex().len(), 64);
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.edge_count(), 2);
    assert_eq!(g.label(NodeId(1)), Some(0));
    assert_eq!(g.label(NodeId(5)), None);
    assert_eq!(NodeId(2).index(), 2);
    // Self-loops count once for undirected edges, as out+in for directed ones.
    let a = g.add_node(5).unwrap();
    g.add_edge(a, a, Edge::undirected()).unwrap();
    let mut h = graph(3, &[(0, 1), (1, 2)]);
    let b = h.add_node(5).unwrap();
    h.add_edge(b, b, Edge::directed()).unwrap();
    assert_ne!(fp(&g), fp(&h));
}

#[test]
fn label_of_is_stable_and_length_prefixed() {
    let a = label_of(&[b"fs.read"]);
    assert_eq!(a, label_of(&[b"fs.read"]));
    assert_ne!(a, label_of(&[b"fs.write"]));
    assert_ne!(a, 0);
    assert_ne!(label_of(&[]), label_of(&[b""]));
    assert_ne!(label_of(&[b"ab", b"c"]), label_of(&[b"a", b"bc"]));
    assert_ne!(label_of(&[b"x"]), u64::MAX);
}
