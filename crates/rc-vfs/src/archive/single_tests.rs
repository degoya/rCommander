use std::path::Path;

use super::inner_name;
use crate::{
    Vfs,
    archive::{
        ArchiveFormat, Compression,
        test_support::{fixture, list, names, open, read},
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_compressed_file_is_an_archive_of_one_entry() {
    let payload = b"single file payload\n".repeat(100);
    let cases = [
        ("single.txt.gz", Compression::Gzip, "single.txt"),
        ("single.txt.xz", Compression::Xz, "single.txt"),
        ("single.txt.bz2", Compression::Bzip2, "single.txt"),
        ("single.txt.zst", Compression::Zstd, "single.txt"),
        ("frames.txt.zst", Compression::Zstd, "frames.txt"),
    ];
    for (name, compression, inner) in cases {
        let fs = open(name).await;
        assert_eq!(fs.format(), ArchiveFormat::Single { compression }, "{name}");
        let entries = list(&fs, "/").await;
        assert_eq!(names(&entries), [inner], "{name}");
        assert_eq!(entries[0].size, payload.len() as u64, "{name}");
        let outer = std::fs::metadata(fixture(name)).expect(name);
        assert_eq!(entries[0].modified, outer.modified().ok(), "{name}");
        let item = fs
            .stat_detailed(Path::new(&format!("/{inner}")))
            .await
            .expect(name);
        assert_eq!(item.info.packed_size, Some(outer.len()), "{name}");
        assert_eq!(read(&fs, &format!("/{inner}")).await, payload, "{name}");
        let other = fs.stat(Path::new("/other.txt")).await;
        assert!(other.is_err(), "{name}: there is one entry only");
    }
}

#[test]
fn the_entry_is_the_name_without_the_suffix() {
    let cases = [
        ("notes.txt.gz", Compression::Gzip, "notes.txt"),
        ("NOTES.TXT.GZ", Compression::Gzip, "NOTES.TXT"),
        ("backup.tgz", Compression::Gzip, "backup.tar"),
        ("log.xz", Compression::Xz, "log"),
        ("data.tbz2", Compression::Bzip2, "data.tar"),
        ("data.bz2", Compression::Bzip2, "data"),
        ("dump.zst", Compression::Zstd, "dump"),
        // No known suffix: the last extension goes, or nothing.
        ("odd.bin", Compression::Gzip, "odd"),
        ("plain", Compression::Xz, "plain"),
        (".gz", Compression::Gzip, ".gz"),
    ];
    for (outer, compression, inner) in cases {
        assert_eq!(inner_name(outer, compression), inner, "{outer}");
    }
}
