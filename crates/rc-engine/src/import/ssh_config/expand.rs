//! What ssh_config values expand to: a leading `~`, `%x` tokens and the globs of `Include`.

use std::fs;
use std::path::{Path, PathBuf};

/// `~` alone or followed by a separator becomes `home`, by text as OpenSSH does; `~user` stays
/// as written.
pub(super) fn expand_tilde(value: &str, home: &Path) -> String {
    let home = home.to_string_lossy();
    let home = home.trim_end_matches(['/', '\\']);
    match value.strip_prefix('~') {
        Some("") => home.to_owned(),
        Some(rest) if rest.starts_with(['/', '\\']) => format!("{home}{rest}"),
        _ => value.to_owned(),
    }
}

/// `value` with each `%x` of `tokens` replaced and `%%` as one `%`. A token not in `tokens`, or
/// a `%` at the end, stays as written and is pushed onto `unknown`.
pub(super) fn expand_tokens(
    value: &str,
    tokens: &[(char, &str)],
    unknown: &mut Vec<String>,
) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(token) => match tokens.iter().find(|(name, _)| *name == token) {
                Some((_, replacement)) => out.push_str(replacement),
                None => {
                    out.push('%');
                    out.push(token);
                    unknown.push(format!("%{token}"));
                }
            },
            None => {
                out.push('%');
                unknown.push("%".to_owned());
            }
        }
    }
    out
}

/// Whether `text` holds a glob wildcard (`*` or `?`).
pub(super) fn has_wildcard(text: &str) -> bool {
    text.contains(['*', '?'])
}

/// Whether `text` matches `pattern`, where `*` stands for any run of characters and `?` for one.
pub(super) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    crate::mask::wildcard(&pattern, &text)
}

/// The files `pattern` names, sorted. A component with a wildcard is matched against its
/// directory's entries (a hidden entry only by a pattern starting with `.`, as `glob(3)` does)
/// and only existing files are kept; a pattern without one comes back as it is, existing or not.
pub(super) fn expand_glob(pattern: &Path) -> Vec<PathBuf> {
    let mut wildcard = false;
    let mut current = vec![PathBuf::new()];
    for component in pattern.components() {
        let part = component.as_os_str();
        let Some(glob) = part.to_str().filter(|text| has_wildcard(text)) else {
            current.iter_mut().for_each(|path| path.push(part));
            continue;
        };
        wildcard = true;
        let mut next = Vec::new();
        for dir in &current {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| !name.starts_with('.') || glob.starts_with('.'))
                .filter(|name| wildcard_match(glob, name))
                .collect();
            names.sort();
            next.extend(names.into_iter().map(|name| dir.join(name)));
        }
        current = next;
    }
    if wildcard {
        current.retain(|path| path.is_file());
    }
    current
}

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
