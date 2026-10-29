//! The owner's logo and icon (`resources/brand/`), compiled in as PNG: the logo for the About
//! page in the light and the dark look, the icon for the window's title bar, Alt+Tab and the
//! taskbar. The exe's own icon is `resources/rcommander.ico`, made from the same PNG.
//!
//! Each picture is decoded once, on first use: a handle made again on every draw would be a new
//! image to the renderer each time. A picture that does not decode (it cannot, short of a broken
//! build) is left out rather than shown broken.

use std::sync::LazyLock;

use ::image::{ImageFormat, RgbaImage};
use iced::{
    widget::image::Handle,
    window::{Icon, icon},
};

const ICON_PNG: &[u8] = include_bytes!("../../../../resources/brand/rCommander-icon.png");
const LOGO_PNG: &[u8] = include_bytes!("../../../../resources/brand/rCommander-logo.png");
const LOGO_DARK_PNG: &[u8] = include_bytes!("../../../../resources/brand/rCommander-logo-dark.png");

static LOGO: LazyLock<Option<Handle>> = LazyLock::new(|| handle(LOGO_PNG));
static LOGO_DARK: LazyLock<Option<Handle>> = LazyLock::new(|| handle(LOGO_DARK_PNG));

/// The logo for the light or the dark look.
pub fn logo(dark: bool) -> Option<Handle> {
    if dark {
        LOGO_DARK.clone()
    } else {
        LOGO.clone()
    }
}

/// The window's icon.
pub fn window_icon() -> Option<Icon> {
    let rgba = decode(ICON_PNG)?;
    let (width, height) = rgba.dimensions();
    icon::from_rgba(rgba.into_raw(), width, height).ok()
}

fn handle(png: &[u8]) -> Option<Handle> {
    let rgba = decode(png)?;
    let (width, height) = rgba.dimensions();
    Some(Handle::from_rgba(width, height, rgba.into_raw()))
}

fn decode(png: &[u8]) -> Option<RgbaImage> {
    ::image::load_from_memory_with_format(png, ImageFormat::Png)
        .ok()
        .map(|picture| picture.into_rgba8())
}

#[cfg(test)]
#[path = "logo_tests.rs"]
mod tests;
