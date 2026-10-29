use super::*;

const TEXT: &str = "# Release notes\n\nIntro.\n\n<!--\nHow a section is written.\n-->\n\n\
## 1.2.0\n\n- Updates install themselves.\n- A long point\n  wrapped onto a second line.\n\n\
## 1.1.0\n\n- Older.\n";

#[test]
fn the_section_of_the_version_becomes_its_points() {
    assert_eq!(
        user_notes(TEXT, "1.2.0").expect("notes").as_deref(),
        Some("- Updates install themselves.\n- A long point wrapped onto a second line.")
    );
    assert_eq!(
        user_notes(TEXT, "1.1.0").expect("notes").as_deref(),
        Some("- Older.")
    );
    assert_eq!(user_notes(TEXT, "9.9.9").expect("notes"), None);
}

#[test]
fn notes_longer_than_a_manifest_carries_are_refused() {
    let long = format!("## 1.0.0\n\n- {}\n", "x".repeat(MAX_NOTES_CHARS));
    assert!(user_notes(&long, "1.0.0").is_err());
}
