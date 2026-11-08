//! Which engine events the connection manager takes (`connections.rs`).

use rc_engine::{EngineEvent, SHARED_CREDENTIALS_KEY};

/// Whether the manager takes `event`; the drive bar may take it as well.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::ConnectionManager
            | EngineEvent::ConnectionTree { .. }
            | EngineEvent::Drives
            | EngineEvent::VaultLocked
            | EngineEvent::VaultUnlocked
            | EngineEvent::VaultRemoved
            | EngineEvent::LoginScript { .. }
            | EngineEvent::SecretNeeded { .. }
            | EngineEvent::CheckoutReasonNeeded { .. }
            | EngineEvent::Session { .. }
            | EngineEvent::TrustedCertificatesChanged
            | EngineEvent::Forward { .. }
    ) || matches!(event, EngineEvent::SettingChanged { key } if key == SHARED_CREDENTIALS_KEY)
}
