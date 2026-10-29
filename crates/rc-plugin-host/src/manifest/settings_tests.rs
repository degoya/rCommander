use super::resolve_settings;
use crate::manifest::{SettingKind, SettingManifest};

fn choice(key: &str, values: &[&str], default: Option<&str>) -> SettingManifest {
    SettingManifest {
        key: key.to_owned(),
        kind: SettingKind::Choice,
        values: values.iter().map(|value| (*value).to_owned()).collect(),
        multiline: false,
        default: default.map(str::to_owned),
        masked: false,
    }
}

fn text(key: &str) -> SettingManifest {
    SettingManifest {
        key: key.to_owned(),
        kind: SettingKind::Text,
        values: Vec::new(),
        multiline: false,
        default: None,
        masked: false,
    }
}

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

/// The person's value, else the default; nothing for a setting with neither, nothing the
/// manifest does not declare (RC-100-14).
#[test]
fn a_plugin_reads_its_declared_settings_only() {
    let declared = [
        choice("format", &["as_is", "pretty"], Some("as_is")),
        choice("theme", &["light", "dark"], Some("light")),
        text("label"),
    ];
    let stored = pairs(&[("theme", "dark"), ("secret", "x"), ("other", "y")]);
    assert_eq!(
        resolve_settings(&declared, &stored),
        pairs(&[("format", "as_is"), ("theme", "dark")])
    );
}

/// A stored value the manifest no longer accepts - a choice an update dropped - is the default.
#[test]
fn a_value_the_manifest_no_longer_accepts_is_the_default() {
    let declared = [choice("format", &["as_is", "pretty"], Some("as_is"))];
    assert_eq!(
        resolve_settings(&declared, &pairs(&[("format", "compact")])),
        pairs(&[("format", "as_is")])
    );
    assert_eq!(
        resolve_settings(&declared, &pairs(&[("format", "pretty")])),
        pairs(&[("format", "pretty")])
    );
}

/// A masked setting is text: a licence key, typed hidden (RC-100-36).
#[test]
fn only_text_is_masked() {
    let masked = SettingManifest {
        masked: true,
        ..text("licence_key")
    };
    super::validate_settings(&[masked]).expect("masked text");
    let choice = SettingManifest {
        masked: true,
        ..choice("theme", &["dark", "light"], None)
    };
    assert!(super::validate_settings(&[choice]).is_err());
}
