//! The native menu bar (RC-080-18): the menus the app builds (`app/palette/menu_bar.rs`) as a
//! muda menu on the main window. On Windows it is a Win32 menu (`windows/menu_bar.rs` attaches
//! it, keeps the client area's size, draws the bar dark and opens it with Alt); elsewhere nothing
//! is attached yet - macOS gets muda's menu in the system's menu bar later (the plan is in the job
//! file), Linux would need GTK, which the window does not use.
//!
//! muda's menu is not `Send`: it is built inside `window::run`, on the event loop's thread, and
//! kept there for the window's life ([`BAR`]). A choice comes back through muda's event handler,
//! which runs on that thread too, as the entry's id ([`events`]).

use std::{
    cell::RefCell,
    sync::{Mutex, OnceLock, PoisonError},
};

use iced::{
    Task,
    futures::{SinkExt, Stream, channel::mpsc},
    window,
};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

/// One menu as the system shows it: its title and its entries, each with its mnemonic's `&`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarMenu {
    pub title: String,
    pub items: Vec<BarItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarItem {
    /// An entry: its id comes back when it is chosen; its text is the label, then a tab and the
    /// key, if it has one.
    Entry {
        id: &'static str,
        text: String,
    },
    Separator,
}

/// How the bar is to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// The setting `appearance.menu_bar`.
    pub shown: bool,
    /// The app's dark look: the bar is drawn dark too.
    pub dark: bool,
    /// Off while a dialog is open: the menus are greyed out, and Alt leaves the bar alone.
    pub enabled: bool,
    /// Whether Alt with a letter opens a menu, or belongs to the quick search or a terminal.
    pub alt_letters: bool,
}

/// How the dark bar is drawn: the diagnostic switch `RC_MENU_BAR_THEME`, read once at start, so
/// the owner can turn rCommander's parts off one at a time without a rebuild. Unset or unknown is
/// the default, [`BarPaint::Opaque`].
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarPaint {
    /// `opaque`: muda draws the dark bar into a buffer that is made opaque before it reaches the
    /// window, and the line muda paints under the bar is painted over opaque. GDI leaves a pixel's
    /// alpha at 0, and where Windows composes the frame with it (the owner's newest Windows 11) the
    /// bar was see-through.
    Opaque,
    /// `muda`: muda draws the dark bar straight onto the window, as before the fix.
    Muda,
    /// `system`: Windows draws the bar; the popup menus still follow the look.
    System,
    /// `light`: Windows draws the bar and the popup menus are left alone (no uxtheme call).
    Light,
    /// `nosubclass`: as `muda`, and the Alt subclass is not installed (Alt does not open the bar).
    NoSubclass,
}

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
impl BarPaint {
    /// The switch as the environment sets it, read once.
    pub fn get() -> Self {
        static PAINT: OnceLock<BarPaint> = OnceLock::new();
        *PAINT.get_or_init(|| Self::parse(std::env::var("RC_MENU_BAR_THEME").ok().as_deref()))
    }

    /// The switch's value; unset or unknown is the default.
    pub fn parse(value: Option<&str>) -> Self {
        match value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("muda") => Self::Muda,
            Some("system") => Self::System,
            Some("light") => Self::Light,
            Some("nosubclass") => Self::NoSubclass,
            _ => Self::Opaque,
        }
    }

    /// Whether muda draws the bar dark for the look `dark`.
    pub fn muda_dark(self, dark: bool) -> bool {
        dark && matches!(self, Self::Opaque | Self::Muda | Self::NoSubclass)
    }

    /// Whether muda's dark drawing goes through the opaque buffer for the look `dark`.
    pub fn opaque(self, dark: bool) -> bool {
        dark && self == Self::Opaque
    }

    /// Whether the Alt subclass is installed.
    pub fn subclass(self) -> bool {
        self != Self::NoSubclass
    }

    /// Whether the popup menus follow the look (uxtheme's `SetPreferredAppMode`).
    pub fn popups(self) -> bool {
        self != Self::Light
    }
}

/// The bar on the main window and the look it has, on the event loop's thread.
struct Attached {
    menu: Menu,
    submenus: Vec<Submenu>,
    look: Look,
}

thread_local! {
    static BAR: RefCell<Option<Attached>> = const { RefCell::new(None) };
}

/// The sender of the running [`events`] stream; muda's handler, which it takes only once, sends
/// through whichever stream runs.
static CHOSEN: Mutex<Option<UnboundedSender<String>>> = Mutex::new(None);

/// Builds `menus` and attaches them to the main window `id` with `look`; once.
pub fn attach(id: window::Id, menus: Vec<BarMenu>, look: Look) -> Task<()> {
    window::run(id, move |window| {
        if BAR.with_borrow(Option::is_some) {
            return;
        }
        // A menu muda cannot build leaves the window without a bar; every key still works.
        let Ok((menu, submenus)) = build(&menus) else {
            return;
        };
        let letters = menus
            .iter()
            .filter_map(|menu| mnemonic(&menu.title))
            .collect();
        if !attach_to(window, &menu, look, letters) {
            return;
        }
        for submenu in &submenus {
            submenu.set_enabled(look.enabled);
        }
        BAR.set(Some(Attached {
            menu,
            submenus,
            look,
        }));
    })
}

/// Brings the attached bar of the main window `id` to `look`.
pub fn apply(id: window::Id, look: Look) -> Task<()> {
    window::run(id, move |window| {
        BAR.with_borrow_mut(|bar| {
            let Some(bar) = bar else {
                return;
            };
            if bar.look.enabled != look.enabled {
                for submenu in &bar.submenus {
                    submenu.set_enabled(look.enabled);
                }
            }
            apply_to(window, &bar.menu, bar.look, look);
            bar.look = look;
        });
    })
}

/// Gives the entries of the attached bar of the main window `id` the texts of `menus`, by their
/// ids: the keys of a changed keymap (RC-100-21). An entry `menus` does not name keeps its text.
pub fn relabel(id: window::Id, menus: Vec<BarMenu>) -> Task<()> {
    window::run(id, move |_window| {
        BAR.with_borrow(|bar| {
            let Some(bar) = bar else {
                return;
            };
            for submenu in &bar.submenus {
                for item in submenu.items() {
                    let Some(item) = item.as_menuitem() else {
                        continue;
                    };
                    let text =
                        menus
                            .iter()
                            .flat_map(|menu| &menu.items)
                            .find_map(|entry| match entry {
                                BarItem::Entry { id, text } if item.id() == *id => Some(text),
                                _ => None,
                            });
                    if let Some(text) = text {
                        item.set_text(text);
                    }
                }
            }
        });
    })
}

/// The ids of the entries chosen, for as long as the stream runs.
pub fn events() -> impl Stream<Item = String> {
    iced::stream::channel(16, async |mut output: mpsc::Sender<String>| {
        let (sender, mut receiver) = unbounded_channel();
        *CHOSEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(sender);
        MenuEvent::set_event_handler(Some(forward));
        while let Some(id) = receiver.recv().await {
            if output.send(id).await.is_err() {
                break;
            }
        }
    })
}

/// muda's event handler, on the window's thread: the id to the running stream.
fn forward(event: MenuEvent) {
    if let Some(sender) = CHOSEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        // Without a stream the window is gone: nobody waits for the choice.
        let _ = sender.send(event.id.0);
    }
}

/// The muda menu of `menus` and its menus, to grey them out.
fn build(menus: &[BarMenu]) -> muda::Result<(Menu, Vec<Submenu>)> {
    let bar = Menu::new();
    let mut submenus = Vec::with_capacity(menus.len());
    for menu in menus {
        let submenu = Submenu::new(&menu.title, true);
        for item in &menu.items {
            match item {
                // No accelerator: the key stays the keymap's, the text after the tab shows it.
                BarItem::Entry { id, text } => {
                    submenu.append(&MenuItem::with_id(*id, text, true, None))?;
                }
                BarItem::Separator => submenu.append(&PredefinedMenuItem::separator())?,
            }
        }
        bar.append(&submenu)?;
        submenus.push(submenu);
    }
    Ok((bar, submenus))
}

/// The mnemonic of a label: the character after its `&` (`&&` is an ampersand), in lower case.
pub fn mnemonic(label: &str) -> Option<char> {
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => continue,
                Some(next) => return next.to_lowercase().next(),
                None => return None,
            }
        }
    }
    None
}

#[cfg(windows)]
fn attach_to(window: &dyn window::Window, menu: &Menu, look: Look, letters: Vec<char>) -> bool {
    super::windows::menu_bar::attach(window, menu, look, letters)
}

#[cfg(not(windows))]
fn attach_to(_window: &dyn window::Window, _menu: &Menu, _look: Look, _letters: Vec<char>) -> bool {
    false
}

#[cfg(windows)]
fn apply_to(window: &dyn window::Window, menu: &Menu, from: Look, to: Look) {
    super::windows::menu_bar::apply(window, menu, from, to);
}

#[cfg(not(windows))]
fn apply_to(_window: &dyn window::Window, _menu: &Menu, _from: Look, _to: Look) {}

/// What Alt does on the main window while the bar is there: pressed and let go alone it
/// highlights the bar; with a menu's letter it opens that menu. Windows does both in
/// `DefWindowProc`, which winit never calls for these keys; `windows/menu_bar.rs` feeds the key
/// messages in and posts what this answers.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
#[derive(Debug, Default)]
pub struct AltKeys {
    /// Alt went down alone, without Ctrl or Shift, and no other key or click came since.
    armed: bool,
    /// The bar is shown and no dialog is open.
    pub active: bool,
    /// Alt with a letter is the bar's ([`Look::alt_letters`]).
    pub letters_free: bool,
    /// The menus' mnemonics, in lower case.
    pub letters: Vec<char>,
}

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
impl AltKeys {
    /// A key went down: `alt` whether it is Alt, `others` whether Ctrl or Shift is held,
    /// `repeat` whether it was down already.
    pub fn down(&mut self, alt: bool, others: bool, repeat: bool) {
        if !alt {
            self.armed = false;
        } else if !repeat {
            self.armed = !others;
        }
    }

    /// A click or the focus lost: Alt is no longer alone.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// Alt went up with Ctrl or Shift held (`others`) or not: whether to highlight the bar.
    pub fn alt_up(&mut self, others: bool) -> bool {
        std::mem::take(&mut self.armed) && !others && self.active
    }

    /// Alt with the character `typed` (Ctrl held: `ctrl`): the mnemonic of the menu to open.
    pub fn letter(&self, typed: char, ctrl: bool) -> Option<char> {
        let letter = typed.to_lowercase().next()?;
        let open = self.active && self.letters_free && !ctrl && self.letters.contains(&letter);
        open.then_some(letter)
    }
}

#[cfg(test)]
#[path = "menu_bar_tests.rs"]
mod tests;
