//! A name a server listed, judged before it becomes an entry: a server's listing is hostile
//! input (RC-060-05, RC-070-03).

/// Whether a listed name is one entry of the listed directory: not empty, not `.` or `..`,
/// without a path separator (either kind: a Windows client splits at `\`) or a control character,
/// and not starting with a drive (`C:x`). A server that lists `../../x`, `a/b` or `C:x` is not
/// believed: such a name would lead a copy, a download or a delete out of the directory - on
/// Windows a folder joined with `C:x` is left for drive C.
pub(crate) fn safe_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..")
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
        && rc_vfs::drive_letter(name).is_none()
}
