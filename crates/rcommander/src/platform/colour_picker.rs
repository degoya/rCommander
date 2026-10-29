//! The system's colour dialog for the Colours tab (RC-100-43): Windows' own "Color" dialog
//! (`ChooseColorW`, `windows/colour_picker.rs`) - the basic colours, the spectrum and sixteen
//! custom colours, as Total Commander uses it. Elsewhere there is none for now: the tab offers no
//! button, and a colour is typed as `#rrggbb`.

use iced::Task;

/// Whether this platform has the dialog.
pub const AVAILABLE: bool = cfg!(windows);

/// The dialog on `initial`, owned by the app's newest window - the settings' own while they have
/// one - and on a thread of its own, so the window keeps drawing; the colour chosen, `None` when
/// it was cancelled.
#[cfg(windows)]
pub fn pick(initial: Option<[u8; 3]>) -> Task<Option<[u8; 3]>> {
    super::windows::colour_picker::task(initial)
}

/// The dialog on `initial`; none on this system, so nothing is chosen.
#[cfg(not(windows))]
pub fn pick(initial: Option<[u8; 3]>) -> Task<Option<[u8; 3]>> {
    let _ = initial;
    Task::done(None)
}
