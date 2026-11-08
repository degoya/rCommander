use std::collections::HashSet;

use super::*;

#[test]
fn every_row_of_the_table_reads() {
    let rows = TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .count();
    assert_eq!(all().len(), rows, "a row of symbols.tsv does not read");
    assert!(
        (300..=500).contains(&rows),
        "{rows} symbols; the job's collection is 300 to 500"
    );
}

#[test]
fn names_and_glyphs_are_unique() {
    let names: HashSet<&str> = all().iter().map(|symbol| symbol.name).collect();
    let glyphs: HashSet<char> = all().iter().map(|symbol| symbol.glyph).collect();
    assert_eq!(names.len(), all().len(), "a name twice");
    assert_eq!(glyphs.len(), all().len(), "a code point twice");
}

#[test]
fn every_category_has_symbols_and_the_order_is_by_category() {
    for category in Category::ALL {
        assert!(
            all().iter().any(|symbol| symbol.category == category),
            "{category:?} is empty"
        );
    }
    let order: Vec<Category> = all().iter().map(|symbol| symbol.category).collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|category| Category::ALL.iter().position(|each| each == category));
    assert_eq!(order, sorted, "the table is not in the categories' order");
}

#[test]
fn a_reference_names_a_symbol_by_its_name() {
    let folder = find("folder_open").expect("the collection has folder_open");
    assert_eq!(folder.reference(), "rc-symbol:folder_open");
    assert_eq!(name_of(" rc-symbol:folder_open "), Some("folder_open"));
    assert_eq!(of_path("rc-symbol:folder_open"), Some(folder));
    assert_eq!(
        position("folder_open").map(|at| all()[at].name),
        Some("folder_open")
    );
    assert!(is_reference("rc-symbol:gone_forever"));
    assert_eq!(of_path("rc-symbol:gone_forever"), None);
}

#[test]
fn a_file_is_no_reference() {
    for path in [
        "",
        "C:\\Tools\\app.exe",
        "%COMMANDER_PATH%\\WCMICONS.DLL",
        "/usr/share/icons/app.png",
        "symbol:folder",
    ] {
        assert!(!is_reference(path), "{path}");
        assert_eq!(of_path(path), None, "{path}");
        assert_eq!(check(path), Ok(()), "{path}");
    }
}

#[test]
fn saving_refuses_a_name_the_collection_does_not_have() {
    assert_eq!(check("rc-symbol:folder"), Ok(()));
    assert!(check("rc-symbol:no_such_symbol").is_err());
    assert!(check("rc-symbol:").is_err());
    assert!(check("rc-symbol:../folder").is_err());
}

#[test]
fn the_search_finds_by_name_and_keyword() {
    let words =
        |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_lowercase).collect() };
    let folder = find("folder_open").expect("listed");
    assert!(folder.matches(&words("folder")));
    assert!(folder.matches(&words("open fold")));
    assert!(folder.matches(&words("browse")), "a keyword");
    assert!(!folder.matches(&words("folder printer")));
    assert!(folder.matches(&[]), "no words match everything");
}
