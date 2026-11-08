//! A file list's git marks (RC-100-97): a letter at the right edge of a file's name cell - M
//! changed, A added, U untracked, R renamed, C conflict - and a dot on a folder with changes
//! below it, in the colour of the most important one; ignored entries dimmed; the branch at the
//! right of the panel's status line (`main` with the commits ahead and behind, `HEAD abc1234`
//! when detached), the whole name in a tooltip, and the files deleted from the folder counted
//! beside it; a column set's field `git` as a word. The colours are the theme's status tones -
//! no colour of its own - and a name keeps the colour its file-type rule gives it.

use iced::{Color, Element};
use rc_engine::git::{GitBranch, GitMark, GitView};

use super::rows::cell;
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, ellipsis, fit},
};

/// How much of its colour an ignored entry's name keeps, as a hidden entry's icon does.
const DIMMED: f32 = 0.55;
/// The widest a branch's name stands in the status line before it is shortened in the middle.
const BRANCH_WIDTH: f32 = 220.0;
/// How many characters of a detached HEAD's commit are shown.
const SHORT_OID: usize = 7;

/// The letter of a file's `mark`; `None` for an ignored one, which is dimmed instead.
pub(super) fn letter(mark: GitMark) -> Option<&'static str> {
    match mark {
        GitMark::Modified => Some(t!("git.letter.modified")),
        GitMark::Added => Some(t!("git.letter.added")),
        GitMark::Untracked => Some(t!("git.letter.untracked")),
        GitMark::Renamed => Some(t!("git.letter.renamed")),
        GitMark::Conflict => Some(t!("git.letter.conflict")),
        GitMark::Ignored => None,
    }
}

/// The colour of `mark`: a conflict as an error, a change as a warning, something new as a
/// success, a rename in the accent.
pub(super) fn colour(mark: GitMark, tokens: &Tokens) -> Color {
    match mark {
        GitMark::Conflict => theme::DANGER,
        GitMark::Modified => theme::WARNING,
        GitMark::Added | GitMark::Untracked => theme::SUCCESS,
        GitMark::Renamed => tokens.accent,
        GitMark::Ignored => tokens.list_text_secondary,
    }
}

/// What stands at the right edge of an entry's name cell for `mark`: a file's letter, a
/// `folder`'s dot; nothing without a mark or for an ignored entry.
pub(super) fn badge(
    mark: Option<GitMark>,
    folder: bool,
    tokens: &Tokens,
) -> Option<(&'static str, Color)> {
    let mark = mark.filter(|mark| *mark != GitMark::Ignored)?;
    let symbol = if folder {
        t!("git.folder_dot")
    } else {
        letter(mark)?
    };
    Some((symbol, colour(mark, tokens)))
}

/// Whether an entry with `mark` is drawn dimmed: an ignored one.
pub(super) fn dimmed(mark: Option<GitMark>) -> bool {
    mark == Some(GitMark::Ignored)
}

/// `colour` as an ignored entry's name is drawn.
pub(super) fn dim(colour: Color) -> Color {
    colour.scale_alpha(DIMMED)
}

/// The git status as a column set's `git` column says it; empty without a mark.
pub(super) fn word(mark: Option<GitMark>) -> &'static str {
    match mark {
        None => "",
        Some(GitMark::Modified) => t!("git.modified"),
        Some(GitMark::Added) => t!("git.added"),
        Some(GitMark::Untracked) => t!("git.untracked"),
        Some(GitMark::Renamed) => t!("git.renamed"),
        Some(GitMark::Conflict) => t!("git.conflict"),
        Some(GitMark::Ignored) => t!("git.ignored"),
    }
}

/// The branch as the status line shows it: its name (shortened in the middle when long) or
/// `HEAD` and the commit when detached, then the commits ahead and behind its upstream.
pub(super) fn branch_text(branch: &GitBranch) -> String {
    let mut parts = vec![match (&branch.head, &branch.oid) {
        (Some(head), _) => {
            ellipsis::middle_ellipsis(head, BRANCH_WIDTH, theme::STATUS_SIZE).into_owned()
        }
        (None, Some(oid)) => t!("git.detached", oid = short(oid)),
        (None, None) => t!("git.detached", oid = ""),
    }];
    if branch.ahead > 0 {
        parts.push(t!("git.ahead", count = branch.ahead));
    }
    if branch.behind > 0 {
        parts.push(t!("git.behind", count = branch.behind));
    }
    parts.join(" ").trim_end().to_owned()
}

/// The branch's whole name and its upstream, for the tooltip.
pub(super) fn branch_tip(branch: &GitBranch) -> String {
    let named = match (&branch.head, &branch.oid) {
        (Some(head), _) => t!("git.tip", branch = head),
        (None, oid) => t!("git.tip_detached", oid = oid.as_deref().unwrap_or_default()),
    };
    match &branch.upstream {
        Some(upstream) => t!("git.tip_upstream", named = named, upstream = upstream),
        None => named,
    }
}

/// The first characters of a commit's id.
fn short(oid: &str) -> &str {
    oid.char_indices()
        .nth(SHORT_OID)
        .map_or(oid, |(end, _)| &oid[..end])
}

/// The status line's git part for a panel with `git`: the branch (its whole name in a tooltip)
/// and the files deleted from the folder; nothing without either.
pub(super) fn status_part<'a>(
    git: Option<&GitView>,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let git = git?;
    let mut parts = Vec::new();
    if let Some(branch) = &git.branch {
        parts.push(branch_text(branch));
    }
    if git.deleted > 0 {
        let count = crate::format::count(git.deleted as u64);
        parts.push(t!("git.deleted", count = count));
    }
    if parts.is_empty() {
        return None;
    }
    let line = cell(parts.join("  "), theme::UI_FONT)
        .size(theme::STATUS_SIZE)
        .color(tokens.text_secondary);
    Some(match &git.branch {
        Some(branch) => fit::tip(line, branch_tip(branch), tokens),
        None => line.into(),
    })
}

#[cfg(test)]
#[path = "git_marks_tests.rs"]
mod tests;
