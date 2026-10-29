//! The internal viewer's switches (RC-020-09): a Markdown file opens as a formatted preview, or
//! as text as before. On by default; not protected - an agent may switch it with `settings_set`.
//! How many pixels the image view shows of a picture before it scales it down (owner,
//! 2026-10-19); not protected either: it only trades sharpness for memory. Which files open as
//! DOS text in code page 437, unwrapped (RC-020-11): `.nfo` and `.diz` by default. Whether
//! Ctrl+wheel, Ctrl++, Ctrl+- and Ctrl+0 change the text size, and whether a changed size stays
//! with its window or is stored as `viewer.font_size` (RC-020-12); the font size itself is held
//! to 6 to 72 points, the range those keys reach. Whether DOS text draws its lines as tall as the
//! font's full block, so the blocks of DOS art touch (owner, 2026-10-20).

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Whether Lister and Quick View show a `.md` or `.markdown` file as a formatted preview.
pub const MARKDOWN_PREVIEW_KEY: &str = "viewer.markdown_preview";
/// The most pixels, in millions, the image view shows of a picture; a larger one is scaled down.
pub const PICTURE_MEGAPIXELS_KEY: &str = "viewer.picture_megapixels";
/// The extensions, without dots and separated by spaces, of the files Lister and Quick View
/// open in CP437 (DOS) and without wrapping.
pub const DOS_EXTENSIONS_KEY: &str = "viewer.dos_extensions";
/// What `viewer.dos_extensions` holds until it is changed: release notes and their descriptions.
pub const DOS_EXTENSIONS_DEFAULT: &str = "nfo diz";
/// Whether DOS text draws its lines as tall as the viewer font's full block (U+2588), so the
/// blocks of DOS art touch; off, its lines are as tall as any text's.
pub const DOS_TIGHT_LINES_KEY: &str = "viewer.dos_tight_lines";
/// Whether Ctrl+wheel, Ctrl++, Ctrl+- and Ctrl+0 change the text size in Lister and Quick View.
pub const TEXT_ZOOM_KEY: &str = "viewer.text_zoom";
/// What a changed text size applies to: `window` (that viewer window and the files it steps
/// to) or `setting` (stored as `viewer.font_size`, so every viewer opens at it).
pub const TEXT_ZOOM_KEEP_KEY: &str = "viewer.text_zoom_keep";
/// The smallest and the largest text size of the viewer in points, of the setting and the keys.
pub const VIEWER_MIN_FONT_SIZE: i64 = 6;
pub const VIEWER_MAX_FONT_SIZE: i64 = 72;

/// On the Appearance page beside the viewer's font: the dialog has no page of the viewer's own.
pub(super) const MARKDOWN_PREVIEW: SettingSpec = SettingSpec {
    key: MARKDOWN_PREVIEW_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.markdown_preview",
    help: "setting.viewer.markdown_preview.help",
    protected: false,
    restart: false,
};

pub(super) const PICTURE_MEGAPIXELS: SettingSpec = SettingSpec {
    key: PICTURE_MEGAPIXELS_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 4,
        max: 256,
        default: 32,
    },
    label: "setting.viewer.picture_megapixels",
    help: "setting.viewer.picture_megapixels.help",
    protected: false,
    restart: false,
};

pub(super) const DOS_EXTENSIONS: SettingSpec = SettingSpec {
    key: DOS_EXTENSIONS_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Extensions {
        default: DOS_EXTENSIONS_DEFAULT,
    },
    label: "setting.viewer.dos_extensions",
    help: "setting.viewer.dos_extensions.help",
    protected: false,
    restart: false,
};

pub(super) const DOS_TIGHT_LINES: SettingSpec = SettingSpec {
    key: DOS_TIGHT_LINES_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.dos_tight_lines",
    help: "setting.viewer.dos_tight_lines.help",
    protected: false,
    restart: false,
};

pub(super) const TEXT_ZOOM: SettingSpec = SettingSpec {
    key: TEXT_ZOOM_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.text_zoom",
    help: "setting.viewer.text_zoom.help",
    protected: false,
    restart: false,
};

const TEXT_ZOOM_KEEPS: &[Choice] = &[
    Choice {
        value: "window",
        label: "setting.viewer.text_zoom_keep.window",
    },
    Choice {
        value: "setting",
        label: "setting.viewer.text_zoom_keep.setting",
    },
];

pub(super) const TEXT_ZOOM_KEEP: SettingSpec = SettingSpec {
    key: TEXT_ZOOM_KEEP_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Choice {
        choices: TEXT_ZOOM_KEEPS,
        default: "window",
    },
    label: "setting.viewer.text_zoom_keep",
    help: "setting.viewer.text_zoom_keep.help",
    protected: false,
    restart: false,
};
