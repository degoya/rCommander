use super::{SecretField, SourceState};
use crate::WIT;

#[test]
fn the_enums_are_named_as_the_contract_names_them() {
    let states = [
        SourceState::Ready,
        SourceState::Locked,
        SourceState::Unreachable,
        SourceState::Unlicensed,
    ];
    let names: Vec<&str> = states.iter().map(|state| state.as_str()).collect();
    assert!(WIT.contains(&format!("enum source-state {{ {} }}", names.join(", "))));
    let fields = [
        SecretField::Password,
        SecretField::PrivateKey,
        SecretField::Passphrase,
        SecretField::Totp,
    ];
    let names: Vec<&str> = fields.iter().map(|field| field.as_str()).collect();
    assert!(WIT.contains(&format!("enum secret-field {{ {} }}", names.join(", "))));
}

/// A lease's value never shows in its `Debug`, which a log line or a panic may carry.
#[test]
fn a_lease_hides_its_value() {
    let lease = super::Lease {
        id: "lease-7".to_owned(),
        value: "canary-lent-4410".to_owned(),
        expires: Some(1_700_000_000),
    };
    let shown = format!("{lease:?}");
    assert!(!shown.contains("canary-lent-4410"), "{shown}");
    assert!(
        shown.contains("lease-7") && shown.contains("[redacted]"),
        "{shown}"
    );
}

/// A password manager lends nothing: the defaults answer `not-supported`, so only a PAM
/// system's plugin implements `checkout` and `checkin` (RC-100-38).
#[test]
fn a_source_lends_nothing_unless_it_says_so() {
    struct Manager;
    impl super::Secrets for Manager {
        fn status(&mut self, _host: &mut dyn super::SecretsHost) -> SourceState {
            SourceState::Ready
        }
        fn search(
            &mut self,
            _host: &mut dyn super::SecretsHost,
            _query: &str,
            _limit: u32,
        ) -> Result<Vec<super::SecretEntry>, crate::Failure> {
            Ok(Vec::new())
        }
        fn get(
            &mut self,
            _host: &mut dyn super::SecretsHost,
            _id: &str,
            _field: SecretField,
        ) -> Result<String, crate::Failure> {
            Ok("value".to_owned())
        }
    }
    struct Host;
    impl super::SecretsHost for Host {
        fn log(&mut self, _level: &str, _message: &str) {}
        fn setting(&mut self, _name: &str) -> Option<String> {
            None
        }
        fn http(
            &mut self,
            _method: &str,
            _url: &str,
            _headers: &[(String, String)],
            _body: &[u8],
        ) -> Result<super::HttpResponse, crate::Failure> {
            Err(crate::Failure::new(crate::FailureKind::Io))
        }
    }
    let lent =
        super::Secrets::checkout(&mut Manager, &mut Host, "7c17", SecretField::Password, None);
    assert_eq!(
        lent.expect_err("not lent").kind,
        crate::FailureKind::NotSupported
    );
    let back = super::Secrets::checkin(&mut Manager, &mut Host, "lease-7");
    assert_eq!(
        back.expect_err("nothing to give back").kind,
        crate::FailureKind::NotSupported
    );
    assert!(
        WIT.contains("checkout: func(id: string, field: secret-field, reason: option<string>)")
    );
    assert!(
        super::REASON_REQUIRED.contains('.'),
        "a plugin code holds a dot"
    );
}
