use std::path::PathBuf;

use super::*;
use crate::dll::{MACHINE_X86, host_machine};

/// A file with a minimal PE head of `machine`.
fn fake_dll(dir: &Path, name: &str, machine: u16) -> PathBuf {
    let mut bytes = vec![0_u8; 0x100];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("fake DLL written");
    path
}

#[test]
fn the_environment_is_the_allowlist_and_the_staging_folder() {
    let staging = Path::new("/staging");
    let env = host_env(
        |name| match name {
            "SystemRoot" => Some(OsString::from("C:\\Windows")),
            _ => None,
        },
        staging,
    );
    let names: Vec<_> = env
        .iter()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["SystemRoot", "TEMP", "TMP", "COMMANDER_PATH"]);
    assert!(
        env[1..]
            .iter()
            .all(|(_, value)| value == staging.as_os_str())
    );
}

#[test]
fn a_dll_of_another_machine_is_refused_before_a_start() {
    let dir = tempfile::tempdir().expect("dir");
    let x86 = fake_dll(dir.path(), "old.wcx", MACHINE_X86);
    let refused = HostProcess::start(Path::new("/no/such/host"), &x86, dir.path());
    assert!(matches!(refused, Err(BridgeError::Refused(reason)) if reason.contains("32-bit")));
    let missing = check_dll(&dir.path().join("missing.wcx64"));
    assert!(matches!(missing, Err(BridgeError::Refused(_))));
    let own = fake_dll(dir.path(), "new.wcx64", host_machine());
    check_dll(&own).expect("the host's own machine");
}
