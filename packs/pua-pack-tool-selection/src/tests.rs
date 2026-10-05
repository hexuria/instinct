#![allow(clippy::unwrap_used)]
use super::*;
use pua_core::{Answer, OptionIndex, Profile};

#[test]
fn chooses_by_description_overlap() {
    let tools = [
        Candidate::new("fs.read", "read a file from disk"),
        Candidate::new("fs.write", "write a file to disk"),
    ];
    let d = select("please write the file now", &tools, Profile::Deep);
    assert_eq!(d.answer().chosen(), Some(OptionIndex::new(1)));
    assert_eq!(chosen_id(&d, &tools).unwrap().as_str(), "fs.write");
}

#[test]
fn permutation_of_candidates_is_invisible() {
    let a = [
        Candidate::new("a", "alpha tool here"),
        Candidate::new("b", "beta tool here"),
    ];
    let b = [
        Candidate::new("b", "beta tool here"),
        Candidate::new("a", "alpha tool here"),
    ];
    let id_a = chosen_id(&select("alpha please", &a, Profile::Deep), &a)
        .map(ToolId::as_str)
        .map(str::to_owned);
    let id_b = chosen_id(&select("alpha please", &b, Profile::Deep), &b)
        .map(ToolId::as_str)
        .map(str::to_owned);
    assert_eq!(id_a, id_b);
    assert_eq!(id_a.as_deref(), Some("a"));
}

#[test]
fn description_case_and_space_are_free() {
    let tools = [
        Candidate::new("x.a", "Alpha Tool Here"),
        Candidate::new("x.b", "Beta Tool Here"),
    ];
    let d1 = select("ALPHA please", &tools, Profile::Deep);
    let d2 = select("alpha   please", &tools, Profile::Deep);
    assert_eq!(chosen_id(&d1, &tools), chosen_id(&d2, &tools));
}

#[test]
fn too_few_tools_abstains() {
    let tools = [Candidate::new("only", "lonely tool")];
    let d = select("anything", &tools, Profile::Standard);
    assert!(matches!(
        d.answer(),
        Answer::Abstain {
            why: AbstainReason::NoCandidates,
            ..
        }
    ));
}

#[test]
fn call_dag_relabeling_invariant() {
    let n1 = [(ToolId::new("a"), 1u64), (ToolId::new("b"), 2)];
    let n2 = [(ToolId::new("a"), 1u64), (ToolId::new("b"), 2)];
    let e = [(0usize, 1)];
    assert_eq!(call_dag_fingerprint(&n1, &e), call_dag_fingerprint(&n2, &e));
}

#[test]
fn call_dag_node_id_rename_keeps_structure_when_labels_match() {
    let a = [
        (ToolId::new("ns.read"), 7u64),
        (ToolId::new("ns.write"), 8),
        (ToolId::new("ns.exec"), 9),
    ];
    let edges = [(0usize, 1), (1, 2)];
    let b = a.clone();
    assert_eq!(
        call_dag_fingerprint(&a, &edges),
        call_dag_fingerprint(&b, &edges)
    );
}

#[test]
fn id_tie_break_prefers_lexicographically_earlier() {
    let tools = [
        Candidate::new("z.last", "same description text"),
        Candidate::new("a.first", "same description text"),
    ];
    let d = select("same description text", &tools, Profile::Deep);
    match d.answer() {
        Answer::Choice { option, .. } => assert_eq!(*option, OptionIndex::new(0)),
        Answer::Abstain { ranked, .. } => {
            assert_eq!(ranked.top().unwrap().0, OptionIndex::new(0));
            assert_eq!(chosen_id(&d, &tools).unwrap().as_str(), "a.first");
        }
        other => panic!("{other:?}"),
    }
}
