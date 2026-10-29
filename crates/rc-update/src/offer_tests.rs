use super::*;
use crate::manifest::tests::{artifact, manifest};

const WINDOWS: Target = Target {
    platform: "windows",
    arch: "x86_64",
    kind: kind::ZIP,
};

#[test]
fn a_newer_stable_release_is_offered_with_this_platforms_artifact() {
    let offer = newest_offer(
        "1.7.0",
        Channel::Stable,
        &[manifest(Channel::Stable, "1.8.0", 1)],
        &WINDOWS,
    )
    .expect("offer");
    assert_eq!(offer.version, "1.8.0");
    assert_eq!(
        offer.artifact,
        Some(artifact("windows", "x86_64", kind::ZIP))
    );
    assert!(offer.schema_change, "unsaid reads as a change");
}

/// No downgrade, and not the version already running.
#[test]
fn the_same_or_an_older_version_is_never_offered() {
    for current in ["1.8.0", "1.9.0", "1.8.1-beta.1"] {
        assert_eq!(
            newest_offer(
                current,
                Channel::Stable,
                &[manifest(Channel::Stable, "1.8.0", 1)],
                &WINDOWS
            ),
            None,
            "{current}"
        );
    }
}

#[test]
fn the_stable_channel_never_offers_a_beta() {
    let manifests = [
        manifest(Channel::Stable, "1.7.1", 1),
        manifest(Channel::Beta, "1.8.0-beta.2", 2),
    ];
    let offer = newest_offer("1.7.0", Channel::Stable, &manifests, &WINDOWS).expect("offer");
    assert_eq!(offer.version, "1.7.1");
}

/// Beta sees stable: `1.8.0-beta.2` is offered the `1.8.0` that follows it.
#[test]
fn the_beta_channel_offers_the_newest_of_both_lines() {
    let manifests = [
        manifest(Channel::Stable, "1.7.1", 1),
        manifest(Channel::Beta, "1.8.0-beta.2", 2),
    ];
    let offer = newest_offer("1.7.0", Channel::Beta, &manifests, &WINDOWS).expect("offer");
    assert_eq!(offer.version, "1.8.0-beta.2");
    let manifests = [
        manifest(Channel::Stable, "1.8.0", 3),
        manifest(Channel::Beta, "1.8.0-beta.2", 2),
    ];
    let offer = newest_offer("1.8.0-beta.2", Channel::Beta, &manifests, &WINDOWS).expect("offer");
    assert_eq!(offer.version, "1.8.0");
}

/// Back on stable, a beta installation waits for the release rather than going back.
#[test]
fn leaving_the_beta_channel_does_not_downgrade() {
    let manifests = [manifest(Channel::Stable, "1.7.1", 1)];
    assert_eq!(
        newest_offer("1.8.0-beta.2", Channel::Stable, &manifests, &WINDOWS),
        None
    );
}

#[test]
fn a_platform_the_release_lacks_is_offered_without_an_artifact() {
    let target = Target {
        platform: "macos",
        arch: "aarch64",
        kind: kind::ZIP,
    };
    let offer = newest_offer(
        "1.7.0",
        Channel::Stable,
        &[manifest(Channel::Stable, "1.8.0", 1)],
        &target,
    )
    .expect("offer");
    assert_eq!(offer.artifact, None);
}

#[test]
fn the_running_platform_takes_its_archive_kind() {
    let current = Target::current();
    let expected = if current.platform == "linux" {
        kind::TAR_GZ
    } else {
        kind::ZIP
    };
    assert_eq!(current.kind, expected);
}

#[test]
fn versions_compare_by_semver_precedence() {
    assert!(is_newer("1.10.0", "1.9.0"));
    assert!(is_newer("v1.8.0", "1.8.0-beta.2"));
    assert!(is_newer("1.8.0-beta.10", "1.8.0-beta.2"));
    assert!(!is_newer("1.8.0+build.5", "1.8.0"));
    assert!(!is_newer("garbage", "1.8.0"));
}
