#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn field_order_invariant_layout() {
    let a = FormRecord::new([("tin", "1"), ("name", "PEÑA"), ("rdo", "RDO-39")]);
    let b = FormRecord::new([("rdo", "RDO-39"), ("tin", "1"), ("name", "PEÑA")]);
    assert_eq!(layout_fingerprint(&a), layout_fingerprint(&b));
    assert_eq!(a.fields().len(), 3);
}

#[test]
fn catalog_sample_is_non_authoritative_but_matches() {
    let c = Catalog::load().unwrap();
    let hit = c.longest_exact("2303").unwrap();
    assert_eq!(hit.code(), "2303");
    assert_eq!(hit.label(), "COR");
    assert!(c.longest_exact("9999").is_none());
    let _ = data_version(&c);
}

#[test]
fn suggestion_reason_never_contains_valid() {
    let s = suggest_tin("000-000-000-00000").unwrap();
    assert!(!format!("{s:?}").to_ascii_lowercase().contains("valid"));
}
