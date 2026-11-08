use super::*;
use crate::test_support::Collect;

fn strings(xml: &str) -> SharedStrings {
    shared_strings(&mut Xml::new(xml.as_bytes(), 64)).expect("shared strings")
}

fn rows(xml: &str, strings: &SharedStrings, max_rows: u64) -> (String, Size) {
    let mut out = Collect::default();
    let size = write(
        &mut Xml::new(xml.as_bytes(), 64),
        strings,
        max_rows,
        &mut out,
    )
    .expect("rows");
    (out.0, size)
}

const STRINGS: &str = r#"<sst xmlns="urn:x" count="3" uniqueCount="3">
<si><t>Name</t></si>
<si><r><t>Ka</t></r><r><t>ren</t></r><rPh sb="0" eb="1"><t>KA</t></rPh></si>
<si/>
<si><t xml:space="preserve">a	tab</t></si>
</sst>"#;

#[test]
fn shared_strings_join_runs_and_leave_out_the_phonetic_guide() {
    let strings = strings(STRINGS);
    assert_eq!(strings.get(0), "Name");
    assert_eq!(strings.get(1), "Karen");
    assert_eq!(strings.get(2), "");
    assert_eq!(strings.get(3), "a\ttab");
    assert_eq!(strings.get(4), "");
}

#[test]
fn cells_show_strings_cached_results_inline_strings_and_booleans() {
    let sheet = r#"<worksheet xmlns="urn:x"><dimension ref="A1:D3"/><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1" t="inlineStr"><is><t>Inline</t></is></c></row>
<row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2"><f>1+2</f><v>3</v></c><c r="C2" t="b"><v>1</v></c>
<c r="D2" t="str"><f>"x"&amp;"y"</f><v>xy</v></c></row>
<row r="3" spans="1:4"/>
<row r="4"><c r="B4" t="s"><v>3</v></c><c r="C4" t="e"><v>#DIV/0!</v></c></row>
</sheetData></worksheet>"#;
    let (text, size) = rows(sheet, &strings(STRINGS), 100);
    assert_eq!(
        text,
        "Name\t\tInline\nKaren\t3\tTRUE\txy\n\ta tab\t#DIV/0!\n"
    );
    assert_eq!(
        size,
        Size {
            rows: 3,
            columns: 4
        }
    );
}

#[test]
fn rows_past_the_setting_are_counted_in_one_line() {
    let mut sheet = String::from("<worksheet><sheetData>");
    for row in 1..=7 {
        sheet.push_str(&format!("<row><c><v>{row}</v></c></row>"));
    }
    sheet.push_str("</sheetData></worksheet>");
    let (text, size) = rows(&sheet, &SharedStrings::default(), 3);
    assert_eq!(text, "1\n2\n3\n4\n[... 3 more rows]\n");
    assert_eq!(size.rows, 7);
}

#[test]
fn size_comes_from_the_dimension_or_is_counted() {
    let declared = r#"<worksheet><dimension ref="B2:E11"/><sheetData/></worksheet>"#;
    let size = size(&mut Xml::new(declared.as_bytes(), 64)).expect("size");
    assert_eq!(
        size,
        Some(Size {
            rows: 10,
            columns: 4
        })
    );
    let counted = r#"<worksheet><sheetData><row><c r="C1"><v>1</v></c></row>
<row><c><v>2</v></c></row></sheetData></worksheet>"#;
    let size = super::size(&mut Xml::new(counted.as_bytes(), 64)).expect("size");
    assert_eq!(
        size,
        Some(Size {
            rows: 2,
            columns: 3
        })
    );
    let empty = "<worksheet><sheetData/></worksheet>";
    assert_eq!(
        super::size(&mut Xml::new(empty.as_bytes(), 64)).expect("size"),
        None
    );
}

#[test]
fn column_references() {
    assert_eq!(column_of("A1"), Some(0));
    assert_eq!(column_of("Z9"), Some(25));
    assert_eq!(column_of("AA1"), Some(26));
    assert_eq!(column_of("XFD1"), Some(16_383));
    assert_eq!(column_of("XFE1"), None);
    assert_eq!(column_of("12"), None);
    assert_eq!(
        dimension("A1"),
        Some(Size {
            rows: 1,
            columns: 1
        })
    );
    assert_eq!(dimension("D20:A1"), None);
}
