use rc_vfs::VfsPath;

use super::{chosen, location, path_key, same_place};

#[test]
fn a_row_is_chosen_by_its_key_or_by_default() {
    let only = vec!["menu1".to_owned(), "left2".to_owned()];
    assert!(chosen("menu1", Some(&only), false));
    assert!(!chosen("menu3", Some(&only), true));
    assert!(chosen("menu3", None, true));
    assert!(!chosen("left0", None, false));
}

#[test]
fn places_compare_as_folders_and_names_without_case() {
    let stored = location("C:\\Work").expect("encodes");
    assert_eq!(
        serde_json::from_str::<VfsPath>(&stored).expect("decodes"),
        VfsPath::local("C:\\Work")
    );
    assert!(same_place(Some(&stored), Some("C:\\Work")));
    assert!(!same_place(Some(&stored), Some("C:\\Other")));
    assert!(!same_place(Some(&stored), None));
    assert!(same_place(None, None));
    assert_eq!(
        path_key(&["Projects".to_owned(), "RC".to_owned()]),
        vec!["projects".to_owned(), "rc".to_owned()]
    );
}
