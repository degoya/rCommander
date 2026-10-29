use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use super::{Platform, ShellEnv, detect_shells, on_path};

/// An empty program file at `dir/name`, executable on Unix.
fn program(dir: &Path, name: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("mkdir");
    let path = dir.join(name);
    fs::write(&path, b"").expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

fn joined(platform: Platform, dirs: &[&Path]) -> OsString {
    let separator = match platform {
        Platform::Windows => ";",
        Platform::Unix => ":",
    };
    let parts: Vec<String> = dirs.iter().map(|dir| dir.display().to_string()).collect();
    parts.join(separator).into()
}

fn ids(shells: &[super::ShellProfile]) -> Vec<&str> {
    shells.iter().map(|shell| shell.id.as_str()).collect()
}

#[test]
fn windows_with_pwsh_on_the_path_offers_it_first() {
    let root = tempfile::tempdir().expect("temp");
    let tools = root.path().join("tools");
    let system32 = root.path().join("Windows").join("System32");
    let pwsh = program(&tools, "pwsh.exe");
    let cmd = program(&system32, "cmd.exe");
    let env = ShellEnv {
        path: Some(joined(Platform::Windows, &[&tools, &system32])),
        system_root: Some(root.path().join("Windows")),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Windows, &env);
    assert_eq!(ids(&shells), ["pwsh", "powershell", "cmd"]);
    assert_eq!(shells[0].program, pwsh);
    assert_eq!(shells[0].args, ["-NoLogo"]);
    assert_eq!(shells[0].label_key, "terminal.shell.pwsh");
    assert_eq!(shells[2].program, cmd);
    // Not found anywhere: the bare name, which Windows finds in its own folders.
    assert_eq!(shells[1].program, PathBuf::from("powershell.exe"));
}

#[test]
fn windows_without_pwsh_defaults_to_windows_powershell() {
    let root = tempfile::tempdir().expect("temp");
    let system32 = root.path().join("Windows").join("System32");
    let powershell = program(
        &system32.join("WindowsPowerShell").join("v1.0"),
        "powershell.exe",
    );
    let env = ShellEnv {
        path: Some(joined(Platform::Windows, &[&system32])),
        system_root: Some(root.path().join("Windows")),
        program_files: Some(root.path().join("Program Files")),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Windows, &env);
    assert_eq!(ids(&shells), ["powershell", "cmd"]);
    assert_eq!(shells[0].program, powershell);
    assert_eq!(shells[1].program, PathBuf::from("cmd.exe"));
}

#[test]
fn windows_finds_pwsh_where_its_installer_puts_it() {
    let root = tempfile::tempdir().expect("temp");
    let program_files = root.path().join("Program Files");
    let pwsh = program(&program_files.join("PowerShell").join("7"), "pwsh.exe");
    let env = ShellEnv {
        path: Some(joined(Platform::Windows, &[&root.path().join("empty")])),
        program_files: Some(program_files),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Windows, &env);
    assert_eq!(shells[0].id, "pwsh");
    assert_eq!(shells[0].program, pwsh);
}

#[test]
fn unix_puts_the_login_shell_first_and_does_not_repeat_it() {
    let root = tempfile::tempdir().expect("temp");
    let bin = root.path().join("bin");
    let zsh = program(&bin, "zsh");
    let bash = program(&bin, "bash");
    program(&bin, "sh");
    let env = ShellEnv {
        path: Some(joined(Platform::Unix, &[&bin])),
        shell: Some(zsh.clone().into_os_string()),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Unix, &env);
    assert_eq!(ids(&shells), ["zsh", "bash", "sh"]);
    assert_eq!(shells[0].program, zsh);
    assert_eq!(shells[1].program, bash);
    assert!(shells.iter().all(|shell| shell.args.is_empty()));
}

#[test]
fn unix_names_an_unknown_login_shell_by_its_file() {
    let root = tempfile::tempdir().expect("temp");
    let nu = program(&root.path().join("opt"), "nu");
    let env = ShellEnv {
        path: Some(joined(Platform::Unix, &[&root.path().join("nothing")])),
        shell: Some(nu.into_os_string()),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Unix, &env);
    assert_eq!(ids(&shells), ["nu"]);
    assert_eq!(shells[0].label_key, "terminal.shell.other");
}

#[test]
fn unix_with_nothing_found_still_offers_sh() {
    let shells = detect_shells(Platform::Unix, &ShellEnv::default());
    assert_eq!(ids(&shells), ["sh"]);
    assert_eq!(shells[0].program, PathBuf::from("/bin/sh"));
}

#[cfg(unix)]
#[test]
fn unix_skips_a_file_that_is_not_executable() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().expect("temp");
    let bin = root.path().join("bin");
    let bash = program(&bin, "bash");
    fs::set_permissions(&bash, fs::Permissions::from_mode(0o644)).expect("chmod");
    program(&bin, "sh");
    let env = ShellEnv {
        path: Some(joined(Platform::Unix, &[&bin])),
        shell: Some(bash.into_os_string()),
        ..ShellEnv::default()
    };
    assert_eq!(ids(&detect_shells(Platform::Unix, &env)), ["sh"]);
}

/// RC-080-43 (S8): `cmd.exe` and Windows PowerShell come from `%SystemRoot%\System32` even when
/// a folder earlier on `PATH` holds a program of that name.
#[test]
fn windows_takes_cmd_and_powershell_from_system32_before_the_path() {
    let root = tempfile::tempdir().expect("temp");
    let planted = root.path().join("planted");
    program(&planted, "cmd.exe");
    program(&planted, "powershell.exe");
    let system32 = root.path().join("Windows").join("System32");
    let cmd = program(&system32, "cmd.exe");
    let powershell = program(
        &system32.join("WindowsPowerShell").join("v1.0"),
        "powershell.exe",
    );
    let env = ShellEnv {
        path: Some(joined(Platform::Windows, &[&planted, &system32])),
        system_root: Some(root.path().join("Windows")),
        ..ShellEnv::default()
    };
    let shells = detect_shells(Platform::Windows, &env);
    assert_eq!(ids(&shells), ["powershell", "cmd"]);
    assert_eq!(shells[0].program, powershell);
    assert_eq!(shells[1].program, cmd);
}

/// A relative `PATH` entry is read against the folder the app runs in: passed over.
#[test]
fn a_relative_path_entry_is_passed_over() {
    // Tests run in the package folder, so `src` holds this crate's sources.
    let relative = ShellEnv {
        path: Some(OsString::from("src")),
        ..ShellEnv::default()
    };
    assert!(Path::new("src").join("shells.rs").is_file());
    assert_eq!(on_path(Platform::Windows, &relative, "shells.rs"), None);
    let absolute = ShellEnv {
        path: Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .into_os_string(),
        ),
        ..ShellEnv::default()
    };
    assert!(on_path(Platform::Windows, &absolute, "shells.rs").is_some());
}
