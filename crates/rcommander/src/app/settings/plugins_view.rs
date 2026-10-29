//! Drawing the Plugins page's tab "Installed" (RC-100-20): "Install..." with the package's path,
//! the preview of a package, the table of installed plugins - one stop of the focus ring - and
//! the selected plugin's details with its buttons, versions, own settings and latest calls. The
//! page's schema rows are its other tab, "Settings" (`tabs.rs`).

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, container, mouse_area, pick_list, row, text_input},
};
use rc_engine::{KeyStatus, PluginInfo, PluginSettingInfo};

use super::{
    plugins::{PluginsMsg, PluginsPage, Preview, page_message},
    view::{hint, label},
};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
    },
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The columns' shares: name, version, kind, state.
const PORTIONS: [u16; 4] = [6, 2, 2, 3];

/// The tab "Installed": install, the preview, the table, the selected plugin.
pub fn view<'a>(page: &'a PluginsPage, tokens: &'static Tokens) -> Column<'a, Message> {
    let idle = !page.busy;
    let path = text_input(t!("settings.plugins.path"), &page.path)
        .on_input(|path| page_message(PluginsMsg::Path(path)))
        .padding([5, 8])
        .size(theme::TEXT_SIZE)
        .font(theme::UI_FONT)
        .width(Length::Fill);
    let install = layout::secondary_icon_button(
        Icon::Import,
        t!("settings.plugins.install"),
        (idle && !page.path.trim().is_empty()).then(|| page_message(PluginsMsg::Preview)),
    );
    let mut body = column![
        fit::prose(t!("settings.plugins.help")).color(tokens.text_secondary),
        row![path]
            .extend(browse::button(PathField::PluginPackage, &page.path, idle))
            .push(install)
            .spacing(8)
            .align_y(Alignment::Center),
    ]
    .spacing(10);
    if let Some(preview) = &page.preview {
        body = body.push(preview_card(preview, idle, tokens));
    }
    body = body.push(table(page, tokens));
    if let Some(plugin) = page.selected.as_deref().and_then(|id| page.plugin(id)) {
        body = body.push(details(page, plugin, tokens));
    }
    body
}

/// What the package is, and Install and Cancel; Install only when it can be installed.
fn preview_card<'a>(
    shown: &'a Preview,
    idle: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let preview = &shown.preview;
    let key = match preview.key {
        KeyStatus::Trusted => t!("settings.plugins.key_trusted"),
        KeyStatus::Untrusted => t!("settings.plugins.key_untrusted"),
        KeyStatus::Mismatch => t!("settings.plugins.key_mismatch"),
        KeyStatus::Revoked => t!("settings.plugins.key_revoked"),
        KeyStatus::Unsigned => t!("settings.plugins.key_unsigned"),
        KeyStatus::Changed => t!("settings.plugins.key_changed"),
    };
    let permissions = &preview.permissions;
    let claims = t!(
        "settings.plugins.claims",
        extensions = permissions.extensions.join(", "),
        outputs = permissions.outputs.join(", ")
    );
    let mut lines = column![
        label(t!(
            "settings.plugins.preview_title",
            name = preview.name.clone(),
            version = preview.version.clone()
        )),
        hint(
            t!(
                "settings.plugins.preview_author",
                author = preview.author.clone(),
                kind = preview.plugin_type.clone()
            ),
            tokens
        ),
        fit::prose(preview.description.clone()),
        hint(claims, tokens),
        label(key),
    ]
    .spacing(4);
    if let Some(fingerprint) = &preview.fingerprint {
        lines = lines.push(fit::cut(
            fingerprint.clone(),
            theme::MONO_FONT,
            None,
            1,
            tokens,
        ));
    }
    if !permissions.capabilities.is_empty() || !shown.added_permissions.is_empty() {
        let asked: Vec<String> = permissions
            .capabilities
            .iter()
            .chain(&shown.added_permissions)
            .cloned()
            .collect();
        lines = lines.push(
            fit::prose(t!("settings.plugins.added", permissions = asked.join(", ")))
                .color(theme::DANGER),
        );
    }
    if !shown.installed_versions.is_empty() {
        lines = lines.push(hint(
            t!(
                "settings.plugins.installed_versions",
                versions = shown.installed_versions.join(", ")
            ),
            tokens,
        ));
    }
    let refusal = if preview.withdrawn {
        Some(t!("settings.plugins.withdrawn").to_owned())
    } else {
        preview
            .incompatible
            .as_ref()
            .map(|reason| t!("settings.plugins.incompatible", reason = reason.clone()))
    };
    if let Some(refusal) = refusal {
        lines = lines.push(fit::prose(refusal).color(theme::DANGER));
    }
    let buttons = layout::button_row(
        Some(layout::primary_icon_button(
            Icon::Confirm,
            t!("settings.plugins.confirm_install"),
            (idle && shown.installable).then(|| page_message(PluginsMsg::Install)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("settings.plugins.cancel_preview"),
            Some(page_message(PluginsMsg::CancelPreview)),
        )],
        [],
    );
    container(lines.push(buttons))
        .padding(10)
        .style(move |_| theme::header(tokens))
        .width(Length::Fill)
        .into()
}

/// The installed plugins; a click selects one.
fn table<'a>(page: &'a PluginsPage, tokens: &'static Tokens) -> Element<'a, Message> {
    let header = fit::table_row(
        false,
        tokens,
        [
            t!("settings.plugins.name"),
            t!("settings.plugins.version"),
            t!("settings.plugins.kind"),
            t!("settings.plugins.state"),
        ]
        .into_iter()
        .zip(PORTIONS)
        .map(|(title, portion)| fit::cell(title, portion, true, tokens))
        .collect(),
    );
    let rows: Element<'a, Message> = match page.plugins.as_deref() {
        None => label(t!("settings.loading")).into(),
        Some([]) => hint(t!("settings.plugins.none"), tokens).into(),
        Some(plugins) => focus::choice(
            Column::with_children(
                plugins
                    .iter()
                    .enumerate()
                    .map(|(index, plugin)| plugin_row(page, plugin, index, tokens)),
            ),
            "settings.plugins.list",
            None,
            None,
        ),
    };
    column![
        container(header)
            .padding([0, 6])
            .style(move |_| theme::header(tokens)),
        rows,
    ]
    .into()
}

fn plugin_row<'a>(
    page: &PluginsPage,
    plugin: &'a PluginInfo,
    index: usize,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let selected = page.selected.as_deref() == Some(plugin.id.as_str());
    let [name, version, kind, state] = PORTIONS;
    let cells = vec![
        fit::cell(plugin.name.clone(), name, false, tokens),
        fit::cell(
            plugin.version.clone().unwrap_or_default(),
            version,
            false,
            tokens,
        ),
        fit::cell(plugin.kind.clone(), kind, true, tokens),
        fit::cell(state_of(page, plugin), state, true, tokens),
    ];
    let alternate = theme::alternate(index);
    mouse_area(
        container(fit::table_row(false, tokens, cells))
            .padding([0, 6])
            .width(Length::Fill)
            .style(move |_| theme::striped_row(tokens, selected, true, false, alternate)),
    )
    .on_press(page_message(PluginsMsg::Select(plugin.id.clone())))
    .into()
}

/// On, off or suspended, and the marks a person must see: unsigned, bundled, and a secret
/// source that does not answer - locked, unreachable, no licence (RC-100-36).
fn state_of(page: &PluginsPage, plugin: &PluginInfo) -> String {
    let mut state = if plugin.suspended {
        t!("settings.plugins.suspended")
    } else if plugin.enabled {
        t!("settings.plugins.on")
    } else {
        t!("settings.plugins.off")
    }
    .to_owned();
    if !plugin.signed {
        state = t!("settings.plugins.unsigned_mark", state = state);
    }
    if plugin.bundled {
        state = t!("settings.plugins.bundled_mark", state = state);
    }
    let source = format!(
        "{}{}",
        rc_engine::secret_sources::PLUGIN_SOURCE_PREFIX,
        plugin.id
    );
    if let Some(info) = page
        .sources
        .iter()
        .find(|info| info.source == source && info.state != "ready")
    {
        let mark = match info.state.as_str() {
            "locked" => t!("connections.source.state.locked"),
            "unreachable" => t!("connections.source.state.unreachable"),
            "unlicensed" => t!("connections.source.state.unlicensed"),
            _ => t!("connections.source.state.failed"),
        };
        state = t!("settings.plugins.source_mark", state = state, mark = mark);
    }
    state
}

/// The selected plugin: its buttons, versions, own settings and latest calls.
fn details<'a>(
    page: &'a PluginsPage,
    plugin: &'a PluginInfo,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let idle = !page.busy;
    let id = &plugin.id;
    let on = |message: PluginsMsg| idle.then(|| page_message(message));
    let has_previous = plugin
        .versions
        .iter()
        .any(|version| version.state == "previous");
    let buttons = row![
        if plugin.enabled {
            layout::secondary_button(
                t!("settings.plugins.disable"),
                on(PluginsMsg::Enable(id.clone(), false)),
            )
        } else {
            layout::secondary_button(
                t!("settings.plugins.enable"),
                on(PluginsMsg::Enable(id.clone(), true)),
            )
        },
        layout::secondary_button(
            t!("settings.plugins.rollback"),
            on(PluginsMsg::Rollback(id.clone())).filter(|_| has_previous),
        ),
        layout::secondary_icon_button(
            Icon::Resume,
            t!("settings.plugins.resume"),
            on(PluginsMsg::Resume(id.clone())).filter(|_| plugin.suspended),
        ),
    ]
    .spacing(layout::BUTTON_SPACING);
    let mut column = column![
        label(plugin.name.clone()),
        fit::prose(plugin.description.clone()).color(tokens.text_secondary),
        hint(
            t!(
                "settings.plugins.signed_by",
                author = plugin.author.clone(),
                key_id = plugin.key_id.clone()
            ),
            tokens
        ),
        buttons,
        label(t!("settings.plugins.versions")),
    ]
    .spacing(6);
    for version in &plugin.versions {
        let state = match version.refused.as_deref() {
            Some(code) => crate::i18n::error_text(code),
            None => version.state.clone(),
        };
        let mut line = format!("{}  {state}", version.version);
        if version.in_use {
            line = t!("settings.plugins.in_use", version = line);
        }
        column = column.push(hint(line, tokens));
    }
    let removable: Vec<String> = plugin
        .versions
        .iter()
        .filter(|version| !version.in_use)
        .map(|version| version.version.clone())
        .collect();
    let choose = |version: String| page_message(PluginsMsg::Version(version));
    let moves = focus::moves(&removable, page.version.as_ref(), &choose);
    let versions = pick_list(removable, page.version.clone(), choose).width(160);
    column = column.push(
        row![
            Element::from(focus::Stop::pick(
                versions,
                "settings.plugins.version_to_remove",
                moves,
            )),
            layout::secondary_icon_button(
                Icon::Delete,
                t!("settings.plugins.remove"),
                on(PluginsMsg::Remove).filter(|_| page.version.is_some()),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    if !plugin.settings.is_empty() {
        column = column.push(label(t!("settings.plugins.own_settings")));
        for setting in &plugin.settings {
            column = column.push(setting_row(page, setting));
        }
        column = column.push(layout::secondary_icon_button(
            Icon::Save,
            t!("settings.plugins.save_settings"),
            on(PluginsMsg::SaveSettings).filter(|_| !page.drafts.is_empty()),
        ));
    }
    column = column.push(label(t!("settings.plugins.log")));
    if page.executions.is_empty() {
        column = column.push(hint(t!("settings.plugins.log_empty"), tokens));
    }
    for call in &page.executions {
        let code = call.code.clone().unwrap_or_default();
        let line = format!(
            "{}  {} {}  {} {}  {} ms",
            super::host_keys_view::shown_time(&call.recorded_at),
            call.plugin_version,
            call.operation,
            call.outcome,
            code,
            call.duration_ms
        );
        column = column.push(hint(line, tokens));
    }
    container(column).padding([6, 0]).into()
}

/// One of the plugin's own settings: its name, and a choice or a text field.
fn setting_row<'a>(page: &'a PluginsPage, setting: &'a PluginSettingInfo) -> Element<'a, Message> {
    let current = page
        .drafts
        .get(&setting.name)
        .cloned()
        .or_else(|| setting.value.clone())
        .or_else(|| setting.default.clone())
        .unwrap_or_default();
    let name = setting.name.clone();
    let control: Element<'a, Message> = if setting.kind == "choice" {
        let edit = move |value: String| page_message(PluginsMsg::Setting(name.clone(), value));
        let list = pick_list(
            setting.values.as_slice(),
            Some(current.clone()),
            edit.clone(),
        )
        .width(200);
        focus::pick(
            list,
            &setting.setting,
            &setting.values,
            Some(&current),
            edit,
        )
    } else {
        // A masked setting (a licence key) is typed hidden, as a password is (RC-100-36).
        text_input("", &current)
            .secure(setting.masked)
            .on_input(move |value| page_message(PluginsMsg::Setting(name.clone(), value)))
            .width(Length::Fill)
            .into()
    };
    row![label(setting.name.clone()).width(160), control]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}
