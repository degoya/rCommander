//! The dialog's fields as typed, and the [`RenameRule`] they make: the dialog keeps no rename
//! state of its own, every input becomes a rule and the engine previews it.

use std::fmt;

use rc_engine::{CaseMode, CounterSettings, RenameRule, SearchReplace};

use crate::i18n::t;

/// A text field of the dialog, in the order Tab walks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    NameMask,
    ExtensionMask,
    Find,
    Replace,
    CounterStart,
    CounterStep,
    CounterWidth,
    PresetName,
    LogPath,
}

impl Field {
    /// The widget id that takes the keys.
    pub fn id(self) -> &'static str {
        match self {
            Self::NameMask => "multi_rename.name_mask",
            Self::ExtensionMask => "multi_rename.extension_mask",
            Self::Find => "multi_rename.find",
            Self::Replace => "multi_rename.replace",
            Self::CounterStart => "multi_rename.counter_start",
            Self::CounterStep => "multi_rename.counter_step",
            Self::CounterWidth => "multi_rename.counter_width",
            Self::PresetName => "multi_rename.preset_name",
            Self::LogPath => "multi_rename.log_path",
        }
    }
}

/// A switch of the search and replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// `^`: upper and lower case differ.
    CaseSensitive,
    /// `1x`: only the first occurrence.
    FirstOnly,
    /// `[E]`: search in the name with its extension.
    WithExtension,
    RegEx,
    /// `Subst.`: a match makes the whole name the replacement.
    Substitute,
}

/// The case of the whole new name, as the drop-down shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseChoice(pub CaseMode);

impl CaseChoice {
    pub const ALL: [Self; 5] = [
        Self(CaseMode::Unchanged),
        Self(CaseMode::Lower),
        Self(CaseMode::Upper),
        Self(CaseMode::FirstUpper),
        Self(CaseMode::WordsUpper),
    ];

    /// The next mode, after the last the first: Alt+C.
    pub fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or_default();
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
}

impl fmt::Display for CaseChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            CaseMode::Unchanged => t!("multi_rename.case_unchanged"),
            CaseMode::Lower => t!("multi_rename.case_lower"),
            CaseMode::Upper => t!("multi_rename.case_upper"),
            CaseMode::FirstUpper => t!("multi_rename.case_first_upper"),
            CaseMode::WordsUpper => t!("multi_rename.case_words_upper"),
        })
    }
}

/// A placeholder the buttons and the menu insert into a mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    Name,
    NameRange,
    Counter,
    Date,
    Time,
    Extension,
    FileName,
    Parent,
    Grandparent,
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Upper,
    Lower,
    FirstUpper,
    CaseAsIs,
    OpenBracket,
    CloseBracket,
}

impl Placeholder {
    /// The buttons beside the masks.
    pub const BUTTONS: [Self; 5] = [
        Self::Name,
        Self::NameRange,
        Self::Counter,
        Self::Date,
        Self::Time,
    ];

    /// The menu with all the others.
    pub const MORE: [Self; 16] = [
        Self::Extension,
        Self::FileName,
        Self::Parent,
        Self::Grandparent,
        Self::Year,
        Self::Month,
        Self::Day,
        Self::Hour,
        Self::Minute,
        Self::Second,
        Self::Upper,
        Self::Lower,
        Self::FirstUpper,
        Self::CaseAsIs,
        Self::OpenBracket,
        Self::CloseBracket,
    ];

    /// What it inserts. `[N1-]` is the range button's start (characters 1 to the end), to be
    /// edited into the range wanted.
    pub fn text(self) -> &'static str {
        match self {
            Self::Name => "[N]",
            Self::NameRange => "[N1-]",
            Self::Counter => "[C]",
            Self::Date => "[YMD]",
            Self::Time => "[hms]",
            Self::Extension => "[E]",
            Self::FileName => "[A]",
            Self::Parent => "[P]",
            Self::Grandparent => "[G]",
            Self::Year => "[Y]",
            Self::Month => "[M]",
            Self::Day => "[D]",
            Self::Hour => "[h]",
            Self::Minute => "[m]",
            Self::Second => "[s]",
            Self::Upper => "[U]",
            Self::Lower => "[L]",
            Self::FirstUpper => "[F]",
            Self::CaseAsIs => "[n]",
            Self::OpenBracket => "[[]",
            Self::CloseBracket => "[]]",
        }
    }

    /// What the button says: the range button shows Total Commander's `[N#-#]`.
    pub fn label(self) -> &'static str {
        match self {
            Self::NameRange => "[N#-#]",
            other => other.text(),
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::Name => t!("multi_rename.ph_name"),
            Self::NameRange => t!("multi_rename.ph_name_range"),
            Self::Counter => t!("multi_rename.ph_counter"),
            Self::Date => t!("multi_rename.ph_date"),
            Self::Time => t!("multi_rename.ph_time"),
            Self::Extension => t!("multi_rename.ph_extension"),
            Self::FileName => t!("multi_rename.ph_file_name"),
            Self::Parent => t!("multi_rename.ph_parent"),
            Self::Grandparent => t!("multi_rename.ph_grandparent"),
            Self::Year => t!("multi_rename.ph_year"),
            Self::Month => t!("multi_rename.ph_month"),
            Self::Day => t!("multi_rename.ph_day"),
            Self::Hour => t!("multi_rename.ph_hour"),
            Self::Minute => t!("multi_rename.ph_minute"),
            Self::Second => t!("multi_rename.ph_second"),
            Self::Upper => t!("multi_rename.ph_upper"),
            Self::Lower => t!("multi_rename.ph_lower"),
            Self::FirstUpper => t!("multi_rename.ph_first_upper"),
            Self::CaseAsIs => t!("multi_rename.ph_case_as_is"),
            Self::OpenBracket => t!("multi_rename.ph_open_bracket"),
            Self::CloseBracket => t!("multi_rename.ph_close_bracket"),
        }
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}  {}", self.label(), self.meaning())
    }
}

/// Which mask a placeholder goes into: the one typed in last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MaskTarget {
    #[default]
    Name,
    Extension,
}

/// The fields of the rule, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub name_mask: String,
    pub extension_mask: String,
    pub find: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub first_only: bool,
    pub with_extension: bool,
    pub regex: bool,
    pub substitute: bool,
    pub case: CaseChoice,
    pub counter_start: String,
    pub counter_step: String,
    pub counter_width: String,
    pub target: MaskTarget,
}

impl Default for Form {
    /// Total Commander's `<Default>`: `[N]`, `[E]`, nothing replaced, the counter from 1.
    fn default() -> Self {
        Self::from_rule(&RenameRule::default())
    }
}

impl Form {
    /// Every field as `rule` has it: what loading a preset does.
    pub fn from_rule(rule: &RenameRule) -> Self {
        let search = rule.search.clone().unwrap_or_default();
        Self {
            name_mask: rule.name_mask.clone(),
            extension_mask: rule.extension_mask.clone(),
            find: search.find,
            replace: search.replace,
            case_sensitive: search.case_sensitive,
            first_only: search.first_only,
            with_extension: search.include_extension,
            regex: search.regex,
            substitute: search.substitute,
            case: CaseChoice(rule.case),
            counter_start: rule.counter.start.to_string(),
            counter_step: rule.counter.step.to_string(),
            counter_width: rule.counter.width.to_string(),
            target: MaskTarget::Name,
        }
    }

    /// The rule the fields make, or the counter field that does not read as a number. The
    /// switches of the search are kept with an empty search too, so a preset restores them.
    pub fn rule(&self) -> Result<RenameRule, Field> {
        let start = self
            .counter_start
            .trim()
            .parse::<i64>()
            .map_err(|_| Field::CounterStart)?;
        let step = self
            .counter_step
            .trim()
            .parse::<i64>()
            .map_err(|_| Field::CounterStep)?;
        let width = self
            .counter_width
            .trim()
            .parse::<u32>()
            .map_err(|_| Field::CounterWidth)?;
        Ok(RenameRule {
            name_mask: self.name_mask.clone(),
            extension_mask: self.extension_mask.clone(),
            case: self.case.0,
            search: Some(SearchReplace {
                find: self.find.clone(),
                replace: self.replace.clone(),
                regex: self.regex,
                case_sensitive: self.case_sensitive,
                first_only: self.first_only,
                include_extension: self.with_extension,
                substitute: self.substitute,
            }),
            counter: CounterSettings { start, step, width },
        })
    }

    /// The text of one of the rule's fields; `None` for the preset name and the log path,
    /// which are the dialog's.
    pub fn text_mut(&mut self, field: Field) -> Option<&mut String> {
        Some(match field {
            Field::NameMask => {
                self.target = MaskTarget::Name;
                &mut self.name_mask
            }
            Field::ExtensionMask => {
                self.target = MaskTarget::Extension;
                &mut self.extension_mask
            }
            Field::Find => &mut self.find,
            Field::Replace => &mut self.replace,
            Field::CounterStart => &mut self.counter_start,
            Field::CounterStep => &mut self.counter_step,
            Field::CounterWidth => &mut self.counter_width,
            Field::PresetName | Field::LogPath => return None,
        })
    }

    pub fn flag_mut(&mut self, flag: Flag) -> &mut bool {
        match flag {
            Flag::CaseSensitive => &mut self.case_sensitive,
            Flag::FirstOnly => &mut self.first_only,
            Flag::WithExtension => &mut self.with_extension,
            Flag::RegEx => &mut self.regex,
            Flag::Substitute => &mut self.substitute,
        }
    }

    /// Appends `placeholder` to the mask typed in last. iced's text field does not tell where
    /// its caret is, so the placeholder goes to the end, not to the caret as in Total Commander.
    pub fn insert(&mut self, placeholder: Placeholder) {
        let mask = match self.target {
            MaskTarget::Name => &mut self.name_mask,
            MaskTarget::Extension => &mut self.extension_mask,
        };
        mask.push_str(placeholder.text());
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
