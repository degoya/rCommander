use super::{AboutLinkKind, NAME, VERSION, about, author, known};

#[test]
fn the_facts_come_from_the_manifest() {
    let answer = about();
    assert_eq!(answer.name, "rCommander");
    assert_eq!(answer.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(answer.license, "GPL-3.0-or-later");
    assert_eq!(answer.authors, ["Alexander Herling / DEGOYA.digital"]);
    assert_eq!((NAME, VERSION), ("rCommander", env!("CARGO_PKG_VERSION")));
    assert_eq!(author(), "Alexander Herling");
    assert!(
        answer.platform.starts_with(std::env::consts::OS),
        "{}",
        answer.platform
    );
}

#[test]
fn every_address_is_public_on_github() {
    let answer = about();
    let kinds: Vec<_> = answer.links.iter().map(|link| link.kind).collect();
    assert_eq!(
        kinds,
        [
            AboutLinkKind::Source,
            AboutLinkKind::Handbook,
            AboutLinkKind::Changelog
        ]
    );
    for link in &answer.links {
        assert!(link.published, "{:?}", link.kind);
    }
    let urls: Vec<_> = answer
        .links
        .iter()
        .map(|link| link.url.as_deref())
        .collect();
    assert_eq!(
        urls,
        [
            Some("https://github.com/degoya/rCommander"),
            Some("https://github.com/degoya/rCommander/wiki"),
            Some("https://github.com/degoya/rCommander/blob/main/CHANGELOG.md"),
        ]
    );
}

#[test]
fn unknown_is_no_value() {
    assert_eq!(known("unknown"), None);
    assert_eq!(known(" "), None);
    assert_eq!(known("1a2b3c4d-dirty").as_deref(), Some("1a2b3c4d-dirty"));
    assert_eq!(
        known("2026-10-18T12:00:00Z").as_deref(),
        Some("2026-10-18T12:00:00Z")
    );
}
