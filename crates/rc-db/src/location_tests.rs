use super::*;

/// RC-100-95: every path that lives in the data folder follows it into the portable copy.
#[test]
fn the_portable_location_moves_every_derived_path() {
    let data = PathBuf::from("stick").join("rcommander").join("data");
    let portable = DataLocation::Portable(data.clone());
    assert!(portable.is_portable());
    assert_eq!(portable.dir(), data.as_path());
    let database = portable.database();
    assert_eq!(database, data.join("rcommander.db"));
    assert_eq!(portable.backups(), data.join("backups"));
    assert_eq!(portable.local_dir(), Some(data.clone()));
    assert!(crate::staged_restore_path(&database).starts_with(&data));
    assert!(crate::rejected_restore_path(&database).starts_with(&data));

    let cell = LocationCell::new();
    cell.set(portable.clone()).expect("first");
    let resolved = cell.resolve().expect("resolved");
    assert_eq!(resolved, portable);
    assert_eq!(resolved.database(), database);

    // The user's folder keeps its machine-local part apart (on Windows `%LOCALAPPDATA%`).
    if let Ok(user) = DataLocation::user() {
        assert!(!user.is_portable());
        assert_eq!(user.database(), user.dir().join(crate::DB_FILE));
        assert!(user.local_dir().is_some());
    }
}

/// RC-100-95: the location is decided once; a second decision changes nothing.
#[test]
fn location_is_set_once() {
    let cell = LocationCell::new();
    assert_eq!(cell.get(), None);
    let first = DataLocation::Portable(PathBuf::from("first"));
    let second = DataLocation::User(PathBuf::from("second"));
    assert_eq!(cell.set(first.clone()), Ok(()));
    assert_eq!(cell.set(second.clone()), Err(second));
    assert_eq!(cell.get(), Some(&first));
    assert_eq!(cell.resolve().expect("set"), first);
}

/// Nothing set: the user's folder, as before RC-100-95.
#[test]
fn an_unset_location_is_the_users_folder() {
    let cell = LocationCell::new();
    if let Ok(user) = DataLocation::user() {
        assert_eq!(cell.resolve().expect("user"), user);
    }
}
