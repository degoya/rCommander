//! The words of the theme editor (RC-100-50): each token's label and the groups the tokens stand
//! in, in the editor's order. A label is a key of its own per token, so the catalogues hold every
//! one (`repo_lints/i18n_keys.rs`).

use crate::i18n::t;

/// A group of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Window,
    Text,
    Accent,
    Lists,
    Controls,
    Syntax,
    Terminal,
}

/// The groups in their order, each with its tokens; the accent itself stands first in its group.
pub const GROUPS: [(Group, &[&str]); 7] = [
    (Group::Window, &["window", "surface", "header", "stroke"]),
    (Group::Text, &["text", "text_secondary"]),
    (
        Group::Accent,
        &[
            "cursor_fill",
            "cursor_fill_inactive",
            "selected_fill",
            "cursor_text",
            "marked",
        ],
    ),
    (
        Group::Lists,
        &[
            "list_fill",
            "list_text",
            "list_text_secondary",
            "stripe_fill",
        ],
    ),
    (Group::Controls, &["control"]),
    (
        Group::Syntax,
        &[
            "syntax.keyword",
            "syntax.key",
            "syntax.string",
            "syntax.number",
            "syntax.literal",
            "syntax.comment",
            "syntax.punctuation",
            "syntax.operator",
            "syntax.type_name",
            "syntax.function",
            "syntax.invalid",
        ],
    ),
    (
        Group::Terminal,
        &[
            "terminal.foreground",
            "terminal.background",
            "terminal.black",
            "terminal.red",
            "terminal.green",
            "terminal.yellow",
            "terminal.blue",
            "terminal.magenta",
            "terminal.cyan",
            "terminal.white",
            "terminal.bright_black",
            "terminal.bright_red",
            "terminal.bright_green",
            "terminal.bright_yellow",
            "terminal.bright_blue",
            "terminal.bright_magenta",
            "terminal.bright_cyan",
            "terminal.bright_white",
        ],
    ),
];

/// The group's title.
pub fn group_title(group: Group) -> &'static str {
    match group {
        Group::Window => t!("theme.group.window"),
        Group::Text => t!("theme.group.text"),
        Group::Accent => t!("theme.group.accent"),
        Group::Lists => t!("theme.group.lists"),
        Group::Controls => t!("theme.group.controls"),
        Group::Syntax => t!("theme.group.syntax"),
        Group::Terminal => t!("theme.group.terminal"),
    }
}

/// The label of the token `key`; the key itself for one no theme has.
pub fn token_label(key: &str) -> &str {
    match key {
        "window" => t!("theme.token.window"),
        "surface" => t!("theme.token.surface"),
        "header" => t!("theme.token.header"),
        "stroke" => t!("theme.token.stroke"),
        "text" => t!("theme.token.text"),
        "text_secondary" => t!("theme.token.text_secondary"),
        "cursor_fill" => t!("theme.token.cursor_fill"),
        "cursor_fill_inactive" => t!("theme.token.cursor_fill_inactive"),
        "selected_fill" => t!("theme.token.selected_fill"),
        "cursor_text" => t!("theme.token.cursor_text"),
        "marked" => t!("theme.token.marked"),
        "list_fill" => t!("theme.token.list_fill"),
        "list_text" => t!("theme.token.list_text"),
        "list_text_secondary" => t!("theme.token.list_text_secondary"),
        "stripe_fill" => t!("theme.token.stripe_fill"),
        "control" => t!("theme.token.control"),
        "syntax.keyword" => t!("theme.token.syntax.keyword"),
        "syntax.key" => t!("theme.token.syntax.key"),
        "syntax.string" => t!("theme.token.syntax.string"),
        "syntax.number" => t!("theme.token.syntax.number"),
        "syntax.literal" => t!("theme.token.syntax.literal"),
        "syntax.comment" => t!("theme.token.syntax.comment"),
        "syntax.punctuation" => t!("theme.token.syntax.punctuation"),
        "syntax.operator" => t!("theme.token.syntax.operator"),
        "syntax.type_name" => t!("theme.token.syntax.type_name"),
        "syntax.function" => t!("theme.token.syntax.function"),
        "syntax.invalid" => t!("theme.token.syntax.invalid"),
        "terminal.foreground" => t!("theme.token.terminal.foreground"),
        "terminal.background" => t!("theme.token.terminal.background"),
        "terminal.black" => t!("theme.token.terminal.black"),
        "terminal.red" => t!("theme.token.terminal.red"),
        "terminal.green" => t!("theme.token.terminal.green"),
        "terminal.yellow" => t!("theme.token.terminal.yellow"),
        "terminal.blue" => t!("theme.token.terminal.blue"),
        "terminal.magenta" => t!("theme.token.terminal.magenta"),
        "terminal.cyan" => t!("theme.token.terminal.cyan"),
        "terminal.white" => t!("theme.token.terminal.white"),
        "terminal.bright_black" => t!("theme.token.terminal.bright_black"),
        "terminal.bright_red" => t!("theme.token.terminal.bright_red"),
        "terminal.bright_green" => t!("theme.token.terminal.bright_green"),
        "terminal.bright_yellow" => t!("theme.token.terminal.bright_yellow"),
        "terminal.bright_blue" => t!("theme.token.terminal.bright_blue"),
        "terminal.bright_magenta" => t!("theme.token.terminal.bright_magenta"),
        "terminal.bright_cyan" => t!("theme.token.terminal.bright_cyan"),
        "terminal.bright_white" => t!("theme.token.terminal.bright_white"),
        other => other,
    }
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
