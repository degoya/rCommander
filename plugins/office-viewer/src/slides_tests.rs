use super::*;
use crate::out::Discard;
use crate::test_support::Collect;

#[test]
fn slide_ids_in_list_order() {
    let xml = r#"<p:presentation xmlns:p="urn:p" xmlns:r="urn:r"><p:sldMasterIdLst>
<p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst><p:sldId id="257" r:id="rId9"/><p:sldId id="256" r:id="rId2"/></p:sldIdLst>
</p:presentation>"#;
    let ids = slide_ids(&mut Xml::new(xml.as_bytes(), 16)).expect("ids");
    assert_eq!(ids, ["rId9", "rId2"]);
}

const SLIDE: &str = r#"<p:sld xmlns:a="urn:a" xmlns:p="urn:p"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:txBody><a:bodyPr/><a:p><a:r><a:t>Quarterly</a:t></a:r><a:r><a:t> report</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Body"/><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>First point</a:t></a:r></a:p><a:p><a:r><a:t>Second point</a:t></a:r></a:p></p:txBody></p:sp>
<p:graphicFrame><a:graphic><a:graphicData><a:tbl>
<a:tr><a:tc><a:txBody><a:p><a:r><a:t>Q1</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>12</a:t></a:r></a:p></a:txBody></a:tc></a:tr>
</a:tbl></a:graphicData></a:graphic></p:graphicFrame>
</p:spTree></p:cSld></p:sld>"#;

#[test]
fn a_slide_writes_its_frames_and_tables_and_names_its_title() {
    let mut out = Collect::default();
    let title = slide(&mut Xml::new(SLIDE.as_bytes(), 64), &mut out).expect("slide");
    assert_eq!(title.as_deref(), Some("Quarterly report"));
    assert_eq!(
        out.0,
        "Quarterly report\nFirst point\nSecond point\nQ1\t12\n"
    );
}

#[test]
fn a_slide_without_a_title_placeholder_has_no_title() {
    let xml = r#"<p:sld xmlns:a="urn:a" xmlns:p="urn:p"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>Only body</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#;
    let title = slide(&mut Xml::new(xml.as_bytes(), 64), &mut Discard).expect("slide");
    assert_eq!(title, None);
}
