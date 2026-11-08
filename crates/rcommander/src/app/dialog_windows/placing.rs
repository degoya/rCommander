//! Where a dialog's window opens (RC-080-15): the place each was left, read before the main
//! window opens, and the settings of a new window at it - the app's icon, invisible until placed
//! and owned, the dialogs' window class, no file drops.

use iced::{
    Point, Size,
    window::{self, Position},
};
use rc_db::Db;

use super::DialogWindow;
use crate::{
    app::{settings, window_state::Geometry},
    platform,
};

/// Where each dialog window was left, read before the main window opens.
pub async fn load(db: &Db) -> Vec<(DialogWindow, Geometry)> {
    let mut stored = Vec::new();
    for kind in DialogWindow::ALL {
        let geometry = db.get::<Geometry>(kind.key()).await.ok().flatten();
        if let Some(geometry) = geometry.and_then(Geometry::sanitized) {
            stored.push((kind, geometry));
        }
    }
    stored
}

/// The window of `kind` at `geometry`: the app's icon, invisible until it is placed and owned
/// (`platform::dialog_window::show`), closed by the app, not by iced; the dialogs' window class
/// (`platform::window_class`).
pub(super) fn dialog_settings(kind: DialogWindow, geometry: Geometry) -> window::Settings {
    let position = geometry.position.map_or(Position::Centered, |(x, y)| {
        Position::Specific(Point::new(x, y))
    });
    let settings = without_drop(window::Settings {
        size: Size::new(geometry.width, geometry.height),
        position,
        maximized: geometry.maximized,
        min_size: Some(kind.min_size()),
        visible: false,
        icon: settings::window_icon(),
        exit_on_close_request: false,
        ..window::Settings::default()
    });
    platform::window_class::with_class(settings, platform::window_class::POPUP)
}

/// Files dropped on a dialog's window go nowhere: only the main window takes them.
#[cfg(windows)]
fn without_drop(settings: window::Settings) -> window::Settings {
    window::Settings {
        platform_specific: window::settings::PlatformSpecific {
            drag_and_drop: false,
            ..settings.platform_specific
        },
        ..settings
    }
}

#[cfg(not(windows))]
fn without_drop(settings: window::Settings) -> window::Settings {
    settings
}
