use super::{is_well_formed, matches};

/// HMAC-SHA1 with the salt `0123456789abcdefghij`, computed outside Rust (Python's `hmac`).
const BOX_22: &str = "|1|MDEyMzQ1Njc4OWFiY2RlZmdoaWo=|Jo2O/bEngyhpxDCBT4DDL9yMUoE=";
const BOX_2222: &str = "|1|MDEyMzQ1Njc4OWFiY2RlZmdoaWo=|m4VieVH/Mlzs78GajMuk/n499ik=";

#[test]
fn a_hashed_name_matches_its_own_name_only() {
    assert!(matches(BOX_22, "box.example"));
    assert!(!matches(BOX_22, "[box.example]:22"));
    assert!(!matches(BOX_22, "other.example"));
    assert!(matches(BOX_2222, "[box.example]:2222"));
    assert!(!matches(BOX_2222, "box.example"));
}

#[test]
fn a_malformed_hashed_name_matches_nothing() {
    for broken in ["|1|", "|1|abc", "|1|!!|!!", "box.example", "|2|MDEy|Jo2O"] {
        assert!(!matches(broken, "box.example"), "{broken}");
        assert!(!is_well_formed(broken), "{broken}");
    }
    assert!(is_well_formed(BOX_22));
}
