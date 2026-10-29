//! One update check (RC-100-05): fetch the manifests a channel reads, verify each against its
//! replay floor.
//!
//! **Where the manifests are.** The stable manifest is an asset of every plain `vX.Y.Z` release,
//! so `releases/latest/download/rcommander-update-stable.json` always names the newest one -
//! GitHub's `latest` never points at a pre-release. A beta has no fixed address, so the beta
//! channel reads the release list (`/repos/<repo>/releases`), takes the release of the highest
//! version that carries `rcommander-update-beta.json` and fetches that asset. The list is unsigned
//! and only *locates* the manifest: the asset must lie under the repository's own
//! `releases/download/`, and what is fetched verifies under the root or is refused. Beta reads
//! stable too. A mirror (`update.source`) serves both files from one https folder, and its
//! downloads must lie under it; it is checked against the same compiled-in key.
//!
//! **Floors.** Each channel has its own; the caller stores [`CheckReport::floors`] before it acts
//! on what verified.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/check.rs` at commit 26c8592a6:
//! rCommander's repository and names, the mirror, typed fetch refusals.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    Fetcher,
    error::UpdateError,
    fetch::{BodyTooLarge, ProxyAuthRequired},
    manifest::{self, Channel, MAX_MANIFEST_BYTES, UpdateManifest},
    offer::parse_version,
    sign::TrustStore,
};

/// The repository the official releases are published in (RC-100-30).
pub const OFFICIAL_REPOSITORY: &str = "degoya/rCommander";
/// Releases the beta channel reads per list.
pub(crate) const RELEASE_LIST_PAGE: u32 = 30;
/// Largest release list read.
pub const MAX_RELEASE_LIST_BYTES: u64 = 8 * 1024 * 1024;

/// Where the beta manifest is found.
#[derive(Clone, Debug)]
pub enum BetaSource {
    /// GitHub's release list, where the newest beta manifest is looked up.
    ReleaseList(url::Url),
    /// A fixed address: a mirror's.
    Fixed(url::Url),
}

/// Where one source publishes its manifests.
#[derive(Clone, Debug)]
pub struct Sources {
    /// The stable manifest.
    pub stable: url::Url,
    pub beta: BetaSource,
    /// Every asset address the list or a manifest may name starts with this.
    pub download_prefix: String,
}

impl Sources {
    /// The official repository's addresses.
    ///
    /// # Errors
    ///
    /// [`UpdateError::Other`] for an address that does not parse, which only a broken build meets.
    pub fn official() -> Result<Self, UpdateError> {
        let parse = |text: String| {
            url::Url::parse(&text)
                .map_err(|error| UpdateError::Other(anyhow::anyhow!("update address: {error}")))
        };
        Ok(Self {
            stable: parse(format!(
                "https://github.com/{OFFICIAL_REPOSITORY}/releases/latest/download/{}",
                Channel::Stable.file_name()
            ))?,
            beta: BetaSource::ReleaseList(parse(format!(
                "https://api.github.com/repos/{OFFICIAL_REPOSITORY}/releases?per_page={RELEASE_LIST_PAGE}"
            ))?),
            download_prefix: format!("https://github.com/{OFFICIAL_REPOSITORY}/releases/download/"),
        })
    }

    /// A mirror at `base`, an `https://` folder serving both manifests and the archives below it.
    ///
    /// # Errors
    ///
    /// `update.invalid` for anything but a plain https folder address.
    pub fn mirror(base: &str) -> Result<Self, UpdateError> {
        let mut folder = base.trim().to_owned();
        if !folder.ends_with('/') {
            folder.push('/');
        }
        let url = url::Url::parse(&folder)
            .map_err(|error| UpdateError::Invalid(format!("update source: {error}")))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(UpdateError::Invalid(
                "the update source is no plain https:// folder".to_owned(),
            ));
        }
        let file = |channel: Channel| {
            url.join(channel.file_name())
                .map_err(|error| UpdateError::Invalid(format!("update source: {error}")))
        };
        Ok(Self {
            stable: file(Channel::Stable)?,
            beta: BetaSource::Fixed(file(Channel::Beta)?),
            download_prefix: url.to_string(),
        })
    }

    /// The official sources for an empty `update.source`, a mirror otherwise.
    ///
    /// # Errors
    ///
    /// As [`Self::mirror`].
    pub fn for_setting(source: &str) -> Result<Self, UpdateError> {
        if source.trim().is_empty() {
            Self::official()
        } else {
            Self::mirror(source)
        }
    }
}

/// The highest manifest sequence accepted per channel.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Floors {
    #[serde(default)]
    pub stable: Option<u64>,
    #[serde(default)]
    pub beta: Option<u64>,
}

impl Floors {
    #[must_use]
    pub fn get(&self, channel: Channel) -> Option<u64> {
        match channel {
            Channel::Stable => self.stable,
            Channel::Beta => self.beta,
        }
    }

    /// Raises `channel`'s floor to `sequence`; never lowers it.
    pub fn raise(&mut self, channel: Channel, sequence: u64) {
        let floor = match channel {
            Channel::Stable => &mut self.stable,
            Channel::Beta => &mut self.beta,
        };
        *floor = Some(floor.map_or(sequence, |known| known.max(sequence)));
    }
}

/// What one check found.
#[derive(Debug)]
pub struct CheckReport {
    /// Every manifest that verified.
    pub manifests: Vec<UpdateManifest>,
    /// The floors after it, to store before anything acts on [`Self::manifests`].
    pub floors: Floors,
    /// What went wrong worth reporting: any refusal of a manifest's integrity, otherwise the
    /// first failure when nothing verified at all.
    pub problem: Option<UpdateError>,
}

/// Fetches and verifies the manifests `channel` reads.
pub async fn check(
    fetcher: &dyn Fetcher,
    sources: &Sources,
    trust: &TrustStore,
    channel: Channel,
    floors: Floors,
    now: DateTime<Utc>,
) -> CheckReport {
    let mut report = CheckReport {
        manifests: Vec::new(),
        floors,
        problem: None,
    };
    let mut failures = Vec::new();
    let stable = fetch_manifest(fetcher, &sources.stable).await;
    record(
        &mut report,
        &mut failures,
        stable,
        (trust, sources),
        Channel::Stable,
        now,
    );
    if channel == Channel::Beta {
        let beta = match locate_beta(fetcher, sources).await {
            Ok(url) => fetch_manifest(fetcher, &url).await,
            Err(error) => Err(error),
        };
        record(
            &mut report,
            &mut failures,
            beta,
            (trust, sources),
            Channel::Beta,
            now,
        );
    }
    let integrity = failures.iter().position(UpdateError::is_integrity);
    report.problem = match integrity {
        Some(index) => Some(failures.swap_remove(index)),
        None if report.manifests.is_empty() => failures.into_iter().next(),
        None => None,
    };
    report
}

fn record(
    report: &mut CheckReport,
    failures: &mut Vec<UpdateError>,
    fetched: Result<Vec<u8>, UpdateError>,
    (trust, sources): (&TrustStore, &Sources),
    channel: Channel,
    now: DateTime<Utc>,
) {
    let verified = fetched
        .and_then(|bytes| {
            manifest::verify_with(&bytes, trust, channel, report.floors.get(channel), now)
        })
        .and_then(|manifest| pinned(manifest, &sources.download_prefix));
    match verified {
        Ok(manifest) => {
            report.floors.raise(channel, manifest.sequence);
            report.manifests.push(manifest);
        }
        Err(error) => {
            tracing::warn!(channel = channel.as_str(), code = error.code(), %error, "update manifest refused");
            failures.push(error);
        }
    }
}

/// Refuses a verified manifest with an artifact outside the source's own downloads: the
/// signature covers every address, and the pin keeps even a signed manifest from sending an
/// installation elsewhere. Compared in the parsed form the download requests, so `..` and its
/// percent-encoded spelling leave nothing out.
///
/// # Errors
///
/// `update.invalid` naming the artifact.
pub fn pinned(
    manifest: UpdateManifest,
    download_prefix: &str,
) -> Result<UpdateManifest, UpdateError> {
    let outside = manifest.artifacts.iter().find(|artifact| {
        !url::Url::parse(&artifact.url).is_ok_and(|url| url.as_str().starts_with(download_prefix))
    });
    match outside {
        Some(artifact) => Err(UpdateError::Invalid(format!(
            "the artifact {} is not under {download_prefix}",
            artifact.url
        ))),
        None => Ok(manifest),
    }
}

/// The fetcher's refusal as the check reports it.
fn fetch_error(error: &anyhow::Error) -> UpdateError {
    if error.downcast_ref::<ProxyAuthRequired>().is_some() {
        UpdateError::ProxyAuthRequired
    } else if error.downcast_ref::<BodyTooLarge>().is_some() {
        UpdateError::TooLarge
    } else {
        UpdateError::Fetch(format!("{error:#}"))
    }
}

async fn fetch_manifest(fetcher: &dyn Fetcher, url: &url::Url) -> Result<Vec<u8>, UpdateError> {
    match fetcher.fetch(url, MAX_MANIFEST_BYTES as u64).await {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(UpdateError::NotPublished),
        Err(error) => Err(fetch_error(&error)),
    }
}

async fn locate_beta(fetcher: &dyn Fetcher, sources: &Sources) -> Result<url::Url, UpdateError> {
    let list_url = match &sources.beta {
        BetaSource::Fixed(url) => return Ok(url.clone()),
        BetaSource::ReleaseList(url) => url,
    };
    let list = match fetcher.fetch(list_url, MAX_RELEASE_LIST_BYTES).await {
        Ok(Some(list)) => list,
        Ok(None) => return Err(UpdateError::NotPublished),
        Err(error) => return Err(fetch_error(&error)),
    };
    beta_manifest_url(&list, &sources.download_prefix)?.ok_or(UpdateError::NotPublished)
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The beta manifest of the highest-versioned release in the release list, if any carries one
/// under `download_prefix`.
pub(crate) fn beta_manifest_url(
    list: &[u8],
    download_prefix: &str,
) -> Result<Option<url::Url>, UpdateError> {
    let releases: Vec<Release> = serde_json::from_slice(list)
        .map_err(|error| UpdateError::Fetch(format!("the release list cannot be read: {error}")))?;
    let newest = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = parse_version(&release.tag_name)?;
            let asset = release.assets.into_iter().find(|asset| {
                asset.name == Channel::Beta.file_name()
                    && asset.browser_download_url.starts_with(download_prefix)
            })?;
            let url = url::Url::parse(&asset.browser_download_url).ok()?;
            (url.scheme() == "https" && url.as_str().starts_with(download_prefix))
                .then_some((version, url))
        })
        .max_by(|(left, _), (right, _)| {
            (left.major, left.minor, left.patch, &left.pre).cmp(&(
                right.major,
                right.minor,
                right.patch,
                &right.pre,
            ))
        });
    Ok(newest.map(|(_, url)| url))
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
