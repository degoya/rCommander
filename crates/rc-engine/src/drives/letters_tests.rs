use std::path::PathBuf;

use super::{present_letters, raw_drive, with_remembered};
use crate::drives::{DriveKind, RawDrive};

#[test]
fn each_bit_of_the_mask_is_a_letter_from_a() {
    assert_eq!(present_letters(0), Vec::<char>::new());
    assert_eq!(present_letters(0b1), vec!['A']);
    // C:, D: and Z:.
    assert_eq!(
        present_letters(0b100 | 0b1000 | (1 << 25)),
        vec!['C', 'D', 'Z']
    );
    // Bits past Z name no drive.
    assert_eq!(present_letters(u32::MAX).len(), 26);
    assert_eq!(present_letters((1 << 26) | (1 << 31)), Vec::<char>::new());
}

#[test]
fn a_drive_is_its_letter_with_a_colon_and_its_root() {
    assert_eq!(
        raw_drive('E', DriveKind::Removable, None),
        RawDrive {
            id: "E:".into(),
            label: "E:".into(),
            kind: DriveKind::Removable,
            root: PathBuf::from("E:\\"),
            remote: None,
        }
    );
}

#[test]
fn a_remembered_drive_joins_only_without_a_letter_and_all_are_sorted() {
    let present = vec![
        raw_drive('Z', DriveKind::Network, Some(r"\\nas\media".into())),
        raw_drive('C', DriveKind::Fixed, None),
    ];
    let drives = with_remembered(
        present,
        [
            ('Z', r"\\other\share".to_owned()),
            ('M', r"\\nas\music".to_owned()),
        ],
    );
    let ids: Vec<&str> = drives.iter().map(|drive| drive.id.as_str()).collect();
    assert_eq!(ids, ["C:", "M:", "Z:"]);
    assert_eq!(drives[1].kind, DriveKind::Network);
    assert_eq!(drives[1].remote.as_deref(), Some(r"\\nas\music"));
    // The connected Z: keeps its own folder.
    assert_eq!(drives[2].remote.as_deref(), Some(r"\\nas\media"));
}
