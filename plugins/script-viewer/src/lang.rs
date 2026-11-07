//! The languages the viewer reads, named by the file's name - its extension, or `Gemfile` and
//! `Rakefile` whole - never guessed from its content, and the words of each.

mod tables;

use tables::{
    LUA_FUNCTIONS, LUA_KEYWORDS, LUA_LITERALS, PERL_FUNCTIONS, PERL_KEYWORDS, PERL_LITERALS,
    RUBY_FUNCTIONS, RUBY_KEYWORDS, RUBY_LITERALS, VBS_FUNCTIONS, VBS_KEYWORDS, VBS_LITERALS,
};

use crate::types::TokenClass;

/// A sorted table of words.
type Table = &'static [&'static [u8]];

/// A scripting language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ruby,
    Perl,
    Lua,
    VbScript,
}

impl Lang {
    /// The language of a file named `name`, in any case (`manifest.toml`'s `[match]`): `Gemfile`
    /// and `Rakefile` are Ruby by their name, every other file by its extension.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        if name == "gemfile" || name == "rakefile" {
            return Some(Self::Ruby);
        }
        let (_, extension) = name.rsplit_once('.')?;
        Some(match extension {
            "rb" => Self::Ruby,
            "pl" | "pm" => Self::Perl,
            "lua" => Self::Lua,
            "vbs" => Self::VbScript,
            _ => return None,
        })
    }

    /// The class of `word` alone: a keyword, a constant, a function every script knows, or
    /// none. VBScript ignores case: its words are compared lowercased.
    pub(crate) fn classify(self, word: &[u8]) -> Option<TokenClass> {
        let lower;
        let word = if self == Self::VbScript {
            lower = word.to_ascii_lowercase();
            lower.as_slice()
        } else {
            word
        };
        let (keywords, literals, functions) = self.tables();
        let has = |table: Table| table.binary_search(&word).is_ok();
        if has(keywords) {
            Some(TokenClass::Keyword)
        } else if has(literals) {
            Some(TokenClass::Literal)
        } else if has(functions) {
            Some(TokenClass::Function)
        } else {
            None
        }
    }

    /// The keywords, constants and well-known functions of the language.
    fn tables(self) -> (Table, Table, Table) {
        match self {
            Self::Ruby => (RUBY_KEYWORDS, RUBY_LITERALS, RUBY_FUNCTIONS),
            Self::Perl => (PERL_KEYWORDS, PERL_LITERALS, PERL_FUNCTIONS),
            Self::Lua => (LUA_KEYWORDS, LUA_LITERALS, LUA_FUNCTIONS),
            Self::VbScript => (VBS_KEYWORDS, VBS_LITERALS, VBS_FUNCTIONS),
        }
    }
}

#[cfg(test)]
#[path = "lang_tests.rs"]
mod tests;
