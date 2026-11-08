//! The markup formats the viewer reads, named by the file's extension - never guessed from its
//! content.

/// A markup format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// reStructuredText, `.rst`.
    Rst,
    /// LaTeX documents and packages, `.tex` and `.sty`.
    Latex,
    /// BibTeX databases, `.bib`.
    Bibtex,
    /// To-do lists in TaskPaper's and PlainTasks' plain text, `.todo` and `.taskpaper`
    /// (RC-110-07).
    Todo,
}

impl Lang {
    /// The format of a file named `name`, by its extension in any case (`manifest.toml`'s
    /// `[match] extensions`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let (_, extension) = name.rsplit_once('.')?;
        Some(match extension.to_ascii_lowercase().as_str() {
            "rst" => Self::Rst,
            "tex" | "sty" => Self::Latex,
            "bib" => Self::Bibtex,
            "todo" | "taskpaper" => Self::Todo,
            _ => return None,
        })
    }
}

#[cfg(test)]
#[path = "lang_tests.rs"]
mod tests;
