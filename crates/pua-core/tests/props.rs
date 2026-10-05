//! Property tests for spec §5.1 at the core level: determinism, candidate permutation
//! invariance, option relabeling equivariance away from ties, option-0 tie-break, abstain rules.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests may unwrap (AGENTS.md rule 4)

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
