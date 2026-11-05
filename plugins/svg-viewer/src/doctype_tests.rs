use super::*;

#[test]
fn a_doctype_turns_into_spaces_and_nothing_else_moves() {
    let text = "\u{feff}<?xml version=\"1.0\"?>\n<!-- a > b -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x>y\" [\n  <!ENTITY e \"]>\">\n]>\n<svg/>";
    let blanked = without_doctype(text);
    assert_eq!(blanked.len(), text.len());
    assert!(!blanked.contains("DOCTYPE") && !blanked.contains("ENTITY"));
    assert!(blanked.ends_with("<svg/>"));
    assert!(blanked.starts_with("\u{feff}<?xml version=\"1.0\"?>\n<!-- a > b -->\n "));
}

#[test]
fn a_document_without_a_doctype_is_left_as_it_is() {
    for text in [
        "<svg/>",
        "<?xml version=\"1.0\"?><svg><!DOCTYPE not in the prolog></svg>",
        "",
    ] {
        assert!(matches!(without_doctype(text), Cow::Borrowed(_)), "{text}");
    }
}

#[test]
fn data_that_is_no_text_stays_the_same_bytes() {
    let png = Arc::new(b"\x89PNG\r\n\x1a\n\xff".to_vec());
    assert!(Arc::ptr_eq(&data_without_doctype(Arc::clone(&png)), &png));
    let svg = Arc::new(b"<!DOCTYPE svg><svg/>".to_vec());
    assert_eq!(
        data_without_doctype(svg).as_slice(),
        b"              <svg/>"
    );
}
