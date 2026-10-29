use super::*;

#[test]
fn a_generated_key_reads_back_and_is_never_overwritten() {
    let folder = tempfile::tempdir().expect("tempdir");
    let public = generate(folder.path(), Role::Update).expect("keygen");
    let (private_file, public_file) = key_files(folder.path(), Role::Update);
    let key = crate::build::read_key_file(&private_file).expect("read the key");
    assert_eq!(STANDARD.encode(key.verifying_key().as_bytes()), public);
    assert_eq!(
        std::fs::read_to_string(&public_file)
            .expect("public")
            .trim(),
        public
    );
    assert!(
        generate(folder.path(), Role::Update).is_err(),
        "never overwritten"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&private_file)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn each_role_has_its_own_files_and_id() {
    let folder = tempfile::tempdir().expect("tempdir");
    generate(folder.path(), Role::Update).expect("update");
    generate(folder.path(), Role::Plugin).expect("plugin");
    assert_ne!(Role::Update.key_id(), Role::Plugin.key_id());
    assert_eq!(Role::Update.key_id(), "rcommander-update-v1");
}
