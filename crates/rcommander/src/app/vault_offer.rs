//! The offer to remove the vault (RC-050-03): once the last stored secret is gone, the engine
//! announces [`EngineEvent::VaultUnused`] and the window asks whether the master password should
//! go too. Remove sends the `GuiOnly` command `vault.remove` (refused with `vault_in_use` if a
//! secret was stored meanwhile; the refusal lands in the panel's status line); Keep or Escape
//! dismisses the offer and the vault stays. While the offer is open no other key or click reaches
//! the window below it; a vault removed elsewhere closes it.

use iced::{Element, widget::column};
use rc_engine::{Command, EngineEvent};

use super::{App, Effect, Message, escape::escape};
use crate::{
    i18n::t,
    theme::{self, Tokens, fit::label, icon::Icon, layout},
};

/// What the offer's buttons say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultOfferMsg {
    Remove,
    Keep,
}

impl App {
    /// The offer's share of every message: the vault events, and while it is open every key
    /// and click. `None` for a message it leaves to the rest of the window.
    pub(super) fn vault_offer_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::VaultUnused) => {
                self.vault_offer = true;
                Some(Vec::new())
            }
            // The connection manager takes this too.
            Message::Engine(EngineEvent::VaultRemoved) => {
                self.vault_offer = false;
                None
            }
            _ if !self.vault_offer => None,
            // Tab and Shift+Tab walk the buttons (RC-080-16).
            Message::Key(chord) if super::field_walk::tab(chord).is_some() => {
                super::field_walk::walk_key(chord)
            }
            Message::Key(chord) | Message::KeyCaptured(chord) => {
                if *chord == escape() {
                    self.vault_offer = false;
                }
                Some(Vec::new())
            }
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// A button of the offer; it closes at once, so a second click cannot send it again.
    pub(super) fn on_vault_offer(&mut self, message: VaultOfferMsg) -> Vec<Effect> {
        if !std::mem::take(&mut self.vault_offer) {
            return Vec::new();
        }
        match message {
            VaultOfferMsg::Remove => {
                vec![Effect::Dispatch(self.active, vec![Command::RemoveVault])]
            }
            VaultOfferMsg::Keep => Vec::new(),
        }
    }

    /// The offer over the window while it is open.
    pub(super) fn vault_offer_layer(
        &self,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let title = label(t!("vault_offer.title")).font(theme::UI_FONT_STRONG);
        Some(layout::dialog_card(
            Message::dialog_head(title),
            self.vault_offer_body()?,
            layout::SMALL_DIALOG_WIDTH,
            tokens,
        ))
    }

    /// The offer's text and buttons while it is open: on its card, or filling a window of its
    /// own (RC-080-23).
    pub(super) fn vault_offer_body(&self) -> Option<Element<'_, Message>> {
        if !self.vault_offer {
            return None;
        }
        let buttons = layout::button_row(
            Some(layout::primary_icon_button(
                Icon::Delete,
                t!("vault_offer.remove"),
                Some(Message::VaultOffer(VaultOfferMsg::Remove)),
            )),
            [layout::secondary_button(
                t!("vault_offer.keep"),
                Some(Message::VaultOffer(VaultOfferMsg::Keep)),
            )],
            [],
        );
        Some(
            column![label(t!("vault_offer.text")), buttons]
                .spacing(10)
                .into(),
        )
    }
}

#[cfg(test)]
#[path = "vault_offer_tests.rs"]
mod tests;
