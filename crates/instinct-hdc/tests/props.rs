//! HDC invariances (spec §6.6; plan T7).
//!
//! Invariant: bundle input order; similarity under a joint XOR-bind of both arguments by one
//! key; similarity under a joint rotation of both; codebook insertion order. `permute` is
//! explicitly not a symmetry (unit test `permute_is_not_a_symmetry`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use instinct_core::Millis;
use instinct_hdc::{Accumulator, Codebook, D1024, D2048, D4096, Dim, Encoder, Hv, bundle};
use proptest::prelude::*;

fn hv<D: Dim>(s: &str) -> Hv<D> {
    Encoder::new("props", 7).encode(s)
}

fn sym() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,8}"
}

fn check_algebra<D: Dim>(a: &str, b: &str, k: &str, s1: u32, s2: u32) -> Result<(), TestCaseError> {
    let (a, b, k) = (hv::<D>(a), hv::<D>(b), hv::<D>(k));
    prop_assert_eq!(a.bind(&k).similarity(&b.bind(&k)), a.similarity(&b));
    prop_assert_eq!(a.permute(s1).similarity(&b.permute(s1)), a.similarity(&b));
    prop_assert_eq!(a.similarity(&b), b.similarity(&a));
    prop_assert_eq!(
        i32::from(a.similarity(&b.negate()).get()),
        -i32::from(a.similarity(&b).get())
    );
    prop_assert_eq!(a.bind(&b).bind(&b), a);
    prop_assert_eq!(a.bind(&b), b.bind(&a));
    prop_assert_eq!(a.bind(&b).bind(&k), a.bind(&b.bind(&k)));
    prop_assert_eq!(
        a.permute(s1).permute(s2),
        a.permute((s1 % D::BITS + s2 % D::BITS) % D::BITS)
    );
    prop_assert_eq!(a.permute(s1 % D::BITS).permute(D::BITS - s1 % D::BITS), a);
    // Binding distributes over permutation.
    prop_assert_eq!(a.bind(&b).permute(s1), a.permute(s1).bind(&b.permute(s1)));
    Ok(())
}

proptest! {
    #[test]
    fn algebra_d1024(a in sym(), b in sym(), k in sym(), s1 in 0u32..1024, s2 in 0u32..1024) {
        check_algebra::<D1024>(&a, &b, &k, s1, s2)?;
    }

    #[test]
    fn algebra_d2048(a in sym(), b in sym(), k in sym(), s1 in 0u32..2048, s2 in 0u32..2048) {
        check_algebra::<D2048>(&a, &b, &k, s1, s2)?;
    }

    #[test]
    fn algebra_d4096(a in sym(), b in sym(), k in sym(), s1 in 0u32..4096, s2 in 0u32..4096) {
        check_algebra::<D4096>(&a, &b, &k, s1, s2)?;
    }

    #[test]
    fn bundle_is_order_invariant(
        items in proptest::collection::vec(sym(), 0..9).prop_shuffle(),
        weights in proptest::collection::vec(-5i32..=5, 9),
    ) {
        let vs: Vec<Hv<D1024>> = items.iter().map(|s| hv(s)).collect();
        let mut rev = vs.clone();
        rev.reverse();
        prop_assert_eq!(bundle(&vs), bundle(&rev));
        let mut sorted = vs.clone();
        sorted.sort();
        prop_assert_eq!(bundle(&vs), bundle(&sorted));
        // Weighted accumulators too.
        let mut x = Accumulator::<D1024>::new();
        let mut y = Accumulator::<D1024>::new();
        for (v, w) in vs.iter().zip(&weights) {
            x.add(v, *w);
        }
        for (v, w) in vs.iter().zip(&weights).rev() {
            y.add(v, *w);
        }
        prop_assert_eq!(x, y);
    }

    #[test]
    fn codebook_is_insertion_order_invariant(
        names in proptest::collection::btree_set(sym(), 1..12),
        q in sym(),
        seed in any::<u64>(),
    ) {
        let entries: Vec<(String, Hv<D1024>)> = names.iter().map(|n| (n.clone(), hv(n))).collect();
        let mut shuffled = entries.clone();
        // Deterministic shuffle from the seed.
        let len = shuffled.len();
        for i in (1..len).rev() {
            let j = usize::try_from(seed.rotate_left(u32::try_from(i).unwrap()) % (i as u64 + 1)).unwrap();
            shuffled.swap(i, j);
        }
        let a = Codebook::new(entries).unwrap();
        let b = Codebook::new(shuffled).unwrap();
        prop_assert_eq!(&a, &b);
        let qv = hv::<D1024>(&q);
        prop_assert_eq!(a.nearest(&qv), b.nearest(&qv));
        let mut acc = Accumulator::new();
        acc.add(&qv, 1);
        prop_assert_eq!(a.decode(&acc, 3, Millis::MIN), b.decode(&acc, 3, Millis::MIN));
    }

    #[test]
    fn nearest_is_maximal_with_lowest_index_on_ties(
        names in proptest::collection::btree_set(sym(), 1..12),
        q in sym(),
    ) {
        let b = Codebook::new(names.iter().map(|n| (n.clone(), hv::<D1024>(n)))).unwrap();
        let qv = hv::<D1024>(&q);
        let n = b.nearest(&qv);
        let sims: Vec<Millis> = b.vectors().iter().map(|v| qv.similarity(v)).collect();
        let best = *sims.iter().max().unwrap();
        prop_assert_eq!(n.similarity, best);
        prop_assert_eq!(n.index, sims.iter().position(|s| *s == best).unwrap());
        prop_assert!(n.margin >= Millis::ZERO);
    }
}
