use super::*;

#[test]
fn every_table_is_sorted_and_no_word_is_in_two() {
    for table in [KEYWORDS, LITERALS, TYPES] {
        assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "{table:?}");
    }
    for word in KEYWORDS.iter().chain(LITERALS).chain(TYPES) {
        let tables = [KEYWORDS, LITERALS, TYPES];
        let found = tables.iter().filter(|table| table.contains(word)).count();
        assert_eq!(found, 1, "{word:?}");
    }
}

#[test]
fn a_word_is_classified_by_its_table_and_case_matters() {
    assert_eq!(classify(b"for"), Some(TokenClass::Keyword));
    assert_eq!(classify(b"endif"), Some(TokenClass::Keyword));
    assert_eq!(classify(b"null"), Some(TokenClass::Literal));
    assert_eq!(classify(b"string"), Some(TokenClass::TypeName));
    assert_eq!(classify(b"True"), None);
    assert_eq!(classify(b"resource"), None);
}

#[test]
fn names_take_dashes_after_the_first_byte() {
    assert!(is_name_start(b'a') && is_name_start(b'_') && is_name_start(0xC3));
    assert!(!is_name_start(b'-') && !is_name_start(b'1'));
    assert!(is_name(b'-') && is_name(b'9'));
}

#[test]
fn operators_are_one_or_two_bytes() {
    assert_eq!(operator_width(b"== 1"), Some(2));
    assert_eq!(operator_width(b"=> v"), Some(2));
    assert_eq!(operator_width(b"= 1"), Some(1));
    assert_eq!(operator_width(b"?"), Some(1));
    assert_eq!(operator_width(b"@x"), None);
    assert_eq!(operator_width(b""), None);
}
