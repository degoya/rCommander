use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    ShellChoice, WSL_SHELL, WslLaunch, choose_distro, choose_shell, wsl_profiles, wsl_program,
};
use crate::{HomeReason, Platform, ShellEnv, TermError, WslCwd};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|&name| name.to_owned()).collect()
}

fn launch(distro: Option<&str>, cwd: WslCwd) -> WslLaunch {
    WslLaunch {
        program: PathBuf::from("wsl.exe"),
        distro: distro.map(Into::into),
        cwd,
    }
}

#[test]
fn the_command_names_the_distribution_and_the_folder_as_separate_arguments() {
    let command = launch(
        Some("Ubuntu"),
        WslCwd::Path("/mnt/c/Users/x/My Documents".to_owned()),
    )
    .command()
    .expect("a command");
    assert_eq!(command.program, PathBuf::from("wsl.exe"));
    assert_eq!(
        command.args,
        ["-d", "Ubuntu", "--cd", "/mnt/c/Users/x/My Documents"]
    );
    assert_eq!(command.cwd, None);
    assert!(
        command.end_programs_on_close,
        "on unless the start says otherwise"
    );

    let default = launch(None, WslCwd::Home(HomeReason::Network))
        .command()
        .expect("a command");
    assert_eq!(default.args, ["--cd", "~"]);
}

#[test]
fn a_distribution_name_that_could_be_an_option_is_refused() {
    for name in [
        "",
        "-e",
        "--exec",
        "Ubuntu --exec rm",
        "a\"b",
        "a\nb",
        "a\tb",
    ] {
        let refused = launch(Some(name), WslCwd::Path("/".to_owned())).command();
        assert!(
            matches!(&refused, Err(TermError::UnknownDistro(shown)) if shown == name),
            "{name:?}: {refused:?}"
        );
    }
}

#[test]
fn a_distribution_is_chosen_among_the_installed_ones() {
    let installed = names(&["Ubuntu-24.04", "Debian"]);
    assert_eq!(choose_distro(&installed, None).expect("default"), None);
    assert_eq!(
        choose_distro(&installed, Some("debian")).expect("found"),
        Some("Debian".to_owned()),
        "ignoring case, spelled as WSL spells it"
    );
    assert!(matches!(
        choose_distro(&installed, Some("Arch")),
        Err(TermError::UnknownDistro(name)) if name == "Arch"
    ));
    assert!(matches!(
        choose_distro(&installed, Some("-e")),
        Err(TermError::UnknownDistro(_))
    ));
    for asked in [None, Some("Ubuntu")] {
        assert!(matches!(
            choose_distro(&[], asked),
            Err(TermError::WslUnavailable)
        ));
    }
}

/// The owner's Windows test (2026-10-18): a panel in `\\wsl.localhost\Debian\home\...` opened
/// PowerShell. Asked for no shell, a folder in a distribution's share is that distribution's.
#[test]
fn a_folder_in_a_distributions_share_opens_its_shell() {
    let wsl = |name: &str| ShellChoice::Wsl(Some(name.to_owned()));
    for (folder, distro) in [
        (r"\\wsl.localhost\Debian\home\x", "Debian"),
        (r"\\wsl.localhost\Debian", "Debian"),
        (r"\\WSL.LOCALHOST\debian\home", "debian"),
        (r"\\wsl$\Ubuntu-24.04\home\x", "Ubuntu-24.04"),
        (r"\\?\UNC\wsl.localhost\Debian\tmp", "Debian"),
        ("//wsl.localhost/Debian/home/x", "Debian"),
    ] {
        assert_eq!(
            choose_shell(None, None, Path::new(folder)),
            wsl(distro),
            "{folder}"
        );
    }
    for folder in [
        r"C:\Users\x",
        r"\\server\share\x",
        r"\\wsl.localhost",
        "relative",
        "/home/x",
    ] {
        assert_eq!(
            choose_shell(None, None, Path::new(folder)),
            ShellChoice::Local(None),
            "{folder}: the default shell"
        );
    }
}

#[test]
fn the_shell_asked_for_wins_over_the_folder() {
    let share = Path::new(r"\\wsl.localhost\Debian\home\x");
    let drive = Path::new(r"C:\Users\x");
    assert_eq!(
        choose_shell(Some("pwsh"), None, share),
        ShellChoice::Local(Some("pwsh".to_owned())),
        "the picker's local shell"
    );
    assert_eq!(
        choose_shell(Some(WSL_SHELL), Some("Ubuntu"), share),
        ShellChoice::Wsl(Some("Ubuntu".to_owned())),
        "the picker's distribution"
    );
    assert_eq!(
        choose_shell(None, Some("Ubuntu"), drive),
        ShellChoice::Wsl(Some("Ubuntu".to_owned()))
    );
    assert_eq!(
        choose_shell(Some(WSL_SHELL), None, share),
        ShellChoice::Wsl(Some("Debian".to_owned())),
        "WSL without a distribution: the share's"
    );
    assert_eq!(
        choose_shell(Some(WSL_SHELL), None, drive),
        ShellChoice::Wsl(None),
        "WSL's default"
    );
}

/// A folder name is never a shell's text: the share's distribution is refused unless it is a
/// plain name, and the folder reaches `wsl.exe` as one argument after `--cd`.
#[test]
fn a_hostile_folder_name_stays_one_argument() {
    let folder = Path::new(r"\\wsl.localhost\Debian\home\x; rm -rf ~ $(id)\--exec");
    let ShellChoice::Wsl(distro) = choose_shell(None, None, folder) else {
        panic!("a WSL shell");
    };
    let cwd = crate::windows_to_wsl_path(folder, distro.as_deref());
    let command = launch(distro.as_deref(), cwd).command().expect("a command");
    assert_eq!(
        command.args,
        ["-d", "Debian", "--cd", "/home/x; rm -rf ~ $(id)/--exec"]
    );
    assert_eq!(
        choose_shell(None, None, Path::new(r"\\wsl.localhost\-e x\home")),
        ShellChoice::Wsl(Some("-e x".to_owned())),
        "taken from the share as it is ..."
    );
    assert!(
        matches!(
            launch(Some("-e x"), WslCwd::Path("/".to_owned())).command(),
            Err(TermError::UnknownDistro(_))
        ),
        "... and refused before it reaches wsl.exe"
    );
}

#[test]
fn every_detected_distribution_is_a_profile_on_windows_only() {
    let env = ShellEnv::default();
    let distros = names(&["Ubuntu", "Debian"]);
    let profiles = wsl_profiles(Platform::Windows, &env, &distros);
    assert_eq!(profiles.len(), 2);
    for (profile, name) in profiles.iter().zip(["Ubuntu", "Debian"]) {
        assert_eq!(profile.id, WSL_SHELL);
        assert_eq!(profile.label_key, "terminal.shell.wsl");
        assert_eq!(profile.distro.as_deref(), Some(name));
        assert_eq!(profile.args, ["-d", name]);
        assert_eq!(profile.program, PathBuf::from("wsl.exe"));
    }
    assert!(wsl_profiles(Platform::Windows, &env, &[]).is_empty());
    assert!(wsl_profiles(Platform::Unix, &env, &distros).is_empty());
}

#[test]
fn wsl_exe_is_taken_from_system32_before_path() {
    let root = tempfile::tempdir().expect("temp dir");
    let system32 = root.path().join("Windows").join("System32");
    let elsewhere = root.path().join("bin");
    fs::create_dir_all(&system32).expect("mkdir");
    fs::create_dir_all(&elsewhere).expect("mkdir");
    fs::write(elsewhere.join("wsl.exe"), b"").expect("write");
    let env = ShellEnv {
        path: Some(elsewhere.clone().into_os_string()),
        system_root: Some(root.path().join("Windows")),
        ..ShellEnv::default()
    };
    assert_eq!(wsl_program(&env), elsewhere.join("wsl.exe"));
    fs::write(system32.join("wsl.exe"), b"").expect("write");
    assert_eq!(wsl_program(&env), system32.join("wsl.exe"));
    assert_eq!(wsl_program(&ShellEnv::default()), PathBuf::from("wsl.exe"));
}

#[cfg(not(windows))]
#[tokio::test]
async fn there_is_no_wsl_shell_outside_windows() {
    let opened = crate::TermSession::open_wsl(
        crate::TermId::next(),
        &launch(None, WslCwd::Home(HomeReason::NotAbsolute)),
        PathBuf::from("/"),
        crate::TermSize::default(),
    );
    assert!(matches!(opened, Err(TermError::WslUnavailable)));
}
