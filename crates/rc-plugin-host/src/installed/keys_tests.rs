use super::InstalledKeys;

fn installed() -> InstalledKeys {
    InstalledKeys::from_entries([
        ("rc.csv".to_owned(), "SHA256:old".to_owned()),
        ("rc.md".to_owned(), String::new()),
    ])
}

#[test]
fn the_same_key_or_a_new_plugin_is_no_change() {
    let keys = installed();
    assert!(!keys.changed("rc.csv", Some("SHA256:old")));
    assert!(!keys.changed("rc.new", Some("SHA256:any")));
    assert!(!keys.changed("rc.new", None));
}

#[test]
fn another_key_for_an_installed_plugin_is_a_change() {
    let keys = installed();
    assert!(keys.changed("rc.csv", Some("SHA256:new")));
    assert!(keys.changed("rc.csv", None));
}

/// A version whose key did not read matches no key: its plugin is removed before an update.
#[test]
fn an_unreadable_installed_key_matches_none() {
    let keys = installed();
    assert!(keys.changed("rc.md", Some("SHA256:old")));
    assert!(keys.changed("rc.md", Some("")));
}

#[test]
fn every_version_counts() {
    let keys = InstalledKeys::from_entries([
        ("rc.csv".to_owned(), "SHA256:a".to_owned()),
        ("rc.csv".to_owned(), "SHA256:b".to_owned()),
    ]);
    assert!(keys.changed("rc.csv", Some("SHA256:a")));
    assert!(keys.changed("rc.csv", Some("SHA256:b")));
}

#[test]
fn an_empty_folder_holds_no_keys() {
    let data = tempfile::tempdir().expect("temp");
    let keys = InstalledKeys::read(data.path()).expect("read");
    assert_eq!(keys, InstalledKeys::default());
}
