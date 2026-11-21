use std::path::Path;

use super::{MountEntry, display, parse_table, remote_at, script_error, strip_scheme, unc_of, url};
use crate::drives::network::NetworkDriveError;

fn reason(result: Result<String, NetworkDriveError>) -> &'static str {
    match result {
        Err(NetworkDriveError::PathInvalid { reason }) => reason,
        other => panic!("refused, got {other:?}"),
    }
}

#[test]
fn the_scheme_is_found_in_any_case() {
    assert_eq!(strip_scheme("smb://nas/media"), Some("nas/media"));
    assert_eq!(strip_scheme("SMB://nas/media"), Some("nas/media"));
    assert_eq!(strip_scheme(r"\\nas\media"), None);
    assert_eq!(strip_scheme("smb:"), None);
    assert_eq!(strip_scheme("sm\u{e4}://x"), None);
}

#[test]
fn an_address_becomes_a_folder_with_its_escapes_undone() {
    assert_eq!(unc_of("nas/media").expect("share"), r"\\nas\media");
    assert_eq!(
        unc_of("nas.local/My%20Share/films/").expect("escaped"),
        r"\\nas.local\My Share\films\"
    );
    assert_eq!(unc_of("nas/100%").expect("a lone %"), r"\\nas\100%");
    assert_eq!(unc_of("nas/%+1").expect("no escape"), r"\\nas\%+1");
}

/// rCommander never touches a password: none travels in the address, nor a user name.
#[test]
fn a_user_name_an_escaped_separator_and_no_text_are_refused() {
    assert!(reason(unc_of("alex@nas/media")).contains("user name"));
    assert!(reason(unc_of("alex:secret@nas/media")).contains("user name"));
    assert!(reason(unc_of("nas/me%2Fdia")).contains("character"));
    assert!(reason(unc_of("nas/me%5Cdia")).contains("character"));
    assert!(reason(unc_of("nas/%FF")).contains("no text"));
}

#[test]
fn a_share_is_mounted_from_its_encoded_address() {
    assert_eq!(url(r"\\nas\media").expect("share"), "smb://nas/media");
    assert_eq!(
        url(r"\\nas.local\My Share$").expect("encoded"),
        "smb://nas.local/My%20Share%24"
    );
    assert_eq!(
        url("\\\\192.168.1.5\\F\u{fc}r").expect("UTF-8"),
        "smb://192.168.1.5/F%C3%BCr"
    );
    assert!(reason(url(r"\\nas\media\films")).contains("not a folder below it"));
    assert!(reason(url(r"\\files@SSL@443\DavWWWRoot")).contains("host name"));
    assert!(reason(url(r"\\na s\media")).contains("host name"));
}

#[test]
fn a_share_shows_as_its_address() {
    assert_eq!(display(r"\\nas\My Share"), "smb://nas/My Share");
}

const TABLE: &str = "\
/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)
devfs on /dev (devfs, local, nobrowse)
//alex@nas._smb._tcp.local/My%20Share on /Volumes/My Share (smbfs, nodev, nosuid, mounted by alex)
//GUEST:@printer/public on /Volumes/public (smbfs, nodev, nosuid, mounted by alex)
nas:/export on /Volumes/export (nfs, nodev, nosuid, mounted by alex)
/dev/disk4s1 on /Volumes/USB (msdos, local, nodev, nosuid, noowners)
a line that is no mount
";

#[test]
fn the_mount_table_is_read_line_by_line() {
    let table = parse_table(TABLE);
    assert_eq!(table.len(), 6, "{table:?}");
    assert_eq!(
        table[0],
        MountEntry {
            from: "/dev/disk3s1s1".to_owned(),
            point: "/".to_owned(),
            fs_type: "apfs".to_owned(),
        }
    );
    assert_eq!(table[2].point, "/Volumes/My Share");
    assert_eq!(table[2].fs_type, "smbfs");
}

/// The user name stays out of the drive bar; a local volume has no folder.
#[test]
fn a_network_volume_shows_its_share_and_a_local_one_none() {
    let table = parse_table(TABLE);
    let at = |point: &str| remote_at(Path::new(point), &table);
    assert_eq!(
        at("/Volumes/My Share").as_deref(),
        Some("smb://nas._smb._tcp.local/My Share")
    );
    assert_eq!(
        at("/Volumes/public").as_deref(),
        Some("smb://printer/public")
    );
    assert_eq!(at("/Volumes/export").as_deref(), Some("nas:/export"));
    assert_eq!(at("/Volumes/USB"), None);
    assert_eq!(at("/"), None);
    assert_eq!(at("/Volumes/gone"), None);
}

#[test]
fn osascript_names_its_error_last() {
    assert_eq!(
        script_error("0:42: execution error: User canceled. (-128)\n"),
        Some(-128)
    );
    assert_eq!(
        script_error("0:42: execution error: There was a problem (some) connecting. (-36)"),
        Some(-36)
    );
    assert_eq!(script_error("osascript: no such file"), None);
    assert_eq!(script_error(""), None);
}
