//! The look of `wincmd.ini` against rCommander's settings (RC-050-14): the plan a preview shows,
//! and with it the writes.
//!
//! - A colour or the font is there already when the setting holds the same; new when it holds the
//!   theme's own (empty); else changed. Both are written through `settings.set`, which checks the
//!   value and announces it, so the window takes it at once.
//! - The rules by file type are appended to the ones there; a rule with the same mask (ignoring
//!   ASCII case) is there already with the same colours, else changed and replaced where it
//!   stands. New rules beyond [`MAX_COLOUR_RULES`] are skipped. The list is written once.

use serde_json::{Value, json};

use super::{
    total_commander::TcSkip,
    total_commander_apply::chosen,
    total_commander_look::{TcLook, TcLookKind},
    total_commander_report::{TcLookRow, TcRowStatus},
};
use crate::{
    Engine, EngineError,
    settings::{
        COLOUR_RULES_KEY, ColourRule, LIST_FONT_KEY, LIST_FONT_SIZE_KEY, MAX_COLOUR_RULES,
        colour_key, parse_colour_rules, stored_setting, write_colour_rules,
    },
};

/// New when `now` is the theme's own, there already when it is `wanted`, else changed.
fn compare(now: &str, wanted: &str) -> TcRowStatus {
    if now == wanted {
        TcRowStatus::Exists
    } else if now.is_empty() {
        TcRowStatus::New
    } else {
        TcRowStatus::Update
    }
}

/// A rule's colours as a row shows them.
fn rule_value(rule: &ColourRule) -> String {
    match &rule.dark {
        Some(dark) => format!("{} / {dark}", rule.light),
        None => rule.light.clone(),
    }
}

impl Engine {
    /// The text the setting `key` holds; empty when it holds none.
    async fn setting_text(&self, key: &str) -> Result<String, EngineError> {
        let value = stored_setting(self.db()?, key).await;
        Ok(value
            .as_ref()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }

    /// The look `looks` against the settings; written unless `dry_run`.
    pub(crate) async fn import_tc_look(
        &self,
        looks: &[TcLook],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<TcLookRow>, EngineError> {
        let db = self.db()?;
        let mut rules =
            parse_colour_rules(&self.setting_text(COLOUR_RULES_KEY).await?).unwrap_or_default();
        let room = MAX_COLOUR_RULES.saturating_sub(rules.len());
        let mut appended = 0;
        let mut rows = Vec::with_capacity(looks.len());
        for look in looks {
            let (value, status) = match &look.kind {
                Err(skip) => (String::new(), skipped(*skip)),
                Ok(TcLookKind::Colour { dark, token, value }) => {
                    let key = colour_key(*dark, token).ok_or(EngineError::SettingUnknown {
                        key: (*token).to_owned(),
                    })?;
                    (
                        value.clone(),
                        compare(&self.setting_text(key).await?, value),
                    )
                }
                Ok(TcLookKind::Rule(rule)) => {
                    let there = rules
                        .iter()
                        .find(|there| there.mask.eq_ignore_ascii_case(&rule.mask));
                    let status = match there {
                        Some(there) if there.light == rule.light && there.dark == rule.dark => {
                            TcRowStatus::Exists
                        }
                        Some(_) => TcRowStatus::Update,
                        None if appended < room => {
                            appended += 1;
                            TcRowStatus::New
                        }
                        None => skipped(TcSkip::TooMany),
                    };
                    (rule_value(rule), status)
                }
                Ok(TcLookKind::ListFont { family, size }) => {
                    let now = self.setting_text(LIST_FONT_KEY).await?;
                    let now_size = stored_setting(db, LIST_FONT_SIZE_KEY)
                        .await
                        .and_then(|value| value.as_i64());
                    let same = now.eq_ignore_ascii_case(family)
                        && size.is_none_or(|size| Some(size) == now_size);
                    let status = match () {
                        () if same => TcRowStatus::Exists,
                        () if now.is_empty() => TcRowStatus::New,
                        () => TcRowStatus::Update,
                    };
                    let value = match size {
                        Some(size) => format!("{family} {size}"),
                        None => family.clone(),
                    };
                    (value, status)
                }
            };
            let importable = matches!(status, TcRowStatus::New | TcRowStatus::Update);
            rows.push(TcLookRow {
                key: look.key.clone(),
                source: look.source.clone(),
                value,
                selected: importable && chosen(&look.key, only, true),
                status,
            });
        }
        if dry_run {
            return Ok(rows);
        }
        let mut rules_changed = false;
        for (row, look) in rows.iter_mut().zip(looks) {
            if !row.selected {
                continue;
            }
            match &look.kind {
                Ok(TcLookKind::Colour { dark, token, value }) => {
                    if let Some(key) = colour_key(*dark, token) {
                        self.set_setting(key, &json!(value)).await?;
                    }
                }
                Ok(TcLookKind::ListFont { family, size }) => {
                    self.set_setting(LIST_FONT_KEY, &json!(family)).await?;
                    if let Some(size) = size {
                        self.set_setting(LIST_FONT_SIZE_KEY, &json!(size)).await?;
                    }
                }
                Ok(TcLookKind::Rule(rule)) => {
                    match rules
                        .iter_mut()
                        .find(|there| there.mask.eq_ignore_ascii_case(&rule.mask))
                    {
                        Some(there) => *there = rule.clone(),
                        None => rules.push(rule.clone()),
                    }
                    rules_changed = true;
                }
                Err(_) => continue,
            }
            row.status = match row.status {
                TcRowStatus::Update => TcRowStatus::Updated,
                _ => TcRowStatus::Created,
            };
        }
        if rules_changed {
            self.set_setting(COLOUR_RULES_KEY, &json!(write_colour_rules(&rules)))
                .await?;
        }
        Ok(rows)
    }
}

fn skipped(skip: TcSkip) -> TcRowStatus {
    TcRowStatus::Skipped {
        reason: skip.code().to_owned(),
    }
}

#[cfg(test)]
#[path = "total_commander_look_apply_tests.rs"]
mod tests;
