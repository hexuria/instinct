#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn fences_and_diffs() {
    let text = "intro\n```rust\nfn main() {}\n```\nout\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-a\n+b\n@@ -5 +5 @@\n-c\n+d\n";
    let f = shape_of(text);
    assert_eq!(f.fenced_blocks, 1);
    assert_eq!(f.diff_hunks, 2);
}

#[test]
fn dominant_script_and_homoglyphs() {
    let latin = shape_of("hello world");
    assert_eq!(latin.dominant_script, DominantScript::Latin);
    assert_eq!(latin.homoglyphs, 0);
    let cyr = shape_of("привет");
    assert_eq!(cyr.dominant_script, DominantScript::Cyrillic);
    // Cyrillic 'ѕ' + Latin "top"
    let homo = shape_of("\u{0455}top the build");
    assert!(homo.homoglyphs >= 1, "{homo:?}");
}

#[test]
fn json_keys_and_structured_output() {
    let plain = shape_of(r#"{"a":1,"b":{"c":2}}"#);
    assert_eq!(plain.json_keys, 3);
    assert!(!plain.structured_output);
    let structured = shape_of(r#"{"model":"x","response_format":{"type":"json_schema"}}"#);
    assert!(structured.structured_output);
    assert!(structured.json_keys >= 3);
    let prose = shape_of("please set response_format to json");
    assert!(prose.structured_output);
    assert_eq!(prose.json_keys, 0);
}

#[test]
fn no_tier_type_in_the_public_api() {
    // Compile-time: ShapeFeatures fields are the whole surface.
    let f = ShapeFeatures {
        fenced_blocks: 0,
        diff_hunks: 0,
        dominant_script: DominantScript::None,
        homoglyphs: 0,
        json_keys: 0,
        structured_output: false,
    };
    assert_eq!(f.fingerprint()[17], 0);
    assert_eq!(DominantScript::Latin.name(), "latin");
}

#[test]
fn unterminated_fence_counts() {
    let f = shape_of("before ```code without close");
    assert_eq!(f.fenced_blocks, 1);
}
