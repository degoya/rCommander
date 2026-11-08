//! The icons on the dialogs' buttons (RC-080-07) and on a button bar's buttons that run
//! rCommander's commands (RC-100-87): a subset of Fluent UI System Icons (MIT,
//! `NOTICE`), compiled in as the font `resources/icons/rcommander-icons.ttf` and handed to iced
//! when the window opens. An icon is a glyph of text, so it takes the colour of the button it
//! sits in - the theme's in light and dark, dimmed with a disabled button - and stays sharp at
//! every size and scaling; a font of 9 KB instead of iced's `svg` feature and its SVG renderer.
//!
//! `scripts/icon-font.sh` writes the font; the tests hold its glyphs and [`Icon`] to each other.
//! [`super::layout::icon_button`] puts an icon before a label.

use iced::Font;

/// The font, loaded with the window (`app::startup`).
pub const FONT_BYTES: &[u8] = include_bytes!("../../resources/icons/rcommander-icons.ttf");
/// The font's family; the settings' font lists leave it out.
pub const FAMILY: &str = "rCommander Icons";
/// The icons' font.
pub const FONT: Font = Font::with_name(FAMILY);
/// An icon's size beside a label at [`super::TEXT_SIZE`]: WinUI's 16 for a button's glyph.
pub const SIZE: f32 = 16.0;

/// An action a button carries; its glyph in the comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// New (`add`).
    Add,
    /// OK, Yes, Allow (`checkmark`).
    Confirm,
    /// Copy (`copy`).
    Copy,
    /// Delete, Remove (`delete`).
    Delete,
    /// Close, Cancel (`dismiss`).
    Close,
    /// New folder (`folder_add`).
    NewFolder,
    /// Browse, Open a file (`folder_open`).
    Open,
    /// Import (`arrow_import`).
    Import,
    /// Into the queue (`list`).
    Queue,
    /// Move (`arrow_move`).
    Move,
    /// Pause (`pause`).
    Pause,
    /// Connect (`plug_connected`).
    Connect,
    /// Disconnect (`plug_disconnected`).
    Disconnect,
    /// Resume, Run, Start (`play`).
    Resume,
    /// Rename (`rename`).
    Rename,
    /// Save (`save`).
    Save,
    /// Search (`search`).
    Search,
    /// Stop (`stop`).
    Stop,
    /// Terminal (`window_console`).
    Terminal,
    /// Test, a test run (`beaker`).
    Test,
    /// Move up (`arrow_up`): a login script's step (RC-050-13).
    MoveUp,
    /// Move down (`arrow_down`).
    MoveDown,
    /// Indent: into the folder above (`text_indent_increase_ltr`, RC-100-63).
    Indent,
    /// Outdent: out of its folder (`text_indent_decrease_ltr`).
    Outdent,
    /// The drive bar's mark (`star`).
    Star,
    /// A button bar's Reread (`arrow_clockwise`, RC-100-87).
    Refresh,
    /// Brief view (`text_column_three`).
    BriefView,
    /// Full view (`text_bullet_list_ltr`).
    FullView,
    /// Invert selection (`select_all_off`).
    InvertSelection,
    /// Pack (`archive`).
    Pack,
    /// Unpack (`archive_arrow_back`).
    Unpack,
    /// Find files (`document_search`).
    FindFiles,
    /// Quick View (`eye`).
    QuickView,
    /// Synchronize folders (`folder_sync`).
    Synchronize,
    /// View a file (`document_text`).
    ViewFile,
    /// Edit a file (`document_edit`).
    EditFile,
    /// Compare by content (`branch_compare`).
    Compare,
    /// Swap the panels (`arrow_swap`).
    Swap,
    /// Parent folder (`folder_arrow_up`).
    ParentFolder,
    /// Folder history (`history`).
    History,
    /// Quit (`power`).
    Quit,
    /// Select all (`checkbox_checked`).
    SelectAll,
}

impl Icon {
    /// Every icon, in the order of `scripts/icon-font.sh` (the font test holds the two together).
    #[cfg(test)]
    pub const ALL: [Self; 42] = [
        Self::Add,
        Self::Confirm,
        Self::Copy,
        Self::Delete,
        Self::Close,
        Self::NewFolder,
        Self::Open,
        Self::Import,
        Self::Queue,
        Self::Move,
        Self::Pause,
        Self::Connect,
        Self::Disconnect,
        Self::Resume,
        Self::Rename,
        Self::Save,
        Self::Search,
        Self::Stop,
        Self::Terminal,
        Self::Test,
        Self::MoveUp,
        Self::MoveDown,
        Self::Indent,
        Self::Outdent,
        Self::Star,
        Self::Refresh,
        Self::BriefView,
        Self::FullView,
        Self::InvertSelection,
        Self::Pack,
        Self::Unpack,
        Self::FindFiles,
        Self::QuickView,
        Self::Synchronize,
        Self::ViewFile,
        Self::EditFile,
        Self::Compare,
        Self::Swap,
        Self::ParentFolder,
        Self::History,
        Self::Quit,
        Self::SelectAll,
    ];

    /// The icon's character in [`FONT`]: the upstream code point the script prints.
    pub const fn glyph(self) -> char {
        match self {
            Self::Add => '\u{F109}',
            Self::Confirm => '\u{F294}',
            Self::Copy => '\u{F32B}',
            Self::Delete => '\u{F34C}',
            Self::Close => '\u{F369}',
            Self::NewFolder => '\u{F41C}',
            Self::Open => '\u{F42E}',
            Self::Import => '\u{F159}',
            Self::Queue => '\u{F4ED}',
            Self::Move => '\u{F8E5}',
            Self::Pause => '\u{F5A1}',
            Self::Connect => '\u{E999}',
            Self::Disconnect => '\u{F609}',
            Self::Resume => '\u{F605}',
            Self::Rename => '\u{F669}',
            Self::Save => '\u{F67F}',
            Self::Search => '\u{F68F}',
            Self::Stop => '\u{F72A}',
            Self::Terminal => '\u{EE6F}',
            Self::Test => '\u{F1D7}',
            Self::MoveUp => '\u{F19B}',
            Self::MoveDown => '\u{F148}',
            Self::Indent => '\u{ED2D}',
            Self::Outdent => '\u{ED23}',
            Self::Star => '\u{F70F}',
            Self::Refresh => '\u{F13D}',
            Self::BriefView => '\u{F7C3}',
            Self::FullView => '\u{ECD9}',
            Self::InvertSelection => '\u{EA85}',
            Self::Pack => '\u{F139}',
            Self::Unpack => '\u{E068}',
            Self::FindFiles => '\u{F3AE}',
            Self::QuickView => '\u{E5F2}',
            Self::Synchronize => '\u{E664}',
            Self::ViewFile => '\u{E557}',
            Self::EditFile => '\u{F386}',
            Self::Compare => '\u{E1DB}',
            Self::Swap => '\u{F18D}',
            Self::ParentFolder => '\u{E651}',
            Self::History => '\u{F47E}',
            Self::Quit => '\u{F60E}',
            Self::SelectAll => '\u{F28D}',
        }
    }
}

#[cfg(test)]
#[path = "icon_tests.rs"]
mod tests;
