use super::*;
use crate::keygen::{Role, generate, key_files};

#[test]
fn the_package_names_are_classified_and_others_left_out() {
    assert_eq!(
        classify("rcommander-1.2.0-windows-x64.zip", "1.2.0"),
        Some(("windows", "x86_64", kind::ZIP))
    );
    assert_eq!(
        classify("rcommander-1.2.0-linux-arm64.tar.gz", "1.2.0"),
        Some(("linux", "aarch64", kind::TAR_GZ))
    );
    assert_eq!(
        classify("rcommander-1.2.0-macos-aarch64.zip", "1.2.0"),
        Some(("macos", "aarch64", kind::ZIP))
    );
    for other in [
        "rcommander-1.1.0-windows-x64.zip",
        "rcommander-1.2.0-windows-x64.tar.gz",
        "rcommander-1.2.0-linux-x64.zip",
        "SHA256SUMS",
        "rcommander-update-stable.json",
    ] {
        assert_eq!(classify(other, "1.2.0"), None, "{other}");
    }
}

/// Build, then read back under the key: what `verify --public-key` does before the root is in.
#[test]
fn a_built_manifest_verifies_under_its_key_and_lists_the_archives() {
    let folder = tempfile::tempdir().expect("tempdir");
    let assets = folder.path().join("assets");
    std::fs::create_dir_all(&assets).expect("assets");
    std::fs::write(
        assets.join("rcommander-1.2.0-windows-x64.zip"),
        b"zip bytes",
    )
    .expect("zip");
    std::fs::write(
        assets.join("rcommander-1.2.0-linux-x64.tar.gz"),
        b"tar bytes",
    )
    .expect("tar");
    std::fs::write(assets.join("SHA256SUMS"), b"ignored").expect("sums");
    let notes = folder.path().join("RELEASE-NOTES.md");
    std::fs::write(&notes, "## 1.2.0\n\n- New.\n").expect("notes");
    let public = generate(folder.path(), Role::Update).expect("keygen");
    let (private, _) = key_files(folder.path(), Role::Update);
    let out = folder
        .path()
        .join("out")
        .join("rcommander-update-stable.json");
    let args = BuildArgs {
        channel: "stable".to_owned(),
        version: "v1.2.0".to_owned(),
        assets: assets.clone(),
        base_url: "https://github.com/degoya/rCommander/releases/download/v1.2.0/".to_owned(),
        key: KeyArgs { key: Some(private) },
        out: out.clone(),
        notes: Some(notes),
        schema_change: Some(false),
        sequence: Some(7),
        valid_days: 30,
    };
    // The test key stands in for the compiled-in root.
    let root: &'static str = Box::leak(public.clone().into_boxed_str());
    let roots = [rc_update::sign::EmbeddedKey {
        key_id: UPDATE_KEY_ID,
        public_key: root,
        not_after: None,
    }];
    build_under(&args, &roots).expect("build");
    let trust = TrustStore::new();
    trust
        .trust_base64(UPDATE_KEY_ID.to_owned(), &public)
        .expect("trust");
    let bytes = std::fs::read(&out).expect("manifest");
    let read =
        manifest::verify_with(&bytes, &trust, Channel::Stable, None, Utc::now()).expect("verifies");
    assert_eq!(read.version, "1.2.0");
    assert_eq!(read.sequence, 7);
    assert_eq!(read.notes, "- New.");
    assert_eq!(read.artifacts.len(), 2);
    assert_eq!(read.schema_change, Some(false));
    crate::verify::check_assets(&read, &assets).expect("archives match");
    std::fs::write(assets.join("rcommander-1.2.0-windows-x64.zip"), b"changed").expect("change");
    assert!(crate::verify::check_assets(&read, &assets).is_err());
}

#[test]
fn a_version_on_the_wrong_channel_or_a_plain_http_base_is_refused() {
    let folder = tempfile::tempdir().expect("tempdir");
    generate(folder.path(), Role::Update).expect("keygen");
    let (private, _) = key_files(folder.path(), Role::Update);
    let args = |channel: &str, version: &str, base: &str| BuildArgs {
        channel: channel.to_owned(),
        version: version.to_owned(),
        assets: folder.path().to_owned(),
        base_url: base.to_owned(),
        key: KeyArgs {
            key: Some(private.clone()),
        },
        out: folder.path().join("never.json"),
        notes: None,
        schema_change: None,
        sequence: None,
        valid_days: 30,
    };
    let https = "https://example.test/";
    assert!(build(&args("stable", "1.2.0-beta.1", https)).is_err());
    assert!(build(&args("beta", "1.2.0", https)).is_err());
    assert!(build(&args("stable", "1.2.0", "http://example.test/")).is_err());
    assert!(!folder.path().join("never.json").exists());
}
