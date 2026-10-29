//! What the About page shows and `app.about` answers (RC-010-19): which build runs, where it
//! comes from, who made it, under which licence.
//!
//! Nothing here is kept twice: the version, the authors, the licence and the repository come
//! from `Cargo.toml`; the commit and the build time are what `rcommander`'s `build.rs` compiled
//! into the binary, handed over once at the start with [`stamp_build`] - this crate has no build
//! script of its own, and the window and `rcommander mcp` both run through the binary's `main`.
//! The list of dependency licences is the window's (`app/settings/about/licenses.rs`): MCP has no
//! use for it.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Whether each address leads anywhere yet, decided per address and only once it does: an
/// address a reader cannot reach is shown as text marked "not yet published", never as a link.
/// The source, the handbook and the changelog are public on GitHub from the first public
/// release (RC-100-30), which is the first build that carries these.
const SOURCE_PUBLISHED: bool = true;
const HANDBOOK_PUBLISHED: bool = true;
const CHANGELOG_PUBLISHED: bool = true;

/// The handbook is the public repository's GitHub wiki (`scripts/export-wiki.sh`); the
/// changelog is `CHANGELOG.md` on its `main`.
const HANDBOOK_URL: Option<&str> = Some("https://github.com/degoya/rCommander/wiki");
const CHANGELOG_URL: Option<&str> =
    Some("https://github.com/degoya/rCommander/blob/main/CHANGELOG.md");

/// The program's name.
pub const NAME: &str = "rCommander";

/// The version: `[workspace.package] version` in `Cargo.toml`, the one place it is kept
/// (`scripts/set-version.sh`). Every crate inherits it, so this is the binary's too.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit and the build time the binary was built with.
static BUILD: OnceLock<(String, String)> = OnceLock::new();

/// What `app.about` answers and the About page shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    pub name: String,
    pub version: String,
    /// Eight characters of the commit, `-dirty` when the tree differed from it, or the form a
    /// package build gives it. `None` when the binary was built without git.
    pub commit: Option<String>,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`. `None` when unknown.
    pub built: Option<String>,
    /// Operating system and architecture (`windows x86_64`).
    pub platform: String,
    /// rCommander's licence, as an SPDX expression.
    pub license: String,
    pub authors: Vec<String>,
    pub links: Vec<AboutLink>,
}

/// One address the page offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutLink {
    pub kind: AboutLinkKind,
    /// `None` where the address is not decided yet: the page then shows the mark alone.
    pub url: Option<String>,
    /// False until the address answers a reader: the page shows it as text, marked "not yet
    /// published".
    pub published: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AboutLinkKind {
    Source,
    /// The wiki.
    Handbook,
    Changelog,
}

/// Hands the binary's build stamp over (`RC_BUILD_COMMIT`, `RC_BUILD_TIME`); the first call
/// counts, a later one changes nothing.
pub fn stamp_build(commit: &str, built: &str) {
    let _ = BUILD.set((commit.to_owned(), built.to_owned()));
}

/// The authors of `Cargo.toml`, each as written there (`Name / Company`).
fn authors() -> impl Iterator<Item = &'static str> {
    env!("CARGO_PKG_AUTHORS")
        .split(':')
        .filter(|author| !author.is_empty())
}

/// The person of the first author, without the company after ` / ` - what the window title
/// names.
#[must_use]
pub fn author() -> &'static str {
    authors().next().map_or("", |author| {
        author.split(" / ").next().unwrap_or(author).trim()
    })
}

/// The About page's facts for this build.
#[must_use]
pub fn about() -> About {
    let (commit, built) = BUILD.get().map_or((None, None), |(commit, built)| {
        (known(commit), known(built))
    });
    let link = |kind, url: Option<String>, published: bool| AboutLink {
        kind,
        // Published without an address is not a state a reader can follow.
        published: published && url.is_some(),
        url,
    };
    About {
        name: NAME.to_owned(),
        version: VERSION.to_owned(),
        commit,
        built,
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        license: env!("CARGO_PKG_LICENSE").to_owned(),
        authors: authors().map(str::to_owned).collect(),
        links: vec![
            link(
                AboutLinkKind::Source,
                Some(env!("CARGO_PKG_REPOSITORY").to_owned()).filter(|url| !url.is_empty()),
                SOURCE_PUBLISHED,
            ),
            link(
                AboutLinkKind::Handbook,
                HANDBOOK_URL.map(str::to_owned),
                HANDBOOK_PUBLISHED,
            ),
            link(
                AboutLinkKind::Changelog,
                CHANGELOG_URL.map(str::to_owned),
                CHANGELOG_PUBLISHED,
            ),
        ],
    }
}

/// `value`, or `None` for what `build.rs` writes when it does not know (`unknown`) or nothing.
fn known(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != "unknown").then(|| value.to_owned())
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
