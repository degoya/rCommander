use super::local_key_path;

#[test]
fn a_drive_an_absolute_path_and_a_wsl_share_are_local() {
    for path in [
        r"C:\Users\ada\.ssh\id_ed25519",
        "c:/Users/ada/.ssh/id_rsa",
        "/home/ada/.ssh/id_ed25519",
        r"\keys\id_rsa",
        r"\\wsl.localhost\Ubuntu\home\ada\.ssh\id_ed25519",
        r"\\WSL.LOCALHOST\Ubuntu\home\ada/.ssh/id_rsa",
        r"\\wsl$\Debian\root\.ssh\id_rsa",
        "//wsl.localhost/Ubuntu/home/ada/.ssh/id_rsa",
        "/opt/keys/../id_rsa",
    ] {
        assert!(local_key_path(path), "{path}");
    }
}

#[test]
fn a_share_a_device_path_a_url_and_a_relative_path_are_refused() {
    for path in [
        r"\\nas\keys\id_rsa",
        "//nas/keys/id_rsa",
        r"\/nas\keys\id_rsa",
        r"\\nas.example.org@SSL\DavWWWRoot\id_rsa",
        r"\\?\C:\keys\id_rsa",
        r"\\?\UNC\nas\keys\id_rsa",
        r"\\.\pipe\keys",
        r"\??\UNC\nas\keys\id_rsa",
        r"\\wsl.localhost\Ubuntu",
        r"\\wsl.localhost\Ubuntu\",
        r"\\wsl.localhost\\home\id_rsa",
        r"\\wsl.localhost\Ubuntu\..\..\nas\keys\id_rsa",
        r"\\wsl.localhost.evil.org\Ubuntu\home\id_rsa",
        "sftp://nas/keys/id_rsa",
        "file://nas/keys/id_rsa",
        "id_rsa",
        ".ssh/id_rsa",
        "~/.ssh/id_rsa",
        "~ada/.ssh/id_rsa",
        "C:id_rsa",
        "1:/keys/id_rsa",
        "/home/ada/.ssh/id\nrsa",
        "",
    ] {
        assert!(!local_key_path(path), "{path:?}");
    }
}
