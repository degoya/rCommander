use serde_json::json;

use super::*;

fn code(result: Result<ColumnSetSpec, ColumnSetError>) -> &'static str {
    result.expect_err("refused").code()
}

#[test]
fn a_spec_round_trips_through_its_json() {
    let mut spec = ColumnSetSpec::of(&[Field::Name, Field::Created, Field::Comment]);
    spec.columns[1].title = Some("Made".into());
    spec.columns[1].align = Align::Right;
    spec.masks = vec![r"D:\Fotos\*".into()];
    let text = spec.to_json().expect("fits");
    assert_eq!(ColumnSetSpec::parse(&text).expect("reads"), spec);
    assert!(spec.shows(Field::Comment));
    assert!(!spec.shows(Field::Size));
}

#[test]
fn every_field_reads_by_its_key_and_the_comments_view_is_fixed() {
    for field in Field::ALL {
        let value = serde_json::to_value(field).expect("serializes");
        assert_eq!(value, field.key(), "{field:?}");
        let column = Column::of(field);
        assert!((MIN_WIDTH..=MAX_WIDTH).contains(&column.width), "{field:?}");
    }
    let comments = ColumnSetSpec::comments();
    let fields: Vec<Field> = comments.columns.iter().map(|column| column.field).collect();
    assert_eq!(fields, [Field::Name, Field::Ext, Field::Comment]);
    assert_eq!(Field::Size.default_align(), Align::Right);
    assert_eq!(Field::Created.sort_key(), Some(crate::SortKey::Created));
    assert_eq!(Field::Rights.sort_key(), None);
}

#[test]
fn another_version_is_refused_before_its_fields_are_read() {
    let value = json!({ "version": 2, "columns": "anything" });
    assert_eq!(code(ColumnSetSpec::from_value(value)), "column_set_version");
    let value = json!({ "columns": [] });
    assert_eq!(code(ColumnSetSpec::from_value(value)), "column_set_invalid");
}

#[test]
fn the_limits_hold_at_the_boundary() {
    let column = |field: &str, width: u64| json!({ "field": field, "width": width });
    let spec = |columns: Vec<serde_json::Value>, masks: Vec<String>| json!({ "version": 1, "columns": columns, "masks": masks });
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(vec![], vec![]))),
        "column_set_invalid",
        "no column"
    );
    let many = (0..=MAX_COLUMNS).map(|_| column("name", 100)).collect();
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(many, vec![]))),
        "column_set_invalid"
    );
    for width in [23, 641] {
        let narrow = vec![column("size", width)];
        assert_eq!(
            code(ColumnSetSpec::from_value(spec(narrow, vec![]))),
            "column_set_invalid",
            "{width}"
        );
    }
    let unknown = vec![column("no_such_field", 100)];
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(unknown, vec![]))),
        "column_set_invalid"
    );
    let masks = (0..=MAX_MASKS).map(|n| format!("/m{n}/*")).collect();
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(
            vec![column("name", 100)],
            masks
        ))),
        "column_set_invalid"
    );
    for mask in ["", "a\nb", &"x".repeat(MAX_MASK_CHARS + 1)] {
        let masks = vec![mask.to_owned()];
        assert_eq!(
            code(ColumnSetSpec::from_value(spec(
                vec![column("name", 100)],
                masks
            ))),
            "column_set_invalid"
        );
    }
    let titled = vec![json!({ "field": "name", "width": 100, "title": "x".repeat(65) })];
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(titled, vec![]))),
        "column_set_invalid"
    );
    let extra = vec![json!({ "field": "name", "width": 100, "colour": "red" })];
    assert_eq!(
        code(ColumnSetSpec::from_value(spec(extra, vec![]))),
        "column_set_invalid"
    );
    let fits = (0..MAX_COLUMNS).map(|_| column("modified", 640)).collect();
    let masks = (0..MAX_MASKS).map(|n| format!("/m{n}/*")).collect();
    assert!(ColumnSetSpec::from_value(spec(fits, masks)).is_ok());
}

#[test]
fn a_spec_larger_than_16_kib_is_refused() {
    let text = format!(
        "{{\"version\":1,\"pad\":\"{}\"}}",
        "x".repeat(MAX_SPEC_BYTES)
    );
    assert_eq!(code(ColumnSetSpec::parse(&text)), "column_set_too_large");
}

#[test]
fn a_name_is_trimmed_and_bounded() {
    assert_eq!(clean_name("  Photos ").expect("fits"), "Photos");
    for name in ["", "   ", "a\tb", &"x".repeat(MAX_NAME_CHARS + 1)] {
        assert_eq!(
            clean_name(name).expect_err("refused").code(),
            "column_set_name"
        );
    }
    assert!(clean_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
}
