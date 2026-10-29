//! Drawing the properties dialogs: a medium card over the dimmed window that takes every click,
//! or a window of its own (RC-080-23, `dialog_windows.rs`).

use iced::{
    Alignment, Element,
    widget::{column, container, mouse_area, row, text, text_input},
};
use rc_engine::{ApplyTo, DirUsage, EntryKind, ItemProperties, LinkKind, PropertiesReport};

use super::{
    DialogKind, LinkForm, Phase, PropsDialog, PropsMsg,
    form::{AttributeForm, Flag, FormMsg, TIME_FORMAT},
};
use crate::{
    app::Message,
    format,
    i18n::t,
    theme::{
        self, Tokens,
        fit::{self, line, prose},
        focus,
        icon::Icon,
        layout,
        mnemonic::Label,
    },
};

const LABEL_WIDTH: f32 = 150.0;

/// The open dialog over the window.
pub fn overlay<'a>(dialog: &'a PropsDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    Message::overlay(
        title(dialog),
        body(dialog, tokens),
        layout::MEDIUM_DIALOG_WIDTH,
        tokens,
    )
}

/// The dialog's title by its kind: in its card's head, or in its window's title bar.
pub fn title(dialog: &PropsDialog) -> &'static str {
    match dialog.kind {
        DialogKind::Properties => t!("props.title_properties"),
        DialogKind::Space => t!("props.title_space"),
        DialogKind::Link => t!("props.title_link"),
        DialogKind::Comment => t!("props.title_comment"),
    }
}

/// The dialog's facts, fields and buttons: on its card, or filling a window of its own.
pub fn body<'a>(dialog: &'a PropsDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut body = column![].spacing(10);
    match (&dialog.phase, dialog.kind) {
        (Phase::Loading, DialogKind::Space) => body = body.push(note(t!("props.counting"), tokens)),
        (Phase::Loading, _) => body = body.push(note(t!("props.loading"), tokens)),
        (_, DialogKind::Properties) => {
            if let Some(report) = &dialog.report {
                body = body.push(facts(report, tokens));
                body = body.push(attributes(&dialog.form, report, tokens));
            }
        }
        (_, DialogKind::Space) => {
            if let Some(usage) = &dialog.space {
                body = body.push(usage_rows(usage, tokens));
            }
        }
        (_, DialogKind::Link) => {
            if let Some(link) = &dialog.link {
                body = body.push(link_fields(link, tokens));
            }
        }
        (_, DialogKind::Comment) => body = body.push(comment_field(dialog)),
    }
    match &dialog.phase {
        Phase::Running => body = body.push(note(t!("props.running"), tokens)),
        Phase::Failed(error) => {
            body = body.push(prose(error.as_str()).color(theme::DANGER));
        }
        Phase::Done(done) if !done.is_empty() => {
            body = body.push(prose(done.as_str()).color(tokens.accent));
        }
        Phase::Loading | Phase::Ready | Phase::Done(_) => {}
    }
    body.push(buttons(dialog)).into()
}

/// What the entries are: one entry's facts, or the sums of several.
fn facts<'a>(report: &'a PropertiesReport, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut rows = column![].spacing(4);
    if let (Some(total), true) = (&report.total, report.items.len() > 1) {
        rows = rows.push(fact(
            t!("props.selection"),
            t!(
                "props.files_and_folders",
                files = format::count(total.files),
                dirs = format::count(total.dirs)
            ),
            tokens,
        ));
        rows = rows.push(fact(t!("props.size"), size_text(total.bytes), tokens));
        return rows.into();
    }
    let Some(item) = report.items.first() else {
        return rows.into();
    };
    rows = rows.push(fact(t!("props.name"), item.entry.name.clone(), tokens));
    if let Some(parent) = item.path.parent() {
        rows = rows.push(path_fact(
            t!("props.location"),
            parent.path.display().to_string(),
            tokens,
        ));
    }
    rows = rows.push(fact(t!("props.type"), kind_text(item), tokens));
    rows = match (&item.entry.kind, &item.usage) {
        (EntryKind::Dir, Some(usage)) => rows
            .push(fact(t!("props.size"), size_text(usage.bytes), tokens))
            .push(fact(
                t!("props.contains"),
                t!(
                    "props.files_and_folders",
                    files = format::count(usage.files),
                    dirs = format::count(usage.dirs)
                ),
                tokens,
            )),
        (EntryKind::Dir, None) => rows.push(fact(
            t!("props.size"),
            t!("props.not_counted").to_owned(),
            tokens,
        )),
        _ => rows.push(fact(t!("props.size"), size_text(item.entry.size), tokens)),
    };
    let allocated = item
        .usage
        .as_ref()
        .map_or(item.details.allocated, |usage| usage.allocated);
    if let Some(allocated) = allocated {
        rows = rows.push(fact(t!("props.on_disk"), size_text(allocated), tokens));
    }
    for (label, time) in [
        (t!("props.created"), item.entry.created),
        (t!("props.modified"), item.entry.modified),
        (t!("props.accessed"), item.details.accessed),
    ] {
        if time.is_some() {
            rows = rows.push(fact(label, format::date_in(time, TIME_FORMAT), tokens));
        }
    }
    if let (Some(uid), Some(gid)) = (item.details.uid, item.details.gid) {
        rows = rows.push(fact(
            t!("props.owner"),
            t!("props.owner_ids", uid = uid, gid = gid),
            tokens,
        ));
    }
    if let Some(links) = item.details.links.filter(|links| *links > 1) {
        rows = rows.push(fact(t!("props.links"), links.to_string(), tokens));
    }
    if let Some(comment) = &item.comment {
        rows = rows.push(fact(t!("props.comment"), comment.clone(), tokens));
    }
    rows.into()
}

/// The kind of an entry, with where a link leads.
fn kind_text(item: &ItemProperties) -> String {
    match &item.entry.kind {
        EntryKind::File => t!("props.kind_file").to_owned(),
        EntryKind::Dir => t!("props.kind_dir").to_owned(),
        EntryKind::Symlink { target, .. } => {
            t!("props.kind_link", target = target.display())
        }
        EntryKind::Other => t!("props.kind_other").to_owned(),
    }
}

/// `1.5 MiB (1 572 864 bytes)`.
fn size_text(bytes: u64) -> String {
    t!(
        "props.size_bytes",
        size = format::size(bytes),
        bytes = format::count(bytes)
    )
}

/// The attribute fields: flags (Windows ones greyed out where they cannot be set), the Unix
/// mode, the times, the recursion.
fn attributes<'a>(
    form: &'a AttributeForm,
    report: &PropertiesReport,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let capabilities = report.capabilities;
    let mut flags = row![].spacing(14);
    for flag in Flag::ALL {
        let label = match flag {
            Flag::Readonly => t!("props.readonly"),
            Flag::Hidden => t!("props.hidden"),
            Flag::System => t!("props.system"),
            Flag::Archive => t!("props.archive"),
        };
        let enabled = flag == Flag::Readonly || capabilities.windows_flags;
        flags = flags.push(toggle(label, form.flag(flag), enabled, move |on| {
            FormMsg::Flag(flag, on)
        }));
    }
    let mut fields = column![
        line(t!("props.attributes")).color(tokens.text_secondary),
        flags
    ]
    .spacing(8);
    if form.has_mode {
        fields = fields.push(field(t!("props.mode"), &form.mode, true, FormMsg::Mode));
    }
    let now = layout::secondary_button(
        t!("props.now"),
        Some(Message::Properties(PropsMsg::Form(FormMsg::Now))),
    );
    fields = fields.push(
        row![
            field(
                t!("props.set_modified"),
                &form.modified,
                true,
                FormMsg::Modified
            ),
            now
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    fields = fields.push(field(
        t!("props.set_created"),
        &form.created,
        capabilities.set_created,
        FormMsg::Created,
    ));
    if form.has_dirs {
        let mut recursion = row![toggle(
            t!("props.recursive"),
            form.recursive,
            true,
            FormMsg::Recursive
        )]
        .spacing(14);
        if form.recursive {
            let apply = form.apply_to;
            recursion = recursion
                .push(toggle(
                    t!("props.apply_files"),
                    apply.files,
                    true,
                    move |on| FormMsg::ApplyTo(ApplyTo { files: on, ..apply }),
                ))
                .push(toggle(
                    t!("props.apply_dirs"),
                    apply.dirs,
                    true,
                    move |on| FormMsg::ApplyTo(ApplyTo { dirs: on, ..apply }),
                ));
        }
        fields = fields.push(recursion);
    }
    fields.into()
}

/// The counted space of Ctrl+L.
fn usage_rows<'a>(usage: &DirUsage, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut rows = column![
        fact(t!("props.size"), size_text(usage.bytes), tokens),
        fact(
            t!("props.contains"),
            t!(
                "props.files_and_folders",
                files = format::count(usage.files),
                dirs = format::count(usage.dirs)
            ),
            tokens
        ),
    ]
    .spacing(4);
    if let Some(allocated) = usage.allocated {
        rows = rows.push(fact(t!("props.on_disk"), size_text(allocated), tokens));
    }
    if usage.hard_links > 0 {
        rows = rows.push(fact(
            t!("props.hard_links"),
            format::count(usage.hard_links),
            tokens,
        ));
    }
    if usage.unreadable > 0 {
        rows = rows.push(fact(
            t!("props.unreadable"),
            format::count(usage.unreadable),
            tokens,
        ));
    }
    rows.into()
}

/// The link dialog: target, kind (what the file system cannot make greyed out), place.
fn link_fields<'a>(link: &'a LinkForm, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut kinds = row![].spacing(6);
    for kind in LinkKind::ALL {
        let label = match kind {
            LinkKind::Shortcut => t!("props.link_shortcut"),
            LinkKind::Symlink => t!("props.link_symlink"),
            LinkKind::Hardlink => t!("props.link_hardlink"),
            LinkKind::Junction => t!("props.link_junction"),
        };
        let current = link.kind == kind;
        let enabled = link.kinds.supports(kind);
        let tab = container(line(label).color(if enabled {
            tokens.text
        } else {
            tokens.text_secondary
        }))
        .padding([4, 12])
        .style(move |_| theme::row(tokens, current, true, false));
        let tab: Element<'a, Message> = if enabled && !current {
            mouse_area(tab)
                .on_press(Message::Properties(PropsMsg::LinkKind(kind)))
                .into()
        } else {
            tab.into()
        };
        kinds = kinds.push(tab);
    }
    // The kinds are one stop of the ring; the arrows choose among those the file system makes.
    let supported: Vec<LinkKind> = LinkKind::ALL
        .into_iter()
        .filter(|kind| link.kinds.supports(*kind))
        .collect();
    let kinds = focus::pick(
        kinds,
        "props.link_kinds",
        &supported,
        Some(&link.kind),
        |kind| Message::Properties(PropsMsg::LinkKind(kind)),
    );
    let place_label = t!("props.link_place");
    let place = text_input(&Label::parse(place_label).text, &link.link)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8])
        .on_input(|path| Message::Properties(PropsMsg::LinkPath(path)));
    column![
        path_fact(
            t!("props.link_target"),
            link.target.path.display().to_string(),
            tokens
        ),
        kinds,
        layout::caption(place_label, Some(tokens.text_secondary)),
        layout::keyed(focus::Stop::field(place, place_label), place_label),
        prose(t!("props.link_note")).color(tokens.text_secondary),
    ]
    .spacing(8)
    .into()
}

/// The comment of Ctrl+Z.
fn comment_field(dialog: &PropsDialog) -> Element<'_, Message> {
    let name = dialog
        .report
        .as_ref()
        .and_then(|report| report.items.first())
        .map_or_else(String::new, |item| item.entry.name.clone());
    column![
        line(name),
        text_input(t!("props.comment_placeholder"), &dialog.comment)
            .font(theme::UI_FONT)
            .size(theme::TEXT_SIZE)
            .padding([6, 8])
            .on_input(|text| Message::Properties(PropsMsg::Comment(text))),
    ]
    .spacing(8)
    .into()
}

/// The buttons of the dialog: Count at the left, the confirmation (the primary) and Close.
fn buttons<'a>(dialog: &PropsDialog) -> Element<'a, Message> {
    let idle = matches!(dialog.phase, Phase::Ready | Phase::Failed(_));
    let on = |message: PropsMsg| idle.then(|| Message::Properties(message));
    let mut extra = Vec::new();
    let confirm = match dialog.kind {
        DialogKind::Properties => {
            let uncounted = dialog.report.as_ref().is_some_and(|report| {
                report
                    .items
                    .iter()
                    .any(|item| item.entry.kind == EntryKind::Dir && item.usage.is_none())
            });
            if uncounted {
                extra.push(layout::secondary_button(
                    t!("props.count"),
                    on(PropsMsg::Count),
                ));
            }
            Some(t!("props.apply"))
        }
        DialogKind::Link => Some(t!("props.create")),
        DialogKind::Comment => Some(t!("props.save")),
        DialogKind::Space => None,
    };
    let confirm = confirm.map(|label| layout::primary_button(label, on(PropsMsg::Confirm)));
    let close = layout::secondary_icon_button(
        Icon::Close,
        t!("props.close"),
        Some(Message::Properties(PropsMsg::Close)),
    );
    layout::button_row(confirm, [close], extra)
}

/// A label and its value on one line.
fn fact<'a>(label: &'static str, value: String, tokens: &'static Tokens) -> Element<'a, Message> {
    row![
        line(label).color(tokens.text_secondary).width(LABEL_WIDTH),
        prose(value)
    ]
    .spacing(8)
    .into()
}

/// A label and a path on one line, the path shortened in the middle.
fn path_fact<'a>(
    label: &'static str,
    value: String,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    row![
        line(label).color(tokens.text_secondary).width(LABEL_WIDTH),
        fit::path_line(value, tokens.text, tokens)
    ]
    .spacing(8)
    .into()
}

/// A checkbox with its access key; greyed out and still when `enabled` is false.
fn toggle<'a>(
    label: &'static str,
    on: bool,
    enabled: bool,
    message: impl Fn(bool) -> FormMsg + 'a,
) -> Element<'a, Message> {
    layout::check(label, on, enabled, move |on| {
        Message::Properties(PropsMsg::Form(message(on)))
    })
}

/// A labelled text field, answering to its label's access key; read only when `enabled` is
/// false.
fn field<'a>(
    label: &'static str,
    value: &'a str,
    enabled: bool,
    message: fn(String) -> FormMsg,
) -> Element<'a, Message> {
    let input = text_input("", value)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([4, 8])
        .width(220);
    if !enabled {
        return row![line(Label::parse(label).text).width(LABEL_WIDTH), input]
            .spacing(8)
            .align_y(Alignment::Center)
            .into();
    }
    let input = input.on_input(move |text| Message::Properties(PropsMsg::Form(message(text))));
    row![
        container(layout::caption(label, None)).width(LABEL_WIDTH),
        layout::keyed(focus::Stop::field(input, label), label)
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn note<'a>(content: &'static str, tokens: &'static Tokens) -> text::Text<'a> {
    prose(content).color(tokens.text_secondary)
}
