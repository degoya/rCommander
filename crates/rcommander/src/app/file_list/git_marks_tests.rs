use rc_engine::{
    VfsPath,
    git::{GitBranch, GitMark, GitView},
};
use serde_json::json;

use super::{badge, branch_text, branch_tip, colour, dimmed, status_part, word};
use crate::theme;

fn branch(head: Option<&str>, oid: Option<&str>, ahead: u32, behind: u32) -> GitBranch {
    GitBranch {
        head: head.map(str::to_owned),
        oid: oid.map(str::to_owned),
        upstream: None,
        ahead,
        behind,
    }
}

/// A file carries its letter, a folder a dot, both in their mark's colour; an ignored entry no
/// symbol but is dimmed.
#[test]
fn files_carry_a_letter_and_folders_a_dot() {
    let tokens = theme::tokens(false);
    assert_eq!(
        badge(Some(GitMark::Modified), false, tokens),
        Some(("M", theme::WARNING))
    );
    assert_eq!(
        badge(Some(GitMark::Untracked), false, tokens),
        Some(("U", theme::SUCCESS))
    );
    assert_eq!(
        badge(Some(GitMark::Conflict), true, tokens),
        Some(("\u{25cf}", theme::DANGER))
    );
    assert_eq!(badge(Some(GitMark::Ignored), false, tokens), None);
    assert_eq!(badge(None, false, tokens), None);
    assert!(dimmed(Some(GitMark::Ignored)));
    assert!(!dimmed(Some(GitMark::Modified)));
    assert_eq!(colour(GitMark::Renamed, tokens), tokens.accent);
}

#[test]
fn the_column_says_the_status_in_words() {
    assert_eq!(word(Some(GitMark::Modified)), "Modified");
    assert_eq!(word(Some(GitMark::Ignored)), "Ignored");
    assert_eq!(word(None), "");
}

/// `main ↑1 ↓2`; detached `HEAD` and the commit's first seven characters; the tooltip names the
/// whole branch and its upstream.
#[test]
fn the_branch_reads_as_in_a_prompt() {
    assert_eq!(
        branch_text(&branch(Some("main"), Some("0123456789"), 1, 2)),
        "main \u{2191}1 \u{2193}2"
    );
    assert_eq!(branch_text(&branch(Some("main"), None, 0, 0)), "main");
    assert_eq!(
        branch_text(&branch(None, Some("abc1234def"), 0, 3)),
        "HEAD abc1234 \u{2193}3"
    );
    let mut followed = branch(Some("feature/long-name"), None, 0, 0);
    followed.upstream = Some("origin/feature/long-name".into());
    assert_eq!(
        branch_tip(&followed),
        "Branch feature/long-name, following origin/feature/long-name"
    );
    assert_eq!(
        branch_tip(&branch(None, Some("abc1234def"), 0, 0)),
        "Detached HEAD at abc1234def"
    );
}

/// A view as the engine hands it to the window.
fn view(branch: Option<&str>, deleted: usize) -> GitView {
    let folder = serde_json::to_value(VfsPath::local("/repo")).expect("path");
    let branch = branch.map(|head| json!({ "head": head, "ahead": 0, "behind": 0 }));
    serde_json::from_value(json!({
        "folder": folder,
        "branch": branch,
        "marks": { "a.txt": "modified" },
        "deleted": deleted,
    }))
    .expect("view")
}

/// The status line has a git part while there is a branch or a deleted file to tell; the marks
/// read by name.
#[test]
fn the_status_line_tells_the_branch_and_the_deleted() {
    let tokens = theme::tokens(false);
    assert!(status_part(Some(&view(Some("main"), 0)), tokens).is_some());
    assert!(status_part(Some(&view(None, 2)), tokens).is_some());
    assert!(status_part(Some(&view(None, 0)), tokens).is_none());
    assert!(status_part(None, tokens).is_none());
    assert_eq!(view(None, 0).mark("a.txt"), Some(GitMark::Modified));
}

/// The window over the whole way (RC-100-97): with the system's git, a repository with a changed,
/// a new and an ignored file; switching `panels.git_status` on makes the engine announce
/// `PanelChange::Git`, which `App::update` takes into the pane - the rows show the letter and
/// the dot, the ignored file is dimmed, the status line has the branch. Skipped without a git
/// of the checked version.
#[cfg(unix)]
mod window {
    use std::{
        path::{Path, PathBuf},
        process::Stdio,
        time::Duration,
    };

    use rc_engine::{PanelId, git::MIN_VERSION, settings::GIT_STATUS_KEY};
    use rc_process::NoConsoleWindow;
    use serde_json::json;

    use super::*;
    use crate::app::harness::{Harness, tree};

    /// The first `git` of the absolute folders of `PATH`.
    fn system_git() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .filter(|folder| folder.is_absolute())
            .map(|folder| folder.join("git"))
            .find(|candidate| candidate.is_file())
    }

    fn git(program: &Path, dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new(program)
            .current_dir(dir)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .no_console_window()
            .output()
            .expect("git runs");
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Whether `git --version` names at least [`MIN_VERSION`].
    fn recent(program: &Path, dir: &Path) -> bool {
        let said = git(program, dir, &["--version"]);
        let mut numbers = said
            .trim()
            .trim_start_matches("git version ")
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0));
        let found = (
            numbers.next().unwrap_or(0),
            numbers.next().unwrap_or(0),
            numbers.next().unwrap_or(0),
        );
        found >= (MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2)
    }

    #[tokio::test]
    async fn the_marks_reach_the_rows_and_the_status_line() {
        let Some(program) = system_git() else {
            eprintln!("no git on the PATH: skipped");
            return;
        };
        let root = tree(&[
            ("README.md", "readme\n"),
            ("src/lib.rs", "one\n"),
            (".gitignore", "*.log\n"),
        ]);
        let repo = root.path();
        if !recent(&program, repo) {
            eprintln!("git older than {MIN_VERSION}: skipped");
            return;
        }
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
        let head = git(&program, repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
        std::fs::write(repo.join("src/lib.rs"), "two\n").expect("write");
        std::fs::write(repo.join("new.txt"), "new\n").expect("write");
        std::fs::write(repo.join("debug.log"), "log\n").expect("write");

        let mut harness = Harness::with_db(repo).await;
        harness
            .app
            .engine()
            .set_protected_setting(GIT_STATUS_KEY, &json!("local"))
            .await
            .expect("switched on");
        let mut arrived = false;
        for _ in 0..250 {
            harness.settle().await;
            if harness.app.pane(PanelId::Left).state.git.is_some() {
                arrived = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(arrived, "no PanelChange::Git reached the window");

        let tokens = theme::tokens(false);
        let git = harness
            .app
            .pane(PanelId::Left)
            .state
            .git
            .clone()
            .expect("marks");
        assert_eq!(
            badge(git.mark("new.txt"), false, tokens),
            Some(("U", theme::SUCCESS))
        );
        assert_eq!(
            badge(git.mark("src"), true, tokens),
            Some(("\u{25cf}", theme::WARNING))
        );
        assert!(dimmed(git.mark("debug.log")));
        assert_eq!(badge(git.mark("README.md"), false, tokens), None);
        let branch = git.branch.as_ref().expect("the branch");
        assert_eq!(branch_text(branch), head.trim());
        assert!(status_part(Some(&git), tokens).is_some());
    }
}
