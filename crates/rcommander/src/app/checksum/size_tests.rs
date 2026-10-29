use super::*;

#[test]
fn sizes_read_as_bytes_or_with_a_binary_unit() {
    assert_eq!(parse_size("4294967295"), Some(4_294_967_295));
    assert_eq!(parse_size("700 MB"), Some(700 * MIB));
    assert_eq!(parse_size("700m"), Some(700 * MIB));
    assert_eq!(parse_size("1.5G"), Some(GIB + GIB / 2));
    assert_eq!(parse_size("1,5 GiB"), Some(GIB + GIB / 2));
    assert_eq!(parse_size(" 100 KiB "), Some(100 * KIB));
    assert_eq!(parse_size("12 b"), Some(12));
    assert_eq!(parse_size(".5k"), Some(512));
}

#[test]
fn what_is_no_size_is_refused() {
    for text in [
        "",
        "0",
        "0.0001",
        "MB",
        "1 TB",
        "1.",
        "1.2.3 M",
        "-5",
        "abc",
        "99999999999 G",
    ] {
        assert_eq!(parse_size(text), None, "{text:?}");
    }
}

#[test]
fn every_preset_reads_back_as_its_size() {
    for preset in SizePreset::ALL {
        assert_eq!(
            parse_size(&preset.text()),
            Some(preset.bytes()),
            "{preset:?}"
        );
    }
    assert_eq!(SizePreset::Fat32.bytes(), u64::from(u32::MAX));
}
