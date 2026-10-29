use secrecy::{ExposeSecret, SecretString};

use super::{ANONYMOUS_PASSWORD, Credentials, Encryption, FtpConfig};

fn with_password() -> FtpConfig {
    FtpConfig::new(
        "ftp.example",
        Encryption::Explicit,
        Credentials::User {
            name: "alice".to_owned(),
            password: SecretString::from("hunter2-secret"),
        },
    )
}

#[test]
fn the_password_is_not_in_debug_output() {
    let config = with_password();
    let printed = format!("{config:?} {:?}", config.credentials);
    assert!(!printed.contains("hunter2-secret"), "{printed}");
    assert!(printed.contains("alice"), "{printed}");
}

#[test]
fn the_port_follows_the_encryption() {
    let implicit = FtpConfig::new("h", Encryption::Implicit, Credentials::Anonymous);
    assert_eq!(implicit.port, 990);
    let explicit = FtpConfig::new("h", Encryption::Explicit, Credentials::Anonymous);
    assert_eq!(explicit.port, 21);
}

#[test]
fn anonymous_sends_a_placeholder_address() {
    // A plausible address would publish someone's data to every anonymous server.
    assert!(ANONYMOUS_PASSWORD.ends_with(".invalid"));
    let config = FtpConfig::new("h", Encryption::None, Credentials::Anonymous);
    let (user, password) = config.login();
    assert_eq!(user, "anonymous");
    assert_eq!(password.expose_secret(), ANONYMOUS_PASSWORD);
}
