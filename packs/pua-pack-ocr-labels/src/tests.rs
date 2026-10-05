#![allow(clippy::unwrap_used)]

use super::*;

/// The three A §5.3 cases: leading spaces, double space + multibyte Ñ, clean prefix.
#[test]
fn three_section_53_cases_extract_correctly() {
    let leading = "  TIN: 123-456-789-000";
    let fields = extract(leading);
    let tin = fields.iter().find(|f| f.label() == "TIN").expect("tin");
    assert_eq!(tin.value_text(leading), "123-456-789-000");
    assert_eq!(tin.match_type(), MatchType::Exact);

    let multi = "PEÑA TRADING  TRADE NAME: PEÑA STORE";
    let fields = extract(multi);
    let trade = fields
        .iter()
        .find(|f| f.label() == "TRADE NAME")
        .expect("trade");
    assert_eq!(trade.value_text(multi), "PEÑA STORE");

    let clean = "TIN:   123-456-789-000";
    let fields = extract(clean);
    let tin = fields.iter().find(|f| f.label() == "TIN").expect("tin");
    assert_eq!(tin.value_text(clean), "123-456-789-000");
}

#[test]
fn label_diacritics_not_folded() {
    // A label written with Ñ must not match a vocabulary label without it.
    let page = "TRADE ÑAME: PEÑA STORE";
    let fields = extract(page);
    assert!(
        fields.iter().all(|f| f.label() != "TRADE NAME"),
        "folded diacritic must not match: {fields:?}"
    );
}

#[test]
fn confidence_by_match_type() {
    assert!(MatchType::Exact.confidence() > MatchType::CaseFolded.confidence());
    assert!(MatchType::CaseFolded.confidence() > MatchType::NextLine.confidence());
}

#[test]
fn layout_fingerprint_stable_under_renumbering() {
    let page = "TIN: 1\nTRADE NAME: X\nRDO: 39\n";
    let a = extract(page);
    let mut b = a.clone();
    b.reverse();
    assert_eq!(layout_fingerprint(&a), layout_fingerprint(&b));
}
