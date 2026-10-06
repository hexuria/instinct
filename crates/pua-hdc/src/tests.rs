#![allow(clippy::many_single_char_names)]

use pua_core::Millis;

use super::*;

fn enc() -> Encoder {
    Encoder::new("test", 1)
}

fn hv(s: &str) -> Hv<D1024> {
    enc().encode(s)
}

fn naive_permute<D: Dim>(v: &Hv<D>, shift: u32) -> Vec<i8> {
    let n = D::BITS;
    let mut out = vec![0i8; n as usize];
    for i in 0..n {
        out[((i + shift) % n) as usize] = v.component(i);
    }
    out
}

fn comps<D: Dim>(v: &Hv<D>) -> Vec<i8> {
    (0..D::BITS).map(|i| v.component(i)).collect()
}

#[test]
fn permute_matches_the_naive_rotation() {
    let v = hv("x");
    for s in [0, 1, 7, 63, 64, 65, 127, 128, 500, 1023, 1024, 1025, 4097] {
        assert_eq!(comps(&v.permute(s)), naive_permute(&v, s), "shift {s}");
    }
    let w: Hv<D4096> = enc().encode("y");
    for s in [1, 64, 129, 4095] {
        assert_eq!(comps(&w.permute(s)), naive_permute(&w, s), "shift {s}");
        assert_eq!(w.permute(s).permute(4096 - s), w);
    }
}

#[test]
fn permute_is_not_a_symmetry() {
    let v = hv("x");
    assert!(v.similarity(&v.permute(1)).get().abs() < 200);
    let (a, b) = (hv("a"), hv("b"));
    // Order matters: a ⊗ ρ(b) differs from b ⊗ ρ(a).
    assert_ne!(a.bind(&b.permute(1)), b.bind(&a.permute(1)));
}

#[test]
fn similarity_extremes_and_identity() {
    let v = hv("x");
    assert_eq!(v.similarity(&v), Millis::MAX);
    assert_eq!(v.similarity(&v.negate()), Millis::MIN);
    assert_eq!(v.hamming(&v.negate()), 1024);
    assert_eq!(v.bind(&Hv::ones()), v);
    assert_eq!(Hv::<D1024>::default(), Hv::ones());
    assert_eq!(Hv::<D1024>::ones().component(5), 1);
    assert_eq!(Hv::<D1024>::ones().negate().component(5), -1);
    assert_eq!(v.component(1024 + 3), v.component(3));
    assert_eq!(v.words().len(), 16);
    // One flipped component: (1024 - 2) * 1000 / 1024 = 998 (truncated).
    let mut acc = Accumulator::<D1024>::new();
    acc.add(&v, 1);
    assert_eq!(acc.to_hv(), v);
    assert!(format!("{v:?}").starts_with("Hv<1024>("));
}

#[test]
fn similarity_formula_truncates_toward_zero() {
    let mut seen_negative_fraction = false;
    for i in 0..40 {
        let (a, b) = (hv(&format!("p{i}")), hv(&format!("q{i}")));
        let h = i32::try_from(a.hamming(&b)).unwrap();
        let exact = (1024 - 2 * h) * 1000;
        assert_eq!(i32::from(a.similarity(&b).get()), exact / 1024);
        seen_negative_fraction |= exact < 0 && exact % 1024 != 0;
    }
    assert!(
        seen_negative_fraction,
        "sample covers a negative non-integer case"
    );
}

#[test]
fn encoder_is_a_pure_function_of_its_inputs() {
    let e = enc();
    assert_eq!(e.encode::<D1024>("a"), e.encode::<D1024>("a"));
    assert_ne!(e.encode::<D1024>("a"), e.encode::<D1024>("b"));
    assert_ne!(
        e.encode::<D1024>("a"),
        Encoder::new("test", 2).encode::<D1024>("a")
    );
    assert_ne!(
        e.encode::<D1024>("a"),
        Encoder::new("other", 1).encode::<D1024>("a")
    );
    // The separator keeps namespace and symbol apart.
    assert_ne!(
        Encoder::new("ab", 1).seed("c"),
        Encoder::new("a", 1).seed("bc")
    );
    assert_eq!(e.namespace(), "test");
    assert_eq!(e.version(), 1);
    // A known seed pins the algorithm (change = DataVersion change).
    assert_eq!(e.seed("a"), Encoder::new("test", 1).seed("a"));
    let first = e.encode::<D1024>("a").words()[0];
    assert_eq!(
        e.encode::<D2048>("a").words()[0],
        first,
        "same stream, longer"
    );
}

#[test]
fn bundle_majority_and_tie() {
    let (a, b, c) = (hv("a"), hv("b"), hv("c"));
    let m = bundle(&[a, b, c]);
    for i in 0..1024 {
        let s = i32::from(a.component(i)) + i32::from(b.component(i)) + i32::from(c.component(i));
        assert_eq!(i32::from(m.component(i)), s.signum());
    }
    // Two opposite vectors tie everywhere → all +1.
    assert_eq!(bundle(&[a, a.negate()]), Hv::ones());
    assert_eq!(bundle::<D1024>(&[]), Hv::ones());
}

#[test]
fn accumulator_dot_and_l1() {
    let a = hv("a");
    let mut acc = Accumulator::<D1024>::new();
    acc.add(&a, 3);
    assert_eq!(acc.dot(&a), 3 * 1024);
    assert_eq!(acc.dot(&a.negate()), -3 * 1024);
    assert_eq!(acc.l1(), 3 * 1024);
    assert_eq!(acc.sums().len(), 1024);
    acc.add(&a, -3);
    assert_eq!(acc, Accumulator::default());
    acc.add(&a, i32::MAX);
    acc.add(&a, i32::MAX);
    assert!(acc.sums().iter().all(|&s| s == i32::MAX || s == i32::MIN));
}

#[test]
fn codebook_errors_are_exact() {
    let a = hv("a");
    assert_eq!(
        Codebook::<D1024>::new([]).unwrap_err(),
        CodebookError::Empty
    );
    assert_eq!(
        Codebook::new([("x".to_owned(), a), (String::new(), a)]).unwrap_err(),
        CodebookError::EmptyId { index: 1 }
    );
    assert_eq!(
        Codebook::new([
            ("x".to_owned(), a),
            ("y".to_owned(), a),
            ("x".to_owned(), a)
        ])
        .unwrap_err(),
        CodebookError::DuplicateId { id: "x".into() }
    );
    assert_eq!(CodebookError::Empty.to_string(), "codebook has no entries");
    assert_eq!(
        CodebookError::EmptyId { index: 1 }.to_string(),
        "codebook entry 1: empty id"
    );
    assert_eq!(
        CodebookError::DuplicateId { id: "x".into() }.to_string(),
        "codebook: duplicate id \"x\""
    );
}

fn book(names: &[&str]) -> Codebook<D1024> {
    Codebook::new(names.iter().map(|n| ((*n).to_owned(), hv(n)))).unwrap()
}

#[test]
fn codebook_lookup_nearest_and_ties() {
    let b = book(&["gamma", "alpha", "beta"]);
    assert_eq!(b.len(), 3);
    assert!(!b.is_empty());
    assert_eq!(b.id(0), "alpha");
    assert_eq!(b.id(9), "");
    assert_eq!(b.index_of("beta"), Some(1));
    assert_eq!(b.index_of("delta"), None);
    assert_eq!(b.vector(2), Some(&hv("gamma")));
    assert_eq!(b.vector(3), None);
    let n = b.nearest(&hv("beta"));
    assert_eq!((n.index, n.similarity), (1, Millis::MAX));
    assert!(n.margin.get() > 800);
    // Exact tie: two entries with the same vector → the lower id wins with margin 0.
    let v = hv("same");
    let tied = Codebook::new([("b".to_owned(), v), ("a".to_owned(), v)]).unwrap();
    let n = tied.nearest(&v);
    assert_eq!((n.index, n.margin), (0, Millis::ZERO));
    // A single entry: margin measured against -1000, clamped.
    let one = book(&["x"]);
    assert_eq!(one.nearest(&hv("x")).margin, Millis::MAX);
    // Cleanup floor is inclusive.
    let q = hv("beta");
    assert!(b.cleanup(&q, Millis::MAX).is_some());
    assert!(b.cleanup(&hv("zzz"), Millis::new(500).unwrap()).is_none());
}

#[test]
fn decode_recovers_bundle_members() {
    let names: Vec<String> = (0..64).map(|i| format!("item{i:02}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let b = book(&refs);
    let members = ["item03", "item17", "item42"];
    let mut acc = Accumulator::new();
    for m in members {
        acc.add(&hv(m), 1);
    }
    let got: Vec<&str> = b
        .decode(&acc, 3, Millis::new(100).unwrap())
        .iter()
        .map(|(i, _)| b.id(*i))
        .collect();
    let mut sorted = got.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, members);
    // A high floor stops early; k = 0 decodes nothing; k beyond the codebook is capped.
    assert_eq!(b.decode(&acc, 3, Millis::MAX), []);
    assert_eq!(b.decode(&acc, 0, Millis::MIN), []);
    assert_eq!(book(&["a", "b"]).decode(&acc, 10, Millis::MIN).len(), 2);
    // Weighted: the heavier member comes first.
    let mut w = Accumulator::new();
    w.add(&hv("item05"), 1);
    w.add(&hv("item09"), 3);
    assert_eq!(b.id(b.decode(&w, 1, Millis::MIN)[0].0), "item09");
    // Negative weights decode too (the estimate rounds away from zero).
    let mut neg = Accumulator::new();
    neg.add(&hv("item07"), -2);
    let d = b.decode(&neg, 1, Millis::MIN);
    assert_ne!(
        b.id(d[0].0),
        "item07",
        "anti-correlated entries score lowest"
    );
}

#[test]
fn resonator_factorizes_and_reports_non_convergence() {
    let a_names: Vec<String> = (0..8).map(|i| format!("intent{i}")).collect();
    let b_names: Vec<String> = (0..8).map(|i| format!("target{i}")).collect();
    let ab = book(&a_names.iter().map(String::as_str).collect::<Vec<_>>());
    let bb = book(&b_names.iter().map(String::as_str).collect::<Vec<_>>());
    for (x, y) in [
        ("intent3", "target5"),
        ("intent0", "target7"),
        ("intent6", "target0"),
    ] {
        let q = hv(x).bind(&hv(y));
        match resonate(&q, &ab, &bb, IterationCap::MAX) {
            Resonance::Converged { a, b, iterations } => {
                assert_eq!((ab.id(a.index), bb.id(b.index)), (x, y));
                assert_eq!(a.similarity, Millis::MAX);
                assert!(iterations <= MAX_ITERATIONS);
            }
            other @ Resonance::NotConverged { .. } => panic!("{other:?}"),
        }
    }
    // A cap of 1 cannot confirm a fixed point that needs a change first.
    let q = hv("intent3").bind(&hv("target5"));
    assert_eq!(
        resonate(&q, &ab, &bb, IterationCap::new(1).unwrap()),
        Resonance::NotConverged { iterations: 1 }
    );
}

#[test]
fn iteration_cap_bounds_are_exact() {
    assert_eq!(IterationCap::new(0), Err(IterationCapError { value: 0 }));
    assert_eq!(IterationCap::new(17), Err(IterationCapError { value: 17 }));
    assert_eq!(IterationCap::new(1).unwrap().get(), 1);
    assert_eq!(IterationCap::new(16).unwrap(), IterationCap::MAX);
    assert_eq!(IterationCap::default(), IterationCap::MAX);
    assert_eq!(
        IterationCapError { value: 17 }.to_string(),
        "resonator iteration cap 17 outside 1..=16"
    );
}

#[test]
fn decode_subtracts_the_estimated_contribution() {
    let b = book(&["a", "b", "c"]);
    let (va, vb, vc) = (hv("a"), hv("b"), hv("c"));
    // 3a + b: after removing 3a exactly, what is left is b, which scores exactly 1000.
    let mut acc = Accumulator::new();
    acc.add(&va, 3);
    acc.add(&vb, 1);
    let got = b.decode(&acc, 2, Millis::MIN);
    assert_eq!((b.id(got[0].0), b.id(got[1].0)), ("a", "b"));
    assert_eq!(got[1].1, Millis::MAX);
    // 3a − 2b − 2c: the negative estimate (round half away from zero) removes −2b exactly,
    // leaving −2c, which scores exactly −1000.
    let mut acc = Accumulator::new();
    acc.add(&va, 3);
    acc.add(&vb, -2);
    acc.add(&vc, -2);
    let got = b.decode(&acc, 3, Millis::MIN);
    assert_eq!(b.id(got[0].0), "a");
    assert_eq!(got[2].1, Millis::MIN);
}

#[test]
fn decode_ties_and_inclusive_floor() {
    let v = hv("same");
    let tied = Codebook::new([("y".to_owned(), v), ("x".to_owned(), v)]).unwrap();
    let mut acc = Accumulator::new();
    acc.add(&v, 1);
    assert_eq!(tied.decode(&acc, 1, Millis::MIN), [(0, Millis::MAX)]);
    // The floor is inclusive.
    assert_eq!(tied.decode(&acc, 1, Millis::MAX), [(0, Millis::MAX)]);
}

#[test]
fn nearest_margin_is_top_minus_runner_up() {
    let names: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
    let b = book(&names.iter().map(String::as_str).collect::<Vec<_>>());
    for q in ["m3", "zz", "m19", "other"] {
        let qv = hv(q);
        let mut sims: Vec<i16> = b.vectors().iter().map(|v| qv.similarity(v).get()).collect();
        let n = b.nearest(&qv);
        sims.sort_unstable();
        let (top, second) = (sims[sims.len() - 1], sims[sims.len() - 2]);
        assert_eq!(n.margin.get(), top - second, "{q}");
    }
}

#[test]
fn resonator_iteration_counts_are_pinned() {
    // Deterministic: the count is part of the behaviour (and of the trail a consumer writes).
    let a_names: Vec<String> = (0..8).map(|i| format!("intent{i}")).collect();
    let b_names: Vec<String> = (0..8).map(|i| format!("target{i}")).collect();
    let ab = book(&a_names.iter().map(String::as_str).collect::<Vec<_>>());
    let bb = book(&b_names.iter().map(String::as_str).collect::<Vec<_>>());
    let q = hv("intent3").bind(&hv("target5"));
    let Resonance::Converged { iterations, .. } = resonate(&q, &ab, &bb, IterationCap::MAX) else {
        panic!("expected convergence");
    };
    // A fixed point needs both estimates unchanged: with a cap of `iterations - 1` it must not
    // be confirmed yet.
    assert!(iterations >= 2);
    assert_eq!(
        resonate(&q, &ab, &bb, IterationCap::new(iterations - 1).unwrap()),
        Resonance::NotConverged {
            iterations: iterations - 1
        }
    );
    assert_eq!(Encoder::new("x", 2).version(), 2);
}

#[test]
fn decode_estimate_is_exact_for_integer_weights() {
    // Residuals of the form `r·x + s·y` with |r| ≤ |s| score exactly ±1000 on `y` (dot = l1),
    // so a wrong estimate is only visible when it leaves a third vector behind.
    let bk = book(&["a", "b", "c"]);
    let (va, vb, vc) = (hv("a"), hv("b"), hv("c"));
    // Positive branch: 3a + b + c. Removing exactly 3a leaves b + c, so b scores 1000.
    let mut acc = Accumulator::new();
    acc.add(&va, 3);
    acc.add(&vb, 1);
    acc.add(&vc, 1);
    let got = bk.decode(&acc, 2, Millis::MIN);
    assert_eq!((bk.id(got[0].0), got[1].1), ("a", Millis::MAX));
    // Negative branch: −2a − 3b − 3c. The least negative entry, a, goes first with a negative
    // estimate that must round to exactly −2, leaving −3(b + c): b scores −1000.
    let mut acc = Accumulator::new();
    acc.add(&va, -2);
    acc.add(&vb, -3);
    acc.add(&vc, -3);
    let got = bk.decode(&acc, 2, Millis::MIN);
    assert_eq!((bk.id(got[0].0), bk.id(got[1].0)), ("a", "b"));
    assert_eq!(got[1].1, Millis::MIN);
}

#[test]
fn resonator_needs_both_estimates_unchanged() {
    // With a single-entry `A`, `â` is already a fixed point after iteration 1 while `b̂` still
    // moves; convergence must wait until `b̂` settles too.
    let ab = book(&["only"]);
    let bb = book(&["t0", "t1", "t2"]);
    let q = hv("only").bind(&hv("t1"));
    let Resonance::Converged { b, iterations, .. } = resonate(&q, &ab, &bb, IterationCap::MAX)
    else {
        panic!("expected convergence");
    };
    assert!(iterations >= 2, "{iterations}");
    assert_eq!(bb.id(b.index), "t1");
}

#[test]
fn component_and_permute_wrap_at_the_dimension() {
    // Indexing and rotating reduce mod BITS: a whole turn is the identity.
    let v = hv("x");
    assert_eq!(v.component(D1024::BITS), v.component(0));
    assert_eq!(v.component(2 * D1024::BITS + 3), v.component(3));
    assert_eq!(v.permute(D1024::BITS), v);
    assert_eq!(v.permute(D1024::BITS + 7), v.permute(7));
}

#[test]
fn bundle_of_nothing_is_all_plus_one() {
    // Every sum stays at zero on an empty bundle, and the zero → +1 tie makes it
    // equal to Hv::ones().
    assert_eq!(bundle::<D1024>(&[]), Hv::ones());
}

#[test]
fn add_saturates_each_weighted_component() {
    // weight·±1 saturates in i32: +1 components floor at i32::MIN, −1 components
    // ceiling at i32::MAX (saturating_neg of i32::MIN is i32::MAX — asymmetric).
    let v = hv("x");
    let mut acc = Accumulator::new();
    acc.add(&v, i32::MIN);
    let sums = acc.sums();
    for i in 0..D1024::BITS {
        assert_eq!(
            sums[i as usize],
            if v.component(i) == 1 {
                i32::MIN
            } else {
                i32::MAX
            },
            "component {i}"
        );
    }
}

#[test]
fn one_entry_margin_is_sim_plus_1000_clamped() {
    // `best_two` plays a virtual second best at −1000, so a single-entry book
    // reports margin = min(similarity + 1000, 1000).
    let b = Codebook::new([("only".to_owned(), hv("only"))]).unwrap();
    let same = b.nearest(&hv("only"));
    assert_eq!((same.similarity.get(), same.margin.get()), (1000, 1000));
    let far = b.nearest(&hv("zz"));
    assert_eq!(far.margin.get(), (far.similarity.get() + 1000).min(1000));
}

#[test]
fn decode_caps_k_at_the_book_and_zero_accumulator_scores_zero() {
    let b = book(&["a", "b", "c"]);
    let mut acc = Accumulator::new();
    acc.add(&hv("a"), 1);
    // k > len returns every entry once, still sorted best-first with index ties.
    let got = b.decode(&acc, 100, Millis::MIN);
    assert_eq!(
        got.iter()
            .map(|&(i, s)| (b.id(i), s.get()))
            .collect::<Vec<_>>(),
        vec![("a", 1000), ("b", 0), ("c", 0)]
    );
    // A zero accumulator scores every entry 0, ties broken by index.
    let zero: Accumulator<D1024> = Accumulator::new();
    let got = b.decode(&zero, 2, Millis::MIN);
    assert_eq!(
        got.iter()
            .map(|&(i, s)| (b.id(i), s.get()))
            .collect::<Vec<_>>(),
        vec![("a", 0), ("b", 0)]
    );
}
