use std::{ffi::OsString, fs, path::PathBuf};

use super::{
    Host, MIN_VERSION, Version, filter_names, filters_call, parse_version, search, status_call,
};

#[test]
fn the_version_is_read_from_its_line() {
    assert_eq!(
        parse_version("git version 2.45.1\n"),
        Some(Version(2, 45, 1))
    );
    assert_eq!(
        parse_version("git version 2.47.0.windows.2"),
        Some(Version(2, 47, 0))
    );
    assert_eq!(parse_version("git version 2.36"), Some(Version(2, 36, 0)));
    assert_eq!(parse_version("hello"), None);
    assert!(Version(2, 35, 9) < MIN_VERSION);
    assert!(Version(2, 36, 0) >= MIN_VERSION);
}

/// Only absolute folders of `PATH` are searched: a relative entry - the panel's folder, the
/// current one - never.
#[test]
fn the_program_is_searched_only_in_absolute_folders() {
    let dir = tempfile::tempdir().expect("temp dir");
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).expect("mkdir");
    let program = bin.join(if cfg!(windows) { "git.exe" } else { "git" });
    fs::write(&program, "").expect("write");
    let path = std::env::join_paths([PathBuf::from("."), PathBuf::from("relative"), bin.clone()])
        .expect("PATH");
    assert_eq!(search(Some(path.as_os_str()), None), Some(program));
    let only_relative =
        std::env::join_paths([PathBuf::from("."), PathBuf::from("bin")]).expect("PATH");
    assert_eq!(search(Some(only_relative.as_os_str()), None), None);
    assert_eq!(search(None, None), None);
}

/// The status runs hardened: no optional locks, no fsmonitor, no untracked cache, in the root
/// with the folder as the pathspec; every filter driver emptied through the environment.
#[test]
fn the_status_call_carries_the_hardening() {
    let call = status_call("/repo", "src/app", &["lfs".to_owned()], true);
    let args: Vec<String> = call
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-C",
            "/repo",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
            "--ignore-submodules=all",
            "--ignored=traditional",
            "--",
            "src/app",
        ]
    );
    let env = |name: &str| {
        call.env
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(env("GIT_TERMINAL_PROMPT"), Some("0"));
    assert_eq!(env("LC_ALL"), Some("C"));
    assert_eq!(env("GIT_LITERAL_PATHSPECS"), Some("1"));
    assert_eq!(env("GIT_CONFIG_COUNT"), Some("4"));
    let pairs: Vec<(&str, &str)> = (0..4)
        .map(|index| {
            (
                env(&format!("GIT_CONFIG_KEY_{index}")).unwrap_or_default(),
                env(&format!("GIT_CONFIG_VALUE_{index}")).unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("filter.lfs.clean", ""),
            ("filter.lfs.smudge", ""),
            ("filter.lfs.process", ""),
            ("filter.lfs.required", "false"),
        ]
    );
    let plain = status_call("/repo", "", &[], false);
    assert!(
        !plain
            .args
            .contains(&OsString::from("--ignored=traditional"))
    );
    assert_eq!(plain.args.last(), Some(&OsString::from(".")));
}

/// The drivers' names from `git config -z --get-regexp`, each once - a name may hold dots,
/// spaces and `=`.
#[test]
fn the_filter_drivers_are_named_once() {
    let answer = b"filter.lfs.clean\ngit-lfs clean -- %f\0filter.lfs.smudge\ngit-lfs smudge\0\
filter.a.b=c.process\nevil\0filter.with space.clean\nx\0";
    assert_eq!(filter_names(answer), ["lfs", "a.b=c", "with space"]);
    assert_eq!(filter_names(b""), Vec::<String>::new());
    let call = filters_call("/repo");
    assert!(call.args.contains(&OsString::from("--get-regexp")));
}

/// In a distribution git is its own `/usr/bin/git`, the environment crosses through `WSLENV`.
#[test]
fn a_distribution_runs_its_own_git() {
    let host = Host::Wsl {
        wsl: PathBuf::from(r"C:\Windows\System32\wsl.exe"),
        distro: "Ubuntu".into(),
    };
    let command = host.command(&status_call("/home/me/repo", "", &[], false));
    let command = command.as_std();
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args[..6],
        [
            "-d",
            "Ubuntu",
            "--cd",
            "/home/me/repo",
            "-e",
            "/usr/bin/git"
        ]
    );
    let wslenv = command
        .get_envs()
        .find(|(key, _)| *key == "WSLENV")
        .and_then(|(_, value)| value)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(wslenv.contains("GIT_TERMINAL_PROMPT/u"), "{wslenv}");
    assert!(wslenv.contains("GIT_CONFIG_COUNT/u"), "{wslenv}");
    assert_eq!(host.key(), "wsl:Ubuntu");
}

/// What would point git elsewhere leaves its environment.
#[test]
fn the_repository_variables_are_removed() {
    let host = Host::Native(PathBuf::from("/usr/bin/git"));
    let command = host.command(&status_call("/repo", "", &[], false));
    let removed: Vec<String> = command
        .as_std()
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect();
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        assert!(removed.iter().any(|key| key == name), "{name}: {removed:?}");
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::Path,
        process::Stdio,
        time::{Duration, Instant},
    };

    use rc_process::NoConsoleWindow;

    use super::*;
    use crate::git::{
        Change, GitMark,
        run::{
            OUTPUT_LIMIT, RunError, TIME_LIMIT, capture, search_system, status, version,
            version_call,
        },
    };

    /// An executable shell script `name` in `dir`.
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// A git that hangs is killed at the time limit.
    #[tokio::test]
    async fn the_time_limit_kills_a_hanging_git() {
        let dir = tempfile::tempdir().expect("temp dir");
        let hanging = script(dir.path(), "git", "exec sleep 30");
        let started = Instant::now();
        let host = Host::Native(hanging);
        let ended = capture(
            host.command(&version_call()),
            Duration::from_millis(300),
            OUTPUT_LIMIT,
        )
        .await;
        assert!(matches!(ended, Err(RunError::TimedOut(_))), "{ended:?}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn too_much_output_gives_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let chatty = script(dir.path(), "git", "exec head -c 100000 /dev/zero");
        let ended = capture(
            Host::Native(chatty).command(&version_call()),
            TIME_LIMIT,
            1000,
        )
        .await;
        assert!(matches!(ended, Err(RunError::TooLong(1000))), "{ended:?}");
    }

    #[tokio::test]
    async fn an_old_git_says_its_version() {
        let dir = tempfile::tempdir().expect("temp dir");
        let old = script(dir.path(), "git", "echo 'git version 2.20.1'");
        let found = version(&Host::Native(old)).await.expect("answered");
        assert_eq!(found, Some(Version(2, 20, 1)));
        assert!(found < Some(MIN_VERSION));
    }

    /// `git` itself with `args` in `dir`, for the setup of a repository.
    fn git(program: &Path, dir: &Path, args: &[&str]) {
        let ran = std::process::Command::new(program)
            .current_dir(dir)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .no_console_window()
            .status()
            .expect("git runs");
        assert!(ran.success(), "git {args:?}");
    }

    /// The hardening (RC-100-97): a repository whose configuration names a script as its
    /// fsmonitor and as the clean filter of every file, with a changed file. git status runs,
    /// the script never does, and the file is changed. Skipped without git.
    #[tokio::test]
    async fn the_repository_s_programs_never_run() {
        let Some(program) = search_system() else {
            eprintln!("no git on the PATH: skipped");
            return;
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).expect("mkdir");
        let marker = dir.path().join("ran");
        let evil = script(
            dir.path(),
            "evil.sh",
            &format!("echo ran >> '{}'\ncat", marker.display()),
        );
        let evil = evil.to_str().expect("utf-8 path");
        git(&program, &repo, &["init", "-q"]);
        fs::write(repo.join("a.txt"), "one\n").expect("write");
        git(&program, &repo, &["add", "a.txt"]);
        git(
            &program,
            &repo,
            &[
                "-c",
                "user.name=rc",
                "-c",
                "user.email=rc@example.org",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                "one",
            ],
        );
        for (key, value) in [
            ("core.fsmonitor", evil),
            ("filter.evil.clean", evil),
            ("filter.evil.smudge", evil),
            ("filter.evil.required", "true"),
        ] {
            git(&program, &repo, &["config", key, value]);
        }
        fs::write(repo.join(".gitattributes"), "* filter=evil\n").expect("write");
        fs::write(repo.join("a.txt"), "two, changed\n").expect("write");
        let _ = fs::remove_file(&marker);

        let found = status(
            &Host::Native(program),
            repo.to_str().expect("utf-8 path"),
            "",
            true,
            TIME_LIMIT,
        )
        .await
        .expect("status");
        assert!(!marker.exists(), "a program of the repository ran");
        let change = |path: &str| {
            found
                .items
                .iter()
                .find(|item| item.path == path)
                .map(|item| item.change)
        };
        assert_eq!(change("a.txt"), Some(Change::Mark(GitMark::Modified)));
        assert_eq!(
            change(".gitattributes"),
            Some(Change::Mark(GitMark::Untracked))
        );
    }
}
