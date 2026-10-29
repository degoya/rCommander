//! Drawing the login-script editor: the scripts on the left, the open one on the right - its
//! name and time limits, its steps, its secrets and the test run. The steps are numbered cards
//! joined by a line, so they read as one run from top to bottom (RC-050-13); each card names its
//! kind in a column of its own, its fields, its moves, and under them why it cannot run, a hint
//! or a warning. Save, Delete and Close are the window's one button row (`window.rs`).

use std::fmt;

use iced::{
    Alignment, Color, Element, Length,
    widget::{
        Column, Space, button, column, container, pick_list, row, scrollable, stack, text,
        text_input,
    },
};
use rc_engine::login_script::{Rehearsal, ScriptOutcome, TEMPLATES};
use rc_vault::ExposeSecret as _;

use super::{
    Editor, Message, SecretWarning, SourceChoice, StepKind, StepRow, draft::template_name,
};
use crate::{
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{help_line, label, prose},
        focus,
        icon::Icon,
        layout::{self, caption, keyed, secondary_button, secondary_icon_button},
    },
};

const GAP: f32 = 8.0;
/// The step number's round badge.
const BADGE: f32 = 26.0;
/// The badge's place under the card's top, so it stands beside the card's first line (padding 8,
/// a field 30 high).
const BADGE_TOP: f32 = 10.0;
/// The column that names a step's kind, so the fields of all steps start in one line.
const KIND_WIDTH: f32 = 96.0;

impl Editor {
    pub(crate) fn view(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        row![
            self.list().width(Length::FillPortion(1)),
            scrollable(self.form(tokens).padding(iced::Padding {
                right: GAP * 2.0,
                ..iced::Padding::ZERO
            }))
            .width(Length::FillPortion(3)),
        ]
        .spacing(GAP * 2.0)
        .padding(GAP * 2.0)
        .into()
    }

    fn list(&self) -> Column<'_, Message> {
        let scripts = self.scripts.iter().map(|script| {
            let face = button(label(&script.name))
                .width(Length::Fill)
                .style(button::secondary)
                .on_press(Message::Select(script.id));
            focus::button(face, &script.name, Some(Message::Select(script.id)))
        });
        let templates = TEMPLATES.iter().map(|template| {
            let name = template_name(template.id);
            let face = button(label(name))
                .width(Length::Fill)
                .style(button::secondary)
                .on_press(Message::Template(template.id));
            focus::button(face, name, Some(Message::Template(template.id)))
        });
        column![
            heading(t!("login_scripts.title")),
            Column::with_children(scripts).spacing(4),
            secondary_icon_button(Icon::Add, t!("login_scripts.new"), Some(Message::New)),
            heading(t!("login_scripts.templates")),
            Column::with_children(templates).spacing(4),
        ]
        .spacing(GAP)
    }

    fn form(&self, tokens: &'static Tokens) -> Column<'_, Message> {
        let draft = &self.draft;
        let add = StepKind::ALL
            .into_iter()
            .map(|kind| secondary_button(kind.label(), Some(Message::Add(kind))));
        let (step_timeout, total_timeout) = (
            t!("login_scripts.step_timeout"),
            t!("login_scripts.total_timeout"),
        );
        let mut form = column![
            text_input(t!("login_scripts.name"), &draft.name).on_input(Message::Name),
            row![
                caption(step_timeout, None),
                keyed(
                    focus::Stop::field(
                        text_input("", &draft.step_timeout).on_input(Message::StepTimeout),
                        step_timeout
                    ),
                    step_timeout
                ),
                caption(total_timeout, None),
                keyed(
                    focus::Stop::field(
                        text_input("", &draft.total_timeout).on_input(Message::TotalTimeout),
                        total_timeout
                    ),
                    total_timeout
                ),
            ]
            .spacing(GAP)
            .align_y(Alignment::Center),
            section(
                t!("login_scripts.steps"),
                t!("login_scripts.steps_help"),
                tokens
            ),
            self.steps(tokens),
            row![label(t!("login_scripts.add_step")).color(tokens.text_secondary)]
                .extend(add)
                .spacing(4)
                .align_y(Alignment::Center),
            self.secrets(tokens),
            section(
                t!("login_scripts.test_run"),
                t!("login_scripts.test_run_help"),
                tokens
            ),
            text_input(t!("login_scripts.sample_hint"), &self.sample).on_input(Message::Sample),
            secondary_icon_button(
                Icon::Test,
                t!("login_scripts.rehearse"),
                Some(Message::Rehearse)
            ),
        ]
        .spacing(GAP);
        if let Some(rehearsal) = &self.rehearsal {
            form = form.push(self.rehearsal(rehearsal));
        }
        if let Some(code) = &self.error {
            form = form.push(prose(error_text(code)).color(theme::DANGER));
        }
        form
    }

    /// The steps as numbered cards, a short line between two; a hint where there is none yet.
    fn steps(&self, tokens: &'static Tokens) -> Element<'_, Message> {
        let steps = &self.draft.steps;
        if steps.is_empty() {
            return prose(t!("login_scripts.no_steps"))
                .color(tokens.text_secondary)
                .into();
        }
        let mut column = Column::new();
        for (at, step) in steps.iter().enumerate() {
            if at > 0 {
                column = column.push(joint(tokens));
            }
            column = column.push(self.step(at, step, at + 1 == steps.len(), tokens));
        }
        column.into()
    }

    fn step<'a>(
        &'a self,
        at: usize,
        step: &'a StepRow,
        last: bool,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        let mut line = row![
            text(step.kind.label())
                .font(theme::UI_FONT_STRONG)
                .size(theme::TEXT_SIZE)
                .width(KIND_WIDTH)
        ]
        .spacing(GAP)
        .align_y(Alignment::Center);
        line = match step.kind {
            StepKind::WaitFor => line
                .push(
                    text_input(t!("login_scripts.pattern"), &step.text)
                        .on_input(move |value| Message::Text(at, value))
                        .width(Length::Fill),
                )
                .push(
                    text_input(t!("login_scripts.timeout_hint"), &step.timeout)
                        .on_input(move |value| Message::Timeout(at, value))
                        .width(150),
                ),
            StepKind::SendText => line
                .push(
                    text_input(t!("login_scripts.text"), &step.text)
                        .on_input(move |value| Message::Text(at, value))
                        .width(Length::Fill),
                )
                .push(enter_after(at, step.newline)),
            StepKind::SendSecret => line
                .push(self.source(at, step.source))
                .push(enter_after(at, step.newline)),
            StepKind::SendLine => line.push(
                label(t!("login_scripts.send_line_note"))
                    .color(tokens.text_secondary)
                    .width(Length::Fill),
            ),
            StepKind::Sleep => line
                .push(
                    text_input(t!("login_scripts.sleep_hint"), &step.ms)
                        .on_input(move |value| Message::Sleep(at, value))
                        .width(120),
                )
                .push(Space::new().width(Length::Fill)),
        };
        let line = line
            .push(layout::glyph_button(
                Icon::MoveUp,
                t!("login_scripts.up"),
                (at > 0).then_some(Message::Up(at)),
                tokens,
            ))
            .push(layout::glyph_button(
                Icon::MoveDown,
                t!("login_scripts.down"),
                (!last).then_some(Message::Down(at)),
                tokens,
            ))
            .push(secondary_icon_button(
                Icon::Delete,
                t!("login_scripts.remove"),
                Some(Message::Remove(at)),
            ));
        let mut card = column![line].spacing(4);

        if let Some(code) = step.error {
            card = card.push(note(error_text(code), theme::DANGER));
        }
        if self.warn_prompt {
            match self.draft.secret_warning(at) {
                Some(SecretWarning::NoWait) => {
                    card = card.push(note(t!("login_scripts.warn.no_wait"), theme::DANGER));
                }
                Some(SecretWarning::NoPrompt) => {
                    card = card.push(note(t!("login_scripts.warn.no_prompt"), theme::DANGER));
                }
                None => {}
            }
        }
        if step.kind == StepKind::SendSecret && self.draft.secrets.is_empty() {
            card = card.push(note(t!("login_scripts.secret_hint"), tokens.text_secondary));
        }
        let card = container(card)
            .padding(GAP)
            .width(Length::Fill)
            .style(move |_| theme::panel(tokens, false));
        let badge = column![Space::new().height(BADGE_TOP), badge(at + 1, tokens)];
        let step = row![badge, card].spacing(GAP).align_y(Alignment::Start);
        // The line comes down from the step before to the badge and goes on from under it to the
        // card's foot, where `joint` takes it to the next step; the card's height sets the
        // stack's, so the line is as long as the card.
        let rule = |height| {
            container(Space::new().width(2).height(height)).style(move |_| theme::divider(tokens))
        };
        let above: Element<'_, Message> = if at > 0 {
            rule(Length::Fixed(BADGE_TOP)).into()
        } else {
            Space::new().height(BADGE_TOP).into()
        };
        let below: Element<'_, Message> = if last {
            Space::new().into()
        } else {
            rule(Length::Fill).into()
        };
        let line = row![
            Space::new().width(BADGE / 2.0 - 1.0),
            column![above, Space::new().height(BADGE), below],
        ];
        stack![step, line].into()
    }

    /// The secret a `send_secret` step sends: the connection's password or one stored with the
    /// script, as a pick list. A stored secret since removed shows none chosen.
    fn source(&self, at: usize, chosen: SourceChoice) -> Element<'_, Message> {
        let options: Vec<Source> = std::iter::once(Source {
            choice: SourceChoice::ConnectionPassword,
            name: t!("login_scripts.connection_password").to_owned(),
        })
        .chain(self.draft.secrets.iter().map(|secret| Source {
            choice: SourceChoice::Stored(secret.id),
            name: secret.label.clone(),
        }))
        .collect();
        let selected = options
            .iter()
            .find(|source| source.choice == chosen)
            .cloned();
        let pick = move |source: Source| Message::Source(at, source.choice);
        let moves = focus::moves(&options, selected.as_ref(), &pick);
        let list = pick_list(options, selected, pick)
            .placeholder(t!("login_scripts.choose_secret"))
            .width(Length::Fill);
        focus::Stop::pick(list, t!("login_scripts.step.send_secret"), moves).into()
    }

    fn secrets(&self, tokens: &'static Tokens) -> Column<'_, Message> {
        let saved = self.draft.id.is_some();
        let stored = self.draft.secrets.iter().map(|secret| {
            row![
                label(&secret.label).width(Length::Fill),
                secondary_icon_button(
                    Icon::Delete,
                    t!("login_scripts.remove"),
                    Some(Message::RemoveSecret(secret.id))
                ),
            ]
            .spacing(GAP)
            .align_y(Alignment::Center)
            .into()
        });
        let mut section = column![
            section(
                t!("login_scripts.secrets"),
                t!("login_scripts.secrets_help"),
                tokens
            ),
            Column::with_children(stored).spacing(4),
            row![
                text_input(t!("login_scripts.secret_label"), &self.secret_label)
                    .on_input(Message::SecretLabel),
                text_input(
                    t!("login_scripts.secret_value"),
                    self.secret_value.expose_secret()
                )
                .secure(true)
                .on_input(|value| Message::SecretValue(value.into())),
                secondary_button(
                    t!("login_scripts.store_secret"),
                    saved.then_some(Message::StoreSecret)
                ),
            ]
            .spacing(GAP),
        ]
        .spacing(GAP);
        if !saved {
            section = section.push(help_line(t!("login_scripts.secrets_unsaved"), tokens));
        }
        section
    }

    fn rehearsal<'a>(&'a self, rehearsal: &'a Rehearsal) -> Column<'a, Message> {
        let lines = rehearsal.reached.iter().map(|reached| {
            let line = match reached.end {
                Some(end) => t!(
                    "login_scripts.rehearsal.matched",
                    step = reached.step + 1,
                    line = line_of(&rehearsal.text, end)
                ),
                None => t!("login_scripts.rehearsal.reached", step = reached.step + 1),
            };
            label(line).into()
        });
        let outcome = match rehearsal.outcome {
            ScriptOutcome::Completed => t!("login_scripts.rehearsal.completed").to_owned(),
            ScriptOutcome::StreamClosed { step } => {
                t!("login_scripts.rehearsal.sample_ended", step = step + 1)
            }
            ScriptOutcome::TimedOut { step } => {
                t!("login_scripts.rehearsal.timed_out", step = step + 1)
            }
            ScriptOutcome::Cancelled => t!("login_scripts.rehearsal.cancelled").to_owned(),
        };
        Column::with_children(lines).push(label(outcome)).spacing(2)
    }
}

/// A secret a step can send, as its pick list names it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    choice: SourceChoice,
    name: String,
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// The line, from 1, that byte `end` of `text` lies on.
pub(super) fn line_of(text: &str, end: usize) -> usize {
    let end = end.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

/// Whether Enter follows the text or the secret; repeated per step, so without access key.
fn enter_after(at: usize, on: bool) -> Element<'static, Message> {
    layout::check(t!("login_scripts.enter_after"), on, true, move |on| {
        Message::Newline(at, on)
    })
}

/// The step's number in a round badge.
fn badge(number: usize, tokens: &'static Tokens) -> Element<'static, Message> {
    container(
        text(number.to_string())
            .font(theme::UI_FONT_STRONG)
            .size(theme::TEXT_SIZE),
    )
    .width(BADGE)
    .height(BADGE)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_| theme::badge(tokens, BADGE))
    .into()
}

/// The line from one step's badge down to the next one's: they run one after the other.
fn joint(tokens: &'static Tokens) -> Element<'static, Message> {
    row![
        Space::new().width(BADGE / 2.0 - 1.0),
        container(Space::new().width(2).height(GAP)).style(move |_| theme::divider(tokens)),
    ]
    .into()
}

/// A section's heading with a line of help under it.
fn section<'a>(title: &'a str, help: &'a str, tokens: &'static Tokens) -> Column<'a, Message> {
    column![heading(title), help_line(help, tokens)].spacing(2)
}

/// A line under a step: why it cannot run, a warning or a hint; it wraps.
fn note<'a>(content: impl text::IntoFragment<'a>, color: Color) -> text::Text<'a> {
    prose(content)
        .size(theme::TEXT_SIZE - 1.0)
        .color(color)
        .width(Length::Fill)
}

fn heading<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content)
        .size(theme::TEXT_SIZE + 2.0)
        .font(theme::UI_FONT_STRONG)
}
