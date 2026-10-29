use super::*;

#[test]
fn auto_takes_the_separator_found_most_often_outside_quotes() {
    assert_eq!(separator("a.csv", b"a,b,c\n1,2,3\n"), b',');
    assert_eq!(separator("a.csv", b"a;b;c\n1;2;3\n"), b';');
    assert_eq!(separator("a.csv", b"a\tb\n1\t2\n"), b'\t');
    assert_eq!(separator("a.csv", b"a|b|c\n1|2|3"), b'|');
    // Quoted separators do not count.
    assert_eq!(separator("a.csv", b"\"x,y,z\";b\n\"1,2,3\";4\n"), b';');
    // A tie goes in the order comma, semicolon, tab, pipe; none found is a comma.
    assert_eq!(separator("a.csv", b"a,b;c\n"), b',');
    assert_eq!(separator("a.csv", b"one\ntwo\n"), b',');
    assert_eq!(separator("a.csv", b""), b',');
}

/// A German Excel file: decimal commas outnumber the semicolons, but only the semicolon is on
/// every line the same number of times.
#[test]
fn a_separator_on_every_line_alike_goes_first() {
    let head = b"Name;Preis;Menge;Rabatt;Summe\nA;1,5;2,5;0,1;3,4\nB;2,5;3,5;0,2;8,1\n";
    assert_eq!(separator("prices.csv", head), b';');
}

#[test]
fn the_extension_decides_for_tsv_tab_and_psv() {
    assert_eq!(separator("data.TSV", b"a,b,c\n"), b'\t');
    assert_eq!(separator("data.tab", b"a,b,c\n"), b'\t');
    assert_eq!(separator("data.psv", b"a,b,c\n"), b'|');
    assert_eq!(separator("data", b"a;b\n"), b';');
}

#[test]
fn only_the_first_lines_are_counted() {
    let mut head = b"a;b\n".repeat(HEAD_LINES);
    head.extend_from_slice(&b"1,2,3,4,5,6,7,8\n".repeat(100));
    assert_eq!(separator("a.csv", &head), b';');
}

#[test]
fn the_first_record_is_read_unquoted() {
    let head = b"\r\nname;\"city, \"\"old\"\"\";\r\n1;2;3\r\n";
    assert_eq!(
        first_record(head, b';'),
        vec![b"name".to_vec(), b"city, \"old\"".to_vec(), Vec::new()]
    );
    // A record over several lines, cut by the head's end.
    assert_eq!(
        first_record(b"a,\"b\nc", b','),
        vec![b"a".to_vec(), b"b\nc".to_vec()]
    );
    assert!(first_record(b"", b',').is_empty());
    let long = format!("{},x", "n".repeat(1000));
    assert_eq!(first_record(long.as_bytes(), b',')[0].len(), NAME_BYTES);
}

#[test]
fn auto_takes_a_first_record_without_numbers_for_a_header() {
    let names = |text: &[u8]| first_record(text, b',');
    assert!(looks_like_header(&names(b"id,name,price\n1,a,2\n")));
    assert!(!looks_like_header(&names(b"1,a,2\n3,b,4\n")));
    assert!(!looks_like_header(&names(b"name,\"1,5\"\n")));
    assert!(!looks_like_header(&names(b"")));
}

#[test]
fn numbers_as_a_spreadsheet_writes_them() {
    for number in [
        "0", "42", "-1", "+3", "1.5", "1,5", ".5", "1.", " 7 ", "1e10", "-2.5E-3",
    ] {
        assert!(is_number(number.as_bytes()), "{number}");
    }
    for text in [
        "", " ", "-", "1.2.3", "1,2,3", "e5", "1e", "abc", "12a", "1 2",
    ] {
        assert!(!is_number(text.as_bytes()), "{text}");
    }
}
