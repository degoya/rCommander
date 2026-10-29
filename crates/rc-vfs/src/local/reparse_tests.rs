use super::{MOUNT_POINT_TAG, mount_point_buffer};

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    String::from_utf16(&units).expect("utf-16")
}

fn short(buffer: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([buffer[at], buffer[at + 1]]))
}

#[test]
fn a_junction_buffer_holds_both_names() {
    let buffer = mount_point_buffer(r"C:\Data\Work").expect("buffer");
    assert_eq!(
        u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]),
        MOUNT_POINT_TAG
    );
    assert_eq!(short(&buffer, 4), buffer.len() - 8, "data length");
    let names = &buffer[16..];
    let (sub_at, sub_len) = (short(&buffer, 8), short(&buffer, 10));
    let (print_at, print_len) = (short(&buffer, 12), short(&buffer, 14));
    assert_eq!(utf16(&names[sub_at..sub_at + sub_len]), r"\??\C:\Data\Work");
    assert_eq!(
        utf16(&names[print_at..print_at + print_len]),
        r"C:\Data\Work"
    );
    assert_eq!(&names[sub_at + sub_len..sub_at + sub_len + 2], &[0, 0]);
    assert_eq!(&names[names.len() - 2..], &[0, 0]);
}

#[test]
fn a_verbatim_prefix_is_dropped() {
    let plain = mount_point_buffer(r"C:\x").expect("plain");
    let verbatim = mount_point_buffer(r"\\?\C:\x").expect("verbatim");
    assert_eq!(plain, verbatim);
}

#[test]
fn network_and_relative_targets_are_refused() {
    for target in [
        r"\\server\share\dir",
        r"dir\inner",
        "C:",
        r"\\?\UNC\server\share",
    ] {
        assert!(mount_point_buffer(target).is_err(), "{target}");
    }
}

#[test]
fn an_overlong_target_is_refused() {
    let long = format!(r"C:\{}", "a".repeat(9000));
    assert!(mount_point_buffer(&long).is_err());
}
