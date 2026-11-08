use super::*;

/// A minimal PE head: `MZ`, the PE header at 0x80, its machine.
fn head(machine: u16) -> Vec<u8> {
    let mut bytes = vec![0_u8; 0x100];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    bytes
}

#[test]
fn the_machine_is_read_from_the_pe_header() {
    assert_eq!(pe_machine(&head(MACHINE_X64)), Some(MACHINE_X64));
    assert_eq!(pe_machine(&head(MACHINE_X86)), Some(MACHINE_X86));
}

#[test]
fn anything_else_is_no_pe_image() {
    assert_eq!(pe_machine(b""), None);
    assert_eq!(pe_machine(b"ELF and more bytes than a header"), None);
    let mut bad_offset = head(MACHINE_X64);
    bad_offset[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(pe_machine(&bad_offset), None);
    let mut no_signature = head(MACHINE_X64);
    no_signature[0x80] = b'X';
    assert_eq!(pe_machine(&no_signature), None);
    let truncated = &head(MACHINE_X64)[..0x85];
    assert_eq!(pe_machine(truncated), None);
}

#[test]
fn only_the_hosts_own_machine_loads() {
    assert_eq!(check_machine(&head(host_machine())), Ok(()));
    let refused = check_machine(&head(MACHINE_X86)).expect_err("32-bit is refused");
    assert!(refused.starts_with("a 32-bit x86 plugin"), "{refused}");
    assert_eq!(check_machine(b"MZ"), Err("not a Windows DLL".to_owned()));
}

#[test]
fn kinds_need_all_their_unicode_exports() {
    let packer = [
        "OpenArchiveW",
        "ReadHeaderExW",
        "ProcessFileW",
        "CloseArchive",
    ];
    assert_eq!(kinds(|name| packer.contains(&name)), vec![Kind::Wcx]);
    let ansi_packer = ["OpenArchive", "ReadHeader", "ProcessFile", "CloseArchive"];
    assert!(kinds(|name| ansi_packer.contains(&name)).is_empty());
    let both = [
        "ListGetPreviewBitmapW",
        "ContentGetSupportedField",
        "ContentGetValueW",
    ];
    assert_eq!(
        kinds(|name| both.contains(&name)),
        vec![Kind::Wlx, Kind::Wdx]
    );
}
