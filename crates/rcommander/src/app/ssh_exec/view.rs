//! Drawing the dialog: the command line, then the end and the two streams, and the row of
//! buttons - Run (Enter), Cancel while the command runs, Close. A card over the connection
//! manager, or a window of its own (RC-080-23, `dialog_windows.rs`).

use iced::{
    Element, Length,
    widget::{Column, column, row, text::Wrapping, text_editor, text_editor::Content, text_input},
};
use rc_engine::ExecOutput;

use super::{ExecDialog, ExecMsg, Stream};
use crate::{
    app::{
        Message,
        viewer::{editor_style, read_only},
    },
    i18n::{self, t},
    theme::{self, Tokens, fit::label_or_glyph, icon::Icon, layout},
};

const STREAM_HEIGHT: f32 = 220.0;

/// The command field, which takes the keyboard when the dialog's window opens.
pub const COMMAND_FIELD: &str = "rcommander.ssh_exec.command";

/// The dialog over the window; `None` while it is closed.
pub fn layer<'a>(
    dialog: Option<&'a ExecDialog>,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let dialog = dialog?;
    let head = label_or_glyph(title(dialog)).font(theme::UI_FONT_STRONG);
    Some(layout::dialog_card(
        Message::dialog_head(head),
        body(dialog, tokens),
        layout::MEDIUM_DIALOG_WIDTH,
        tokens,
    ))
}

/// The dialog's title, with the connection's name: in its card's head, or in its window's
/// title bar.
pub fn title(dialog: &ExecDialog) -> String {
    t!("ssh_exec.title", name = dialog.name)
}

/// The command line, the result and the buttons: on the card, or filling a window of its own.
pub fn body<'a>(dialog: &'a ExecDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut input = text_input(t!("ssh_exec.command_hint"), &dialog.command)
        .id(COMMAND_FIELD)
        .font(theme::MONO_FONT)
        .size(theme::TEXT_SIZE)
        .width(Length::Fill);
    if !dialog.is_running() {
        input = input
            .on_input(|command| Message::SshExec(ExecMsg::Command(command)))
            .on_submit(Message::SshExec(ExecMsg::Run));
    }
    let mut body = column![
        input,
        label_or_glyph(t!("ssh_exec.note")).color(tokens.text_secondary),
    ]
    .spacing(10);
    if dialog.is_running() {
        body = body.push(label_or_glyph(t!("ssh_exec.running")));
    }
    match (&dialog.result, &dialog.shown) {
        (Some(Ok(output)), Some(shown)) => body = body.push(result(output, shown, tokens)),
        (Some(Ok(_)), None) => {}
        (Some(Err(code)), _) => {
            body = body.push(label_or_glyph(i18n::error_text(code)).color(theme::DANGER));
        }
        (None, _) => {}
    }
    let running = dialog.is_running();
    let can_run = !running && !dialog.command.trim().is_empty();
    let mut secondary = Vec::new();
    if running {
        secondary.push(layout::secondary_icon_button(
            Icon::Close,
            t!("ssh_exec.cancel"),
            Some(Message::SshExec(ExecMsg::Cancel)),
        ));
    }
    secondary.push(layout::secondary_icon_button(
        Icon::Close,
        t!("ssh_exec.close"),
        Some(Message::SshExec(ExecMsg::Close)),
    ));
    let buttons = layout::button_row(
        Some(layout::primary_icon_button(
            Icon::Resume,
            t!("ssh_exec.run"),
            can_run.then_some(Message::SshExec(ExecMsg::Run)),
        )),
        secondary,
        [],
    );
    body.push(buttons).into()
}

/// How the command ended, then its output and its errors.
fn result<'a>(
    output: &'a ExecOutput,
    [stdout, stderr]: &'a [Content; 2],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let end = if output.timed_out {
        t!("ssh_exec.timed_out", seconds = output.duration_ms / 1000)
    } else if let Some(signal) = &output.exit_signal {
        t!("ssh_exec.exit_signal", signal = signal)
    } else if let Some(code) = output.exit_code {
        t!("ssh_exec.exit_code", code = code)
    } else {
        t!("ssh_exec.no_status").to_owned()
    };
    column![
        label_or_glyph(end),
        stream(
            (t!("ssh_exec.stdout"), Stream::Out),
            stdout,
            output.stdout_truncated,
            tokens
        ),
        stream(
            (t!("ssh_exec.stderr"), Stream::Err),
            stderr,
            output.stderr_truncated,
            tokens
        ),
    ]
    .spacing(8)
    .into()
}

/// One stream under its title, read only and scrolling, with the note when it was cut off.
fn stream<'a>(
    (title, which): (&'static str, Stream),
    content: &'a Content,
    truncated: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut head = row![label_or_glyph(title).color(tokens.text_secondary)].spacing(8);
    if truncated {
        head = head.push(label_or_glyph(t!("ssh_exec.truncated")).color(tokens.accent));
    }
    let lines = text_editor(content)
        .on_action(move |action| Message::SshExec(ExecMsg::Scroll(which, action)))
        .key_binding(read_only)
        .size(theme::TEXT_SIZE)
        .font(theme::MONO_FONT)
        .wrapping(Wrapping::Glyph)
        .max_height(STREAM_HEIGHT)
        .style(move |_, _| editor_style(tokens));
    Column::new().spacing(4).push(head).push(lines).into()
}
