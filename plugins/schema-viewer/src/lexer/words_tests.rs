use super::*;

#[test]
fn every_table_is_sorted_and_no_word_is_in_two_of_a_language() {
    for tables in [
        [PROTO_KEYWORDS, PROTO_LITERALS, PROTO_TYPES],
        [GRAPHQL_KEYWORDS, GRAPHQL_LITERALS, GRAPHQL_TYPES],
    ] {
        for table in tables {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "{table:?}");
            for word in table {
                let found = tables.iter().filter(|other| other.contains(word)).count();
                assert_eq!(found, 1, "{word:?}");
            }
        }
    }
}

#[test]
fn each_language_classifies_its_own_words_and_case_matters() {
    assert_eq!(proto_class(b"message"), Some(TokenClass::Keyword));
    assert_eq!(proto_class(b"sfixed64"), Some(TokenClass::TypeName));
    assert_eq!(proto_class(b"inf"), Some(TokenClass::Literal));
    assert_eq!(proto_class(b"Message"), None);
    assert_eq!(proto_class(b"query"), None);
    assert_eq!(graphql_class(b"fragment"), Some(TokenClass::Keyword));
    assert_eq!(graphql_class(b"ID"), Some(TokenClass::TypeName));
    assert_eq!(graphql_class(b"null"), Some(TokenClass::Literal));
    assert_eq!(graphql_class(b"string"), None);
    assert_eq!(graphql_class(b"message"), None);
}

#[test]
fn declaring_keywords_name_what_follows() {
    assert_eq!(proto_declares(b"message"), Mode::TypeName);
    assert_eq!(proto_declares(b"rpc"), Mode::FunctionName);
    assert_eq!(proto_declares(b"repeated"), Mode::Expr);
    assert_eq!(graphql_declares(b"implements"), Mode::TypeName);
    assert_eq!(graphql_declares(b"fragment"), Mode::FunctionName);
    assert_eq!(graphql_declares(b"schema"), Mode::Expr);
}
