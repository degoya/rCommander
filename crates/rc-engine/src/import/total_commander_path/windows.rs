//! Total Commander's registry key: where it keeps `wincmd.ini` and `wcx_ftp.ini`, and where it
//! is installed (`%COMMANDER_PATH%`). The user's key first, then the machine's.

use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

use super::Registry;

const KEY: &str = r"Software\Ghisler\Total Commander";

/// The values that are set and not blank.
pub(super) fn read() -> Registry {
    let value = |name: &str| -> Option<String> {
        [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .find_map(|hive| {
                let key = RegKey::predef(hive).open_subkey(KEY).ok()?;
                let value: String = key.get_value(name).ok()?;
                (!value.trim().is_empty()).then_some(value)
            })
    };
    Registry {
        ini_file: value("IniFileName"),
        ftp_ini: value("FtpIniName"),
        install_dir: value("InstallDir"),
    }
}
