//! Property tests for spec §5.1 at the core level: determinism, candidate permutation
//! invariance, option relabeling equivariance away from ties, option-0 tie-break, abstain rules.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)] // tests may unwrap (AGENTS.md rule 4)

use proptest::prelude::*;
use pua_core::{
    Answer, CandidateId, CandidateSet, Confidence, OptionIndex, Profile, Question, Scores, decide,
};

fn profile() -> impl Strategy<Value = Profile> {
    prop_oneof![
        Just(Profile::Fast),
        Just(Profile::Standard),
        Just(Profile::Deep)
    ]
}

fn conf() -> impl Strategy<Value = Confidence> {
    // Bias toward collisions so ties are exercised.
    prop_oneof![
        (0i16..=1000).prop_map(|v| Confidence::new(v).unwrap()),
        (0i16..=10).prop_map(|v| Confidence::new(v * 100).unwrap())
    ]
}

fn question(n: usize) -> Question {
    let labels: Vec<String> = (0..n).map(|i| format!("opt{i}")).collect();
    Question::choice("q", &labels).unwrap()
}

fn answer_for(q: &Question, vals: &[Confidence], p: Profile) -> Answer {
    let mut s = Scores::new(q);
    for (i, c) in vals.iter().enumerate() {
        s.set(OptionIndex::new(u16::try_from(i).unwrap()), *c)
            .unwrap();
    }
    decide(&s, p)
}

proptest! {
    #[test]
    fn decide_is_deterministic(vals in prop::collection::vec(conf(), 2..12), p in profile()) {
        let q = question(vals.len());
        let a = answer_for(&q, &vals, p);
        prop_assert_eq!(&a, &answer_for(&q, &vals, p));
        // Also across threads.
        let (q2, v2) = (q.clone(), vals.clone());
        let b = std::thread::spawn(move || answer_for(&q2, &v2, p)).join().unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn option_relabeling_is_equivariant_away_from_ties(
        vals in prop::collection::vec(conf(), 2..10),
        perm_seed in prop::collection::vec(any::<u32>(), 10),
        p in profile(),
    ) {
        let n = vals.len();
        // A permutation sigma: new position sigma[i] holds old option i.
        let mut sigma: Vec<usize> = (0..n).collect();
        sigma.sort_by_key(|i| (perm_seed[*i], *i));
        let mut permuted = vec![Confidence::ZERO; n];
        for (old, new) in sigma.iter().enumerate() {
            permuted[*new] = vals[old];
        }
        let q = question(n);
        let a = answer_for(&q, &vals, p);
        let b = answer_for(&q, &permuted, p);
        let mut sorted = vals.clone();
        sorted.sort_unstable_by(|x, y| y.cmp(x));
        let margin_nonzero = sorted[0] != sorted[1];
        if margin_nonzero {
            prop_assert_eq!(a.is_abstain(), b.is_abstain());
            if let (Some(ca), Some(cb)) = (a.chosen(), b.chosen()) {
                prop_assert_eq!(sigma[usize::from(ca.get())], usize::from(cb.get()));
            }
        } else {
            // Exact top-2 tie: every profile's min margin is positive, so both abstain.
            prop_assert!(a.is_abstain() && b.is_abstain());
        }
    }

    #[test]
    fn abstain_iff_below_threshold_or_margin(vals in prop::collection::vec(conf(), 2..10), p in profile()) {
        let q = question(vals.len());
        let a = answer_for(&q, &vals, p);
        let mut sorted = vals.clone();
        sorted.sort_unstable_by(|x, y| y.cmp(x));
        let t = p.thresholds();
        let should_abstain = sorted[0] < t.min_confidence
            || sorted[0].saturating_sub(sorted[1]) < t.min_margin;
        prop_assert_eq!(a.is_abstain(), should_abstain);
    }

    #[test]
    fn ties_rank_the_lower_index_first(v in conf(), n in 2usize..8) {
        let q = question(n);
        let vals = vec![v; n];
        let Answer::Abstain { ranked, .. } = answer_for(&q, &vals, Profile::Deep) else {
            return Err(TestCaseError::fail("an all-tie must abstain"));
        };
        let idx: Vec<u16> = ranked.entries().iter().map(|e| e.0.get()).collect();
        prop_assert_eq!(idx, (0..u16::try_from(n).unwrap()).collect::<Vec<_>>());
    }

    #[test]
    fn candidate_permutation_is_invariant(
        scores in prop::collection::vec(conf(), 2..16),
        perm_seed in prop::collection::vec(any::<u32>(), 16),
        p in profile(),
    ) {
        let items: Vec<(CandidateId, Confidence)> = scores
            .iter()
            .enumerate()
            .map(|(i, c)| (CandidateId::new(&format!("run_{i:02}")).unwrap(), *c))
            .collect();
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|i| (perm_seed[*i], *i));
        let shuffled: Vec<_> = order.iter().map(|i| items[*i].clone()).collect();
        let pick = |items: &[(CandidateId, Confidence)]| {
            let set = CandidateSet::new(items.iter().map(|e| e.0.clone())).unwrap();
            let q = set.question("run").unwrap();
            let mut sc = Scores::new(&q);
            for (id, c) in items {
                sc.set(set.index_of(id.as_str()).unwrap(), *c).unwrap();
            }
            let a = decide(&sc, p);
            (set.chosen(&a).cloned(), a)
        };
        prop_assert_eq!(pick(&items), pick(&shuffled));
    }

    #[test]
    fn picked_candidate_clears_both_thresholds(scores in prop::collection::vec(conf(), 2..8), p in profile()) {
        let set = CandidateSet::new((0..scores.len()).map(|i| CandidateId::new(&i.to_string()).unwrap())).unwrap();
        let q = set.question("c").unwrap();
        let mut sc = Scores::new(&q);
        for (i, c) in scores.iter().enumerate() {
            sc.set(set.index_of(&i.to_string()).unwrap(), *c).unwrap();
        }
        let a = decide(&sc, p);
        if let Some(id) = set.chosen(&a) {
            let mine = scores[id.as_str().parse::<usize>().unwrap()];
            let mut rest: Vec<Confidence> = scores.clone();
            rest.sort_unstable_by(|x, y| y.cmp(x));
            let t = p.thresholds();
            prop_assert_eq!(mine, rest[0]);
            prop_assert!(mine >= t.min_confidence && mine.saturating_sub(rest[1]) >= t.min_margin);
        } else {
            prop_assert!(a.is_abstain());
        }
    }
}

#[test]
fn exact_boundaries_and_conversions() {
    use pua_core::{DataVersion, Decision, Millis, Options, Trail};
    let c = |v| Confidence::new(v).unwrap();
    // saturating_add must add, not clamp something else.
    assert_eq!(c(100).saturating_add(c(200)), c(300));
    assert_eq!(c(500).saturating_sub(c(200)), c(300));
    assert_eq!(i16::from(c(321)), 321);
    assert_eq!(i16::from(Millis::new(-321).unwrap()), -321);
    assert_eq!(Confidence::try_from(5i16), Ok(c(5)));
    assert_eq!(Millis::try_from(-5i16).unwrap().get(), -5);
    assert_eq!(Millis::new(-12).unwrap().to_string(), "-12");
    assert_eq!(c(12).to_string(), "12");
    // Candidate thresholds are inclusive minimums: candidates go through decide.
    let id = |s: &str| CandidateId::new(s).unwrap();
    let set = CandidateSet::new([id("b"), id("a")]).unwrap();
    let cq = set.question("c").unwrap();
    let mut sc = Scores::new(&cq);
    sc.set(OptionIndex::new(0), c(750)).unwrap();
    sc.set(OptionIndex::new(1), c(600)).unwrap();
    assert_eq!(set.chosen(&decide(&sc, Profile::Standard)), Some(&id("a")));
    // Accessors return what was stored.
    let q = question(2);
    let o: &Options = q.options().unwrap();
    assert_eq!(
        o.labels()
            .iter()
            .map(pua_core::Label::as_str)
            .collect::<Vec<_>>(),
        ["opt0", "opt1"]
    );
    assert_eq!(String::from(o.labels()[1].clone()), "opt1");
    assert_eq!(String::from(id("x")), "x");
    assert_eq!(Vec::from(o.clone()).len(), 2);
    let mut s = Scores::new(&q);
    s.set(OptionIndex::new(1), c(900)).unwrap();
    let mut t = Trail::new();
    t.push(pua_core::TrailRecord::new(pua_core::StageKind::Decide, "x"));
    let d = Decision::new(
        decide(&s, Profile::Deep),
        Profile::Deep,
        DataVersion::builder("t").finish(),
        t.clone(),
    );
    assert_eq!(d.profile(), Profile::Deep);
    assert_eq!(d.trail(), &t);
    let (a, p, _, tr) = d.clone().into_parts();
    assert_eq!((&a, p, &tr), (d.answer(), Profile::Deep, &t));
    if let Answer::Choice { ranked, .. } = a {
        assert_eq!(Vec::from(ranked).len(), 2);
    }
    // Ranked rejects an equal-confidence pair in descending index order (tie order is part of
    // the contract) and accepts the canonical order.
    let ranked = |v: Vec<(u16, i16)>| {
        pua_core::Ranked::try_from(
            v.into_iter()
                .map(|(i, x)| (OptionIndex::new(i), c(x)))
                .collect::<Vec<_>>(),
        )
    };
    assert!(ranked(vec![(1, 500), (0, 500)]).is_err());
    assert!(ranked(vec![(0, 500), (1, 500)]).is_ok());
    assert!(ranked(vec![(1, 600), (0, 500)]).is_ok());
    // Span relations: containment needs both ends inside; touching spans do not overlap.
    let sp = |a, b| pua_core::Span::new(a, b).unwrap();
    assert!(sp(0, 5).contains(sp(1, 5)));
    assert!(!sp(0, 5).contains(sp(3, 8)));
    assert!(!sp(2, 5).contains(sp(0, 4)));
    assert!(sp(0, 3).overlaps(sp(2, 5)));
    assert!(!sp(0, 3).overlaps(sp(3, 5)));
    assert!(!sp(3, 5).overlaps(sp(0, 3)));
    // 65535 options are allowed (upper bound inclusive).
    let many: Vec<String> = (0..usize::from(u16::MAX)).map(|i| i.to_string()).collect();
    assert_eq!(Options::new(&many).unwrap().len(), u16::MAX);
}
