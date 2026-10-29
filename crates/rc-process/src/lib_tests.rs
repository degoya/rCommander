use super::NoConsoleWindow;

#[test]
fn a_std_command_still_runs() {
    let status = std::process::Command::new("cargo")
        .arg("--version")
        .no_console_window()
        .stdout(std::process::Stdio::null())
        .status()
        .expect("cargo runs");
    assert!(status.success());
}

#[tokio::test]
async fn a_tokio_command_still_runs() {
    let status = tokio::process::Command::new("cargo")
        .arg("--version")
        .no_console_window()
        .stdout(std::process::Stdio::null())
        .status()
        .await
        .expect("cargo runs");
    assert!(status.success());
}

#[test]
fn a_detached_command_still_runs() {
    use super::Detached;
    let status = std::process::Command::new("cargo")
        .arg("--version")
        .no_console_window()
        .detached(false)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("cargo runs");
    assert!(status.success());
}
