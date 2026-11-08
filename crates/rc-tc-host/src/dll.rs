//! What a TC plugin DLL is, read without loading it: its machine from the PE header, and its kinds
//! from the functions it exports.
//!
//! The host is 64-bit only (RC-100-02's decision): a 32-bit `.wcx`/`.wlx` is refused before a
//! process starts, with its machine named, instead of failing inside `LoadLibraryExW`.

use crate::rpc::Kind;

/// `IMAGE_FILE_MACHINE_I386`.
pub const MACHINE_X86: u16 = 0x014c;
/// `IMAGE_FILE_MACHINE_AMD64`.
pub const MACHINE_X64: u16 = 0x8664;
/// `IMAGE_FILE_MACHINE_ARM64`.
pub const MACHINE_ARM64: u16 = 0xaa64;

/// The bytes of a DLL to read for [`pe_machine`]: the DOS header points at the PE header within
/// them for every linker in use.
pub const HEADER_BYTES: usize = 4096;

/// The machine field of a PE image's file header; `None` when `head` is no PE image.
pub fn pe_machine(head: &[u8]) -> Option<u16> {
    if head.get(..2)? != b"MZ" {
        return None;
    }
    let offset = u32::from_le_bytes(head.get(0x3c..0x40)?.try_into().ok()?) as usize;
    let signature = head.get(offset..offset.checked_add(4)?)?;
    if signature != b"PE\0\0" {
        return None;
    }
    let machine = head.get(offset + 4..offset + 6)?;
    Some(u16::from_le_bytes([machine[0], machine[1]]))
}

/// The machine a DLL must have to load into this host.
pub fn host_machine() -> u16 {
    if cfg!(target_arch = "aarch64") {
        MACHINE_ARM64
    } else {
        MACHINE_X64
    }
}

/// A short name of a machine for messages.
pub fn machine_name(machine: u16) -> String {
    match machine {
        MACHINE_X86 => "32-bit x86".to_owned(),
        MACHINE_X64 => "64-bit x64".to_owned(),
        MACHINE_ARM64 => "64-bit ARM".to_owned(),
        other => format!("machine 0x{other:04x}"),
    }
}

/// Whether a DLL whose header starts with `head` loads into this host; the reason when not.
pub fn check_machine(head: &[u8]) -> Result<(), String> {
    match pe_machine(head) {
        None => Err("not a Windows DLL".to_owned()),
        Some(machine) if machine == host_machine() => Ok(()),
        Some(machine) => Err(format!(
            "a {} plugin; this host loads {} plugins only",
            machine_name(machine),
            machine_name(host_machine())
        )),
    }
}

/// The exports that make a kind usable through the host: the Unicode functions only, since every
/// 64-bit plugin has them and a path outside the ANSI code page must not be lost.
const WCX: &[&str] = &[
    "OpenArchiveW",
    "ReadHeaderExW",
    "ProcessFileW",
    "CloseArchive",
];
const WLX: &[&str] = &["ListGetPreviewBitmapW"];
const WFX: &[&str] = &["FsInitW", "FsFindFirstW", "FsFindNextW", "FsFindClose"];
const WDX: &[&str] = &["ContentGetSupportedField", "ContentGetValueW"];

/// The kinds a DLL offers, by asking `exports` for each function's name.
pub fn kinds(exports: impl Fn(&str) -> bool) -> Vec<Kind> {
    [
        (Kind::Wcx, WCX),
        (Kind::Wlx, WLX),
        (Kind::Wfx, WFX),
        (Kind::Wdx, WDX),
    ]
    .into_iter()
    .filter(|(_, names)| names.iter().all(|name| exports(name)))
    .map(|(kind, _)| kind)
    .collect()
}

#[cfg(test)]
#[path = "dll_tests.rs"]
mod tests;
