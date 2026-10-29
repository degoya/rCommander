use super::*;

type Record = Vec<(String, bool)>;

/// Ends the field in `field`, and the record when `last`.
fn end(broken: bool, last: bool, field: &mut Vec<u8>, record: &mut Record, all: &mut Vec<Record>) {
    record.push((String::from_utf8_lossy(field).into_owned(), broken));
    field.clear();
    if last {
        all.push(std::mem::take(record));
    }
}

/// The records of `text`: each field's value and whether it was broken.
fn records(text: &[u8], sep: u8) -> Vec<Record> {
    let mut machine = Machine::new(sep, Mode::RecordStart);
    let (mut all, mut record, mut field) = (Vec::new(), Vec::new(), Vec::new());
    for byte in text {
        match machine.feed(*byte).kind {
            Kind::Blank | Kind::Content(None) => {}
            Kind::Content(Some(byte)) => field.push(byte),
            Kind::End {
                broken,
                record: last,
            } => {
                end(broken, last, &mut field, &mut record, &mut all);
            }
        }
    }
    if let Some(Kind::End { broken, .. }) = machine.finish() {
        end(broken, true, &mut field, &mut record, &mut all);
    }
    all
}

fn plain(fields: &[&str]) -> Record {
    fields
        .iter()
        .map(|field| ((*field).to_owned(), false))
        .collect()
}

#[test]
fn quoted_fields_hold_quotes_separators_and_line_breaks() {
    let text = b"a,\"b \"\"q\"\" , c\",\"x\ny\"\n1,,3";
    assert_eq!(
        records(text, b','),
        vec![plain(&["a", "b \"q\" , c", "x\ny"]), plain(&["1", "", "3"])]
    );
}

#[test]
fn blank_lines_are_no_records_and_crlf_ends_a_line() {
    let text = b"\r\n\na;b\r\n\r\n\"c\";d\r\n";
    assert_eq!(
        records(text, b';'),
        vec![plain(&["a", "b\r"]), plain(&["c", "d\r"])]
    );
}

#[test]
fn text_after_a_closing_quote_breaks_the_field() {
    let text = b"\"ok\",\"no\"x,\"fine\"\r\n\"open";
    let got = records(text, b',');
    assert_eq!(
        got[0],
        vec![
            ("ok".to_owned(), false),
            ("nox".to_owned(), true),
            ("fine".to_owned(), false)
        ]
    );
    // A quote still open at the end of the file breaks its field too.
    assert_eq!(got[1], vec![("open".to_owned(), true)]);
}

#[test]
fn a_quote_inside_an_unquoted_field_is_text() {
    assert_eq!(
        records(b"5\" disk|x", b'|'),
        vec![plain(&["5\" disk", "x"])]
    );
}

#[test]
fn the_first_byte_of_a_record_begins_it() {
    let mut machine = Machine::new(b'\t', Mode::RecordStart);
    let began: Vec<bool> = b"\n\ta\n"
        .iter()
        .map(|byte| machine.feed(*byte).began)
        .collect();
    assert_eq!(began, vec![false, true, false, false]);
    assert_eq!(machine.finish(), None);
}
