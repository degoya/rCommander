use std::path::{Path, PathBuf};

use super::{
    Registry, WCX_FTP_INI, environment, ftp_candidates, is_named, resolve_known, wincmd_candidates,
};
use crate::import::total_commander::{TcSkip, folder};

fn lookup(name: &str) -> Option<String> {
    match name {
        "APPDATA" => Some("/users/me/AppData/Roaming".to_owned()),
        "COMMANDER_PATH" => Some("/opt/totalcmd".to_owned()),
        _ => None,
    }
}

#[test]
fn wincmd_ini_is_in_appdata_then_where_the_registry_says() {
    let registry = Registry {
        ini_file: Some("%COMMANDER_PATH%/wincmd.ini".to_owned()),
        ..Registry::default()
    };
    assert_eq!(
        wincmd_candidates(&lookup, &registry),
        vec![
            PathBuf::from("/users/me/AppData/Roaming/GHISLER/wincmd.ini"),
            PathBuf::from("/opt/totalcmd/wincmd.ini"),
        ]
    );
    let relative = Registry {
        ini_file: Some(".\\wincmd.ini".to_owned()),
        install_dir: Some("/opt/tc".to_owned()),
        ..Registry::default()
    };
    assert_eq!(
        wincmd_candidates(&lookup, &relative)[1],
        PathBuf::from("/opt/tc/wincmd.ini")
    );
}

#[test]
fn only_a_file_named_wcx_ftp_ini_is_followed() {
    let wincmd = Path::new("/users/me/tc/wincmd.ini");
    let named = ftp_candidates(
        Some(wincmd),
        Some("%COMMANDER_PATH%/WCX_FTP.INI"),
        &lookup,
        &Registry::default(),
    );
    assert_eq!(
        named,
        vec![
            PathBuf::from("/opt/totalcmd/WCX_FTP.INI"),
            PathBuf::from("/users/me/tc/wcx_ftp.ini"),
        ]
    );
    // Pointing elsewhere is ignored: only the file beside wincmd.ini is left.
    let hostile = ftp_candidates(
        Some(wincmd),
        Some("/home/me/.ssh/id_ed25519"),
        &lookup,
        &Registry::default(),
    );
    assert_eq!(hostile, vec![PathBuf::from("/users/me/tc/wcx_ftp.ini")]);
    // An unknown variable names nothing.
    let unknown = ftp_candidates(
        None,
        Some("%NOPE%\\wcx_ftp.ini"),
        &lookup,
        &Registry::default(),
    );
    assert!(unknown.is_empty());
}

#[test]
fn names_compare_by_the_last_part_whatever_the_separator() {
    assert!(is_named(Path::new("C:\\tc\\Wcx_Ftp.ini"), WCX_FTP_INI));
    assert!(is_named(Path::new("/x/wcx_ftp.ini"), WCX_FTP_INI));
    assert!(!is_named(Path::new("/x/wcx_ftp.ini.bak"), WCX_FTP_INI));
}

#[test]
fn commander_path_falls_back_to_the_install_dir() {
    assert_eq!(
        environment("COMMANDER_PATH_SURELY_UNSET", Some("/opt/tc")),
        None
    );
    if std::env::var("COMMANDER_PATH").is_err() {
        assert_eq!(
            environment("COMMANDER_PATH", Some("/opt/tc")),
            Some("/opt/tc".to_owned())
        );
    }
}

/// An environment that holds a secret beside the variables Total Commander uses.
fn env(name: &str) -> Option<String> {
    match name {
        "APPDATA" | "appdata" => Some("C:\\Users\\me\\AppData\\Roaming".to_owned()),
        "ProgramFiles(x86)" => Some("C:\\Program Files (x86)".to_owned()),
        "AWS_SECRET_ACCESS_KEY" => Some("the-value-of-a-secret".to_owned()),
        _ => None,
    }
}

/// RC-080-35: only the variables Total Commander puts into paths resolve, whatever their case.
#[test]
fn only_known_variables_resolve() {
    assert_eq!(
        resolve_known("APPDATA", None, &env).as_deref(),
        Some("C:\\Users\\me\\AppData\\Roaming")
    );
    assert_eq!(
        resolve_known("appdata", None, &env).as_deref(),
        Some("C:\\Users\\me\\AppData\\Roaming")
    );
    assert_eq!(
        resolve_known("ProgramFiles(x86)", None, &env).as_deref(),
        Some("C:\\Program Files (x86)")
    );
    assert_eq!(resolve_known("AWS_SECRET_ACCESS_KEY", None, &env), None);
    assert_eq!(resolve_known("PATH", None, &env), None);
    // COMMANDER_PATH still falls back to the install folder.
    assert_eq!(
        resolve_known("COMMANDER_PATH", Some("C:\\totalcmd"), &env).as_deref(),
        Some("C:\\totalcmd")
    );
    assert_eq!(resolve_known("COMMANDER_PATH", None, &env), None);
}

/// A folder naming a secret stays unresolved and is reported as an unknown variable: the value
/// never reaches the report.
#[test]
fn a_secret_variable_is_skipped_as_unknown() {
    let lookup = |name: &str| resolve_known(name, None, &env);
    assert_eq!(
        folder("C:\\%AWS_SECRET_ACCESS_KEY%", &lookup),
        Err(TcSkip::UnknownVariable)
    );
    assert_eq!(
        folder("%APPDATA%\\GHISLER", &lookup).as_deref(),
        Ok("C:\\Users\\me\\AppData\\Roaming\\GHISLER")
    );
    assert_eq!(
        folder("%ProgramFiles(x86)%\\tc", &lookup).as_deref(),
        Ok("C:\\Program Files (x86)\\tc")
    );
    // The process environment goes through the same list.
    assert_eq!(environment("AWS_SECRET_ACCESS_KEY", None), None);
}
