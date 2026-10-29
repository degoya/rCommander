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
pub mod menu_bar;
pub mod popup_menus;
pub mod shell_menu;
#[cfg(windows)]
mod windows;
