use super::decode;

fn decoded(text: &str) -> Option<Vec<u8>> {
    decode(text).map(|bytes| bytes.to_vec())
}

#[test]
fn rfc_4648_vectors_decode() {
    for (text, bytes) in [
        ("MY======", "f"),
        ("MZXQ====", "fo"),
        ("MZXW6===", "foo"),
        ("MZXW6YQ=", "foob"),
        ("MZXW6YTB", "fooba"),
        ("MZXW6YTBOI======", "foobar"),
    ] {
        assert_eq!(decoded(text).as_deref(), Some(bytes.as_bytes()), "{text}");
    }
}

#[test]
fn case_groups_and_missing_padding_are_taken() {
    assert_eq!(
        decoded("mzxw-6ytb oi").as_deref(),
        Some(b"foobar".as_slice())
    );
}

#[test]
fn what_is_no_base32_is_refused() {
    for text in ["", "  ", "MZXW1", "MZXW8", "MZ=XW", "MZXW6!"] {
        assert_eq!(decoded(text), None, "{text:?}");
    }
}
