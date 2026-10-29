use super::*;

/// The framing is frozen: published manifests are signed with it. A byte literal rather than a
/// recomputation - a test that hashes the way the implementation does would pass after a change
/// that breaks every manifest in the field. The same bytes as rDownloader's framing.
#[test]
fn the_framing_is_pinned_to_its_bytes() {
    let mut builder = DigestBuilder::new();
    builder.field(b"manifest");
    builder.field(b"component");
    // rDownloader's vector ends with an empty `named_fields`: a zero count.
    builder.hasher.update(0_u64.to_be_bytes());
    assert_eq!(
        hex(&builder.finish()),
        "7e2934c3fbc462ddc323ac3ad1aa91dc25995266e07865ee80f48913e1e141aa"
    );
}

/// Length prefixes exist to stop a member boundary from moving unnoticed.
#[test]
fn a_moved_field_boundary_changes_the_digest() {
    let mut left = DigestBuilder::new();
    left.field(b"ab").field(b"c");
    let mut right = DigestBuilder::new();
    right.field(b"a").field(b"bc");
    assert_ne!(left.finish(), right.finish());
}

#[test]
fn hex_is_lowercase_and_two_digits_per_byte() {
    assert_eq!(hex(&[0, 15, 255]), "000fff");
    assert_eq!(hex_sha256(b"").len(), 64);
}
