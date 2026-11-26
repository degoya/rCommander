use super::*;

#[test]
fn the_context_follows_the_roots_and_the_trust_only() {
    let state = PluginState::default();
    let base = context("roots", &state);
    assert_ne!(base, context("other roots", &state));

    let mut withdrawn = PluginState::default();
    withdrawn.revoked_keys.insert("fingerprint".to_owned());
    assert_ne!(base, context("roots", &withdrawn));
    let mut trusted = PluginState::default();
    trusted
        .trusted_keys
        .insert("key".to_owned(), "public".to_owned());
    assert_ne!(base, context("roots", &trusted));
    let development = PluginState {
        development_mode: true,
        ..PluginState::default()
    };
    assert_ne!(base, context("roots", &development));

    // A switch is no trust: the record holds.
    let mut switched_off = PluginState::default();
    switched_off.disabled.insert("plugin".to_owned());
    assert_eq!(base, context("roots", &switched_off));
}

#[test]
fn a_record_under_other_trust_or_that_does_not_read_is_none() {
    let data = tempfile::tempdir().expect("temp dir");
    let file = Path::new("/opt/rcommander/plugins/csv.rcplug");
    let mut seen = Seen::new("first".to_owned());
    seen.keep(
        file,
        Judged {
            stamp: FileStamp {
                size: 1,
                secs: 2,
                nanos: 3,
            },
            id: "csv".to_owned(),
            version: "1.0.0".to_owned(),
            digest: "00".to_owned(),
        },
    );
    seen.write(data.path()).expect("write");
    assert!(Seen::read(data.path(), "first").holds(file));
    assert!(!Seen::read(data.path(), "second").holds(file));

    std::fs::write(plugins_root(data.path()).join(SEEN_FILE), b"{ broken").expect("write");
    assert!(!Seen::read(data.path(), "first").holds(file));
}

#[test]
fn the_roots_stamp_names_every_root_this_build_trusts() {
    let stamp = roots_stamp();
    for root in root_keys() {
        assert!(stamp.contains(root.public_key), "{}", root.key_id);
    }
}
