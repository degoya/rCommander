use crate::{Command, Engine, Outcome, PanelId, test_support};

async fn refresh(engine: &Engine) {
    engine
        .dispatch(Command::Refresh {
            panel: Some(PanelId::Left),
        })
        .await
        .expect("refresh");
}

/// Without a database nothing is switched on, and `panel.git_status` answers what the panel
/// knows: nothing.
#[tokio::test]
async fn the_command_reports_what_the_panel_knows() {
    let (_root, engine) = test_support::engine(&[(".git/", ""), ("a.txt", "a")]).await;
    refresh(&engine).await;
    match engine
        .dispatch(Command::GitStatus { panel: None })
        .await
        .expect("answered")
    {
        Outcome::GitStatus { panel, git } => {
            assert_eq!(panel, PanelId::Left);
            assert_eq!(git, None);
        }
        other => panic!("{other:?}"),
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Stdio,
    };

    use std::time::Duration;

    use rc_process::NoConsoleWindow;
    use serde_json::json;

    use super::*;
    use crate::{
        git::{GitMark, MIN_VERSION, run},
        settings::{GIT_PROGRAM_KEY, GIT_STATUS_KEY},
    };

    /// Waits up to five seconds for `done`.
    async fn until(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..250 {
            if done() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    /// A stand-in for git that writes each call's arguments to `log` and answers `--version`
    /// with `version`.
    fn fake_git(dir: &Path, log: &Path, version: &str) -> PathBuf {
        let path = dir.join("fake-git");
        let body = format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\n\
             case \"$1\" in --version) echo 'git version {version}';; esac\n",
            log.display()
        );
        fs::write(&path, body).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    async fn use_program(engine: &Engine, program: &Path) {
        engine
            .set_protected_setting(GIT_PROGRAM_KEY, &json!(program.to_str().expect("utf-8")))
            .await
            .expect("program");
    }

    async fn switch(engine: &Engine, scope: &str) {
        engine
            .set_protected_setting(GIT_STATUS_KEY, &json!(scope))
            .await
            .expect("scope");
    }

    /// Owner 2026-11-07: off by default, and then even a navigation - an agent's over MCP
    /// among them - starts no process, in a repository too.
    #[tokio::test]
    async fn navigation_starts_no_git_when_off() {
        let (root, engine) =
            test_support::engine_with_db(&[(".git/", ""), ("src/a.txt", "a")]).await;
        let tools = tempfile::tempdir().expect("temp dir");
        let log = tools.path().join("calls");
        use_program(&engine, &fake_git(tools.path(), &log, "2.45.0")).await;
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: root.path().join("src").to_string_lossy().into_owned(),
            })
            .await
            .expect("navigate");
        refresh(&engine).await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(!log.exists(), "git was started");
        assert_eq!(engine.panel(PanelId::Left).git, None);
    }

    /// Outside a repository no process starts, the setting on or not.
    #[tokio::test]
    async fn outside_a_repository_nothing_starts() {
        let (root, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
        if crate::git::discover::repository_root(root.path()).is_some() {
            eprintln!("the temporary folder lies in a repository: skipped");
            return;
        }
        let tools = tempfile::tempdir().expect("temp dir");
        let log = tools.path().join("calls");
        use_program(&engine, &fake_git(tools.path(), &log, "2.45.0")).await;
        switch(&engine, "local").await;
        refresh(&engine).await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert!(!log.exists(), "git was started");
    }

    /// A git older than the checked version gives no marks: it is asked its version, nothing
    /// else.
    #[tokio::test]
    async fn an_old_git_gives_no_marks() {
        let (_root, engine) = test_support::engine_with_db(&[(".git/", ""), ("a.txt", "a")]).await;
        let tools = tempfile::tempdir().expect("temp dir");
        let log = tools.path().join("calls");
        use_program(&engine, &fake_git(tools.path(), &log, "2.20.0")).await;
        switch(&engine, "local").await;
        refresh(&engine).await;
        assert!(until(|| log.exists()).await, "the version was not asked");
        tokio::time::sleep(Duration::from_millis(600)).await;
        let calls = fs::read_to_string(&log).expect("calls");
        assert!(
            calls.lines().all(|line| line == "--version"),
            "more than the version: {calls}"
        );
        assert_eq!(engine.panel(PanelId::Left).git, None);
    }

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

    /// The whole way with the system's git: the marks and the branch arrive after a refresh,
    /// `panel.git_status` reports them, switching off takes them away. Skipped without a git
    /// of the checked version.
    #[tokio::test]
    async fn the_marks_arrive_and_go() {
        let Some(program) = run::search_system() else {
            eprintln!("no git on the PATH: skipped");
            return;
        };
        let host = run::Host::Native(program.clone());
        if !run::version(&host)
            .await
            .ok()
            .flatten()
            .is_some_and(|found| found >= MIN_VERSION)
        {
            eprintln!("git older than {MIN_VERSION}: skipped");
            return;
        }
        let (root, engine) = test_support::engine_with_db(&[
            ("README.md", "readme\n"),
            ("src/lib.rs", "one\n"),
            (".gitignore", "*.log\n"),
        ])
        .await;
        let repo = root.path();
        git(&program, repo, &["init", "-q"]);
        git(
            &program,
            repo,
            &["add", "README.md", "src/lib.rs", ".gitignore"],
        );
        git(
            &program,
            repo,
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
        fs::write(repo.join("src/lib.rs"), "two\n").expect("write");
        fs::write(repo.join("new.txt"), "new\n").expect("write");
        fs::write(repo.join("debug.log"), "log\n").expect("write");
        switch(&engine, "local").await;
        refresh(&engine).await;
        assert!(
            until(|| engine.panel(PanelId::Left).git.is_some()).await,
            "no marks arrived"
        );
        let view = engine.panel(PanelId::Left).git.expect("marks");
        assert_eq!(view.mark("src"), Some(GitMark::Modified));
        assert_eq!(view.mark("new.txt"), Some(GitMark::Untracked));
        assert_eq!(view.mark("debug.log"), Some(GitMark::Ignored));
        assert_eq!(view.mark("README.md"), None);
        let branch = view.branch.clone().expect("the branch");
        assert!(branch.oid.is_some() && branch.head.is_some(), "{branch:?}");

        let snapshot = engine.panel_state(None);
        assert_eq!(snapshot.git, Some(branch));
        match engine
            .dispatch(Command::GitStatus { panel: None })
            .await
            .expect("answered")
        {
            Outcome::GitStatus { git: Some(git), .. } => assert_eq!(*git, *view),
            other => panic!("{other:?}"),
        }

        switch(&engine, "off").await;
        assert!(
            until(|| engine.panel(PanelId::Left).git.is_none()).await,
            "the marks stayed"
        );
    }
}
