use std::fs;

use std::path::Path;

use rc_vfs::BackendId;

use super::{Caller, Wanted, load};
use crate::read_guard::{ReadGuard, TC_FTP_PASSWORDS};

const WINDOW: Caller = Caller {
    mcp: false,
    guard: None,
};
const MCP: Caller = Caller {
    mcp: true,
    guard: None,
};

fn wanted(wincmd: Option<String>, ftp: Option<String>) -> Wanted {
    Wanted {
        wincmd_ini: wincmd,
        ftp_ini: ftp,
        wincmd: true,
        ftp: true,
    }
}

#[test]
fn wcx_ftp_ini_is_found_beside_wincmd_ini() {
    let dir = tempfile::tempdir().expect("dir");
    let wincmd = dir.path().join("wincmd.ini");
    fs::write(&wincmd, "[left]\npath=C:\\\n").expect("write");
    fs::write(dir.path().join("wcx_ftp.ini"), "[connections]\n1=a\n").expect("write");
    let loaded = load(&wanted(Some(wincmd.display().to_string()), None), MCP).expect("load");
    assert_eq!(
        loaded.ftp.as_ref().map(|(path, _)| path.clone()),
        Some(dir.path().join("wcx_ftp.ini"))
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn ftp_ini_name_is_followed_only_to_a_wcx_ftp_ini() {
    let dir = tempfile::tempdir().expect("dir");
    let other = dir.path().join("elsewhere");
    fs::create_dir(&other).expect("dir");
    fs::write(other.join("wcx_ftp.ini"), "[connections]\n1=b\n").expect("write");
    fs::write(other.join("secret.txt"), "[connections]\n1=c\n").expect("write");
    let wincmd = dir.path().join("wincmd.ini");
    for (named, expected) in [
        ("elsewhere/wcx_ftp.ini", Some(other.join("wcx_ftp.ini"))),
        ("elsewhere/secret.txt", None),
    ] {
        fs::write(&wincmd, format!("[FTP]\nFtpIniName={named}\n")).expect("write");
        let loaded = load(&wanted(Some(wincmd.display().to_string()), None), WINDOW).expect("load");
        assert_eq!(loaded.ftp.as_ref().map(|(path, _)| path.clone()), expected);
    }
}

#[test]
fn over_mcp_only_the_two_file_names_are_read_and_a_refusal_says_nothing_more() {
    let dir = tempfile::tempdir().expect("dir");
    let notes = dir.path().join("notes.ini");
    fs::write(&notes, "[left]\npath=C:\\\n").expect("write");
    for path in [notes.clone(), dir.path().join("missing.ini")] {
        let error =
            load(&wanted(Some(path.display().to_string()), None), MCP).expect_err("refused");
        assert_eq!(error.code(), "tc_ini_path_refused");
    }
    // The window reads the file the person picked, whatever its name.
    let loaded = load(&wanted(Some(notes.display().to_string()), None), WINDOW).expect("load");
    assert!(loaded.wincmd.is_some());
    assert_eq!(loaded.warnings, vec!["ftp_ini_not_found".to_owned()]);
}

#[test]
fn a_named_file_that_is_missing_or_too_large_is_an_error() {
    let dir = tempfile::tempdir().expect("dir");
    let missing = dir.path().join("wincmd.ini");
    let error =
        load(&wanted(Some(missing.display().to_string()), None), WINDOW).expect_err("missing");
    assert_eq!(error.code(), "tc_ini_unreadable");
    let big = dir.path().join("wcx_ftp.ini");
    let size = usize::try_from(super::tc_ini::MAX_FILE_BYTES).expect("fits") + 1;
    fs::write(&big, vec![b';'; size]).expect("write");
    let error =
        load(&wanted(None, Some(big.display().to_string())), WINDOW).expect_err("too large");
    assert_eq!(error.code(), "tc_ini_too_large");
}

/// The MCP guard's verdicts as far as this import meets them: `wcx_ftp.ini` under its own reason,
/// a key under another.
fn judge(path: &Path) -> Option<&'static str> {
    match path.file_name()?.to_str()? {
        "wcx_ftp.ini" => Some(TC_FTP_PASSWORDS),
        "id_ed25519" => Some("an SSH key"),
        _ => None,
    }
}

/// RC-080-12 (M7): the guard refuses `wcx_ftp.ini` to every reading tool, and this import alone
/// reads it under exactly that reason - a link of that name to a key stays refused.
#[test]
fn over_mcp_the_guard_lets_the_import_read_wcx_ftp_ini_and_nothing_behind_its_name() {
    let dir = tempfile::tempdir().expect("dir");
    let ftp_ini = dir.path().join("wcx_ftp.ini");
    fs::write(&ftp_ini, "[connections]\n1=a\n").expect("write");
    let guarded = Caller {
        mcp: true,
        guard: Some(ReadGuard::new(judge, &BackendId::Local)),
    };
    let wanted_ftp = |path: &Path| Wanted {
        ftp_ini: Some(path.display().to_string()),
        ftp: true,
        ..Wanted::default()
    };
    let loaded = load(&wanted_ftp(&ftp_ini), guarded).expect("the import reads it");
    assert_eq!(loaded.ftp.map(|(path, _)| path), Some(ftp_ini.clone()));

    // The same reason for wincmd.ini's name is no pass.
    let wincmd = dir.path().join("wincmd.ini");
    fs::write(&wincmd, "[left]\npath=C:\\\n").expect("write");
    let refuse_all = Caller {
        mcp: true,
        guard: Some(ReadGuard::new(
            |_| Some(TC_FTP_PASSWORDS),
            &BackendId::Local,
        )),
    };
    let wanted_wincmd = Wanted {
        wincmd_ini: Some(wincmd.display().to_string()),
        wincmd: true,
        ..Wanted::default()
    };
    let error = load(&wanted_wincmd, refuse_all).expect_err("refused");
    assert_eq!(error.code(), "tc_ini_path_refused");

    #[cfg(unix)]
    {
        let keys = dir.path().join("keys");
        fs::create_dir(&keys).expect("dir");
        let key = keys.join("id_ed25519");
        fs::write(&key, "[connections]\n1=key\n").expect("write");
        let link = keys.join("wcx_ftp.ini");
        std::os::unix::fs::symlink(&key, &link).expect("link");
        let error = load(&wanted_ftp(&link), guarded).expect_err("refused");
        assert_eq!(error.code(), "tc_ini_path_refused");
    }
}
