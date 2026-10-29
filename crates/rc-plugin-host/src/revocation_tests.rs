use super::*;

#[test]
fn the_hex_form_round_trips_and_upper_case_is_taken() {
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::try_from(index)
            .expect("small")
            .wrapping_mul(7)
            .wrapping_add(3);
    }
    let text = format_package_digest(&digest);
    assert_eq!(text.len(), 64);
    assert_eq!(parse_package_digest(&text).expect("parse"), digest);
    assert_eq!(
        parse_package_digest(&text.to_uppercase()).expect("parse"),
        digest
    );
}

#[test]
fn a_value_that_is_not_a_digest_is_refused() {
    for bad in ["", "abc", &"z".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
        assert!(parse_package_digest(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn withdrawals_reach_clones_can_be_taken_back_and_seeding_replaces() {
    let set = RevokedDigests::new();
    let clone = set.clone();
    assert!(set.insert([4; 32]).expect("insert"));
    assert!(!set.insert([4; 32]).expect("insert"));
    assert!(clone.contains(&[4; 32]).expect("read"));
    assert!(clone.remove(&[4; 32]).expect("remove"));
    assert!(!set.contains(&[4; 32]).expect("read"));
    set.insert([1; 32]).expect("insert");
    set.replace([[2; 32]]).expect("seed");
    assert!(!set.contains(&[1; 32]).expect("read"));
    assert!(set.contains(&[2; 32]).expect("read"));

    let keys = RevokedKeys::new();
    assert!(keys.insert("ABCD").expect("insert"));
    assert!(keys.contains("abcd").expect("read"));
    keys.replace(["EF".to_owned()]).expect("seed");
    assert!(!keys.contains("abcd").expect("read"));
    assert!(keys.contains("ef").expect("read"));
}
