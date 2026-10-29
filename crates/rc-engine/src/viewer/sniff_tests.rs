use super::*;
use crate::viewer::detect;

/// The first bytes of a file of each format, as far as the sniffer looks.
fn heads() -> Vec<(ImageFormat, Vec<u8>)> {
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&[0; 12]);
    bmp.extend_from_slice(&40u32.to_le_bytes());
    let mut ico = vec![0, 0, 1, 0, 1, 0];
    ico.extend_from_slice(&[16, 16, 0, 0, 1, 0, 32, 0]);
    ico.extend_from_slice(&[0; 8]);
    vec![
        (ImageFormat::Png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec()),
        (ImageFormat::Jpeg, b"\xFF\xD8\xFF\xE0\0\x10JFIF".to_vec()),
        (ImageFormat::Gif, b"GIF89a\x01\0\x01\0".to_vec()),
        (ImageFormat::Gif, b"GIF87a".to_vec()),
        (ImageFormat::WebP, b"RIFF\x24\0\0\0WEBPVP8 ".to_vec()),
        (ImageFormat::Bmp, bmp),
        (ImageFormat::Ico, ico),
    ]
}

#[test]
fn each_format_is_known_by_its_signature() {
    for (format, head) in heads() {
        assert_eq!(sniff_image(&head), Some(format), "{}", format.label());
        assert_eq!(suggest_mode(&head), ViewMode::Image);
    }
}

#[test]
fn a_cut_or_damaged_signature_is_no_image_and_never_panics() {
    for (format, head) in heads() {
        for len in 0..head.len() {
            // Every cut length is looked at without reading past it.
            let _ = sniff_image(&head[..len]);
        }
        for len in 0..head.len().min(3) {
            let cut = &head[..len];
            assert_ne!(
                sniff_image(cut),
                Some(format),
                "{} cut to {len}",
                format.label()
            );
        }
    }
    assert_eq!(sniff_image(b"BMW owners' club minutes\n"), None);
    assert_eq!(sniff_image(b"RIFF\x24\0\0\0WAVEfmt "), None);
    assert_eq!(suggest_mode(b"plain text"), ViewMode::Text);
}

#[test]
fn a_zero_byte_marks_binary_except_in_utf16() {
    assert!(looks_binary(b"MZ\x90\0\x03", detect(b"MZ\x90\0\x03")));
    assert!(!looks_binary(b"text\n", detect(b"text\n")));
    let utf16 = b"\xFF\xFEa\0b\0";
    assert!(!looks_binary(utf16, detect(utf16)));
}
