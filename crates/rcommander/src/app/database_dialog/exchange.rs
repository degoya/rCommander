//! The export and import of settings and connections in the database dialog (RC-080-19).
//!
//! Export: the four parts, each a box; unencrypted or encrypted with an export password typed
//! twice (the default); passwords and keys only with a box of their own, which for an
//! unencrypted file asks for a second box that confirms its warning. The dialog opens with the
//! parts and the encryption of the last export (`exchange.export.*`), which the engine keeps.
//! Import: the file and, for an encrypted one, its export password; Read shows every row with
//! its status - new, already there, changed, not imported - and a box, the conflicts and the
//! protected settings unticked with their warning; Import writes the ticked ones. When the vault
//! is locked (or absent) and a secret goes in or out, the dialog asks for the master password
//! (a new one twice) and runs the command again after it.

use std::collections::BTreeSet;

use rc_engine::{
    Command, ConfigExportRequest, ConfigImportRequest, SecretParam,
    exchange::{ExchangeReport, ExportParts},
};
use rc_vault::{ExposeSecret as _, SecretString};

use crate::{
    app::vault_ask::{Ask, Unsent, vault_command},
    i18n::t,
};

/// A part of the export, for its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Settings,
    Connections,
    Trust,
    Hotlist,
}

impl Part {
    pub const ALL: [Self; 4] = [
        Self::Settings,
        Self::Connections,
        Self::Trust,
        Self::Hotlist,
    ];

    pub fn of(self, parts: ExportParts) -> bool {
        match self {
            Self::Settings => parts.settings,
            Self::Connections => parts.connections,
            Self::Trust => parts.trust,
            Self::Hotlist => parts.hotlist,
        }
    }

    fn set(self, parts: &mut ExportParts, on: bool) {
        match self {
            Self::Settings => parts.settings = on,
            Self::Connections => parts.connections = on,
            Self::Trust => parts.trust = on,
            Self::Hotlist => parts.hotlist = on,
        }
    }
}

/// The export's and the import's fields.
#[derive(Debug, Clone)]
pub struct ExchangeForm {
    pub parts: ExportParts,
    pub encrypted: bool,
    /// The export password: typed for an export, given for an encrypted import.
    pub password: SecretString,
    pub repeat: SecretString,
    /// Passwords, keys and passphrases go with the connections.
    pub secrets: bool,
    /// The warning that an unencrypted file holds them in clear, confirmed.
    pub plain_confirmed: bool,
    /// The import's preview, or what it did.
    pub report: Option<ExchangeReport>,
    /// The rows ticked, by key.
    pub chosen: BTreeSet<String>,
    /// The vault's question, while it is asked.
    pub vault: Option<Ask>,
    pub master: SecretString,
    pub master_repeat: SecretString,
    /// What keeps the form from running, as an i18n text.
    pub problem: Option<&'static str>,
}

impl Default for ExchangeForm {
    fn default() -> Self {
        Self {
            parts: ExportParts::ALL,
            encrypted: true,
            password: SecretString::from(""),
            repeat: SecretString::from(""),
            secrets: false,
            plain_confirmed: false,
            report: None,
            chosen: BTreeSet::new(),
            vault: None,
            master: SecretString::from(""),
            master_repeat: SecretString::from(""),
            problem: None,
        }
    }
}

/// The export's and the import's messages; a typed password only as a [`SecretString`], never
/// in `Debug` (RC-080-39).
#[derive(Debug, Clone)]
pub enum ExchangeMsg {
    Part(Part, bool),
    Encrypted(bool),
    Password(SecretString),
    Repeat(SecretString),
    Secrets(bool),
    PlainConfirmed(bool),
    /// The import's Read: the file previewed.
    Read,
    Choose(String, bool),
    /// Every choosable row ticked, or none.
    ChooseAll(bool),
    Master(SecretString),
    MasterRepeat(SecretString),
    /// The parts and the encryption of the last export, read when the dialog opens.
    Choices(ExportParts, bool),
}

impl ExchangeForm {
    /// Applies a message of the form; a problem shown before goes.
    pub fn update(&mut self, message: ExchangeMsg) {
        self.problem = None;
        match message {
            ExchangeMsg::Part(part, on) => part.set(&mut self.parts, on),
            ExchangeMsg::Encrypted(on) => {
                self.encrypted = on;
                self.plain_confirmed = false;
            }
            ExchangeMsg::Password(typed) => self.password = typed,
            ExchangeMsg::Repeat(typed) => self.repeat = typed,
            ExchangeMsg::Secrets(on) => {
                self.secrets = on;
                self.plain_confirmed = false;
            }
            ExchangeMsg::PlainConfirmed(on) => self.plain_confirmed = on,
            ExchangeMsg::Choose(key, on) => {
                if on {
                    self.chosen.insert(key);
                } else {
                    self.chosen.remove(&key);
                }
            }
            ExchangeMsg::ChooseAll(on) => {
                self.chosen.clear();
                if on && let Some(report) = &self.report {
                    self.chosen.extend(
                        report
                            .rows
                            .iter()
                            .filter(|row| row.status.choosable(row.secrets))
                            .map(|row| row.key.clone()),
                    );
                }
            }
            ExchangeMsg::Master(typed) => self.master = typed,
            ExchangeMsg::MasterRepeat(typed) => self.master_repeat = typed,
            ExchangeMsg::Choices(parts, encrypted) => {
                self.parts = parts;
                self.encrypted = encrypted;
            }
            // The dialog runs it.
            ExchangeMsg::Read => {}
        }
    }

    /// The commands of an export to `path`: the vault's answer first when it was asked. `Err`
    /// with the i18n text of what is missing.
    pub fn export(&self, path: String, overwrite: bool) -> Result<Vec<Command>, &'static str> {
        if !self.parts.any() {
            return Err(t!("exchange.problem.no_part"));
        }
        let password = if self.encrypted {
            let typed = self.password.expose_secret();
            if typed.is_empty() {
                return Err(t!("exchange.problem.password_empty"));
            }
            if typed != self.repeat.expose_secret() {
                return Err(t!("exchange.problem.password_mismatch"));
            }
            Some(SecretParam::from(typed))
        } else {
            None
        };
        let secrets = self.secrets && self.parts.connections;
        if secrets && !self.encrypted && !self.plain_confirmed {
            return Err(t!("exchange.problem.plain_unconfirmed"));
        }
        let mut commands = self.vault_commands()?;
        commands.push(Command::ExportConfig(ConfigExportRequest {
            path,
            parts: self.parts,
            password,
            secrets,
            plain_secrets: secrets && !self.encrypted,
            overwrite,
        }));
        Ok(commands)
    }

    /// The commands of an import of `path`: a preview (`dry_run`), or the ticked rows written,
    /// after the vault's answer when it was asked.
    pub fn import(&self, path: String, dry_run: bool) -> Result<Vec<Command>, &'static str> {
        let typed = self.password.expose_secret();
        let mut commands = if dry_run {
            Vec::new()
        } else {
            self.vault_commands()?
        };
        commands.push(Command::ImportConfig(ConfigImportRequest {
            path,
            password: (!typed.is_empty()).then(|| SecretParam::from(typed)),
            dry_run,
            only: (!dry_run).then(|| self.chosen.iter().cloned().collect()),
        }));
        Ok(commands)
    }

    /// The vault's answer, when it was asked: unlock, or a new master password typed twice.
    fn vault_commands(&self) -> Result<Vec<Command>, &'static str> {
        let Some(ask) = self.vault else {
            return Ok(Vec::new());
        };
        match vault_command(ask, &self.master, &self.master_repeat) {
            Ok(command) => Ok(vec![command]),
            Err(Unsent::Empty) => Err(t!("exchange.problem.master_empty")),
            Err(Unsent::Mismatch) => Err(t!("exchange.problem.password_mismatch")),
        }
    }

    /// The preview `report` arrived: the rows the engine chose are ticked.
    pub fn previewed(&mut self, report: ExchangeReport) {
        self.chosen = report
            .rows
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.key.clone())
            .collect();
        self.report = Some(report);
    }
}

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
