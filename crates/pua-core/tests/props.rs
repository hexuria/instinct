//! Property tests for spec §5.1 at the core level: determinism, candidate permutation
//! invariance, option relabeling equivariance away from ties, option-0 tie-break, abstain rules.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names
)] // tests may unwrap (AGENTS.md rule 4)

use proptest::prelude::*;
use pua_core::{
    Answer, CandidateId, CandidatePick, Confidence, OptionIndex, Profile, Question, Scores, decide,
    rank_candidates,
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
        scores in prop::collection::vec(conf(), 0..16),
        perm_seed in prop::collection::vec(any::<u32>(), 16),
        p in profile(),
    ) {
        let items: Vec<(CandidateId, Confidence)> = scores
            .iter()
            .enumerate()
            .map(|(i, c)| (CandidateId::new(&format!("run_{i:02}")).unwrap(), *c))
            .collect();
        let mut shuffled = items.clone();
        let mut order: Vec<usize> = (0..shuffled.len()).collect();
        order.sort_by_key(|i| (perm_seed[*i], *i));
        shuffled = order.iter().map(|i| shuffled[*i].clone()).collect();
        let a = rank_candidates(items).unwrap();
        let b = rank_candidates(shuffled).unwrap();
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(a.pick(p), b.pick(p));
    }

    #[test]
    fn picked_candidate_clears_both_thresholds(scores in prop::collection::vec(conf(), 0..8), p in profile()) {
        let items = scores.iter().enumerate().map(|(i, c)| (CandidateId::new(&i.to_string()).unwrap(), *c));
        let r = rank_candidates(items).unwrap();
        if let CandidatePick::Picked { confidence, margin, .. } = r.pick(p) {
            let t = p.thresholds();
            prop_assert!(confidence >= t.min_confidence && margin >= t.min_margin);
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
    // Candidate thresholds are inclusive minimums, exactly like decide.
    let id = |s: &str| CandidateId::new(s).unwrap();
    let at_min = rank_candidates([(id("a"), c(750)), (id("b"), c(600))]).unwrap();
    assert!(matches!(
        at_min.pick(Profile::Standard),
        CandidatePick::Picked { .. }
    ));
    let empty = rank_candidates([]).unwrap();
    assert!(empty.is_empty() && empty.entries().is_empty());
    assert!(!at_min.is_empty());
    // Accessors return what was stored.
    let q = question(2);
    let o: &Options = q.options().unwrap();
    assert_eq!(
        o.labels().iter().map(|l| l.as_str()).collect::<Vec<_>>(),
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
    // 65535 options are allowed (upper bound inclusive).
    let many: Vec<String> = (0..usize::from(u16::MAX)).map(|i| i.to_string()).collect();
    assert_eq!(Options::new(&many).unwrap().len(), u16::MAX);
}
