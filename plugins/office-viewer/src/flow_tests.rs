use super::*;
use crate::test_support::Collect;
use crate::xml::Xml;

fn flow(xml: &str) -> String {
    let mut reader = Xml::new(xml.as_bytes(), 64);
    let mut flow = Flow::default();
    let mut out = Collect::default();
    loop {
        let ev = reader.next().expect("event");
        if matches!(ev, Ev::Eof) {
            break;
        }
        flow.event(&ev, &mut out).expect("flow");
    }
    flow.finish(&mut out).expect("finish");
    out.0
}

#[test]
fn paragraphs_are_lines_with_their_tabs_and_breaks() {
    let xml = r#"<w:body xmlns:w="urn:w">
<w:p><w:pPr><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr>
<w:r><w:t>One</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">two </w:t></w:r></w:p>
<w:p/>
<w:p><w:r><w:t>Three</w:t><w:br/><w:t>four</w:t></w:r>
<w:r><w:delText>gone</w:delText><w:instrText>PAGE</w:instrText></w:r></w:p>
</w:body>"#;
    assert_eq!(flow(xml), "One\ttwo \n\nThree\nfour\n");
}

#[test]
fn a_table_row_is_a_line_of_cells_separated_by_tabs() {
    let xml = r#"<w:body xmlns:w="urn:w">
<w:p><w:r><w:t>Before</w:t></w:r></w:p>
<w:tbl>
 <w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Age</w:t></w:r></w:p></w:tc></w:tr>
 <w:tr><w:tc><w:p><w:r><w:t>Ada</w:t></w:r></w:p><w:p><w:r><w:t>Lovelace</w:t></w:r></w:p></w:tc>
  <w:tc><w:tbl><w:tr><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>6</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>After</w:t></w:r></w:p>
</w:body>"#;
    assert_eq!(flow(xml), "Before\nName\tAge\nAda Lovelace\t3 6\nAfter\n");
}

#[test]
fn a_fallback_is_read_once() {
    let xml = r#"<w:p xmlns:w="urn:w" xmlns:mc="urn:mc"><w:r><mc:AlternateContent>
<mc:Choice><w:t>box</w:t></mc:Choice>
<mc:Fallback><w:t>box again</w:t></mc:Fallback>
</mc:AlternateContent></w:r></w:p>"#;
    assert_eq!(flow(xml), "box\n");
}

#[test]
fn drawingml_paragraphs_and_breaks_between_runs() {
    let xml = r#"<p:txBody xmlns:a="urn:a" xmlns:p="urn:p"><a:bodyPr/>
<a:p><a:r><a:t>Line one</a:t></a:r><a:br/><a:r><a:t>line two</a:t></a:r></a:p>
<a:p><a:fld type="slidenum"><a:t>7</a:t></a:fld></a:p></p:txBody>"#;
    assert_eq!(flow(xml), "Line one\nline two\n7\n");
}
