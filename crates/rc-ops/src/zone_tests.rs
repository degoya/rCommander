use std::{io, path::Path};

use rc_vfs::{ArchiveFormat, BackendId, VfsPath};

use super::{Carry, INTERNET, MAX_STREAM, Origin, Zone, carry_with, origin, to_write};

/// A browser's stream: zone 3, where the download started and where it came from.
const DOWNLOADED: &[u8] = b"[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.org/page\r\nHostUrl=https://example.org/tool.zip\r\n";

fn archive_on(backend: BackendId, path: &str) -> BackendId {
    BackendId::Archive {
        outer: Box::new(VfsPath {
            backend,
            path: path.into(),
        }),
        format: ArchiveFormat::Zip,
    }
}

fn no_archive_read(_: &Path) -> io::Result<Option<Vec<u8>>> {
    panic!("no archive stream is read for this origin")
}

#[test]
fn a_browser_stream_reads_with_its_zone_and_both_urls() {
    let zone = Zone::parse(DOWNLOADED);
    assert_eq!(zone.id, 3);
    assert_eq!(
        zone.referrer_url.as_deref(),
        Some("https://example.org/page")
    );
    assert_eq!(
        zone.host_url.as_deref(),
        Some("https://example.org/tool.zip")
    );
}

#[test]
fn a_stream_without_a_readable_zone_is_the_internet() {
    assert_eq!(Zone::parse(b"").id, INTERNET);
    assert_eq!(Zone::parse(b"[ZoneTransfer]\r\nZoneId=x\r\n").id, INTERNET);
    assert_eq!(Zone::parse(b"\xff\xfe garbage").id, INTERNET);
    assert_eq!(Zone::parse(b"zoneid = 4\n").id, 4, "keys in any case");
}

#[test]
fn an_unpacked_entry_is_at_least_in_the_internet_zone() {
    let trusted = Zone::parse(b"[ZoneTransfer]\r\nZoneId=2\r\nHostUrl=https://intra/a.zip\r\n");
    let unpacked = trusted.unpacked();
    assert_eq!(unpacked.id, INTERNET);
    assert_eq!(unpacked.host_url.as_deref(), Some("https://intra/a.zip"));
    let restricted = Zone::parse(b"ZoneId=4").unpacked();
    assert_eq!(restricted.id, 4, "a stricter zone stays");
}

#[test]
fn the_stream_is_written_as_windows_writes_it() {
    let zone = Zone::parse(DOWNLOADED);
    assert_eq!(zone.render().as_bytes(), DOWNLOADED);
    assert_eq!(
        Zone::internet(None).render(),
        "[ZoneTransfer]\r\nZoneId=3\r\n"
    );
}

#[test]
fn a_url_with_a_line_break_never_adds_a_line() {
    let zone = Zone::internet(Some("sftp://host/\r\nZoneId=0"));
    assert_eq!(zone.host_url, None);
    assert_eq!(zone.render(), "[ZoneTransfer]\r\nZoneId=3\r\n");
}

#[test]
fn each_backend_has_its_origin() {
    assert_eq!(origin(&BackendId::Local), Origin::OwnStream);
    let sftp = BackendId::Sftp { connection_id: 7 };
    assert_eq!(origin(&sftp), Origin::Server(sftp.clone()));
    let ftp = BackendId::Ftp { connection_id: 8 };
    assert_eq!(origin(&ftp), Origin::Server(ftp.clone()));
    let device = BackendId::Device {
        device: "phone".to_owned(),
    };
    assert_eq!(origin(&device), Origin::Unmarked);
}

#[test]
fn an_archive_takes_its_origin_from_where_its_file_lies() {
    let on_disk = archive_on(BackendId::Local, "C:/Downloads/tool.zip");
    assert_eq!(
        origin(&on_disk),
        Origin::Archive("C:/Downloads/tool.zip".into())
    );
    let nested = archive_on(on_disk, "/inner.zip");
    assert_eq!(
        origin(&nested),
        Origin::Archive("C:/Downloads/tool.zip".into()),
        "the outermost archive's stream"
    );
    let sftp = BackendId::Sftp { connection_id: 7 };
    assert_eq!(
        origin(&archive_on(sftp.clone(), "/srv/tool.zip")),
        Origin::Server(sftp)
    );
}

#[test]
fn a_server_gives_the_internet_zone_with_its_host() {
    let carried = carry_with(
        &Origin::Server(BackendId::Sftp { connection_id: 7 }),
        Some("sftp://files.example.org:22/"),
        no_archive_read,
    );
    let expected = Zone {
        id: INTERNET,
        referrer_url: None,
        host_url: Some("sftp://files.example.org:22/".to_owned()),
    };
    assert_eq!(carried, Carry::Fixed(expected));
    let unnamed = carry_with(
        &Origin::Server(BackendId::Ftp { connection_id: 9 }),
        None,
        no_archive_read,
    );
    assert_eq!(unnamed, Carry::Fixed(Zone::internet(None)));
}

#[test]
fn an_archive_with_a_zone_hands_it_to_every_entry() {
    let archive = Origin::Archive("C:/Downloads/tool.zip".into());
    let carried = carry_with(&archive, None, |path| {
        assert_eq!(path, Path::new("C:/Downloads/tool.zip"));
        Ok(Some(
            b"[ZoneTransfer]\r\nZoneId=1\r\nHostUrl=https://x/t.zip\r\n".to_vec(),
        ))
    });
    let Carry::Fixed(zone) = carried else {
        panic!("a fixed zone: {carried:?}");
    };
    assert_eq!(zone.id, INTERNET, "at least the internet's");
    assert_eq!(zone.host_url.as_deref(), Some("https://x/t.zip"));
}

#[test]
fn an_archive_without_a_zone_or_unreadable_gives_none() {
    let archive = Origin::Archive("C:/a.zip".into());
    assert_eq!(carry_with(&archive, None, |_| Ok(None)), Carry::Nothing);
    let denied = carry_with(&archive, None, |_| {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    });
    assert_eq!(denied, Carry::Nothing);
    assert_eq!(
        carry_with(&Origin::Unmarked, None, no_archive_read),
        Carry::Nothing
    );
    assert_eq!(
        carry_with(&Origin::OwnStream, None, no_archive_read),
        Carry::OwnStream
    );
}

#[test]
fn a_local_copy_passes_its_own_stream_on_byte_for_byte() {
    let own = || Ok(Some(DOWNLOADED.to_vec()));
    assert_eq!(to_write(&Carry::OwnStream, own), Some(DOWNLOADED.to_vec()));
    assert_eq!(to_write(&Carry::OwnStream, || Ok(None)), None);
    let unreadable = || Err(io::Error::from(io::ErrorKind::InvalidInput));
    assert_eq!(to_write(&Carry::OwnStream, unreadable), None);
}

#[test]
fn an_oversized_own_stream_becomes_the_internet_zone() {
    let len = usize::try_from(MAX_STREAM).expect("64 KiB fit") + 1;
    let huge = move || Ok(Some(vec![b'x'; len]));
    assert_eq!(
        to_write(&Carry::OwnStream, huge),
        Some(Zone::internet(None).render().into_bytes())
    );
}

#[test]
fn a_fixed_zone_is_written_without_reading_the_source() {
    let zone = Zone::internet(Some("ftp://h:21/"));
    let written = to_write(&Carry::Fixed(zone.clone()), || {
        panic!("a fixed zone reads no source stream")
    });
    assert_eq!(written, Some(zone.render().into_bytes()));
    assert_eq!(
        to_write(&Carry::Nothing, || panic!("nothing reads nothing")),
        None
    );
}

/// The stream itself, on an NTFS temp folder.
#[cfg(windows)]
mod windows {
    use super::super::{Carry, Zone, keep, mark, read, write};
    use super::DOWNLOADED;

    #[test]
    fn the_stream_is_written_and_read_beside_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("tool.exe");
        std::fs::write(&file, b"MZ").expect("file");
        assert_eq!(read(&file).expect("no stream yet"), None);
        write(&file, DOWNLOADED).expect("stream written");
        assert_eq!(read(&file).expect("stream"), Some(DOWNLOADED.to_vec()));
        assert_eq!(
            std::fs::read(&file).expect("file"),
            b"MZ",
            "the data untouched"
        );
    }

    #[test]
    fn a_copy_gets_its_sources_stream_and_a_server_file_the_internet_zone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (source, copy, fetched) = (
            dir.path().join("a.exe"),
            dir.path().join("b.exe"),
            dir.path().join("c.exe"),
        );
        for file in [&source, &copy, &fetched] {
            std::fs::write(file, b"MZ").expect("file");
        }
        write(&source, DOWNLOADED).expect("source marked");
        keep(&Carry::OwnStream, &source, &copy).expect("kept");
        assert_eq!(read(&copy).expect("stream"), Some(DOWNLOADED.to_vec()));
        let server = Zone::internet(Some("sftp://h:22/"));
        mark(&fetched, &server).expect("marked");
        let written = read(&fetched).expect("stream").expect("there");
        assert_eq!(Zone::parse(&written), server);
    }
}
