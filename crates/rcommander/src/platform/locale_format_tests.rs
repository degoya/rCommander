use super::written_text;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[test]
fn the_written_units_but_the_terminator_are_the_format() {
    let mut buffer = units("dd.MM.yyyy");
    let written = i32::try_from(buffer.len()).expect("small");
    buffer.resize(128, 0);
    assert_eq!(
        written_text(&buffer, written).as_deref(),
        Some("dd.MM.yyyy")
    );
}

#[test]
fn a_failed_call_an_empty_text_or_a_count_past_the_buffer_is_none() {
    let buffer = units("HH:mm:ss");
    for written in [0, -1, 1] {
        assert_eq!(written_text(&buffer, written), None, "{written}");
    }
    let past = i32::try_from(buffer.len() + 1).expect("small");
    assert_eq!(written_text(&buffer, past), None);
}

#[test]
fn units_that_are_no_utf16_are_none() {
    // A lone high surrogate, then the terminator.
    assert_eq!(written_text(&[0x0048, 0xD800, 0], 3), None);
}
