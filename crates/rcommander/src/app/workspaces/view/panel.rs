//! A panel of the form: one line per tab - its place among the drive bar's, its path, "Take the
//! panel's folder", up, down, remove and the lock -, "Add tab", then the active tab, the view and,
//! for a terminal, its kind with its shell, distribution or server and its start folder.

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, row},
};
use rc_engine::{ConnectionId, PanelId, PanelSpec, PanelView, TermSpec};

use super::{
    field,
    form::{Labeled, choice, form_message, labeled},
};
use crate::{
    app::Message,
    app::workspaces::{
        Shown,
        form::{Form, FormMsg},
        panel_edit::{
            PanelMsg, Place, TermKind, cwd_of, distros, kind_of, place_of, places, ssh_servers,
            switched, with_cwd, with_shell,
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, icon::Icon, layout},
};

/// The width of a tab's place.
const PLACE_WIDTH: f32 = 130.0;

/// A change of `panel`, as the window's message.
fn message(panel: PanelId, change: PanelMsg) -> Message {
    form_message(FormMsg::Panel(panel, change))
}

/// The panel `id` of `form`.
pub(super) fn section<'a>(
    form: &'a Form,
    id: PanelId,
    shown: &Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let spec = form.panel(id);
    let title = match id {
        PanelId::Left => t!("workspaces.panel.left"),
        PanelId::Right => t!("workspaces.panel.right"),
    };
    let places = places(&shown.drives);
    let mut tabs = Column::new().spacing(4);
    for (at, tab) in spec.tabs.iter().enumerate() {
        let place = place_of(&places, &shown.drives, &tab.location).cloned();
        tabs = tabs.push(tab_line(id, spec, at, place, places.clone(), tokens));
    }
    let add = layout::secondary_icon_button(
        Icon::Add,
        t!("workspaces.tab.add"),
        Some(message(id, PanelMsg::Add)),
    );
    let mut section = column![
        fit::label(title).font(theme::UI_FONT_STRONG),
        tabs,
        add,
        labeled(t!("workspaces.field.active_tab"), active_tab(id, spec)),
        labeled(t!("workspaces.field.view"), view_choice(id, spec)),
    ]
    .spacing(8);
    if spec.view == PanelView::Terminal
        && let Some(terminal) = &spec.terminal
    {
        section = section.push(terminal_fields(id, terminal, shown));
    }
    section.padding([8, 0]).into()
}

/// One tab: place, path, "Take the panel's folder", up, down, remove, locked.
fn tab_line<'a>(
    id: PanelId,
    spec: &PanelSpec,
    at: usize,
    place: Option<Place>,
    places: Vec<Place>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let tab = &spec.tabs[at];
    let name = format!("workspaces.{id:?}.{at}.place");
    let place = choice(
        places
            .into_iter()
            .map(|place| Labeled(place.clone(), place.label))
            .collect(),
        place.as_ref(),
        move |place: Place| message(id, PanelMsg::Place(at, place.root)),
        &name,
        PLACE_WIDTH,
    );
    let path = field(
        t!("workspaces.tab.path"),
        &tab.location.path.to_string_lossy(),
    )
    .on_input(move |typed| message(id, PanelMsg::Path(at, typed)));
    let last = spec.tabs.len() - 1;
    let glyph = |icon, label, change: Option<PanelMsg>| {
        layout::glyph_button(
            icon,
            label,
            change.map(|change| message(id, change)),
            tokens,
        )
    };
    row![
        place,
        path,
        glyph(
            Icon::Import,
            t!("workspaces.tab.from_panel"),
            Some(PanelMsg::FromPanel(at)),
        ),
        glyph(
            Icon::MoveUp,
            t!("workspaces.tab.up"),
            (at > 0).then_some(PanelMsg::Up(at)),
        ),
        glyph(
            Icon::MoveDown,
            t!("workspaces.tab.down"),
            (at < last).then_some(PanelMsg::Down(at)),
        ),
        glyph(
            Icon::Delete,
            t!("workspaces.tab.remove"),
            (last > 0).then_some(PanelMsg::Remove(at)),
        ),
        layout::check(
            t!("workspaces.tab.locked"),
            tab.locked,
            true,
            move |locked| message(id, PanelMsg::Locked(at, locked)),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

/// The tab the panel shows after the workspace is applied: "2: /srv/www".
fn active_tab<'a>(id: PanelId, spec: &PanelSpec) -> Element<'a, Message> {
    let options = spec
        .tabs
        .iter()
        .enumerate()
        .map(|(at, tab)| {
            let path = tab.location.path.to_string_lossy();
            Labeled(at, format!("{}: {path}", at + 1))
        })
        .collect();
    choice(
        options,
        Some(&spec.active_tab),
        move |at| message(id, PanelMsg::ActiveTab(at)),
        &format!("workspaces.{id:?}.active_tab"),
        Length::Fill,
    )
}

/// Files or a terminal.
fn view_choice<'a>(id: PanelId, spec: &PanelSpec) -> Element<'a, Message> {
    let options = vec![
        Labeled(PanelView::Files, t!("workspaces.view.files").to_owned()),
        Labeled(
            PanelView::Terminal,
            t!("workspaces.view.terminal").to_owned(),
        ),
    ];
    choice(
        options,
        Some(&spec.view),
        move |view| message(id, PanelMsg::View(view)),
        &format!("workspaces.{id:?}.view"),
        260,
    )
}

/// The terminal's kind - this computer, a WSL distribution, an SSH server, each while the drive
/// bar has one -, its shell, distribution or server, and its start folder.
fn terminal_fields<'a>(id: PanelId, terminal: &TermSpec, shown: &Shown) -> Element<'a, Message> {
    let distros = distros(&shown.drives);
    let servers = ssh_servers(&shown.drives);
    let first_distro = distros.first().cloned();
    let first_server = servers.first().map(|(server, _)| *server);
    let mut kinds = vec![Labeled(
        TermKind::Local,
        t!("workspaces.terminal.local").to_owned(),
    )];
    if first_distro.is_some() || kind_of(terminal) == TermKind::Wsl {
        kinds.push(Labeled(
            TermKind::Wsl,
            t!("workspaces.terminal.wsl").to_owned(),
        ));
    }
    if first_server.is_some() || kind_of(terminal) == TermKind::Ssh {
        kinds.push(Labeled(
            TermKind::Ssh,
            t!("workspaces.terminal.ssh").to_owned(),
        ));
    }
    let current = terminal.clone();
    let kind = choice(
        kinds,
        Some(&kind_of(terminal)),
        move |kind| {
            let changed = switched(Some(&current), kind, first_distro.as_deref(), first_server)
                .unwrap_or_else(|| current.clone());
            message(id, PanelMsg::Terminal(changed))
        },
        &format!("workspaces.{id:?}.terminal"),
        260,
    );
    let mut fields = column![labeled(t!("workspaces.field.terminal"), kind)].spacing(8);
    match terminal {
        TermSpec::Local { shell, .. } => {
            let current = terminal.clone();
            let shell = field(
                t!("workspaces.shell_default"),
                shell.as_deref().unwrap_or_default(),
            )
            .on_input(move |typed| message(id, PanelMsg::Terminal(with_shell(&current, typed))));
            fields = fields.push(labeled(t!("workspaces.field.shell"), shell));
        }
        TermSpec::Wsl { distro, cwd } => {
            let options = distros
                .into_iter()
                .chain(std::iter::once(distro.clone()))
                .fold(Vec::new(), |mut known: Vec<String>, name| {
                    if !known.contains(&name) {
                        known.push(name);
                    }
                    known
                })
                .into_iter()
                .map(|name| Labeled(name.clone(), name))
                .collect();
            let cwd = cwd.clone();
            let pick = choice(
                options,
                Some(distro),
                move |distro| {
                    message(
                        id,
                        PanelMsg::Terminal(TermSpec::Wsl {
                            distro,
                            cwd: cwd.clone(),
                        }),
                    )
                },
                &format!("workspaces.{id:?}.distro"),
                260,
            );
            fields = fields.push(labeled(t!("workspaces.field.distro"), pick));
        }
        TermSpec::Ssh { connection_id, cwd } => {
            let mut options: Vec<Labeled<ConnectionId>> = servers
                .into_iter()
                .map(|(server, name)| Labeled(server, name))
                .collect();
            if !options.iter().any(|option| option.0 == *connection_id) {
                // A server the drive bar no longer lists: applying skips it with a note.
                options.push(Labeled(*connection_id, connection_id.0.to_string()));
            }
            let cwd = cwd.clone();
            let pick = choice(
                options,
                Some(connection_id),
                move |connection_id| {
                    message(
                        id,
                        PanelMsg::Terminal(TermSpec::Ssh {
                            connection_id,
                            cwd: cwd.clone(),
                        }),
                    )
                },
                &format!("workspaces.{id:?}.server"),
                260,
            );
            fields = fields.push(labeled(t!("workspaces.field.server"), pick));
        }
    }
    let current = terminal.clone();
    let cwd = field(t!("workspaces.cwd_default"), cwd_of(terminal))
        .on_input(move |typed| message(id, PanelMsg::Terminal(with_cwd(&current, typed))));
    fields = fields.push(labeled(t!("workspaces.field.cwd"), cwd));
    fields.into()
}
