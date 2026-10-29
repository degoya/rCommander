//! The Win32 halves of the platform modules; `unsafe` lives only here, in one module per
//! feature, each with its own allow and reason.

pub mod clipboard;
pub mod colour_picker;
pub mod context_menu;
pub mod dialog_window;
pub mod drag_out;
pub mod file_dialog;
pub mod gpu_loss;
pub mod menu_bar;
pub mod popup_menus;
