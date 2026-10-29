//! The notes a manifest carries: the version's section of `RELEASE-NOTES.md` (`## X.Y.Z`, one to
//! eight `- ` points), as plain text - the window shows it as text, never as Markdown.

use anyhow::{Result, ensure};
use rc_update::manifest::MAX_NOTES_CHARS;

/// The `- ` points of `version`'s section in `text`, one per line; `None` without a section.
///
/// # Errors
///
/// The section is longer than a manifest carries.
pub(crate) fn user_notes(text: &str, version: &str) -> Result<Option<String>> {
    let heading = format!("## {version}");
    let mut lines = text.lines();
    if !lines.any(|line| line.trim_end() == heading) {
        return Ok(None);
    }
    let mut points: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("## ") {
            break;
        }
        let trimmed = line.trim_end();
        if let Some(point) = trimmed.strip_prefix("- ") {
            points.push(format!("- {}", point.trim()));
        } else if !trimmed.trim().is_empty()
            && !trimmed.trim_start().starts_with("<!--")
            && let Some(last) = points.last_mut()
        {
            // A point wrapped onto the next line.
            last.push(' ');
            last.push_str(trimmed.trim());
        }
    }
    let notes = points.join("\n");
    ensure!(
        notes.chars().count() <= MAX_NOTES_CHARS,
        "the notes of {version} are longer than {MAX_NOTES_CHARS} characters"
    );
    Ok(Some(notes))
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
