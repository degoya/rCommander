//! The colour dialog of `platform/colour_picker.rs`: `ChooseColorW` through winsafe, opened full
//! (the spectrum beside the basic colours), owned by rCommander's newest window - the settings -
//! so it stays over it, whichever window was in front when it was asked for (RC-080-53). The
//! sixteen custom colours are kept for the session, as Windows leaves them to the caller.

use std::{num::NonZeroIsize, sync::Mutex};

use iced::{Task, window};
use winsafe::{self as w, co};

/// White, the custom colours' start.
const WHITE: [u8; 3] = [0xFF; 3];

/// The dialog's custom colours, white until the person keeps some.
static CUSTOM: Mutex<[[u8; 3]; 16]> = Mutex::new([WHITE; 16]);

/// The dialog on `initial` over the app's newest window, run off the UI thread.
pub fn task(initial: Option<[u8; 3]>) -> Task<Option<[u8; 3]>> {
    window::latest()
        .then(|id| match id {
            Some(id) => crate::native_window::hwnd(id),
            None => Task::done(None),
        })
        .then(move |owner| {
            Task::future(async move {
                tokio::task::spawn_blocking(move || pick(owner, initial))
                    .await
                    .ok()
                    .flatten()
            })
        })
}

/// The colour chosen on the dialog opened on `initial` over `owner`; `None` when cancelled or
/// failed.
fn pick(owner: Option<NonZeroIsize>, initial: Option<[u8; 3]>) -> Option<[u8; 3]> {
    let rgb = |[red, green, blue]: [u8; 3]| w::COLORREF::from_rgb(red, green, blue);
    let parts = |colour: w::COLORREF| [colour.GetRValue(), colour.GetGValue(), colour.GetBValue()];
    let mut custom = CUSTOM.lock().map_or([WHITE; 16], |custom| *custom).map(rgb);
    let mut dialog = w::CHOOSECOLOR::default();
    if let Some(owner) = owner {
        dialog.hwndOwner = super::file_dialog::hwnd(owner);
    }
    dialog.Flags = co::CC::FULLOPEN | co::CC::ANYCOLOR;
    if let Some(initial) = initial {
        dialog.rgbResult = rgb(initial);
        dialog.Flags |= co::CC::RGBINIT;
    }
    dialog.set_lpCustColors(Some(&mut custom));
    let chosen = w::ChooseColor(&mut dialog).ok()?;
    let result = parts(dialog.rgbResult);
    if let Ok(mut kept) = CUSTOM.lock() {
        *kept = custom.map(parts);
    }
    chosen.then_some(result)
}
