//! Small Office packages for the tests, written as Word, Excel and PowerPoint write them: the
//! content types, the package relationships, the properties and the main part with its own
//! relationships.

use crate::test_support::zip_of;

const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DOC_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(crate) const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
</Types>"#;

pub(crate) const CORE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<dc:title>Quarterly figures</dc:title><dc:subject>Sales</dc:subject><dc:creator>Ada Lovelace</dc:creator>
<cp:lastModifiedBy>Charles Babbage</cp:lastModifiedBy>
<dcterms:created xsi:type="dcterms:W3CDTF">2026-01-02T03:04:05Z</dcterms:created>
<dcterms:modified xsi:type="dcterms:W3CDTF">2026-11-07T08:09:10Z</dcterms:modified>
</cp:coreProperties>"#;

pub(crate) const APP: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">
<Application>Microsoft Office Word</Application><Pages>2</Pages><Words>17</Words></Properties>"#;

/// `_rels/.rels` naming `main` as the office document, with the properties.
pub(crate) fn root_rels(main: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{RELS_NS}">
<Relationship Id="rId3" Type="{DOC_REL}/extended-properties" Target="docProps/app.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
<Relationship Id="rId1" Type="{DOC_REL}/officeDocument" Target="{main}"/>
</Relationships>"#
    )
}

/// A relationships part of `(id, type, target)` rows.
pub(crate) fn rels(rows: &[(&str, &str, &str)]) -> String {
    let mut xml =
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELS_NS}">"#);
    for (id, kind, target) in rows {
        xml.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{DOC_REL}/{kind}" Target="{target}"/>"#
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

pub(crate) const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
<w:p><w:r><w:t>Dear reader,</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">the figures </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>follow</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>
<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>
<w:tr><w:tc><w:p><w:r><w:t>Region</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Sales</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>North</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1 200</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>Regards</w:t></w:r></w:p>
<w:sectPr/></w:body></w:document>"#;

/// A Word document whose body is `document`, with the properties and `document_rels` as the
/// main part's relationships part.
pub(crate) fn docx_with(document: &str, document_rels: &str) -> Vec<u8> {
    let root = root_rels("word/document.xml");
    zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("docProps/core.xml", CORE.as_bytes()),
        ("docProps/app.xml", APP.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", document_rels.as_bytes()),
    ])
}

/// A Word document with two paragraphs, a table and a closing paragraph.
pub(crate) fn docx() -> Vec<u8> {
    docx_with(DOCUMENT, &rels(&[("rId1", "styles", "styles.xml")]))
}

const WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Summary" sheetId="2" r:id="rId2"/><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;

const SHARED: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="4" uniqueCount="4">
<si><t>Item</t></si><si><t>Price</t></si><si><t>Apples</t></si><si><t>Total</t></si></sst>"#;

const DATA_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:B3"/><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>1.5</v></c></row>
<row r="3"><c r="A3" t="inlineStr"><is><t>Pears</t></is></c><c r="B3"><v>2</v></c></row>
</sheetData></worksheet>"#;

const SUMMARY_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:B1"/><sheetData>
<row r="1"><c r="A1" t="s"><v>3</v></c><c r="B1"><f>SUM(Data!B2:B3)</f><v>3.5</v></c></row>
</sheetData></worksheet>"#;

/// A workbook of two sheets listed `Summary`, `Data` - the reverse of their part names - with
/// shared strings, an inline string and a formula with its cached value.
pub(crate) fn xlsx() -> Vec<u8> {
    let root = root_rels("xl/workbook.xml");
    let workbook_rels = rels(&[
        ("rId1", "worksheet", "worksheets/sheet1.xml"),
        ("rId2", "worksheet", "/xl/worksheets/sheet2.xml"),
        ("rId3", "sharedStrings", "sharedStrings.xml"),
    ]);
    zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("docProps/core.xml", CORE.as_bytes()),
        ("xl/workbook.xml", WORKBOOK.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/sharedStrings.xml", SHARED.as_bytes()),
        ("xl/worksheets/sheet1.xml", DATA_SHEET.as_bytes()),
        ("xl/worksheets/sheet2.xml", SUMMARY_SHEET.as_bytes()),
    ])
}

const PRESENTATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldIdLst><p:sldId id="256" r:id="rId4"/><p:sldId id="257" r:id="rId2"/><p:sldId id="258" r:id="rId3"/></p:sldIdLst>
</p:presentation>"#;

/// A slide whose title placeholder says `title` and whose body says `body`.
pub(crate) fn slide(title: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:txBody><a:bodyPr/><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:cNvPr id="3" name="Body"/><p:cNvSpPr/><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:txBody><a:bodyPr/><a:p><a:r><a:t>{body}</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#
    )
}

/// A presentation of three slides shown `slide3.xml`, `slide1.xml`, `slide2.xml`: the order of
/// `presentation.xml`, not of the part names.
pub(crate) fn pptx() -> Vec<u8> {
    let root = root_rels("ppt/presentation.xml");
    let presentation_rels = rels(&[
        ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
        ("rId2", "slide", "slides/slide1.xml"),
        ("rId3", "slide", "slides/slide2.xml"),
        ("rId4", "slide", "slides/slide3.xml"),
    ]);
    let (one, two, three) = (
        slide("Agenda", "Where we stand"),
        slide("Numbers", "Up by a third"),
        slide("Welcome", "Hello all"),
    );
    zip_of(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("ppt/presentation.xml", PRESENTATION.as_bytes()),
        (
            "ppt/_rels/presentation.xml.rels",
            presentation_rels.as_bytes(),
        ),
        ("ppt/slides/slide1.xml", one.as_bytes()),
        ("ppt/slides/slide2.xml", two.as_bytes()),
        ("ppt/slides/slide3.xml", three.as_bytes()),
    ])
}
