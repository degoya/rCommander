//! The bridge against the real `rc-tc-host` binary, started as rCommander starts it. A real TC DLL
//! is the Windows acceptance of RC-100-02; here the DLL is a file with a PE header only.

use std::path::{Path, PathBuf};

use rc_tc_host::dll::host_machine;
use rc_tc_host::{BridgeError, HostProcess};

fn host() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_rc-tc-host"))
}

/// A file with a minimal PE head of the host's own machine and nothing behind it.
fn fake_dll(dir: &Path) -> PathBuf {
    let mut bytes = vec![0_u8; 0x100];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&host_machine().to_le_bytes());
    let path = dir.join("fake.wcx64");
    std::fs::write(&path, bytes).expect("fake DLL written");
    path
}

#[cfg(not(windows))]
#[test]
fn the_portable_host_speaks_the_protocol_and_supports_nothing() {
    use rc_tc_host::rpc::{PROTOCOL, code};

    let dir = tempfile::tempdir().expect("dir");
    let staging = tempfile::tempdir().expect("staging");
    let dll = fake_dll(dir.path());
    let mut process = HostProcess::start(host(), &dll, staging.path()).expect("started");
    let hello = process.bridge().hello().expect("hello");
    assert_eq!(hello.protocol, PROTOCOL);
    assert!(hello.kinds.is_empty());
    let refused = process.bridge().list(&dir.path().join("a.zip"));
    assert!(
        matches!(refused, Err(BridgeError::Remote { code: found, .. }) if found == code::UNSUPPORTED)
    );
    assert!(process.is_running());
}

#[cfg(windows)]
#[test]
fn a_dll_that_does_not_load_ends_only_the_host() {
    let dir = tempfile::tempdir().expect("dir");
    let staging = tempfile::tempdir().expect("staging");
    let dll = fake_dll(dir.path());
    let mut process = HostProcess::start(host(), &dll, staging.path()).expect("started");
    assert!(matches!(
        process.bridge().hello(),
        Err(BridgeError::HostGone)
    ));
}
