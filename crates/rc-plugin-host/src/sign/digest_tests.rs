use super::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The framing is frozen: shipped packages are signed with it. A byte literal rather than a
/// recomputation - a test hashing the way the code does would pass a change that breaks every
/// installed package. The vector is rDownloader's, so both projects keep one framing.
#[test]
fn the_framing_is_pinned_to_its_shipped_bytes() {
    let mut builder = DigestBuilder::new();
    builder.field(b"manifest");
    builder.field(b"component");
    builder.named_fields(&[]);
    assert_eq!(
        hex(&builder.finish()),
        "7e2934c3fbc462ddc323ac3ad1aa91dc25995266e07865ee80f48913e1e141aa"
    );
}

#[test]
fn a_moved_field_boundary_changes_the_digest() {
    let mut left = DigestBuilder::new();
    left.field(b"ab").field(b"c");
    let mut right = DigestBuilder::new();
    right.field(b"a").field(b"bc");
    assert_ne!(left.finish(), right.finish());
}

#[test]
fn named_fields_do_not_depend_on_input_order_and_are_covered() {
    let ascending = [
        ("de".to_owned(), b"{\"a\":1}".to_vec()),
        ("en".to_owned(), b"{\"b\":2}".to_vec()),
    ];
    let descending = [ascending[1].clone(), ascending[0].clone()];
    let mut left = DigestBuilder::new();
    left.named_fields(&ascending);
    let mut right = DigestBuilder::new();
    right.named_fields(&descending);
    assert_eq!(left.finish(), right.finish());

    let mut empty = DigestBuilder::new();
    empty.named_fields(&[]);
    let mut one = DigestBuilder::new();
    one.named_fields(&ascending[..1]);
    assert_ne!(empty.finish(), one.finish());
}

#[test]
fn hex_sha256_is_lower_case_and_64_characters() {
    let text = hex_sha256(b"abc");
    assert_eq!(
        text,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
