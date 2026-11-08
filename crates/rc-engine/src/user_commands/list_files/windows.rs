//! The system's ANSI code page on Windows, read from the registry (`winreg`, no `unsafe`): the
//! value `ACP` under `HKLM\SYSTEM\CurrentControlSet\Control\Nls\CodePage`, which `GetACP` answers
//! too.

use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

const KEY: &str = r"SYSTEM\CurrentControlSet\Control\Nls\CodePage";

/// The ANSI code page; `None` when the value cannot be read.
pub(super) fn ansi_code_page() -> Option<u16> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(KEY).ok()?;
    let value: String = key.get_value("ACP").ok()?;
    value.trim().parse().ok()
}
