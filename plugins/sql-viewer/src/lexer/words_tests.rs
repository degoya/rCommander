use super::*;

#[test]
fn every_table_is_sorted_upper_case_and_short_enough() {
    for table in [KEYWORDS, TYPES, LITERALS] {
        assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "{table:?}");
        for word in table {
            assert_eq!(word.to_ascii_uppercase(), *word);
            assert!(word.len() <= LONGEST, "{word}");
        }
    }
}

#[test]
fn no_word_is_in_two_tables() {
    for word in KEYWORDS {
        assert!(!TYPES.contains(word) && !LITERALS.contains(word), "{word}");
    }
    for word in TYPES {
        assert!(!LITERALS.contains(word), "{word}");
    }
}

#[test]
fn a_word_is_classified_in_any_case() {
    assert_eq!(classify(b"select"), Word::Keyword);
    assert_eq!(classify(b"SeLeCt"), Word::Keyword);
    assert_eq!(classify(b"varchar"), Word::Type);
    assert_eq!(classify(b"Null"), Word::Literal);
    assert_eq!(classify(b"customers"), Word::Name);
    assert_eq!(classify(b"a_name_far_longer_than_any_keyword"), Word::Name);
    assert_eq!(classify("caf\u{e9}".as_bytes()), Word::Name);
}
