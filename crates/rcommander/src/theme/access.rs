//! Windows' accessibility settings the window follows (RC-110-79): a high-contrast theme, whose
//! system colours replace every look while it is on ([`contrast_look`], the built-in High
//! contrast theme with them laid over it); the text size of Settings, Accessibility, which scales
//! the whole interface ([`text_scale`], iced's scale factor); and a running screen reader, which
//! makes the Win32 menu the bar of a person who never chose one (`app/palette/menu_bar/drawn.rs`).
//!
//! The start reads them before the window opens ([`super::init`]); while it runs they are read
//! again every [`POLL`] off the UI thread ([`changes`]), so switching high contrast or the text
//! size on or off is taken over without a restart. The calls are Windows' (`windows.rs`);
//! elsewhere nothing is read: no high contrast, a scale of 1, no screen reader.

#[cfg(windows)]
mod windows;

use std::{
    sync::{
        Mutex, RwLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use iced::futures::{Stream, stream};

use super::{Tokens, builtin};
use crate::recover::{LockOrRecover as _, RwLockOrRecover as _};

/// How often the settings are read again while the window runs.
pub const POLL: Duration = Duration::from_secs(2);

/// The text sizes Windows offers, 100 to 225 %, as a factor.
const MIN_SCALE: f32 = 1.0;
const MAX_SCALE: f32 = 2.25;

/// The colours of Windows' high-contrast theme (`GetSysColor`), each `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SystemColours {
    /// `COLOR_WINDOW`: every background.
    pub window: u32,
    /// `COLOR_WINDOWTEXT`: the text and the lines.
    pub window_text: u32,
    /// `COLOR_HIGHLIGHT`: the active panel's cursor row.
    pub highlight: u32,
    /// `COLOR_HIGHLIGHTTEXT`: the text of a cursor row.
    pub highlight_text: u32,
    /// `COLOR_HOTLIGHT`, a link's colour: the accent and a marked name.
    pub hot_light: u32,
    /// `COLOR_GRAYTEXT`: the inactive panel's cursor row.
    pub gray_text: u32,
}

/// What the system asks of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Access {
    /// The high-contrast theme's colours while one is on.
    pub contrast: Option<SystemColours>,
    /// The text size as a factor, 1 to 2.25.
    pub text_scale: f32,
}

/// The settings as the system has them now; outside Windows no high contrast and a scale of 1.
#[must_use]
pub fn read() -> Access {
    #[cfg(windows)]
    let (contrast, percent) = (windows::high_contrast(), windows::text_scale_percent());
    #[cfg(not(windows))]
    let (contrast, percent) = (None, None);
    Access {
        contrast,
        text_scale: scale_of(percent),
    }
}

/// Whether a screen reader runs (`SPI_GETSCREENREADER`, set by Narrator, NVDA and JAWS while they
/// run); `false` outside Windows.
#[must_use]
pub fn screen_reader() -> bool {
    #[cfg(windows)]
    {
        windows::screen_reader()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Windows' `TextScaleFactor` in percent as a scale: 1 to 2.25, 1 for a value missing or out of
/// that range.
#[must_use]
pub fn scale_of(percent: Option<u32>) -> f32 {
    let Some(percent) = percent else {
        return MIN_SCALE;
    };
    // The value is 100 to 225; anything else is no setting of Windows'.
    let scale = f32::from(u16::try_from(percent).unwrap_or(0)) / 100.0;
    if (MIN_SCALE..=MAX_SCALE).contains(&scale) {
        scale
    } else {
        MIN_SCALE
    }
}

/// The scale in force, as `f32` bits; 1 until [`apply`] stores another.
static SCALE: AtomicU32 = AtomicU32::new(MIN_SCALE.to_bits());
/// The high-contrast colours in force and their look.
static CONTRAST: RwLock<Option<(SystemColours, &'static Tokens)>> = RwLock::new(None);
/// Every high-contrast look made in this run: a theme switched back to takes the one made then.
static MADE: Mutex<Vec<(SystemColours, &'static Tokens)>> = Mutex::new(Vec::new());

/// `access` from now on; whether anything changed.
pub fn apply(access: Access) -> bool {
    let scale = access.text_scale.clamp(MIN_SCALE, MAX_SCALE);
    let scaled = SCALE.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits();
    let mut current = CONTRAST.write_or_recover("access.contrast");
    let was = current.as_ref().map(|(colours, _)| *colours);
    if was == access.contrast {
        return scaled;
    }
    *current = access.contrast.map(|colours| (colours, look(colours)));
    true
}

/// The look of the high-contrast `colours`: the one made before for them, or a new one kept for
/// the rest of the run.
fn look(colours: SystemColours) -> &'static Tokens {
    let mut made = MADE.lock_or_recover("access.made");
    if let Some((_, tokens)) = made.iter().find(|(was, _)| *was == colours) {
        return tokens;
    }
    let tokens: &'static Tokens = Box::leak(Box::new(builtin::system_high_contrast(&colours)));
    made.push((colours, tokens));
    tokens
}

/// The scale the interface draws with: the text size of Windows' accessibility settings.
#[must_use]
pub fn text_scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// The look every part of the window draws with while Windows' high contrast is on; `None`
/// while it is off.
#[must_use]
pub fn contrast_look() -> Option<&'static Tokens> {
    CONTRAST
        .read_or_recover("access.contrast")
        .as_ref()
        .map(|(_, tokens)| *tokens)
}

/// The settings once at once and then each time they change, read every [`POLL`].
pub fn changes() -> impl Stream<Item = Access> {
    stream::unfold(None, |last: Option<Access>| async move {
        let mut first = last.is_none();
        loop {
            if !first {
                tokio::time::sleep(POLL).await;
            }
            first = false;
            let now = read();
            if last != Some(now) {
                return Some((now, Some(now)));
            }
        }
    })
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
