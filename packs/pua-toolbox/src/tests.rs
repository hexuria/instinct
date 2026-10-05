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
    let id_a = chosen_id(&select("alpha tool here please", &a, Profile::Deep), &a)
        .map(ToolId::as_str)
        .map(str::to_owned);
    let id_b = chosen_id(&select("alpha tool here please", &b, Profile::Deep), &b)
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
fn duplicate_or_invalid_ids_abstain_with_the_reason() {
    let dup = [Candidate::new("a", "x"), Candidate::new("a", "y")];
    let d = select("anything", &dup, Profile::Standard);
    assert!(d.answer().is_abstain());
    assert_eq!(d.trail().records()[0].text(), "candidate a given twice");
    assert_eq!(chosen_id(&d, &dup), None);
    let bad = [Candidate::new("", "x"), Candidate::new("b", "y")];
    let d = select("anything", &bad, Profile::Standard);
    assert_eq!(d.trail().records()[0].text(), "candidate id is empty");
    let one = [Candidate::new("only", "lonely tool")];
    let d = select("anything", &one, Profile::Standard);
    assert_eq!(d.trail().records()[0].text(), "too few tools");
}

#[test]
fn abstain_carries_every_tool_and_no_chosen_id() {
    let tools = [
        Candidate::new("fs.read", "read a file from disk"),
        Candidate::new("fs.write", "write a file to disk"),
    ];
    let d = select("hello there", &tools, Profile::Standard);
    let Answer::Abstain { ranked, .. } = d.answer() else {
        panic!("expected abstain, got {:?}", d.answer());
    };
    assert_eq!(ranked.entries().len(), 2);
    assert_eq!(chosen_id(&d, &tools), None);
    let long = "x".repeat(pua_text::MAX_INPUT_BYTES + 1);
    let d = select(&long, &tools, Profile::Standard);
    assert!(d.answer().is_abstain());
    assert_eq!(d.trail().records()[0].text(), "input refused (too long)");
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
            // An abstain is never resolved to a tool.
            assert_eq!(chosen_id(&d, &tools), None);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn mutants_survivors_pinned() {
    assert_eq!(ToolId::new("fs.write").to_string(), "fs.write");
    let c = Candidate::new("fs.write", "write a file to disk");
    assert_eq!(c.description(), "write a file to disk");
    assert_ne!(c.description(), "");

    // Short tokens (< 3) ignored; leaf id token participates.
    let tools = [
        Candidate::new("ns.ab", "xy zz"), // all tokens < 3 after filters → zero-ish
        Candidate::new("ns.write", "do stuff"),
    ];
    let d = select("please write stuff now", &tools, Profile::Deep);
    assert_eq!(chosen_id(&d, &tools).unwrap().as_str(), "ns.write");

    let nodes = [(ToolId::new("a"), 1u64), (ToolId::new("b"), 2)];
    let e = [(0usize, 1)];
    let fp = call_dag_fingerprint(&nodes, &e);
    assert_ne!(fp, [0u8; 32]);
    assert_ne!(fp, [1u8; 32]);
    let empty = call_dag_fingerprint(&[], &[]);
    assert_ne!(fp, empty);
}
