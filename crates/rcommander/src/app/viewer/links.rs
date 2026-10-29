//! A link clicked in the Markdown preview (RC-020-09). The file is foreign input, so a link opens
//! only on a click, only as `http`, `https` or `mailto`, and only through the system's handler -
//! the same `open::that_detached` (`ShellExecuteW` on Windows) that `file.open` hands a file to,
//! on a blocking thread. Every other scheme (`file:`, `javascript:`, a program's own) never
//! opens; the status line says so and names the target.

/// The longest link that opens; a longer one is refused.
const MAX_URL: usize = 2048;
/// The characters of a link the status line and the tooltips show.
const SHOWN_CHARS: usize = 120;

/// Whether a click on `url` opens it: `http://`, `https://` or `mailto:`, without whitespace or
/// control characters, at most [`MAX_URL`] bytes.
pub fn opens(url: &str) -> bool {
    if url.len() > MAX_URL || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => rest.len() > 2 && rest.starts_with("//"),
        "mailto" => !rest.is_empty(),
        _ => false,
    }
}

/// `url` as the status line and a tooltip show it: control characters as `?`, cut after
/// [`SHOWN_CHARS`] characters.
pub fn shown(url: &str) -> String {
    let mut shown: String = url
        .chars()
        .take(SHOWN_CHARS)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    if url.chars().nth(SHOWN_CHARS).is_some() {
        shown.push('\u{2026}');
    }
    shown
}

/// `url` handed to the system, when it [`opens`]; whether the system took it.
pub async fn open(url: String) -> bool {
    if !opens(&url) {
        return false;
    }
    matches!(
        tokio::task::spawn_blocking(move || launch(&url)).await,
        Ok(Ok(()))
    )
}

/// Hands `url` to the system's handler without waiting for the program it starts.
#[cfg(not(test))]
fn launch(url: &str) -> std::io::Result<()> {
    open::that_detached(url)
}

/// The tests start nothing: the link is recorded.
#[cfg(test)]
fn launch(url: &str) -> std::io::Result<()> {
    tests::LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(url.to_owned());
    Ok(())
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
