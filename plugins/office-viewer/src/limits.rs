//! The bounds every package is read within, and the failure codes of `locales/en.json` that say
//! which one a document broke.

/// What one package may cost. [`Limits::DEFAULT`] is what the plugin uses; the tests shrink it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Entries in the zip, checked on its end record before anything else is read.
    pub max_entries: u64,
    /// Bytes one part inflates to: declared, and counted while it inflates (one byte past it is
    /// the most ever inflated).
    pub max_part_bytes: u64,
    /// Inflated bytes per packed byte, as `rc-vfs`'s `ArchiveLimits::max_ratio`.
    pub max_ratio: u64,
    /// The ratio is checked once a part has inflated more than this. `ArchiveLimits`' own
    /// threshold (256 MiB) lies above [`Limits::max_part_bytes`] and would never apply.
    pub ratio_floor: u64,
    /// Elements open at once in one XML part.
    pub max_depth: usize,
    /// Bytes of `text` output; the host's `plugins.viewer_output_limit_mib` cuts at its own.
    pub max_text_bytes: u64,
}

impl Limits {
    /// The limits of RC-100-99: 10 000 entries, 64 MiB a part, ratio 500 past 1 MiB, 256
    /// elements deep, 256 MiB of text.
    pub const DEFAULT: Self = Self {
        max_entries: 10_000,
        max_part_bytes: 64 << 20,
        max_ratio: 500,
        ratio_floor: 1 << 20,
        max_depth: 256,
        max_text_bytes: 256 << 20,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The failure codes of `locales/en.json`, under the plugin's id; each comes with `bad-data`.
pub mod code {
    /// The zip has more entries than [`super::Limits::max_entries`].
    pub const TOO_MANY_ENTRIES: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.too_many_entries";
    /// A part inflates past [`super::Limits::max_part_bytes`].
    pub const TOO_LARGE: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.too_large";
    /// A part inflates past [`super::Limits::max_ratio`] times its packed size: a zip bomb.
    pub const TOO_COMPRESSED: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.too_compressed";
    /// A part nests its XML deeper than [`super::Limits::max_depth`].
    pub const TOO_DEEP: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.too_deep";
    /// Not a readable zip, or a part that is encrypted, packed in another way than Deflate, or
    /// not well-formed XML.
    pub const DAMAGED: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.damaged";
    /// A zip without a Word document, workbook or presentation in it.
    pub const NOT_OFFICE: &str = "01a1b720-fdc2-7718-851e-092f28c86d2f.not_office";
}
