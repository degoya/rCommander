use std::path::PathBuf;

use super::{parse, unescape};
use crate::drives::DriveKind;

const WSL_MOUNTS: &str = "\
none /mnt/wsl tmpfs rw,relatime 0 0
/dev/sdc / ext4 rw,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec 0 0
C:\\134 /mnt/c 9p rw,noatime 0 0
none /mnt/wslg tmpfs rw,relatime 0 0
/dev/sdd /media/alex/USB\\040STICK vfat rw 0 0
nas:/export /srv/nas nfs4 rw 0 0
/dev/sdc /mnt/c 9p rw 0 0
";

#[test]
fn mount_points_of_media_mnt_and_network_file_systems_are_drives() {
    let drives = parse(WSL_MOUNTS);
    let summary: Vec<(&str, &str, DriveKind)> = drives
        .iter()
        .map(|drive| (drive.id.as_str(), drive.label.as_str(), drive.kind))
        .collect();
    assert_eq!(
        summary,
        [
            ("/mnt/c", "c", DriveKind::Fixed),
            ("/media/alex/USB STICK", "USB STICK", DriveKind::Removable),
            ("/srv/nas", "nas", DriveKind::Network),
        ]
    );
    assert_eq!(drives[1].root, PathBuf::from("/media/alex/USB STICK"));
}

#[test]
fn a_short_line_is_skipped() {
    assert!(parse("garbage\n\n/dev/sda /mnt\n").is_empty());
}

#[test]
fn octal_escapes_are_undone_and_a_lone_backslash_stays() {
    assert_eq!(unescape("a\\040b\\011c"), "a b\tc");
    assert_eq!(unescape("end\\"), "end\\");
    assert_eq!(unescape("\\9zz"), "\\9zz");
}
