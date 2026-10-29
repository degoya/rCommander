//! The accent palette from the registry.

use winreg::{RegKey, enums::HKEY_CURRENT_USER};

const ACCENT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";

/// The raw `AccentPalette` value; `None` where Windows keeps none.
pub(super) fn accent_palette() -> Option<Vec<u8>> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(ACCENT).ok()?;
    let value = key.get_raw_value("AccentPalette").ok()?;
    Some(value.bytes.into_owned())
}
