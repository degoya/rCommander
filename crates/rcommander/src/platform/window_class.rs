//! The window class a Linux desktop knows rCommander's windows by (RC-100-70): the Wayland
//! `app_id` and the X11 `WM_CLASS`, which tiling compositors such as Hyprland, Sway and i3 match
//! their rules on. The main window is `rcommander`, the name of the menu entry
//! (`packaging/linux/rcommander.desktop`, `StartupWMClass`); every dialog window is
//! `rcommander-popup`, so one rule lets them float (`packaging/linux/rcommander-popup.desktop`
//! gives them the icon). Other systems have no such class and keep their settings unchanged.

use iced::window;

/// The class of the main window.
pub const MAIN: &str = "rcommander";

/// The class of every dialog window.
pub const POPUP: &str = "rcommander-popup";

/// `settings` with the window class `class` (Linux).
#[cfg(target_os = "linux")]
pub fn with_class(settings: window::Settings, class: &str) -> window::Settings {
    window::Settings {
        platform_specific: window::settings::PlatformSpecific {
            application_id: class.to_owned(),
            ..settings.platform_specific
        },
        ..settings
    }
}

/// `settings` unchanged: this system has no window class.
#[cfg(not(target_os = "linux"))]
pub fn with_class(settings: window::Settings, _class: &str) -> window::Settings {
    settings
}

#[cfg(test)]
#[path = "window_class_tests.rs"]
mod tests;
