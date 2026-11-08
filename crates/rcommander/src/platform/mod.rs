//! Platform code behind `cfg(windows)` (ADR 0001): one module per native feature, each with a
//! portable face the app calls on every system.

pub mod clipboard_files;
pub mod clipboard_image;
pub mod colour_picker;
pub mod dialog_window;
pub mod drag_out;
pub mod dropfiles;
pub mod file_dialog;
pub mod gpu_loss;
pub mod key_label;
pub mod locale_format;
pub mod menu_bar;
pub mod popup_menus;
pub mod shell_menu;
pub mod taskbar;
pub mod window_class;
#[cfg(windows)]
mod windows;
// WM_TRANSIENT_FOR and the dialog window type under X11 (RC-080-73).
#[cfg(all(unix, not(target_os = "macos")))]
mod x11_dialog;
