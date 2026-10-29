//! The questions a job asks in its dialog: a target that exists (both files side by side, the
//! answers of the conflict protocol, "rename to" with an editable proposal) and a failing file
//! (retry, skip, skip all, cancel).

use std::time::{Duration, UNIX_EPOCH};

use iced::{
    Alignment, Element, Length,
    widget::{Space, column, row, text_input},
};
use rc_engine::{ConflictChoice, EntryInfo, ErrorChoice, Question};

use super::{FileOpsMsg, PROPOSAL_ID, jobs::Tracked, wrap};
use crate::{
    app::Message,
    format,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        icon::Icon,
        layout::{button_row, secondary_button, secondary_icon_button},
    },
};

/// A button of a question: an answer, or Cancel.
fn button<'a>(label: &'static str, message: FileOpsMsg) -> Element<'a, Message> {
    secondary_button(label, Some(wrap(message)))
}

/// The Cancel of `job`'s question, which Escape and the card's X answer with; `None` for a
/// question the archive dialogs ask.
pub(super) fn cancel(job: &Tracked) -> Option<FileOpsMsg> {
    let id = job.id();
    match job.snapshot.question.as_ref()? {
        Question::Conflict { .. } => Some(FileOpsMsg::Conflict {
            job: id,
            choice: ConflictChoice::Cancel,
            for_all: false,
        }),
        Question::Error(_) => Some(FileOpsMsg::Error {
            job: id,
            choice: ErrorChoice::Cancel,
            all: false,
        }),
        Question::Password { .. } => None,
    }
}

/// A card's title, for its head, and its body.
pub(super) type Card<'a> = (Element<'a, Message>, Element<'a, Message>);

pub(super) fn card<'a>(job: &'a Tracked, tokens: &'static Tokens) -> Card<'a> {
    let id = job.id();
    match &job.snapshot.question {
        Some(Question::Conflict {
            source,
            target,
            resumable,
        }) => conflict_card(job, source, target, *resumable, tokens),
        Some(Question::Error(failure)) => {
            let error = |choice, all| FileOpsMsg::Error {
                job: id,
                choice,
                all,
            };
            // Enter answers nothing here: no primary.
            let buttons = button_row(
                None,
                [
                    button(t!("ops.error.retry"), error(ErrorChoice::Retry, false)),
                    button(t!("ops.error.skip"), error(ErrorChoice::Skip, false)),
                    button(t!("ops.error.skip_all"), error(ErrorChoice::Skip, true)),
                    secondary_icon_button(
                        Icon::Close,
                        t!("ops.cancel"),
                        Some(wrap(error(ErrorChoice::Cancel, false))),
                    ),
                ],
                [],
            );
            let mut body = column![fit::path_line(
                failure.path.path.display().to_string(),
                tokens.text,
                tokens
            )]
            .spacing(10);
            // A copy between two servers names the side that failed when it was the target
            // (RC-060-06); `path` is always the source.
            if let Some(target) = &failure.target {
                body = body
                    .push(line(t!("ops.error.on_target")))
                    .push(fit::path_line(
                        target.path.display().to_string(),
                        tokens.text,
                        tokens,
                    ));
            }
            let body = body
                .push(fit::prose(error_text(&failure.code)).color(theme::DANGER))
                .push(buttons);
            let title = line(t!("ops.error.title")).font(theme::UI_FONT_STRONG);
            (title.into(), body.into())
        }
        // An archive's password is asked by the archive dialogs (`archive_ops`).
        Some(Question::Password { .. }) | None => (Space::new().into(), Space::new().into()),
    }
}

/// `resumable`: the target is the part file an interrupted copy left, which "Resume" continues
/// (RC-060-07), as Total Commander's overwrite dialog offers.
fn conflict_card<'a>(
    job: &'a Tracked,
    source: &'a EntryInfo,
    target: &'a EntryInfo,
    resumable: bool,
    tokens: &'static Tokens,
) -> Card<'a> {
    let id = job.id();
    let answer = |label: &'static str, choice, for_all| {
        button(
            label,
            FileOpsMsg::Conflict {
                job: id,
                choice,
                for_all,
            },
        )
    };
    let side = |title: &'static str, info: &'a EntryInfo| {
        let modified = info
            .modified_unix
            .map(|secs| UNIX_EPOCH + Duration::from_secs(secs));
        column![
            line(title).color(tokens.text_secondary),
            fit::path_line(info.path.path.display().to_string(), tokens.text, tokens),
            line(format::size(info.size)),
            line(format::date(modified)),
        ]
        .spacing(4)
        .width(Length::FillPortion(1))
    };
    let title = if resumable {
        t!("ops.conflict.title_part")
    } else {
        t!("ops.conflict.title")
    };
    let mut first = row![].spacing(6);
    if resumable {
        first = first.push(answer(
            t!("ops.conflict.resume"),
            ConflictChoice::Resume,
            false,
        ));
    }
    let first = first
        .push(answer(
            t!("ops.conflict.overwrite"),
            ConflictChoice::Overwrite,
            false,
        ))
        .push(answer(
            t!("ops.conflict.overwrite_all"),
            ConflictChoice::Overwrite,
            true,
        ))
        .push(answer(t!("ops.conflict.skip"), ConflictChoice::Skip, false))
        .push(answer(
            t!("ops.conflict.skip_all"),
            ConflictChoice::Skip,
            true,
        ));
    let mut body = column![
        row![
            side(t!("ops.conflict.source"), source),
            side(t!("ops.conflict.target"), target)
        ]
        .spacing(16),
        first.wrap(),
        row![
            answer(
                t!("ops.conflict.older"),
                ConflictChoice::OverwriteOlder,
                true
            ),
            answer(
                t!("ops.conflict.larger"),
                ConflictChoice::OverwriteLarger,
                true
            ),
            answer(
                t!("ops.conflict.smaller"),
                ConflictChoice::OverwriteSmaller,
                true
            ),
        ]
        .spacing(6)
        .wrap(),
        row![
            answer(t!("ops.conflict.rename"), ConflictChoice::Rename, false),
            answer(t!("ops.conflict.rename_all"), ConflictChoice::Rename, true),
            answer(
                t!("ops.conflict.rename_existing"),
                ConflictChoice::RenameExisting,
                false
            ),
        ]
        .spacing(6)
        .wrap(),
        row![
            text_input("", &job.rename_to)
                .id(PROPOSAL_ID)
                .on_input(move |text| wrap(FileOpsMsg::ProposalInput(id, text)))
                .on_submit(wrap(FileOpsMsg::RenameTo(id)))
                .font(theme::UI_FONT)
                .size(theme::TEXT_SIZE)
                .padding([4, 8]),
            secondary_icon_button(
                Icon::Rename,
                t!("ops.conflict.rename_to"),
                Some(wrap(FileOpsMsg::RenameTo(id))),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    if let Some(code) = job.note {
        body = body.push(fit::prose(error_text(code)).color(theme::DANGER));
    }
    // The answers stand in the body; the row holds Compare at the left and Cancel.
    let mut extra = Vec::new();
    if !source.is_dir && !target.is_dir {
        // The two files in the diff window over this question, which stays open (RC-020-04).
        extra.push(button(t!("ops.conflict.compare"), FileOpsMsg::Compare(id)));
    }
    let cancel = secondary_icon_button(
        Icon::Close,
        t!("ops.cancel"),
        Some(wrap(FileOpsMsg::Conflict {
            job: id,
            choice: ConflictChoice::Cancel,
            for_all: false,
        })),
    );
    let body = body.push(button_row(None, [cancel], extra));
    (line(title).font(theme::UI_FONT_STRONG).into(), body.into())
}
