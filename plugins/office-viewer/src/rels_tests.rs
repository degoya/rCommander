use super::*;

#[test]
fn the_rels_part_of_a_part_and_of_the_package() {
    assert_eq!(
        rels_name("word/document.xml"),
        "word/_rels/document.xml.rels"
    );
    assert_eq!(
        rels_name("ppt/slides/slide1.xml"),
        "ppt/slides/_rels/slide1.xml.rels"
    );
    assert_eq!(rels_name(""), "_rels/.rels");
}

#[test]
fn targets_resolve_inside_the_package_only() {
    assert_eq!(
        resolve("xl", "worksheets/sheet1.xml").as_deref(),
        Some("xl/worksheets/sheet1.xml")
    );
    assert_eq!(
        resolve("ppt/slides", "../media/a.png").as_deref(),
        Some("ppt/media/a.png")
    );
    assert_eq!(
        resolve("xl", "/xl/styles.xml").as_deref(),
        Some("xl/styles.xml")
    );
    assert_eq!(
        resolve("", "word/document.xml").as_deref(),
        Some("word/document.xml")
    );
    assert_eq!(
        resolve("word", "my%20file.xml").as_deref(),
        Some("word/my file.xml")
    );
    assert_eq!(resolve("word", "../../etc/passwd"), None);
    assert_eq!(resolve("", "../outside.xml"), None);
    assert_eq!(resolve("word", "file:///C:/secret.xml"), None);
    assert_eq!(resolve("word", "https://example.com/a.xml"), None);
    assert_eq!(resolve("word", "..\\..\\a.xml"), None);
    assert_eq!(resolve("word", "%zz"), None);
}

#[test]
fn an_external_relationship_is_dropped() {
    let xml = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/" TargetMode="External"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/a.png" TargetMode="External"/>
<Relationship Id="rId4" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>
</Relationships>"#;
    let rels = read(&mut Xml::new(xml.as_bytes(), 16), "word/document.xml").expect("rels");
    assert_eq!(rels.len(), 2);
    assert_eq!(by_id(&rels, "rId1"), Some("word/styles.xml"));
    assert_eq!(by_id(&rels, "rId2"), None);
    assert_eq!(by_id(&rels, "rId3"), None);
    assert_eq!(by_kind(&rels, "styles"), Some("word/styles.xml"));
    assert_eq!(by_kind(&rels, "vbaProject"), Some("word/vbaProject.bin"));
}
