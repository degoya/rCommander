//! The languages the viewer reads, named by the file's extension - never guessed from its
//! content - and what sets each apart from the shared lexer: its words, and the few rules that
//! differ (directives, nested comments, strings over lines, templates, annotations).

mod tables;

use tables::{
    C_KEYWORDS, C_LITERALS, C_TYPES, CPP_KEYWORDS, CSHARP_KEYWORDS, CSHARP_TYPES, GO_KEYWORDS,
    GO_LITERALS, GO_TYPES, JAVA_KEYWORDS, JAVA_TYPES, KOTLIN_KEYWORDS, KOTLIN_TYPES, NULL_LITERALS,
    RUST_KEYWORDS, RUST_LITERALS, RUST_TYPES,
};

use crate::types::TokenClass;

/// A sorted table of words.
type Table = &'static [&'static [u8]];

/// A language of the C family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    C,
    Cpp,
    CSharp,
    Java,
    Kotlin,
    Go,
    Rust,
}

/// What a word declares: the name after it is coloured as what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Declares {
    /// `fn`, `func`, `fun`: a function.
    Function,
    /// `class`, `struct`, `enum`, `union`, `interface`, `trait`, `record`, `object`, `type`,
    /// `typealias`: a type.
    Type,
}

impl Lang {
    /// The language of a file named `name`, by its extension in any case (`manifest.toml`'s
    /// `[match] extensions`): `.h` is read as C++, which holds C's words too.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let (_, extension) = name.rsplit_once('.')?;
        Some(match extension.to_ascii_lowercase().as_str() {
            "c" => Self::C,
            "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "ino" => Self::Cpp,
            "cs" | "csx" => Self::CSharp,
            "java" => Self::Java,
            "kt" | "kts" => Self::Kotlin,
            "go" => Self::Go,
            "rs" => Self::Rust,
            _ => return None,
        })
    }

    /// `#` heading a line starts a directive: C and C++'s preprocessor, C#'s `#region`, `#if`.
    pub(crate) fn directives(self) -> bool {
        matches!(self, Self::C | Self::Cpp | Self::CSharp)
    }

    /// A backslash ending a line joins the next to it: a directive, a `//` comment and a string
    /// go on there (C and C++).
    pub(crate) fn splices(self) -> bool {
        matches!(self, Self::C | Self::Cpp)
    }

    /// `/* /* */ */` nests (Rust, Kotlin).
    pub(crate) fn nested_comments(self) -> bool {
        matches!(self, Self::Rust | Self::Kotlin)
    }

    /// A `"..."` string goes on over lines without a backslash (Rust).
    pub(crate) fn multiline_quoted(self) -> bool {
        self == Self::Rust
    }

    /// `"""` opens a string over lines (C#, Java, Kotlin).
    pub(crate) fn triple_quotes(self) -> bool {
        matches!(self, Self::CSharp | Self::Java | Self::Kotlin)
    }

    /// `@name` is an annotation (Java, Kotlin).
    pub(crate) fn annotations(self) -> bool {
        matches!(self, Self::Java | Self::Kotlin)
    }

    /// `'` between the digits of a number separates them: `1'000'000` (C++14, C23).
    pub(crate) fn digit_quotes(self) -> bool {
        matches!(self, Self::C | Self::Cpp)
    }

    /// `$` is a byte of a name: in Java, in GNU C, in Rust's macro variables. In C# it opens an
    /// interpolated string, in Kotlin's strings a template.
    pub(crate) fn dollar_in_names(self) -> bool {
        !matches!(self, Self::CSharp | Self::Kotlin)
    }

    /// The class of `word` alone: a keyword, a constant, a built-in type, or none.
    pub(crate) fn classify(self, word: &[u8]) -> Option<TokenClass> {
        let (keywords, types, literals) = self.tables();
        let has = |table: Table| table.binary_search(&word).is_ok();
        if has(keywords) {
            Some(TokenClass::Keyword)
        } else if has(literals) {
            Some(TokenClass::Literal)
        } else if has(types) {
            Some(TokenClass::TypeName)
        } else {
            None
        }
    }

    /// What the keyword `word` declares, if anything.
    pub(crate) fn declares(self, word: &[u8]) -> Option<Declares> {
        match word {
            b"fn" | b"func" | b"fun" => Some(Declares::Function),
            b"class" | b"struct" | b"enum" | b"union" | b"interface" | b"trait" | b"record"
            | b"object" | b"typealias" => Some(Declares::Type),
            b"type" if matches!(self, Self::Go | Self::Rust) => Some(Declares::Type),
            _ => None,
        }
    }

    /// The keywords, types and constants of the language.
    fn tables(self) -> (Table, Table, Table) {
        match self {
            Self::C => (C_KEYWORDS, C_TYPES, C_LITERALS),
            Self::Cpp => (CPP_KEYWORDS, C_TYPES, C_LITERALS),
            Self::CSharp => (CSHARP_KEYWORDS, CSHARP_TYPES, NULL_LITERALS),
            Self::Java => (JAVA_KEYWORDS, JAVA_TYPES, NULL_LITERALS),
            Self::Kotlin => (KOTLIN_KEYWORDS, KOTLIN_TYPES, NULL_LITERALS),
            Self::Go => (GO_KEYWORDS, GO_TYPES, GO_LITERALS),
            Self::Rust => (RUST_KEYWORDS, RUST_TYPES, RUST_LITERALS),
        }
    }
}

#[cfg(test)]
#[path = "lang_tests.rs"]
mod tests;
