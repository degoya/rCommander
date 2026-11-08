use super::*;

fn owned(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

#[test]
fn a_typed_line_replaces_its_own_and_an_emptied_one_goes() {
    let lines = owned(&["*.bak", "Thumbs.db", ""]);
    assert_eq!(joined(&lines, 1, "desktop.ini"), "*.bak\ndesktop.ini");
    assert_eq!(
        joined(&lines, 2, "x"),
        "*.bak\nThumbs.db\nx",
        "the empty field adds one"
    );
    assert_eq!(joined(&lines, 0, ""), "Thumbs.db");
}
