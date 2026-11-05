//! `[match] names`: whole file names a plugin claims (RC-100-80), for the files whose extension
//! says nothing - `Dockerfile`, `PKGBUILD`, `.env.local`.
//!
//! A name is compared with the file name alone, never its folder, without regard to case;
//! `*` is the only wildcard and stands for any run of characters, the empty one included. A name
//! without `*` is exact and is asked before every extension; one with `*` is a pattern and is
//! asked after them, so `Dockerfile.toml` stays a TOML file while `Dockerfile.dev`, whose `dev`
//! nobody claims, is a Dockerfile (`MatchSection::rank`).

use anyhow::{Result, bail};

/// How many names one plugin claims, and how long each is.
const MAX_NAMES: usize = 32;
const MAX_NAME_CHARS: usize = 64;
/// The first rCommander release that reads `names`: a manifest using them names it, or a later
/// one, as its `min_app_version`; an older release refuses the unknown field anyway.
pub const NAMES_MIN_APP_VERSION: &str = "0.19.0";
/// Characters no file name on Windows holds, `?` among them, so nobody takes it for a
/// wildcard.
const FORBIDDEN: [char; 8] = ['/', '\\', '<', '>', ':', '"', '|', '?'];

/// The last component of `file_name`, so a caller handing a path still compares the name.
fn base_name(file_name: &str) -> &str {
    file_name.rsplit(['/', '\\']).next().unwrap_or(file_name)
}

/// Whether `name` is a pattern (holds `*`) rather than an exact name.
#[must_use]
pub(crate) fn is_pattern(name: &str) -> bool {
    name.contains('*')
}

/// Whether the file `file_name` is the one `name` describes: case-insensitive, `*` any run of
/// characters, against the last component only.
#[must_use]
pub fn name_matches(name: &str, file_name: &str) -> bool {
    let pattern: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    let file: Vec<char> = base_name(file_name)
        .chars()
        .flat_map(char::to_lowercase)
        .collect();
    // The classic linear wildcard walk: on a mismatch, the last `*` swallows one more character.
    let (mut at_pattern, mut at_file) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while let Some(&wanted) = file.get(at_file) {
        match pattern.get(at_pattern) {
            Some('*') => {
                star = Some((at_pattern, at_file));
                at_pattern += 1;
            }
            Some(&literal) if literal == wanted => {
                at_pattern += 1;
                at_file += 1;
            }
            _ => match star {
                Some((star_at, swallowed)) => {
                    at_pattern = star_at + 1;
                    at_file = swallowed + 1;
                    star = Some((star_at, swallowed + 1));
                }
                None => return false,
            },
        }
    }
    pattern
        .get(at_pattern..)
        .is_some_and(|rest| rest.iter().all(|symbol| *symbol == '*'))
}

/// The checks of `[match] names`: at most [`MAX_NAMES`], each a file name of at most
/// [`MAX_NAME_CHARS`] with a character beyond `*` and `.` (so no name claims every file), none
/// twice, and the release that reads them as `min_app_version`.
pub(crate) fn validate_names(names: &[String], min_app_version: Option<&str>) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    if names.len() > MAX_NAMES {
        bail!("[match] names more than {MAX_NAMES} file names");
    }
    for (index, name) in names.iter().enumerate() {
        let plain = !name.is_empty()
            && name.chars().count() <= MAX_NAME_CHARS
            && name.trim() == name
            && !name
                .chars()
                .any(|symbol| symbol.is_control() || FORBIDDEN.contains(&symbol))
            && name.chars().any(|symbol| !matches!(symbol, '*' | '.'));
        let twice = names[..index]
            .iter()
            .any(|earlier| earlier.to_lowercase() == name.to_lowercase());
        if !plain || twice {
            bail!(
                "[match] name `{name}` is not a plain file name of at most {MAX_NAME_CHARS} characters, or twice"
            );
        }
    }
    let needed = semver::Version::parse(NAMES_MIN_APP_VERSION)
        .expect("NAMES_MIN_APP_VERSION is a SemVer literal");
    let stated = min_app_version.and_then(|version| semver::Version::parse(version).ok());
    if stated.is_none_or(|stated| stated < needed) {
        bail!("[match] names need metadata.min_app_version {NAMES_MIN_APP_VERSION} or later");
    }
    Ok(())
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
