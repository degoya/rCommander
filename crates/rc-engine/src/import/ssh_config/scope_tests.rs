use std::{fs, io, path::Path};

use rc_vfs::BackendId;

use super::{MAX_FILE_BYTES, McpScope, read_text};
use crate::read_guard::{ReadGuard, SSH_PRIVATE_FILE};

/// The MCP guard's verdicts as far as an import meets them: anything in `.ssh` private, a
/// credential file refused for itself.
fn judge(path: &Path) -> Option<&'static str> {
    if path.file_name().is_some_and(|name| name == ".netrc") {
        return Some("a file of credentials");
    }
    path.components()
        .any(|component| component.as_os_str() == ".ssh")
        .then_some(SSH_PRIVATE_FILE)
}

/// RC-080-12 (X4): under MCP an include stays inside `.ssh`, as it really is, and passes the
/// read guard but for the guard's own verdict on `.ssh`.
#[test]
fn an_mcp_include_stays_inside_ssh_and_passes_the_guard() {
    let home = tempfile::tempdir().expect("dir");
    let ssh = home.path().join(".ssh");
    fs::create_dir_all(ssh.join("config.d")).expect("dir");
    let inside = ssh.join("config.d").join("work");
    let outside = home.path().join("work.conf");
    let netrc = ssh.join(".netrc");
    for file in [&inside, &outside, &netrc] {
        fs::write(file, "Host a\n").expect("write");
    }
    let canonical = |path: &Path| fs::canonicalize(path).expect("canonical");
    let scope = McpScope::new(&ssh, Some(ReadGuard::new(judge, &BackendId::Local)));
    assert!(scope.admits(&canonical(&inside)));
    assert!(!scope.admits(&canonical(&outside)), "outside .ssh");
    assert!(!scope.admits(&canonical(&ssh)), "the folder itself");
    assert!(!scope.admits(&canonical(&netrc)), "refused for itself");
    #[cfg(unix)]
    {
        let link = ssh.join("linked");
        std::os::unix::fs::symlink(&outside, &link).expect("link");
        assert!(!scope.admits(&canonical(&link)), "a link out of .ssh");
    }
    // Without a judge attached only the folder rule holds.
    let unjudged = McpScope::new(&ssh, None);
    assert!(unjudged.admits(&canonical(&netrc)));
    assert!(!unjudged.admits(&canonical(&outside)));
}

/// RC-080-12 (X4): no configuration file is read past 1 MiB, and no folder is read as one.
#[test]
fn a_file_over_the_cap_or_no_file_is_not_read() {
    let dir = tempfile::tempdir().expect("dir");
    let fits = dir.path().join("fits");
    let large = dir.path().join("large");
    let size = usize::try_from(MAX_FILE_BYTES).expect("fits a usize");
    fs::write(&fits, "#".repeat(size)).expect("write");
    fs::write(&large, "#".repeat(size + 1)).expect("write");
    assert_eq!(read_text(&fits).expect("read").len(), size);
    let error = read_text(&large).expect_err("too large");
    assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    let error = read_text(dir.path()).expect_err("a folder");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}
