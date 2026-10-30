//! The menu bar at the window's top (RC-080-18): the menus of `menu_bar/spec.rs`, each entry a row
//! of the command palette by its name - so it runs what the palette runs - or a dialog without a
//! row, labelled `menubar.<id>` and showing its key from the keymap. [`build`] turns the menus into
//! the system menu's text; `platform::menu_bar` attaches it (Windows: a Win32 menu through muda)
//! and hands a choice back as [`MenuBarMsg::Chosen`] with the entry's id.
//!
//! The app keeps the bar in step after every update ([`App::sync_menu_bar`], from
//! `dialog_windows.rs`): attached once the setting `appearance.menu_bar` is read and on, hidden
//! when it is off, dark with the app's look, greyed out while a dialog is open (a choice that
//! still arrives then is ignored), and Alt with a letter left to the quick search or a terminal
//! that wants it; its keys follow a changed keymap (`relabel_menu_bar`, RC-100-21) and a switched
//! keyboard layout (`follow_layout`, RC-100-48). The popup
//! menus - the bar's and the Explorer's - follow the look too
//! (`platform::popup_menus`, owner 2026-10-18).

mod drawn;
mod spec;
pub(in crate::app) mod view;

use iced::{
    Subscription, Task,
    futures::{Stream, StreamExt},
};
use rc_engine::{
    Keymap, PanelView,
    settings::{MENU_BAR_KEY, stored_bool},
};

use self::drawn::OpenMenu;
pub use self::spec::MENUS;
use super::{Entry, PaletteMsg, reach};
use crate::{
    app::{
        App, Effect, Message,
        cmdline::SearchKeys,
        connections::{ConnMsg, ImportMsg, RdmImportMsg, TcImportMsg},
        settings::{Page, SettingsMessage},
    },
    i18n,
    platform::{
        menu_bar::{self as platform, BarItem, BarMenu, Look},
        popup_menus,
    },
};

/// One menu of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    /// Its title's i18n key is `menubar.title.<id>`.
    pub id: &'static str,
    pub slots: &'static [Slot],
}

/// A place in a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The palette's row of this name; the name is the entry's id.
    Entry(&'static str),
    /// A dialog the palette has no row for, under its id.
    Dialog(&'static str, Dialog),
    Separator,
}

/// A dialog an entry opens without a palette row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    /// The command palette itself (its window key, Ctrl+Shift+P).
    Palette,
    /// A page of the settings.
    Page(Page),
    /// The import from Total Commander (RC-050-09).
    TcImport,
    /// The connection manager's import of a Remote Desktop Manager export (RC-050-11).
    RdmImport,
    /// The connection manager's import of an ssh_config (RC-050-06).
    SshConfigImport,
}

/// The menu bar's messages, through [`PaletteMsg::MenuBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuBarMsg {
    /// The setting `appearance.menu_bar`, read.
    Shown(bool),
    /// The entry with this id was chosen.
    Chosen(String),
    /// A click on the drawn bar's title of this menu (`drawn.rs`).
    Title(usize),
    /// The pointer on the drawn bar's title of this menu: it opens while another one is open.
    HoverTitle(usize),
    /// The pointer on this entry of the open drawn menu.
    HoverEntry(usize),
    /// A click beside the open drawn menu.
    Close,
}

/// The bar's books: what the setting says and what the platform shows.
#[derive(Debug, Default)]
pub struct MenuBar {
    /// The setting `appearance.menu_bar`; `None` until it is read.
    pub shown: Option<bool>,
    /// The menus the drawn bar shows, built when it is shown and after the keymap changed.
    pub menus: Vec<BarMenu>,
    /// The drawn bar's open menu.
    pub open: Option<OpenMenu>,
    /// The look handed to the platform last; `None` until the bar is attached.
    applied: Option<Look>,
    /// Whether the popup menus were asked to be dark, last; `None` before the first time.
    popups_dark: Option<bool>,
    /// What the keyboard layout printed on the keys named by place when last looked at.
    layout: Option<String>,
}

impl MenuBar {
    /// Whether the popup menus are to be told the look `dark`: the first time and on a change,
    /// with the bar shown or not (the Explorer's context menu follows too).
    fn popups(&mut self, dark: bool) -> bool {
        self.popups_dark.replace(dark) != Some(dark)
    }

    /// What to tell the platform for the look `wanted`: attach the bar, change it, or nothing.
    fn step(&mut self, wanted: Look) -> Step {
        match self.applied {
            None if !wanted.shown => Step::Nothing,
            None => {
                self.applied = Some(wanted);
                Step::Attach
            }
            Some(applied) if applied == wanted => Step::Nothing,
            Some(_) => {
                self.applied = Some(wanted);
                Step::Apply
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Nothing,
    Attach,
    Apply,
}

/// The menus as the system shows them: each title and entry its label, an entry's key after a
/// tab (Windows draws it right-aligned). Entries the palette does not offer are left out, then
/// separators at either end or twice in a row, then menus left empty.
pub fn build(keymap: &Keymap, entries: &[Entry]) -> Vec<BarMenu> {
    MENUS
        .iter()
        .map(|menu| BarMenu {
            title: i18n::code_text("menubar.title", menu.id),
            items: items(menu, keymap, entries),
        })
        .filter(|menu| !menu.items.is_empty())
        .collect()
}

fn items(menu: &Menu, keymap: &Keymap, entries: &[Entry]) -> Vec<BarItem> {
    let mut items: Vec<BarItem> = Vec::new();
    for slot in menu.slots {
        let id = match slot {
            Slot::Separator => {
                if items.last().is_some_and(|last| *last != BarItem::Separator) {
                    items.push(BarItem::Separator);
                }
                continue;
            }
            Slot::Entry(name) if !entries.iter().any(|entry| entry.name == *name) => continue,
            Slot::Entry(id) | Slot::Dialog(id, _) => *id,
        };
        let label = i18n::code_text("menubar", id);
        let text = match key_of(keymap, id) {
            Some(key) => format!("{label}\t{key}"),
            None => label,
        };
        items.push(BarItem::Entry { id, text });
    }
    if items.last() == Some(&BarItem::Separator) {
        items.pop();
    }
    items
}

/// The first key the keymap gives `name`, as the palette prints it: a key named by place as the
/// active layout prints it.
pub fn key_of(keymap: &Keymap, name: &str) -> Option<String> {
    let binding = keymap
        .bindings()
        .iter()
        .find(|binding| binding.name == name);
    let window = keymap.window().iter().find(|key| key.name == name);
    binding
        .map(|binding| crate::keymap::label(&binding.chord))
        .or_else(|| window.map(|key| crate::keymap::label(&key.chord)))
}

/// The slot of the entry `id`, in any menu.
pub fn slot(id: &str) -> Option<Slot> {
    MENUS
        .iter()
        .flat_map(|menu| menu.slots)
        .copied()
        .find(|slot| match slot {
            Slot::Entry(name) | Slot::Dialog(name, _) => *name == id,
            Slot::Separator => false,
        })
}

/// The choices made in the bar, as messages.
pub fn subscription() -> Subscription<Message> {
    Subscription::run(chosen)
}

fn chosen() -> impl Stream<Item = Message> {
    platform::events().map(|id| Message::Palette(PaletteMsg::MenuBar(MenuBarMsg::Chosen(id))))
}

impl App {
    /// Reads whether the bar shows (shown without a database); at the start and when the setting
    /// changes.
    pub(in crate::app) fn menu_bar_task(&self) -> Task<Message> {
        let shown = |shown| Message::Palette(PaletteMsg::MenuBar(MenuBarMsg::Shown(shown)));
        let Some(db) = self.db.clone() else {
            return Task::done(shown(true));
        };
        Task::perform(async move { stored_bool(&db, MENU_BAR_KEY).await }, shown)
    }

    pub(super) fn on_menu_bar(&mut self, message: MenuBarMsg) -> Vec<Effect> {
        match message {
            MenuBarMsg::Shown(shown) => {
                self.dialog_windows.menu_bar.shown = Some(shown);
                if !shown {
                    self.dialog_windows.menu_bar.open = None;
                }
                if self.drawn_bar_shown() {
                    self.rebuild_drawn_menus();
                }
                Vec::new()
            }
            MenuBarMsg::Title(menu) => {
                self.on_drawn_title(menu);
                Vec::new()
            }
            MenuBarMsg::HoverTitle(menu) => {
                let bar = &mut self.dialog_windows.menu_bar;
                if bar.open.is_some_and(|open| open.menu != menu) {
                    bar.open = Some(OpenMenu { menu, cursor: None });
                }
                Vec::new()
            }
            MenuBarMsg::HoverEntry(entry) => {
                if let Some(open) = &mut self.dialog_windows.menu_bar.open {
                    open.cursor = Some(entry);
                }
                Vec::new()
            }
            MenuBarMsg::Close => {
                self.dialog_windows.menu_bar.open = None;
                Vec::new()
            }
            // The drawn bar's menu closes before its choice runs.
            MenuBarMsg::Chosen(id) if self.dialog_windows.menu_bar.open.is_some() => {
                self.dialog_windows.menu_bar.open = None;
                self.on_menu_bar(MenuBarMsg::Chosen(id))
            }
            // The menus are greyed out while a dialog is open; a late choice does nothing.
            MenuBarMsg::Chosen(_) if self.dialog_open() => Vec::new(),
            MenuBarMsg::Chosen(id) => match slot(&id) {
                Some(Slot::Entry(name)) => self.run_menu_entry(name),
                Some(Slot::Dialog(_, dialog)) => self.open_menu_dialog(dialog),
                Some(Slot::Separator) | None => Vec::new(),
            },
        }
    }

    /// What the palette's row `name` runs.
    fn run_menu_entry(&mut self, name: &str) -> Vec<Effect> {
        let entries = super::entries(&self.keymap, &reach::standard(), self.drive_bar.network);
        match entries.into_iter().find(|entry| entry.name == name) {
            Some(entry) => self.run_palette(entry.action),
            None => Vec::new(),
        }
    }

    fn open_menu_dialog(&mut self, dialog: Dialog) -> Vec<Effect> {
        let connections = Message::Connections;
        match dialog {
            Dialog::Palette => self.open_palette(),
            Dialog::Page(page) => self.apply(Message::Settings(SettingsMessage::OpenPage(page))),
            Dialog::TcImport => self.apply(connections(ConnMsg::TcImport(TcImportMsg::Open))),
            // The manager first: its import pages open inside it.
            Dialog::RdmImport => {
                let mut effects = self.apply(connections(ConnMsg::Open));
                effects.extend(self.apply(connections(ConnMsg::RdmImport(RdmImportMsg::Open))));
                effects
            }
            Dialog::SshConfigImport => {
                let mut effects = self.apply(connections(ConnMsg::Open));
                effects.extend(self.apply(connections(ConnMsg::Import(ImportMsg::Open))));
                effects
            }
        }
    }

    /// Attaches the bar, or brings it and the popup menus in step with the app; after every
    /// update.
    pub(in crate::app) fn sync_menu_bar(&mut self) -> Task<Message> {
        let Some(main) = self.dialog_windows.main() else {
            return Task::none();
        };
        let popups = if self.dialog_windows.menu_bar.popups(self.dark) {
            popup_menus::follow(main, self.dark).discard()
        } else {
            Task::none()
        };
        let Some(shown) = self.dialog_windows.menu_bar.shown else {
            return popups;
        };
        let wanted = Look {
            shown,
            dark: self.dark,
            enabled: !self.dialog_open(),
            alt_letters: self.alt_letters_free(),
        };
        let bar = match self.dialog_windows.menu_bar.step(wanted) {
            Step::Nothing => Task::none(),
            Step::Attach => {
                let entries =
                    super::entries(&self.keymap, &reach::standard(), self.drive_bar.network);
                platform::attach(main, build(&self.keymap, &entries), wanted).discard()
            }
            Step::Apply => platform::apply(main, wanted).discard(),
        };
        Task::batch([popups, bar])
    }

    /// The attached bar's entries with the keys of the keymap in force, after it changed
    /// (RC-100-21).
    pub(in crate::app) fn relabel_menu_bar(&mut self) -> Task<Message> {
        if self.drawn_bar_shown() {
            self.rebuild_drawn_menus();
        }
        let Some(main) = self.dialog_windows.main() else {
            return Task::none();
        };
        let entries = super::entries(&self.keymap, &reach::standard(), self.drive_bar.network);
        platform::relabel(main, build(&self.keymap, &entries)).discard()
    }

    /// The bar's keys again when the keyboard layout changed, so a key named by place shows the
    /// new layout's character (RC-100-48). Looked at when a modifier changes: a layout switch is
    /// Alt+Shift, Ctrl+Shift or Win+Space.
    pub(in crate::app) fn follow_layout(&mut self) -> Vec<Effect> {
        let layout = crate::keymap::layout();
        let bar = &mut self.dialog_windows.menu_bar;
        if bar.layout.as_ref() == Some(&layout) {
            return Vec::new();
        }
        bar.layout = Some(layout);
        vec![Effect::Task(self.relabel_menu_bar())]
    }

    /// Whether Alt with a letter may open a menu: not when it is the quick search's key, nor while
    /// the active panel shows a terminal, whose shell reads Alt+letter.
    fn alt_letters_free(&self) -> bool {
        self.cmdline.options.keys != SearchKeys::Alt
            && self.pane(self.active).state.view != PanelView::Terminal
    }
}

#[cfg(test)]
#[path = "menu_bar_tests.rs"]
mod tests;
