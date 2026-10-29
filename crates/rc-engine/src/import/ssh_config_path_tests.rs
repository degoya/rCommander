use std::path::{Path, PathBuf};

use super::{config_home, context_for, plain_wsl};
use crate::{import::ssh_config::parse_str, secrets::local_key_path};

#[cfg(unix)]
#[test]
fn only_the_config_and_its_drop_in_directory_are_read() {
    let home = Some(Path::new("/home/me"));
    let allowed = |path: &str| config_home(Path::new(path), home);
    assert_eq!(
        allowed("/home/me/.ssh/config"),
        Some(PathBuf::from("/home/me"))
    );
    assert_eq!(
        allowed("/home/me/.ssh/config.d/work"),
        Some(PathBuf::from("/home/me"))
    );
    assert_eq!(
        allowed("/home/me/.ssh/config.d/a/b"),
        Some(PathBuf::from("/home/me"))
    );

    for refused in [
        "/home/me/.ssh/config.d",
        "/home/me/.ssh/id_ed25519",
        "/home/me/.ssh/known_hosts",
        "/home/me/.ssh/other",
        "/home/me/Documents/config",
        "/home/other/.ssh/config",
        "/etc/passwd",
        "/home/me/.ssh/config.d/../id_rsa",
        "/home/me/.ssh/config/../../../../etc/passwd",
        ".ssh/config",
    ] {
        assert_eq!(allowed(refused), None, "{refused}");
    }
    assert_eq!(
        config_home(Path::new("/home/me/.ssh/config"), None),
        None,
        "no home, no file"
    );
}

#[cfg(windows)]
#[test]
fn a_wsl_distributions_home_is_read_on_its_share() {
    let home = Some(Path::new(r"C:\Users\Me"));
    let allowed = |path: &str| config_home(Path::new(path), home);
    assert_eq!(
        allowed(r"\\wsl.localhost\Ubuntu\home\alice\.ssh\config"),
        Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\home\alice"))
    );
    assert_eq!(
        allowed(r"\\?\UNC\wsl$\Debian\root\.ssh\config.d\work"),
        Some(PathBuf::from(r"\\?\UNC\wsl$\Debian\root"))
    );
    assert_eq!(
        allowed(r"C:\Users\Me\.ssh\config"),
        Some(PathBuf::from(r"C:\Users\Me"))
    );
    for refused in [
        r"\\wsl.localhost\Ubuntu\etc\ssh\ssh_config",
        r"\\wsl.localhost\Ubuntu\home\alice\.ssh\id_rsa",
        r"\\fileserver\share\home\alice\.ssh\config",
        r"C:\Users\Me\Documents\config",
    ] {
        assert_eq!(allowed(refused), None, "{refused}");
    }
}

#[test]
fn the_context_follows_the_home_not_the_local_user() {
    let own = context_for(Path::new("/home/me"), Some(Path::new("/home/me")), "Me");
    assert_eq!(own.ssh_dir, PathBuf::from("/home/me/.ssh"));
    assert_eq!(own.local_user, "Me");
    let distro = context_for(
        Path::new("/mnt/wsl/home/alice"),
        Some(Path::new("/home/me")),
        "Me",
    );
    assert_eq!(distro.home, PathBuf::from("/mnt/wsl/home/alice"));
    assert_eq!(distro.local_user, "alice");
}

#[test]
fn a_wsl_home_loses_its_verbatim_prefix_so_its_keys_can_be_taken() {
    let home = plain_wsl(Path::new(r"\\?\UNC\wsl$\debian\home\aherl"));
    assert_eq!(home, PathBuf::from(r"\\wsl$\debian\home\aherl"));
    assert_eq!(
        plain_wsl(Path::new(r"\\?\UNC\wsl.localhost\Ubuntu\root")),
        PathBuf::from(r"\\wsl.localhost\Ubuntu\root")
    );
    let context = context_for(&home, Some(Path::new(r"C:\Users\aherl")), "aherl");
    let found = parse_str(
        "Host web\n  IdentityFile ~/.ssh/id_ed25519\n",
        &context.user_config(),
        &context,
    )
    .expect("parsed");
    let key = found.hosts[0].identity_files[0].display().to_string();
    assert_eq!(key, r"\\wsl$\debian\home\aherl/.ssh/id_ed25519");
    assert!(local_key_path(&key), "{key}");
    assert!(!local_key_path(
        r"\\?\UNC\wsl$\debian\home\aherl\.ssh\id_ed25519"
    ));

    // Any other share, and a name a plain path would read differently, keep their prefix.
    for kept in [
        r"\\?\UNC\fileserver\share\home\aherl",
        r"\\?\UNC\wsl$\debian\home\aherl.",
        r"\\?\UNC\wsl$\debian\home\aherl \",
        r"\\wsl.localhost\Ubuntu\home\alice",
        "/home/me",
    ] {
        assert_eq!(plain_wsl(Path::new(kept)), PathBuf::from(kept), "{kept}");
    }
    assert!(!local_key_path(
        r"\\?\UNC\fileserver\share\home\aherl\.ssh\id_rsa"
    ));
}
