//! The colours by file type (RC-100-43, Total Commander's "Define colors by file type"): the
//! rules of `appearance.colour_rules` compiled once - each mask parsed into Total Commander's
//! selection mask (`rc_engine::Mask`), each colour read - and asked for every name a list
//! draws. The first rule whose mask matches colours the name; a folder only by a mask that names
//! folders (`*\`), as Num+ selects them.

use std::sync::RwLock;

use iced::Color;
use rc_engine::Mask;
use rc_engine::settings::parse_colour_rules;

use super::colours::parse_hex;

/// One compiled rule.
#[derive(Debug, Clone)]
pub struct NameRule {
    mask: Mask,
    light: Color,
    dark: Color,
}

/// The rules of `text`, compiled; none for a list that does not read.
#[must_use]
pub fn compile(text: &str) -> Vec<NameRule> {
    parse_colour_rules(text)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|rule| {
            let light = parse_hex(&rule.light)?;
            let dark = rule.dark.as_deref().and_then(parse_hex).unwrap_or(light);
            Some(NameRule {
                mask: Mask::parse(&rule.mask),
                light,
                dark,
            })
        })
        .collect()
}

/// The colour the first of `rules` matching `name` gives it in the look `dark`.
#[must_use]
pub fn first_match(rules: &[NameRule], name: &str, dir: bool, dark: bool) -> Option<Color> {
    rules
        .iter()
        .find(|rule| rule.mask.matches(name, dir))
        .map(|rule| if dark { rule.dark } else { rule.light })
}

static RULES: RwLock<Vec<NameRule>> = RwLock::new(Vec::new());

/// The rules from now on.
pub fn set_rules(text: &str) {
    if let Ok(mut rules) = RULES.write() {
        *rules = compile(text);
    }
}

/// The colour the rules give the name `name` (a folder when `dir`) in the look `dark`; `None`
/// when no rule matches, and the list draws it in its own text colour.
#[must_use]
pub fn name_colour(name: &str, dir: bool, dark: bool) -> Option<Color> {
    let rules = RULES.read().ok()?;
    first_match(&rules, name, dir, dark)
}

#[cfg(test)]
#[path = "colour_rules_tests.rs"]
mod tests;
