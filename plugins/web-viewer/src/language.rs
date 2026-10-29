//! The language a document is read in: by the file's extension, else by its first bytes.

use crate::lexer::state::{CssDialect, CssFrame, Dialect, End, Frame, HtmlSub, JsFrame};

/// The language at the bottom of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// HTML, XHTML and Vue's single-file components: HTML with `<script>` and `<style>`.
    Html,
    /// HTML with `<?php ... ?>` anywhere.
    Php,
    Css,
    Scss,
    Less,
    Js,
    Jsx,
    Ts,
    Tsx,
}

impl Language {
    /// The language of a file with the extension `extension`, in any case.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        let language = match extension.to_ascii_lowercase().as_str() {
            "html" | "htm" | "xhtml" | "vue" => Self::Html,
            "php" | "phtml" => Self::Php,
            "css" => Self::Css,
            "scss" => Self::Scss,
            "less" => Self::Less,
            "js" | "mjs" | "cjs" => Self::Js,
            "jsx" => Self::Jsx,
            "ts" | "mts" | "cts" => Self::Ts,
            "tsx" => Self::Tsx,
            _ => return None,
        };
        Some(language)
    }

    /// The language of the file named `name` whose text starts with `head`: by the name's
    /// extension; for a name without a known one, PHP when an opening tag is in `head`, HTML
    /// when it starts with `<`, else JavaScript.
    #[must_use]
    pub fn of(name: &str, head: &[u8]) -> Self {
        if let Some(language) = name
            .rsplit_once('.')
            .and_then(|(_, extension)| Self::from_extension(extension))
        {
            return language;
        }
        let opens = head
            .windows(5)
            .any(|window| window.eq_ignore_ascii_case(b"<?php"))
            || head.windows(3).any(|window| window == b"<?=");
        if opens {
            return Self::Php;
        }
        if head.trim_ascii_start().starts_with(b"<") {
            Self::Html
        } else {
            Self::Js
        }
    }

    /// Whether `<?php` opens PHP in the document.
    pub(crate) fn php(self) -> bool {
        self == Self::Php
    }

    /// The frame at the bottom of the document's stack.
    pub(crate) fn root(self) -> Frame {
        let css = |dialect| Frame::Css(CssFrame::new(dialect, End::Never, 0));
        let js = |dialect| Frame::Js(JsFrame::new(dialect, End::Never));
        match self {
            Self::Html | Self::Php => Frame::Html(HtmlSub::Text),
            Self::Css => css(CssDialect::Css),
            Self::Scss => css(CssDialect::Scss),
            Self::Less => css(CssDialect::Less),
            Self::Js => js(Dialect::Js),
            Self::Jsx => js(Dialect::Jsx),
            Self::Ts => js(Dialect::Ts),
            Self::Tsx => js(Dialect::Tsx),
        }
    }
}
