use super::*;
use crate::Keymap;

#[test]
fn the_table_is_sorted_lower_case_and_unique() {
    let names: Vec<&str> = TC_NAMES.iter().map(|(tc, _)| *tc).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "sorted and once each, for the binary search");
    for name in names {
        assert!(name.starts_with("cm_"), "{name}");
        assert_eq!(name, name.to_ascii_lowercase(), "{name}");
    }
}

/// Every row the table names is one the standard keymap has: a binding or a window key.
#[test]
fn cm_names_map_to_rows() {
    let standard = Keymap::shared_standard();
    for (tc, row) in TC_NAMES {
        let known = standard
            .bindings()
            .iter()
            .any(|binding| binding.name == *row)
            || standard.window().iter().any(|key| key.name == *row);
        assert!(known, "{tc} maps to {row}, which the keymap does not have");
    }
    assert_eq!(row_of("cm_Copy"), Some("copy"));
    assert_eq!(row_of("CM_LIST"), Some("view_file"));
    assert_eq!(row_of(" cm_RenMov "), Some("move"));
    assert_eq!(row_of("cm_NoSuchCommand"), None);
    assert_eq!(row_of("copy"), None);
}
