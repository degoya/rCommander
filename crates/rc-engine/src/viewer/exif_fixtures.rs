//! Hand-built EXIF blocks and photos for the tests (RC-100-99): a little-endian TIFF block
//! with IFD0, an Exif IFD and a GPS IFD, and a JPEG that carries one beside an XMP segment.

/// One IFD entry: tag, TIFF type, count and the value's bytes (little-endian).
pub(crate) struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    data: Vec<u8>,
}

pub(crate) fn ascii(tag: u16, text: &str) -> Entry {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    Entry {
        tag,
        kind: 2,
        count: data.len() as u32,
        data,
    }
}

pub(crate) fn short(tag: u16, value: u16) -> Entry {
    Entry {
        tag,
        kind: 3,
        count: 1,
        data: value.to_le_bytes().to_vec(),
    }
}

fn long(tag: u16, value: u32) -> Entry {
    Entry {
        tag,
        kind: 4,
        count: 1,
        data: value.to_le_bytes().to_vec(),
    }
}

pub(crate) fn rational(tag: u16, parts: &[(u32, u32)]) -> Entry {
    let mut data = Vec::new();
    for (num, denom) in parts {
        data.extend(num.to_le_bytes());
        data.extend(denom.to_le_bytes());
    }
    Entry {
        tag,
        kind: 5,
        count: parts.len() as u32,
        data,
    }
}

/// An IFD placed at `at` (an offset into the TIFF data), followed by its values' area.
fn ifd(entries: &[Entry], at: u32) -> Vec<u8> {
    let mut data_at = at + 2 + 12 * entries.len() as u32 + 4;
    let mut out = (entries.len() as u16).to_le_bytes().to_vec();
    let mut values = Vec::new();
    for entry in entries {
        out.extend(entry.tag.to_le_bytes());
        out.extend(entry.kind.to_le_bytes());
        out.extend(entry.count.to_le_bytes());
        if entry.data.len() <= 4 {
            let mut inline = entry.data.clone();
            inline.resize(4, 0);
            out.extend(inline);
        } else {
            out.extend(data_at.to_le_bytes());
            values.extend(&entry.data);
            if values.len() % 2 == 1 {
                values.push(0);
            }
            data_at = at + 2 + 12 * entries.len() as u32 + 4 + values.len() as u32;
        }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(values);
    out
}

/// A little-endian TIFF block: IFD0 with `image` and pointers to an Exif IFD with `photo` and a
/// GPS IFD with `gps` (each left out when empty).
pub(crate) fn tiff(image: Vec<Entry>, photo: Vec<Entry>, gps: Vec<Entry>) -> Vec<u8> {
    let with_pointers = |exif_at: u32, gps_at: u32| {
        let mut entries: Vec<Entry> = image
            .iter()
            .map(|entry| Entry {
                tag: entry.tag,
                kind: entry.kind,
                count: entry.count,
                data: entry.data.clone(),
            })
            .collect();
        if !photo.is_empty() {
            entries.push(long(0x8769, exif_at));
        }
        if !gps.is_empty() {
            entries.push(long(0x8825, gps_at));
        }
        entries
    };
    let ifd0_len = ifd(&with_pointers(0, 0), 8).len() as u32;
    let exif_at = 8 + ifd0_len;
    let exif = if photo.is_empty() {
        Vec::new()
    } else {
        ifd(&photo, exif_at)
    };
    let gps_at = exif_at + exif.len() as u32;
    let gps = if gps.is_empty() {
        Vec::new()
    } else {
        ifd(&gps, gps_at)
    };
    let mut out = b"II*\0".to_vec();
    out.extend(8u32.to_le_bytes());
    out.extend(ifd(&with_pointers(exif_at, gps_at), 8));
    out.extend(exif);
    out.extend(gps);
    out
}

/// A camera's block: make and model, exposure, date, and a position in Paris.
pub(crate) fn camera_tiff() -> Vec<u8> {
    tiff(
        vec![ascii(0x010F, "Canon"), ascii(0x0110, "EOS R5")],
        vec![
            rational(0x829A, &[(1, 125)]),
            rational(0x829D, &[(28, 10)]),
            short(0x8827, 100),
            ascii(0x9003, "2024:05:01 12:34:56"),
            rational(0x920A, &[(50, 1)]),
        ],
        vec![
            ascii(0x0001, "N"),
            rational(0x0002, &[(48, 1), (51, 1), (2400, 100)]),
            ascii(0x0003, "E"),
            rational(0x0004, &[(2, 1), (17, 1), (4000, 100)]),
        ],
    )
}

/// A JPEG: a JFIF `APP0`, an XMP `APP1`, the EXIF `APP1`, the start of scan.
pub(crate) fn jpeg_with(tiff: &[u8]) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    out.extend([0xFF, 0xE0, 0x00, 0x10]);
    out.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    let xmp = b"http://ns.adobe.com/xap/1.0/\0<x/>";
    out.extend([0xFF, 0xE1]);
    out.extend(((xmp.len() + 2) as u16).to_be_bytes());
    out.extend(xmp);
    out.extend([0xFF, 0xE1]);
    out.extend(((tiff.len() + 8) as u16).to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out.extend([0xFF, 0xDA, 0x00, 0x02, 0x11, 0x22, 0xFF, 0xD9]);
    out
}
