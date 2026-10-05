//! Invariances for tool-selection (spec §6.5).
#![allow(clippy::unwrap_used)]
use proptest::prelude::*;
use pua_core::Profile;
use pua_toolbox::{Candidate, ToolId, call_dag_fingerprint, chosen_id, select};

fn catalog() -> [Candidate; 3] {
    [
        Candidate::new("fs.read", "read a file from disk"),
        Candidate::new("fs.write", "write a file to disk"),
        Candidate::new("git.status", "show git working tree status"),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn tool_list_permutation_invisible(seed in any::<u8>()) {
        let base = catalog();
        let mut perm = base.clone();
        let n = (seed as usize) % perm.len();
        perm.rotate_left(n);
        let msg = "please write the file";
        let da = select(msg, &base, Profile::Deep);
        let db = select(msg, &perm, Profile::Deep);
        prop_assert_eq!(
            chosen_id(&da, &base).map(ToolId::as_str),
            chosen_id(&db, &perm).map(ToolId::as_str)
        );
    }

    #[test]
    fn description_case_space_invisible(
        upper in prop::bool::ANY,
        spaces in 1usize..4
    ) {
        let tools = catalog();
        let msg = if upper {
            format!(
                "PLEASE{:spaces$}WRITE{:spaces$}THE{:spaces$}FILE",
                "",
                "",
                "",
                spaces = spaces
            )
        } else {
            format!(
                "please{:spaces$}write{:spaces$}the{:spaces$}file",
                "",
                "",
                "",
                spaces = spaces
            )
        };
        let d = select(&msg, &tools, Profile::Deep);
        prop_assert_eq!(
            chosen_id(&d, &tools).map(ToolId::as_str),
            Some("fs.write")
        );
    }

    #[test]
    fn call_dag_edge_order_invisible(swap in prop::bool::ANY) {
        let nodes = [
            (ToolId::new("a"), 1u64),
            (ToolId::new("b"), 2),
            (ToolId::new("c"), 3),
        ];
        let e1 = [(0usize, 1), (1, 2)];
        let e2 = if swap {
            [(1usize, 2), (0, 1)]
        } else {
            e1
        };
        prop_assert_eq!(
            call_dag_fingerprint(&nodes, &e1),
            call_dag_fingerprint(&nodes, &e2)
        );
    }
}
