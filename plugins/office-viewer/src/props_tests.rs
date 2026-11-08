use super::*;

#[test]
fn core_properties_in_their_order() {
    let xml = r#"<cp:coreProperties xmlns:cp="urn:cp" xmlns:dc="urn:dc" xmlns:dcterms="urn:dt">
<dcterms:modified xsi:type="dcterms:W3CDTF">2026-11-07T10:00:00Z</dcterms:modified>
<dc:creator>Ada &amp; Charles</dc:creator>
<dc:title>  Notes  </dc:title>
<dc:subject></dc:subject>
<cp:keywords>left out</cp:keywords>
</cp:coreProperties>"#;
    let found = read(&mut Xml::new(xml.as_bytes(), 16), CORE).expect("properties");
    assert_eq!(
        found,
        vec![
            ("title", "Notes".to_owned()),
            ("creator", "Ada & Charles".to_owned()),
            ("modified", "2026-11-07T10:00:00Z".to_owned()),
        ]
    );
}

#[test]
fn nested_values_are_not_taken_and_long_ones_are_cut() {
    let long = "\u{e9}".repeat(1000);
    let xml = format!(
        "<Properties><Application>{long}</Application><HeadingPairs><Pages>9</Pages></HeadingPairs><Words>12</Words></Properties>"
    );
    let found = read(&mut Xml::new(xml.as_bytes(), 16), APP).expect("properties");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].0, "application");
    assert_eq!(found[0].1.len(), MAX_VALUE_BYTES);
    assert_eq!(found[1], ("words", "12".to_owned()));
}
