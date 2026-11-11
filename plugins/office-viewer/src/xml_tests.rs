use super::*;

fn texts(xml: &str) -> Result<String, Failure> {
    texts_of(xml.as_bytes())
}

fn texts_of(xml: &[u8]) -> Result<String, Failure> {
    let mut reader = Xml::new(xml, 8);
    let mut out = String::new();
    loop {
        match reader.next()? {
            Ev::Text(text) => out.push_str(&text),
            Ev::Eof => return Ok(out),
            _ => {}
        }
    }
}

#[test]
fn text_resolves_the_predefined_entities_and_character_references() {
    let text = texts("<a>x &amp; y &lt;z&gt; &#65;&#x42; <![CDATA[<c>]]></a>").expect("text");
    assert_eq!(text, "x & y <z> AB <c>");
}

#[test]
fn a_doctype_entity_is_not_expanded() {
    let xml = r#"<?xml version="1.0"?>
<!DOCTYPE a [<!ENTITY lol "lollollollollol">]>
<a>&lol;&lol;</a>"#;
    assert_eq!(texts(xml).expect("text").trim(), "");
}

#[test]
fn nesting_past_the_limit_is_too_deep() {
    let deep = format!("{}{}", "<a>".repeat(9), "</a>".repeat(9));
    let failure = texts(&deep).expect_err("too deep");
    assert_eq!(failure.code.as_deref(), Some(code::TOO_DEEP));
    let fine = format!("{}{}", "<a>".repeat(8), "</a>".repeat(8));
    assert!(texts(&fine).is_ok());
}

#[test]
fn a_mismatched_end_tag_is_damaged() {
    let failure = texts("<a><b></a>").expect_err("damaged");
    assert_eq!(failure.code.as_deref(), Some(code::DAMAGED));
}

#[test]
fn attributes_by_local_name_with_and_without_prefix() {
    let mut reader = Xml::new(
        &br#"<p:sldId xmlns:r="urn:r" id="256" r:id="rId7" name="a &amp; b"/>"#[..],
        8,
    );
    match reader.next().expect("event") {
        Ev::Empty(start) => {
            assert_eq!(local(&start), b"sldId");
            assert_eq!(attr(&start, b"id").as_deref(), Some("256"));
            assert_eq!(prefixed_attr(&start, b"id").as_deref(), Some("rId7"));
            assert_eq!(attr(&start, b"name").as_deref(), Some("a & b"));
            assert_eq!(attr(&start, b"missing"), None);
        }
        _ => panic!("an empty tag"),
    }
}

#[test]
fn a_utf8_bom_is_skipped() {
    let text = texts_of(b"\xEF\xBB\xBF<a>x</a>").expect("text");
    assert_eq!(text, "x");
}

/// quick-xml 0.42 checks every event for UTF-8 (RC-110-12): a stray byte in a name, an attribute
/// or a comment, which 0.41 passed by, makes the part damaged as one in the text always did.
#[test]
fn a_byte_that_is_not_utf8_is_damaged_wherever_it_is() {
    let parts: [&[u8]; 5] = [
        b"<a>x\xFFy</a>",
        b"<a t=\"\xFF\">x</a>",
        b"<a\xFF>x</a\xFF>",
        b"<a><!-- \xFF -->x</a>",
        b"\xFF\xFE<\x00a\x00/\x00>\x00",
    ];
    for part in parts {
        let failure = texts_of(part).expect_err("damaged");
        assert_eq!(failure.code.as_deref(), Some(code::DAMAGED), "{part:?}");
    }
}
