//! The WSL distributions an ssh_config import can read from: a button per distribution fills
//! the path with its config.

use std::path::PathBuf;

use rc_engine::{Drive, DriveGroup};

/// The WSL distributions of the drive bar, by name and share. The name is the drive's label,
/// else read from the share: a share's root (`\\wsl.localhost\<distro>\`) has no file name on
/// Windows, so the button would have no caption.
pub fn wsl_roots(drives: &[Drive]) -> Vec<(String, PathBuf)> {
    drives
        .iter()
        .filter(|drive| drive.group == DriveGroup::Wsl)
        .filter_map(|drive| {
            let root = drive.root.as_ref()?.path.clone();
            let name = if drive.label.is_empty() {
                rc_vfs::parse_wsl_unc(&root).map(|wsl| wsl.distro)?
            } else {
                drive.label.clone()
            };
            Some((name, root))
        })
        .collect()
}

/// A distribution's config for the user of the same name as on Windows, which is WSL's
/// default; the person corrects the name when theirs differs.
pub(super) fn wsl_config(root: &std::path::Path) -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
        .to_lowercase();
    root.join("home")
        .join(user)
        .join(".ssh")
        .join("config")
        .display()
        .to_string()
}
