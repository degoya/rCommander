//! What the dialog edits: every setting as typed, the retention limits and the MCP policy - and
//! the changes against what was loaded, checked before anything is stored.

use std::collections::BTreeMap;

use rc_engine::{
    CommandClass, RetentionStatus, SETTINGS, SettingValue,
    settings::{SettingKind, SettingSpec},
};
use rc_mcp::{Policy, Rule};
use serde_json::Value;

use super::fonts_page::{self, FontChoice};
use crate::theme::fonts::Family;

/// What the dialog shows when it opens.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub values: Vec<SettingValue>,
    pub retention: Vec<RetentionStatus>,
    pub policy: Policy,
    /// The ids of the shells this machine has, the default first.
    pub shells: Vec<String>,
    /// The font families this machine has, by name (RC-010-17).
    pub fonts: Vec<Family>,
}

/// One setting as edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Bool(bool),
    /// A number, a choice, an id or a path, as typed.
    Text(String),
}

/// A retained table's limits as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionDraft {
    pub status: RetentionStatus,
    pub rows: String,
    pub days: String,
}

/// Which of a table's two limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    Rows,
    Days,
}

/// What to store: only what differs from what was loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    pub settings: Vec<(&'static str, Value)>,
    /// Table, rows, days.
    pub retention: Vec<(String, u64, u32)>,
    pub policy: Option<Policy>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.settings.is_empty() && self.retention.is_empty() && self.policy.is_none()
    }
}

/// Why the changes cannot be stored: the i18n key of the field's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid {
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct Draft {
    loaded: Loaded,
    fields: BTreeMap<&'static str, Field>,
    pub retention: Vec<RetentionDraft>,
    pub policy: Policy,
    /// The font lists offer monospaced families only.
    pub mono_only: bool,
    /// The font lists' choices - every family, monospaced ones only - built once, not on every
    /// frame (RC-080-53).
    font_choices: [Vec<FontChoice>; 2],
}

/// The four classes the policy sets, in the dialog's order.
pub const POLICY_CLASSES: [CommandClass; 4] = [
    CommandClass::Read,
    CommandClass::Write,
    CommandClass::Destructive,
    CommandClass::Exec,
];

/// The rules a class can have in the dialog: destructive and exec never run without a
/// confirmation, so `allowed` is not offered for them.
pub fn rules_for(class: CommandClass) -> &'static [Rule] {
    match class {
        CommandClass::Destructive | CommandClass::Exec => &[Rule::Ask, Rule::Deny],
        _ => &[Rule::Allowed, Rule::Ask, Rule::Deny],
    }
}

/// The policy's rule for `class` as the dialog shows it.
pub fn rule_of(policy: &Policy, class: CommandClass) -> Rule {
    policy.decide(class)
}

fn set_rule(policy: &mut Policy, class: CommandClass, rule: Rule) {
    match class {
        CommandClass::Read => policy.read = rule,
        CommandClass::Write => policy.write = rule,
        CommandClass::Destructive => policy.destructive = rule,
        CommandClass::Exec => policy.exec = rule,
        CommandClass::GuiOnly => {}
    }
}

fn field_of(spec: &SettingSpec, value: &Value) -> Field {
    match spec.kind {
        SettingKind::Bool { .. } => Field::Bool(value.as_bool().unwrap_or_default()),
        SettingKind::Int { .. } => Field::Text(value.as_i64().unwrap_or_default().to_string()),
        _ => Field::Text(value.as_str().unwrap_or_default().to_owned()),
    }
}

/// The value of an edited field, if it is one for `spec`.
fn value_of(spec: &SettingSpec, field: &Field) -> Option<Value> {
    let value = match (spec.kind, field) {
        (SettingKind::Bool { .. }, Field::Bool(on)) => Value::Bool(*on),
        (SettingKind::Int { .. }, Field::Text(text)) => {
            Value::from(text.trim().parse::<i64>().ok()?)
        }
        (_, Field::Text(text)) => Value::from(text.trim()),
        (_, Field::Bool(_)) => return None,
    };
    spec.check(&value).ok()
}

impl Draft {
    pub fn new(loaded: Loaded) -> Self {
        let fields = SETTINGS
            .iter()
            .map(|spec| {
                let value = loaded
                    .values
                    .iter()
                    .find(|value| value.key == spec.key)
                    .map_or_else(|| spec.default_value(), |value| value.value.clone());
                (spec.key, field_of(spec, &value))
            })
            .collect();
        let retention = loaded
            .retention
            .iter()
            .map(|status| RetentionDraft {
                status: status.clone(),
                rows: status.limits.max_rows.to_string(),
                days: status.limits.max_age_days.to_string(),
            })
            .collect();
        Self {
            policy: loaded.policy,
            fields,
            retention,
            mono_only: false,
            font_choices: fonts_page::choices(&loaded.fonts),
            loaded,
        }
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.get(key)
    }

    pub fn shells(&self) -> &[String] {
        &self.loaded.shells
    }

    pub fn fonts(&self) -> &[Family] {
        &self.loaded.fonts
    }

    /// The choices of a font list: the area's default, then the families it offers now.
    pub(super) fn font_choices(&self) -> &[FontChoice] {
        &self.font_choices[usize::from(self.mono_only)]
    }

    /// Changes the field `key`; a field of another type stays.
    pub fn edit(&mut self, key: &str, field: Field) {
        if let Some(current) = self.fields.get_mut(key)
            && std::mem::discriminant(current) == std::mem::discriminant(&field)
        {
            *current = field;
        }
    }

    pub fn edit_retention(&mut self, index: usize, limit: Limit, text: String) {
        if let Some(draft) = self.retention.get_mut(index) {
            match limit {
                Limit::Rows => draft.rows = text,
                Limit::Days => draft.days = text,
            }
        }
    }

    pub fn set_rule(&mut self, class: CommandClass, rule: Rule) {
        set_rule(&mut self.policy, class, rule);
    }

    /// Everything that differs from what was loaded, or the first field that is not valid.
    pub fn changes(&self) -> Result<Changes, Invalid> {
        let mut changes = Changes::default();
        for spec in SETTINGS {
            let Some(field) = self.fields.get(spec.key) else {
                continue;
            };
            let value = value_of(spec, field).ok_or(Invalid { label: spec.label })?;
            let loaded = self.loaded.values.iter().find(|v| v.key == spec.key);
            if loaded.is_none_or(|loaded| loaded.value != value) {
                changes.settings.push((spec.key, value));
            }
        }
        for draft in &self.retention {
            let status = &draft.status;
            let invalid = Invalid {
                label: "settings.retention.title",
            };
            let rows: u64 = draft.rows.trim().parse().map_err(|_| invalid)?;
            let days: u32 = draft.days.trim().parse().map_err(|_| invalid)?;
            let within = (status.floor.max_rows..=status.ceiling.max_rows).contains(&rows)
                && (status.floor.max_age_days..=status.ceiling.max_age_days).contains(&days);
            if !within {
                return Err(invalid);
            }
            if (rows, days) != (status.limits.max_rows, status.limits.max_age_days) {
                changes.retention.push((status.table.clone(), rows, days));
            }
        }
        if self.policy != self.loaded.policy {
            changes.policy = Some(self.policy);
        }
        Ok(changes)
    }

    /// The draft after `changes` were stored: they count as loaded.
    pub fn stored(&mut self, changes: &Changes) {
        for (key, value) in &changes.settings {
            if let Some(loaded) = self.loaded.values.iter_mut().find(|v| v.key == *key) {
                loaded.value = value.clone();
            }
        }
        for (table, rows, days) in &changes.retention {
            if let Some(draft) = self.retention.iter_mut().find(|d| d.status.table == *table) {
                draft.status.limits.max_rows = *rows;
                draft.status.limits.max_age_days = *days;
            }
        }
        if let Some(policy) = changes.policy {
            self.loaded.policy = policy;
        }
    }
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
