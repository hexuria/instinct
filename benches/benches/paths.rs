//! Callgrind Ir benches for the §5.3 hot paths (docs/benchmarks.md).
#![allow(
    missing_docs,
    unused_qualifications,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout
)]

use std::hint::black_box;

use gungraun::{library_benchmark, library_benchmark_group, main};
use instinct_core::{Millis, Profile};
use instinct_graph::{Edge, LabeledGraph, Rounds, wl_refine};
use instinct_hdc::{Codebook, D1024, Encoder, IterationCap, resonate};
use instinct_rules::{ClassifierSpec, OnConfusable, RuleClassifier};
use instinct_text::{Fold, NormalizeConfig, PunctRuns, normalize};

fn prose_32kib() -> String {
    // ~32 KiB of Latin prose with a couple of protected spans.
    let unit =
        "The quick brown fox jumps over the lazy dog. See `code` and https://example.com/a. ";
    unit.repeat(32_768 / unit.len() + 1)
}

/// The engine's realistic regression fixture (`crates/instinct-rules/tests/fixtures/delivery`).
fn delivery_classifier() -> RuleClassifier {
    const LEXICON: &str =
        include_str!("../../crates/instinct-rules/tests/fixtures/delivery/lexicon.toml");
    const RULES: &str =
        include_str!("../../crates/instinct-rules/tests/fixtures/delivery/rules.toml");
    RuleClassifier::new(&ClassifierSpec {
        domain: "pua-steer/1".into(),
        question: "delivery".into(),
        normalize: NormalizeConfig {
            fold: Fold::UnicodeLower,
            punct_runs: PunctRuns::Collapse,
        },
        lexicon: toml::from_str(LEXICON).expect("fixture lexicon"),
        rules: toml::from_str(RULES).expect("fixture rules"),
        on_confusable: OnConfusable::Abstain,
    })
    .expect("fixture classifier")
}

#[library_benchmark]
#[bench::typical(setup = delivery_classifier)]
fn classifier_decide(classifier: RuleClassifier) {
    let text = "please stop and wait for the current answer before continuing";
    let _ = black_box(classifier.decide(black_box(text), Profile::Standard));
}

#[library_benchmark]
#[bench::kib32(setup = prose_32kib)]
fn normalize_32kib(text: String) {
    let _ = black_box(normalize(black_box(&text), NormalizeConfig::default()));
}

fn codebook_4096() -> (Codebook<D1024>, instinct_hdc::Hv<D1024>) {
    let enc = Encoder::new("bench", 1);
    let entries: Vec<(String, _)> = (0..4096)
        .map(|i| {
            let name = format!("e{i:04}");
            let hv = enc.encode::<D1024>(&name);
            (name, hv)
        })
        .collect();
    let book = Codebook::new(entries).expect("codebook");
    let q = enc.encode::<D1024>("e2048");
    (book, q)
}

#[library_benchmark]
#[bench::max_codebook(setup = codebook_4096)]
fn cleanup_4096(input: (Codebook<D1024>, instinct_hdc::Hv<D1024>)) {
    let (book, q) = input;
    let _ = black_box(book.cleanup(black_box(&q), Millis::ZERO));
}

fn resonator_worst() -> (
    instinct_hdc::Hv<D1024>,
    Codebook<D1024>,
    Codebook<D1024>,
    IterationCap,
) {
    let enc = Encoder::new("res", 1);
    let a_entries: Vec<(String, _)> = (0..64)
        .map(|i| {
            let n = format!("a{i:02}");
            (n.clone(), enc.encode::<D1024>(&n))
        })
        .collect();
    let b_entries: Vec<(String, _)> = (0..64)
        .map(|i| {
            let n = format!("b{i:02}");
            (n.clone(), enc.encode::<D1024>(&n))
        })
        .collect();
    let a_book = Codebook::new(a_entries).expect("a");
    let b_book = Codebook::new(b_entries).expect("b");
    // Query deliberately unpaired so the resonator runs to the iteration cap.
    let q = enc
        .encode::<D1024>("noise")
        .bind(&enc.encode::<D1024>("other"));
    (q, a_book, b_book, IterationCap::MAX)
}

#[library_benchmark]
#[bench::cap16(setup = resonator_worst)]
fn resonator_64x64(
    input: (
        instinct_hdc::Hv<D1024>,
        Codebook<D1024>,
        Codebook<D1024>,
        IterationCap,
    ),
) {
    let (q, a, b, cap) = input;
    let _ = black_box(resonate(black_box(&q), black_box(&a), black_box(&b), cap));
}

fn graph_500() -> LabeledGraph {
    let mut g = LabeledGraph::new();
    let mut nodes = Vec::with_capacity(500);
    for i in 0..500u64 {
        nodes.push(
            g.add_node(i.wrapping_mul(0x9E37_79B9_7F4A_7C15))
                .expect("node"),
        );
    }
    // ~2000 directed edges in a deterministic pattern.
    for e in 0..2000usize {
        let a = nodes[e % 500];
        let b = nodes[(e.wrapping_mul(7) + 13) % 500];
        let _ = g.add_edge(a, b, Edge::directed());
    }
    g
}

#[library_benchmark]
#[bench::n500_e2000(setup = graph_500)]
fn wl_fingerprint_500(g: LabeledGraph) {
    let _ = black_box(wl_refine(black_box(&g), Rounds::Fixed(3)).fingerprint());
}

library_benchmark_group!(
    name = section_53,
    benchmarks = [
        classifier_decide,
        normalize_32kib,
        cleanup_4096,
        resonator_64x64,
        wl_fingerprint_500,
    ]
);

main!(library_benchmark_groups = section_53);
