use russh_sftp::protocol::FileAttributes;

use super::*;

fn owner(user: &str, group: &str) -> Option<Owner> {
    Some(Owner {
        user: user.to_owned(),
        group: group.to_owned(),
    })
}

#[test]
fn a_longname_as_openssh_writes_it_names_user_and_group() {
    let line = "-rw-r--r--    1 alice    staff        1234 Jan  1 12:00 a b.txt";
    assert_eq!(of_longname(line), owner("alice", "staff"));
    let acl = "drwxr-x---+   3 www-data www-data     4096 Mar 3  2024 site";
    assert_eq!(of_longname(acl), owner("www-data", "www-data"));
    // The test server writes the numbers where it knows no name.
    assert_eq!(
        of_longname("drwxr-xr-x 0 1000 1000 4096 Jan 01 2024 00:00 dir"),
        owner("1000", "1000")
    );
}

#[test]
fn a_longname_of_another_shape_names_no_owner() {
    for line in [
        "",
        "a.txt",
        "-rw-r--r-- 1 alice",
        "-rw-r--r-- 1 alice staff",
        "rw-r 1 alice staff 12 a",
        "-rw-r--r-- one alice staff 12 a",
        "12-01-24  10:00AM  <DIR>  folder",
    ] {
        assert_eq!(of_longname(line), None, "{line:?}");
    }
}

#[test]
fn hostile_owner_names_are_not_believed() {
    let long = "u".repeat(65);
    let line = format!("-rw-r--r-- 1 {long} staff 12 Jan 1 12:00 a");
    assert_eq!(of_longname(&line), None);
    let line = "-rw-r--r-- 1 al\u{1b}[31mice staff 12 Jan 1 12:00 a";
    assert_eq!(of_longname(line), None);
    let attrs = FileAttributes {
        uid: Some(1000),
        user: Some("bad\nname".to_owned()),
        gid: Some(100),
        group: Some("users".to_owned()),
        ..FileAttributes::default()
    };
    assert_eq!(of_attrs(&attrs), owner("1000", "users"));
}

#[test]
fn attributes_without_an_owner_name_none() {
    assert_eq!(of_attrs(&FileAttributes::default()), None);
    let numbers = FileAttributes {
        uid: Some(0),
        gid: Some(0),
        ..FileAttributes::default()
    };
    assert_eq!(of_attrs(&numbers), owner("0", "0"));
}
