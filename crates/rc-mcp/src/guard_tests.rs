use std::path::PathBuf;

use rc_db::BACKUP_DIR;

use super::{as_windows_opens, judge, sensitive_path};

fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

#[test]
fn ssh_private_files_are_sensitive_and_public_ones_are_not() {
    for parts in [
        ["home", "bob", ".ssh", "id_rsa"],
        ["home", "bob", ".ssh", "id_ed25519"],
        ["Users", "Bob", ".SSH", "ID_RSA"],
        ["home", "bob", ".ssh", "work-server"],
        ["home", "bob", ".ssh", "authorized_keys"],
    ] {
        assert!(sensitive_path(&path(&parts)).is_some(), "{parts:?}");
    }
    for name in ["id_rsa.pub", "known_hosts", "config", "ID_ED25519.PUB"] {
        let parts = ["home", "bob", ".ssh", name];
        assert_eq!(sensitive_path(&path(&parts)), None, "{name}");
    }
}

#[test]
fn keys_stores_and_credential_files_are_sensitive() {
    for parts in [
        &["srv", "tls", "server.key"][..],
        &["srv", "tls", "bundle.PEM"],
        &["keys", "putty.ppk"],
        &["keys", "client.p12"],
        &["safe", "passwords.kdbx"],
        &["project", ".env"],
        &["project", ".env.local"],
        &["home", "bob", ".netrc"],
        &["home", "bob", ".git-credentials"],
        &["home", "bob", ".aws", "credentials"],
        &["home", "bob", ".kube", "config"],
        &["home", "bob", ".gnupg", "pubring.kbx"],
        &["etc", "shadow"],
        &["data", "rcommander.db"],
        &["data", "rcommander.db-wal"],
        &["data", "rcommander.db-shm"],
        &["data", "RCommander.DB"],
        &["data", "rcommander.db.bak"],
        &["data", "mcp-instance.json"],
        &["data", "MCP-Instance.JSON"],
    ] {
        assert!(sensitive_path(&path(parts)).is_some(), "{parts:?}");
    }
}

/// RC-080-03: other file managers' stores of reversibly kept passwords.
#[test]
fn other_clients_password_stores_are_sensitive() {
    for parts in [
        &[
            "Users",
            "bob",
            "AppData",
            "Roaming",
            "FileZilla",
            "sitemanager.xml",
        ][..],
        &["home", "bob", ".config", "filezilla", "recentservers.xml"],
        &["tools", "WinSCP", "WinSCP.ini"],
        &[
            "Users",
            "bob",
            "AppData",
            "Roaming",
            "GHISLER",
            "wcx_ftp.ini",
        ],
        &["totalcmd", "WCX_FTP.INI"],
    ] {
        assert!(sensitive_path(&path(parts)).is_some(), "{parts:?}");
    }
    // RC-080-12: refused with the reason the import from Total Commander alone reads it under.
    assert_eq!(
        sensitive_path(&path(&["totalcmd", "wcx_ftp.ini"])),
        Some(rc_engine::TC_FTP_PASSWORDS)
    );
    // RC-050-11: an RDM export, refused with the reason its import alone reads it under.
    for parts in [&["Documents", "export.rdm"][..], &["backup", "VAULT.RDM"]] {
        assert_eq!(
            sensitive_path(&path(parts)),
            Some(rc_engine::RDM_EXPORT_PASSWORDS),
            "{parts:?}"
        );
    }
    assert!(sensitive_path(&path(&["work", "sitemanager.xml"])).is_none());
}

#[test]
fn ordinary_files_are_not_sensitive() {
    for parts in [
        &["home", "bob", "notes.txt"][..],
        &["project", "env.txt"],
        &["project", "credentials"],
        &["etc", "hosts"],
        // `rcommander*.db*` is rc_db's database name (`is_database_file_name`), wide on purpose.
        &["data", "notes.dbx"],
        &["keys.txt"],
    ] {
        assert_eq!(sensitive_path(&path(parts)), None, "{parts:?}");
    }
    assert_eq!(sensitive_path(&PathBuf::new()), None);
}

#[test]
fn everything_in_the_backup_folder_is_sensitive_written_either_way() {
    let data: PathBuf = ["home", "bob", ".local", "share", "rcommander"]
        .iter()
        .collect();
    let dirs = [data.join(BACKUP_DIR)];
    let backup = data
        .join(BACKUP_DIR)
        .join("2026-10-17")
        .join("connections.json");
    assert!(judge(&backup, &dirs).is_some());
    // Case and a verbatim prefix differ on Windows; the folders are what count.
    let shouted: PathBuf = [
        "HOME",
        "Bob",
        ".local",
        "share",
        "rCommander",
        "Backups",
        "x.zip",
    ]
    .iter()
    .collect();
    assert!(judge(&shouted, &dirs).is_some());
    assert_eq!(judge(&data.join("notes.txt"), &dirs), None);
    assert_eq!(judge(&data.join("backups-old.txt"), &dirs), None);
    assert_eq!(judge(&backup, &[]), None);
}

/// Windows opens `key.pem.`, `key.pem ` and `rcommander.db::$DATA` as the file without the
/// suffix: each is judged as that file too, while ordinary names stay ordinary.
#[test]
fn names_are_judged_as_windows_opens_them() {
    for parts in [
        &["srv", "tls", "server.pem."][..],
        &["srv", "tls", "server.key . ."],
        &["data", "rcommander.db::$DATA"],
        &["data", "rcommander.db:stream"],
        &["home", "bob", ".aws", "credentials:x"],
        &["home", "bob", ".aws.", "credentials"],
        &["project", ".env:$DATA"],
        &["home", "bob", ".ssh.", "id_rsa"],
    ] {
        assert!(sensitive_path(&path(parts)).is_some(), "{parts:?}");
    }
    for parts in [
        &["home", "bob", "notes.txt."][..],
        &["project", "readme.md:Zone.Identifier"],
    ] {
        assert_eq!(sensitive_path(&path(parts)), None, "{parts:?}");
    }
    assert_eq!(as_windows_opens(&path(&["a", "b.txt"])), None);
    assert_eq!(
        as_windows_opens(&path(&["a.", "b.txt::$DATA"])),
        Some(path(&["a", "b.txt"]))
    );
}

/// A name the guard lets pass stays refused when its stream suffix names an ordinary file:
/// both forms are judged, never only the opened one.
#[test]
fn an_ssh_file_with_a_stream_suffix_stays_refused() {
    let parts = ["home", "bob", ".ssh", "known_hosts:evil"];
    assert!(sensitive_path(&path(&parts)).is_some());
}
