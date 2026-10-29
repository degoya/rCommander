//! The check against a served stable manifest and release list: where each channel looks, what a
//! floor does, and what a bad document is reported as.

use super::*;
use crate::{
    MemoryFetcher,
    manifest::tests::{PREFIX, at, key, manifest, signed, trust_for},
    sign::SigningKey,
};

fn beta_url(tag: &str) -> String {
    format!("{PREFIX}{tag}/{}", Channel::Beta.file_name())
}

fn list_url(sources: &Sources) -> String {
    match &sources.beta {
        BetaSource::ReleaseList(url) | BetaSource::Fixed(url) => url.to_string(),
    }
}

fn release_list(entries: &[(&str, bool)]) -> Vec<u8> {
    let releases: Vec<serde_json::Value> = entries
        .iter()
        .map(|(tag, with_manifest)| {
            let mut assets = vec![serde_json::json!({
                "name": "rcommander-windows-x64.zip",
                "browser_download_url": format!("{PREFIX}{tag}/rcommander-windows-x64.zip"),
            })];
            if *with_manifest {
                assets.push(serde_json::json!({
                    "name": Channel::Beta.file_name(),
                    "browser_download_url": beta_url(tag),
                }));
            }
            serde_json::json!({ "tag_name": tag, "draft": false, "prerelease": true, "assets": assets })
        })
        .collect();
    serde_json::to_vec(&releases).expect("encode")
}

fn served() -> (MemoryFetcher, Sources) {
    let sources = Sources::official().expect("official sources");
    let fetcher = MemoryFetcher::new();
    fetcher.serve(
        sources.stable.as_str(),
        signed(&manifest(Channel::Stable, "1.8.0", 10)),
    );
    (fetcher, sources)
}

async fn run(fetcher: &MemoryFetcher, sources: &Sources, channel: Channel) -> CheckReport {
    check(
        fetcher,
        sources,
        &trust_for(&key()),
        channel,
        Floors::default(),
        at(1),
    )
    .await
}

#[tokio::test]
async fn the_stable_channel_reads_the_stable_manifest_only() {
    let (fetcher, sources) = served();
    let report = run(&fetcher, &sources, Channel::Stable).await;
    assert!(report.problem.is_none(), "{:?}", report.problem);
    assert_eq!(report.manifests.len(), 1);
    assert_eq!(report.floors.stable, Some(10));
    assert_eq!(fetcher.requests(), vec![sources.stable.to_string()]);
    assert_eq!(
        sources.stable.as_str(),
        "https://github.com/degoya/rCommander/releases/latest/download/rcommander-update-stable.json"
    );
}

#[tokio::test]
async fn the_beta_channel_finds_the_newest_beta_manifest_in_the_release_list() {
    let (fetcher, sources) = served();
    fetcher.serve(
        &list_url(&sources),
        release_list(&[
            ("v1.8.0-beta.1", true),
            ("v1.8.1-beta.2", true),
            ("v1.8.1-beta.3", false),
        ]),
    );
    fetcher.serve(
        &beta_url("v1.8.1-beta.2"),
        signed(&manifest(Channel::Beta, "1.8.1-beta.2", 20)),
    );
    let report = run(&fetcher, &sources, Channel::Beta).await;
    assert!(report.problem.is_none(), "{:?}", report.problem);
    let versions: Vec<&str> = report
        .manifests
        .iter()
        .map(|m| m.version.as_str())
        .collect();
    assert_eq!(versions, vec!["1.8.0", "1.8.1-beta.2"]);
    assert_eq!(
        report.floors,
        Floors {
            stable: Some(10),
            beta: Some(20)
        }
    );
}

/// A 404 is `update.not_published`, apart from a failure to reach the host.
#[tokio::test]
async fn a_missing_manifest_is_not_published() {
    let sources = Sources::official().expect("official sources");
    let report = run(&MemoryFetcher::new(), &sources, Channel::Stable).await;
    assert_eq!(
        report.problem.map(|problem| problem.code()),
        Some("update.not_published")
    );
}

/// An answer over 256 KiB is refused as too large before it is parsed.
#[tokio::test]
async fn a_manifest_over_the_limit_is_too_large() {
    let sources = Sources::official().expect("official sources");
    let fetcher = MemoryFetcher::new();
    fetcher.serve(sources.stable.as_str(), vec![b' '; MAX_MANIFEST_BYTES + 1]);
    let report = run(&fetcher, &sources, Channel::Stable).await;
    assert_eq!(
        report.problem.map(|problem| problem.code()),
        Some("update.too_large")
    );
}

#[tokio::test]
async fn a_manifest_below_its_floor_is_reported_and_the_floor_stays() {
    let (fetcher, sources) = served();
    let floors = Floors {
        stable: Some(11),
        beta: None,
    };
    let report = check(
        &fetcher,
        &sources,
        &trust_for(&key()),
        Channel::Stable,
        floors,
        at(1),
    )
    .await;
    assert!(report.manifests.is_empty());
    assert_eq!(report.floors, floors);
    assert_eq!(
        report.problem.map(|problem| problem.code()),
        Some("update.stale")
    );
}

/// Each channel keeps its own floor: a high beta floor does not refuse the stable manifest.
#[tokio::test]
async fn the_floors_are_kept_per_channel() {
    let (fetcher, sources) = served();
    let floors = Floors {
        stable: None,
        beta: Some(1_000),
    };
    let report = check(
        &fetcher,
        &sources,
        &trust_for(&key()),
        Channel::Stable,
        floors,
        at(1),
    )
    .await;
    assert_eq!(report.manifests.len(), 1);
    assert_eq!(report.floors.stable, Some(10));
    assert_eq!(report.floors.beta, Some(1_000));
}

#[tokio::test]
async fn a_forged_beta_manifest_is_reported_beside_a_good_stable_one() {
    let (fetcher, sources) = served();
    fetcher.serve(
        &list_url(&sources),
        release_list(&[("v1.9.0-beta.1", true)]),
    );
    let forged = crate::manifest::sign(
        "rcommander-update-v1",
        &SigningKey::from_bytes(&[1; 32]),
        &manifest(Channel::Beta, "1.9.0-beta.1", 30),
    )
    .expect("sign");
    fetcher.serve(&beta_url("v1.9.0-beta.1"), forged);
    let report = run(&fetcher, &sources, Channel::Beta).await;
    assert_eq!(report.manifests.len(), 1);
    assert_eq!(report.floors.beta, None);
    assert_eq!(
        report.problem.map(|problem| problem.code()),
        Some("update.bad_signature")
    );
}

#[test]
fn the_release_list_only_locates_assets_under_the_repository() {
    let list = serde_json::to_vec(&serde_json::json!([
        {
            "tag_name": "v9.0.0-beta.1",
            "assets": [{ "name": Channel::Beta.file_name(), "browser_download_url": "https://evil.example/rcommander-update-beta.json" }]
        },
        {
            "tag_name": "v1.8.0-beta.1",
            "draft": true,
            "assets": [{ "name": Channel::Beta.file_name(), "browser_download_url": beta_url("v1.8.0-beta.1") }]
        }
    ]))
    .expect("encode");
    assert_eq!(beta_manifest_url(&list, PREFIX).expect("parse"), None);
    assert!(beta_manifest_url(b"not json", PREFIX).is_err());
}

/// A signed manifest may name only the source's own downloads - also through `..`, its
/// percent-encoded spelling and `http:` - and a refused one raises no floor.
#[tokio::test]
async fn an_artifact_outside_the_download_prefix_refuses_the_whole_manifest() {
    for url in [
        "https://evil.example/rcommander-windows-x64.zip",
        "https://github.com/degoya/rCommander/releases/download/../../../evil/a.zip",
        "https://github.com/degoya/rCommander/releases/download/%2e%2e/%2E%2E/%2e%2e/evil/a.zip",
        "https://github.com/degoya/rCommander-fork/releases/download/v1.8.0/a.zip",
        "http://github.com/degoya/rCommander/releases/download/v1.8.0/a.zip",
    ] {
        let sources = Sources::official().expect("official sources");
        let fetcher = MemoryFetcher::new();
        let mut release = manifest(Channel::Stable, "1.8.0", 10);
        release.artifacts[0].url = url.to_owned();
        // Signed around `sign`'s content check, as a careless publisher could.
        let document = crate::sign::sign_document(
            manifest::UPDATE_MANIFEST_DOMAIN,
            "rcommander-update-v1",
            &key(),
            &release,
        )
        .expect("sign");
        fetcher.serve(
            sources.stable.as_str(),
            serde_json::to_vec(&document).expect("encode"),
        );
        let report = run(&fetcher, &sources, Channel::Stable).await;
        assert!(report.manifests.is_empty(), "{url}");
        assert_eq!(report.floors.stable, None, "{url}");
        assert_eq!(
            report.problem.map(|problem| problem.code()),
            Some("update.invalid"),
            "{url}"
        );
    }
    assert!(pinned(manifest(Channel::Stable, "1.8.0", 10), PREFIX).is_ok());
}

#[test]
fn a_mirror_serves_both_files_from_one_https_folder() {
    let sources = Sources::mirror("https://mirror.example/rcommander").expect("mirror");
    assert_eq!(
        sources.stable.as_str(),
        "https://mirror.example/rcommander/rcommander-update-stable.json"
    );
    assert_eq!(
        sources.download_prefix,
        "https://mirror.example/rcommander/"
    );
    assert!(Sources::mirror("http://mirror.example/").is_err());
    assert!(Sources::mirror("https://user:pass@mirror.example/").is_err());
    assert!(Sources::for_setting("").is_ok());
}

/// The check sends nothing that names the installation: the addresses carry no query but the
/// release list's page size, whatever the caller knows (the canary is never handed over).
#[tokio::test]
async fn a_check_asks_for_public_files_only() {
    let (fetcher, sources) = served();
    let _ = run(&fetcher, &sources, Channel::Beta).await;
    let canary = "installation-0f3c";
    for request in fetcher.requests() {
        assert!(!request.contains(canary), "{request}");
        let url = url::Url::parse(&request).expect("url");
        assert!(
            url.query().is_none_or(|query| query == "per_page=30"),
            "{request}"
        );
    }
}

#[test]
fn floors_only_rise() {
    let mut floors = Floors::default();
    floors.raise(Channel::Beta, 5);
    floors.raise(Channel::Beta, 3);
    assert_eq!(
        floors,
        Floors {
            stable: None,
            beta: Some(5)
        }
    );
}
