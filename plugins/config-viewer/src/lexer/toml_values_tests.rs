use super::*;

#[test]
fn every_kind_of_bare_value() {
    for word in [
        "0",
        "+99",
        "-17",
        "1_000",
        "0xDEADBEEF",
        "0xdead_beef",
        "0o755",
        "0b1101",
        "3.1415",
        "-0.01",
        "5e+22",
        "1e06",
        "6.626e-34",
        "224_617.445_991",
        "inf",
        "-inf",
        "nan",
        "+nan",
    ] {
        assert_eq!(class(word.as_bytes()), TokenClass::Number, "{word}");
    }
    for word in [
        "true",
        "false",
        "1979-05-27T07:32:00Z",
        "1979-05-27T00:32:00-07:00",
        "1979-05-27 07:32:00",
        "1979-05-27t07:32:00.999999",
        "1979-05-27",
        "07:32:00",
        "00:32:00.999999",
    ] {
        assert_eq!(class(word.as_bytes()), TokenClass::Literal, "{word}");
    }
    for word in [
        "",
        "1__0",
        "_1",
        "1_",
        "0x",
        "0o8",
        "1.",
        ".5",
        "True",
        "abc",
        "1979-5-27",
    ] {
        assert_eq!(class(word.as_bytes()), TokenClass::Invalid, "{word}");
    }
    assert!(is_date(b"1979-05-27"));
    assert!(!is_date(b"1979-05-27T07"));
}
