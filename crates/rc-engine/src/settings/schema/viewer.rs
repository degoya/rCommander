//! The internal viewer's switches (RC-020-09): a Markdown file opens as a formatted preview, or
//! as text as before. On by default; not protected - an agent may switch it with `settings_set`.
//! Whether the mouse, Ctrl+A and Ctrl+C select and copy in that preview (RC-020-14), on by
//! default and not protected either: it reads nothing the preview does not already show.
//! How many pixels the image view shows of a picture before it scales it down (owner,
//! 2026-10-19); not protected either: it only trades sharpness for memory. Which files open as
//! DOS text in code page 437, unwrapped (RC-020-11): `.nfo` and `.diz` by default. Whether
//! Ctrl+wheel, Ctrl++, Ctrl+- and Ctrl+0 change the text size, and whether a changed size stays
//! with its window or is stored as `viewer.font_size` (RC-020-12); the font size itself is held
//! to 6 to 72 points, the range those keys reach. Whether DOS text draws its lines as tall as the
//! font's full block, so the blocks of DOS art touch (owner, 2026-10-20). Whether the image view
//! offers a photo's EXIF as a second view (RC-100-99), on by default and not protected: it only
//! shows what the file holds. Whether certificates and keys open as their report and tree, and
//! how many days before its end a certificate warns (RC-110-08); neither is protected - the
//! report never shows private key material either way, and that is no setting. How long Quick
//! View waits for the cursor to rest on a server, a device or in an archive before it reads the
//! entry (RC-110-28); not protected: it only trades a wait for requests.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Whether Lister and Quick View show a `.md` or `.markdown` file as a formatted preview.
pub const MARKDOWN_PREVIEW_KEY: &str = "viewer.markdown_preview";
/// Whether the Markdown preview selects and copies its text.
pub const MARKDOWN_SELECT_KEY: &str = "viewer.markdown_select";
/// Whether the Markdown preview colours a fenced code block whose tag names a language it knows
/// (RC-110-06).
pub const MARKDOWN_HIGHLIGHT_KEY: &str = "viewer.markdown_code_highlight";
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
/// Whether the image view offers a JPEG's, PNG's or WebP's EXIF as a tree beside the picture
/// (`7` switches).
pub const EXIF_KEY: &str = "viewer.exif";
/// Whether Lister and Quick View show a certificate or key file as its checks and objects, as
/// text and as a tree (`7` switches); off, the file shows as it is (RC-110-08).
pub const CERTIFICATES_KEY: &str = "viewer.certificates";
/// The days before a certificate's end from which the certificate view warns; 0 never warns.
pub const CERT_EXPIRY_WARNING_DAYS_KEY: &str = "viewer.cert_expiry_warning_days";
/// Whether F3 with marked files lets N and P step through the marked ones only, as Total
/// Commander's Lister does (RC-100-91); off, they step through every file of the panel.
pub const STEP_THROUGH_SELECTION_KEY: &str = "viewer.step_through_selection";
/// The milliseconds the cursor rests on an entry that is not on a local disk before Quick View
/// lists or opens it; 0 reads at every step. Local folders are read at once either way.
pub const QUICK_VIEW_PAUSE_KEY: &str = "viewer.quick_view_pause_ms";
/// What `viewer.quick_view_pause_ms` holds until it is changed, and its upper end.
pub const QUICK_VIEW_PAUSE_DEFAULT_MS: i64 = 300;
pub const QUICK_VIEW_PAUSE_MAX_MS: i64 = 2000;

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

pub(super) const MARKDOWN_SELECT: SettingSpec = SettingSpec {
    key: MARKDOWN_SELECT_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.markdown_select",
    help: "setting.viewer.markdown_select.help",
    protected: false,
    restart: false,
};

pub(super) const MARKDOWN_HIGHLIGHT: SettingSpec = SettingSpec {
    key: MARKDOWN_HIGHLIGHT_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.markdown_code_highlight",
    help: "setting.viewer.markdown_code_highlight.help",
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

pub(super) const EXIF: SettingSpec = SettingSpec {
    key: EXIF_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.exif",
    help: "setting.viewer.exif.help",
    protected: false,
    restart: false,
};

/// Not protected: it only chooses which files N and P show.
pub(super) const STEP_THROUGH_SELECTION: SettingSpec = SettingSpec {
    key: STEP_THROUGH_SELECTION_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.step_through_selection",
    help: "setting.viewer.step_through_selection.help",
    protected: false,
    restart: false,
};

pub(super) const CERTIFICATES: SettingSpec = SettingSpec {
    key: CERTIFICATES_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.viewer.certificates",
    help: "setting.viewer.certificates.help",
    protected: false,
    restart: false,
};

pub(super) const CERT_EXPIRY_WARNING_DAYS: SettingSpec = SettingSpec {
    key: CERT_EXPIRY_WARNING_DAYS_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 0,
        max: 365,
        default: 30,
    },
    label: "setting.viewer.cert_expiry_warning_days",
    help: "setting.viewer.cert_expiry_warning_days.help",
    protected: false,
    restart: false,
};

pub(super) const QUICK_VIEW_PAUSE: SettingSpec = SettingSpec {
    key: QUICK_VIEW_PAUSE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 0,
        max: QUICK_VIEW_PAUSE_MAX_MS,
        default: QUICK_VIEW_PAUSE_DEFAULT_MS,
    },
    label: "setting.viewer.quick_view_pause_ms",
    help: "setting.viewer.quick_view_pause_ms.help",
    protected: false,
    restart: false,
};
