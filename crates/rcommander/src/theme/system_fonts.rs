//! The system's interface and monospace families behind [`super::UI_FONT`] and
//! [`super::MONO_FONT`] on Linux and the BSDs (RC-100-71). cosmic-text names its own defaults for
//! the generic families - Open Sans and Noto Sans Mono - and where they are not installed its
//! fallback picks a family per weight, so a dialog's semibold title drew in another font than
//! its buttons. At the start [`apply`] asks fontconfig (`fc-match`) for the system's choice, else
//! takes the first installed of a short list, and sets only a family iced's font database knows.
//! Windows and macOS keep their own (`UI_FONT`, `mac.rs`).
//!
//! The choice is pure ([`parse_family`], [`choose`]); [`apply`] runs the programs and sets it.
#![cfg_attr(
    not(all(unix, not(target_os = "macos"))),
    allow(dead_code, reason = "called on Linux and the BSDs only")
)]

use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::PoisonError,
    thread,
    time::{Duration, Instant},
};

use iced::advanced::graphics::text::font_system;
use rc_process::NoConsoleWindow;

use super::fonts::{self, Family};

/// The interface families tried in order when fontconfig names none that is installed.
pub const SANS_FALLBACKS: [&str; 5] = [
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Cantarell",
    "Ubuntu",
];
/// The monospace families tried in order when fontconfig names none that is installed.
pub const MONO_FALLBACKS: [&str; 3] = ["Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono"];

/// How long both `fc-match` runs together may take; past it the fallback list decides.
const TIMEOUT: Duration = Duration::from_secs(2);
/// How often a running `fc-match` is looked at.
const POLL: Duration = Duration::from_millis(10);
/// The longest family name taken from `fc-match`; its output is read no further.
const MAX_NAME: usize = 128;

/// The one family name `fc-match -f '%{family[0]}'` printed: `None` when the output is empty,
/// not UTF-8, more than one line, too long or holds a control character.
pub fn parse_family(output: &[u8]) -> Option<&str> {
    let name = std::str::from_utf8(output).ok()?.trim();
    let valid = !name.is_empty() && name.len() <= MAX_NAME && !name.chars().any(char::is_control);
    valid.then_some(name)
}

/// The family to set: the system's (`system`) when it is installed, else the first installed of
/// `fallbacks`; a monospace family (`mono`) only when it has a monospaced face. Names compare
/// without regard to ASCII case and come back as the font database spells them, since its
/// lookup is exact. `None` when none of them is installed: the default stays.
pub fn choose<'a>(
    system: Option<&str>,
    fallbacks: &[&str],
    installed: &'a [Family],
    mono: bool,
) -> Option<&'a str> {
    let find = |name: &str| {
        installed
            .iter()
            .find(|family| family.name.eq_ignore_ascii_case(name) && (family.mono || !mono))
            .map(|family| family.name.as_str())
    };
    system
        .into_iter()
        .chain(fallbacks.iter().copied())
        .find_map(find)
}

/// Sets the system's interface and monospace families in iced's font database, before the first
/// text is shaped (the start calls it from `theme::init`). Both `fc-match` runs go while the
/// database loads; a missing `fc-match`, a failure or the time limit leaves the fallback list.
pub(super) fn apply() {
    let started = Instant::now();
    let sans_run = fc_match("sans-serif");
    let mono_run = fc_match("monospace");
    // Loads the system's fonts once, as the first frame would.
    let installed = fonts::installed();
    let deadline = started + TIMEOUT;
    let sans_name = sans_run.and_then(|child| finish(child, deadline));
    let mono_name = mono_run.and_then(|child| finish(child, deadline));
    let sans = choose(
        sans_name.as_deref().and_then(parse_family),
        &SANS_FALLBACKS,
        &installed,
        false,
    );
    let mono = choose(
        mono_name.as_deref().and_then(parse_family),
        &MONO_FALLBACKS,
        &installed,
        true,
    );
    tracing::debug!(?sans, ?mono, "the system's font families");
    if sans.is_none() && mono.is_none() {
        return;
    }
    let mut system = font_system()
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    let db = system.raw().db_mut();
    if let Some(name) = sans {
        db.set_sans_serif_family(name);
    }
    if let Some(name) = mono {
        db.set_monospace_family(name);
    }
}

/// `fc-match -f '%{family[0]}' <pattern>` started, its output piped; `None` when it does not
/// start (no fontconfig).
fn fc_match(pattern: &str) -> Option<Child> {
    Command::new("fc-match")
        .args(["-f", "%{family[0]}", pattern])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .no_console_window()
        .spawn()
        .ok()
}

/// The output of `child` once it ended successfully by `deadline`, at most [`MAX_NAME`] bytes and
/// one more (so a longer name is refused); killed and `None` past the deadline or on a failure.
fn finish(mut child: Child, deadline: Instant) -> Option<Vec<u8>> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(Some(_)) => return None,
            Ok(None) | Err(_) => {
                // Gone already when the kill fails; the wait reaps it either way.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()?
        .take(MAX_NAME as u64 + 1)
        .read_to_end(&mut output)
        .ok()?;
    Some(output)
}

#[cfg(test)]
#[path = "system_fonts_tests.rs"]
mod tests;
