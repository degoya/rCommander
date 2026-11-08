use zip::CompressionMethod;

use super::*;
use crate::fixtures::{self, CONTENT_TYPES, docx, docx_with, pptx, rels, root_rels, xlsx};
use crate::settings::SheetRows;
use crate::test_support::{TestSource, VecSink, zip_of, zip_with};

fn open(bytes: Vec<u8>) -> (Viewer<TestSource>, Document) {
    Viewer::open(TestSource::new(bytes), Settings::default())
        .unwrap_or_else(|_| panic!("the document opens"))
}

fn open_failure(bytes: Vec<u8>) -> Failure {
    Viewer::open(TestSource::new(bytes), Settings::default())
        .err()
        .expect("the document is refused")
}

fn text_of<S: Source>(viewer: &mut Viewer<S>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

fn labels(nodes: &[TreeNode]) -> Vec<(String, Option<String>)> {
    nodes
        .iter()
        .map(|node| (node.label.clone(), node.value.clone()))
        .collect()
}

fn pair(label: &str, value: &str) -> (String, Option<String>) {
    (label.to_owned(), Some(value.to_owned()))
}

#[test]
fn a_word_document_shows_its_paragraphs_and_its_table() {
    let (mut viewer, document) = open(docx());
    assert_eq!(viewer.kind(), Kind::Word);
    assert_eq!(document.outputs, [Output::Text, Output::Tree]);
    assert_eq!(document.title.as_deref(), Some("Quarterly figures"));
    assert_eq!(
        text_of(&mut viewer),
        "Dear reader,\nthe figures follow.\nRegion\tSales\nNorth\t1 200\nRegards\n"
    );
}

#[test]
fn the_properties_are_the_tree_of_a_word_document() {
    let (mut viewer, _) = open(docx());
    let top = viewer.children(None).expect("tree");
    assert_eq!(
        labels(&top),
        [
            pair("title", "Quarterly figures"),
            pair("subject", "Sales"),
            pair("creator", "Ada Lovelace"),
            pair("last modified by", "Charles Babbage"),
            pair("created", "2026-01-02T03:04:05Z"),
            pair("modified", "2026-11-07T08:09:10Z"),
            pair("application", "Microsoft Office Word"),
            pair("pages", "2"),
            pair("words", "17"),
        ]
    );
    assert!(top.iter().all(|node| !node.has_children));
    assert_eq!(
        viewer.children(Some(999)).err().map(|f| f.kind),
        Some(FailureKind::NotFound)
    );
}

#[test]
fn a_workbook_shows_its_sheets_in_order_with_strings_and_cached_results() {
    let (mut viewer, _) = open(xlsx());
    assert_eq!(viewer.kind(), Kind::Excel);
    assert_eq!(
        text_of(&mut viewer),
        "Summary\nTotal\t3.5\n\nData\nItem\tPrice\nApples\t1.5\nPears\t2\n"
    );
    let top = viewer.children(None).expect("tree");
    let sheets = top.last().expect("the sheets node");
    assert_eq!(sheets.label, "sheets");
    assert_eq!(sheets.value.as_deref(), Some("2"));
    assert!(sheets.has_children);
    let children = viewer.children(Some(sheets.id)).expect("sheets");
    assert_eq!(
        labels(&children),
        [
            pair("Summary", "rows 1, columns 2"),
            pair("Data", "rows 3, columns 2")
        ]
    );
}

#[test]
fn a_presentation_shows_its_slides_in_the_order_of_presentation_xml() {
    let (mut viewer, _) = open(pptx());
    assert_eq!(viewer.kind(), Kind::PowerPoint);
    assert_eq!(
        text_of(&mut viewer),
        "Slide 1\nWelcome\nHello all\n\nSlide 2\nAgenda\nWhere we stand\n\n\
         Slide 3\nNumbers\nUp by a third\n"
    );
    let top = viewer.children(None).expect("tree");
    let slides = top.last().expect("the slides node");
    assert_eq!(slides.label, "slides");
    assert_eq!(slides.value.as_deref(), Some("3"));
    let children = viewer.children(Some(slides.id)).expect("slides");
    assert_eq!(
        labels(&children),
        [
            pair("slide 1", "Welcome"),
            pair("slide 2", "Agenda"),
            pair("slide 3", "Numbers")
        ]
    );
}

#[test]
fn rows_past_the_setting_end_in_one_line() {
    let mut sheet = String::from("<worksheet><sheetData>");
    for row in 0..1501 {
        sheet.push_str(&format!("<row><c><v>{row}</v></c></row>"));
    }
    sheet.push_str("</sheetData></worksheet>");
    let root = root_rels("xl/workbook.xml");
    let workbook =
        r#"<workbook xmlns:r="urn:r"><sheets><sheet name="Long" r:id="rId1"/></sheets></workbook>"#;
    let workbook_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml")]);
    let zip = zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ]);
    let settings = Settings {
        sheet_rows: SheetRows::Thousand,
    };
    let (mut viewer, _) =
        Viewer::open(TestSource::new(zip.clone()), settings).unwrap_or_else(|_| panic!("opens"));
    let text = text_of(&mut viewer);
    assert!(text.starts_with("Long\n0\n1\n"));
    assert!(
        text.ends_with("\n1000\n[... 500 more rows]\n"),
        "{}",
        &text[text.len() - 40..]
    );
    let (mut viewer, _) = open(zip);
    let text = text_of(&mut viewer);
    assert!(text.ends_with("\n1500\n"));
}

#[test]
fn a_zip_bomb_part_is_refused() {
    let mut bomb = String::from("<w:document><w:body>");
    bomb.push_str(&" ".repeat(8 << 20));
    bomb.push_str("</w:body></w:document>");
    let failure = open_failure(docx_with(&bomb, &rels(&[])));
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code::TOO_COMPRESSED));
}

#[test]
fn ten_thousand_and_one_entries_are_refused_before_the_directory_is_read() {
    let names: Vec<String> = (0..10_001).map(|index| format!("e/{index}.xml")).collect();
    let mut parts: Vec<(&str, &[u8])> = vec![("[Content_Types].xml", CONTENT_TYPES.as_bytes())];
    parts.extend(
        names
            .iter()
            .take(10_000)
            .map(|name| (name.as_str(), &b"<e/>"[..])),
    );
    let zip = zip_of(&parts);
    let mut src = TestSource::new(zip);
    let failure = Viewer::open(&mut src, Settings::default())
        .err()
        .expect("too many entries");
    assert_eq!(failure.code.as_deref(), Some(code::TOO_MANY_ENTRIES));
    assert_eq!(src.reads, 1);
    assert!(src.bytes_read <= 1 << 17, "only the end record was read");
    assert!(
        probe(&mut src),
        "the viewer takes it, to say why it is not shown"
    );
}

#[test]
fn the_macro_part_is_never_read() {
    // A large, broken `vbaProject.bin` first in the zip, named by the document's relationships.
    let junk = vec![0xFFu8; 3 << 20];
    let document_rels = rels(&[
        ("rId1", "styles", "styles.xml"),
        ("rId9", "vbaProject", "vbaProject.bin"),
    ]);
    let root = root_rels("word/document.xml");
    let zip = zip_with(&[
        ("word/vbaProject.bin", &junk[..], CompressionMethod::Stored),
        (
            "[Content_Types].xml",
            CONTENT_TYPES.as_bytes(),
            CompressionMethod::Deflated,
        ),
        ("_rels/.rels", root.as_bytes(), CompressionMethod::Deflated),
        (
            "word/document.xml",
            fixtures::DOCUMENT.as_bytes(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels",
            document_rels.as_bytes(),
            CompressionMethod::Deflated,
        ),
    ]);
    let mut src = TestSource::new(zip);
    {
        let (mut viewer, _) =
            Viewer::open(&mut src, Settings::default()).unwrap_or_else(|_| panic!("opens"));
        assert!(text_of(&mut viewer).starts_with("Dear reader,\n"));
        viewer.children(None).expect("tree");
    }
    // Reading the part would pass through its middle; the end record, the directory and the
    // other parts lie past its last 256 KiB.
    let (start, end) = (1u64 << 20, 2u64 << 20);
    let inside = src
        .touched
        .iter()
        .filter(|(from, to)| *from < end && *to > start)
        .count();
    assert_eq!(inside, 0, "the macro part was read: {:?}", src.touched);
    assert!(src.bytes_read < 3 << 20);
}

#[test]
fn an_external_relationship_is_never_followed() {
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml" TargetMode="External"/>
</Relationships>"#;
    let zip = zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("word/document.xml", fixtures::DOCUMENT.as_bytes()),
    ]);
    assert_eq!(open_failure(zip).code.as_deref(), Some(code::NOT_OFFICE));
}

#[test]
fn a_sheet_outside_the_package_is_a_heading_only() {
    let root = root_rels("xl/workbook.xml");
    let workbook =
        r#"<workbook xmlns:r="urn:r"><sheets><sheet name="Away" r:id="rId1"/></sheets></workbook>"#;
    let workbook_rels = rels(&[("rId1", "worksheet", "../../../sheet.xml")]);
    let zip = zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("sheet.xml", b"<worksheet/>"),
    ]);
    let (mut viewer, _) = open(zip);
    assert_eq!(text_of(&mut viewer), "Away\n");
}

#[test]
fn probe_takes_office_packages_only() {
    assert!(probe(&mut TestSource::new(docx())));
    assert!(probe(&mut TestSource::new(xlsx())));
    assert!(probe(&mut TestSource::new(pptx())));
    let plain = zip_of(&[("readme.txt", b"no package")]);
    assert!(!probe(&mut TestSource::new(plain.clone())));
    assert!(!probe(&mut TestSource::new(
        b"PK\x03\x04 not really".to_vec()
    )));
    assert!(!probe(&mut TestSource::new(Vec::new())));
    assert_eq!(open_failure(plain).code.as_deref(), Some(code::NOT_OFFICE));
}

#[test]
fn moving_on_cancels() {
    let mut src = TestSource::new(docx());
    src.stop_after = Some(0);
    let failure = Viewer::open(src, Settings::default())
        .err()
        .expect("cancelled");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}
