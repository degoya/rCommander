use super::{LocalForwardSpec, Parsed, parse};

fn args(listen: &str, target: &str) -> Vec<String> {
    vec![listen.to_owned(), target.to_owned()]
}

fn spec(bind: Option<&str>, port: u16, host: &str, host_port: u16) -> LocalForwardSpec {
    LocalForwardSpec {
        bind: bind.map(str::to_owned),
        port,
        host: host.to_owned(),
        host_port,
    }
}

#[test]
fn the_forms_of_openssh_are_read() {
    assert_eq!(
        parse(&args("15432", "db:5432")),
        Parsed::Forward(spec(None, 15432, "db", 5432))
    );
    assert_eq!(
        parse(&args("127.0.0.1:8080", "web/80")),
        Parsed::Forward(spec(Some("127.0.0.1"), 8080, "web", 80))
    );
    assert_eq!(
        parse(&args("[::1]:8443", "[fd00::5]:443")),
        Parsed::Forward(spec(Some("::1"), 8443, "fd00::5", 443))
    );
    assert_eq!(
        parse(&args("localhost:9000", "app:9000")),
        Parsed::Forward(spec(None, 9000, "app", 9000))
    );
}

#[test]
fn a_bind_other_than_loopback_is_lowered() {
    for listen in ["0.0.0.0:8080", "*:8080", "192.168.1.5:8080", "[::]:8080"] {
        assert_eq!(
            parse(&args(listen, "web:80")),
            Parsed::Lowered(spec(None, 8080, "web", 80)),
            "{listen}"
        );
    }
}

#[test]
fn sockets_and_bad_ports_are_not_forwards() {
    for (listen, target) in [
        ("/tmp/local.sock", "db:5432"),
        ("8080", "/var/run/docker.sock"),
        ("0", "db:5432"),
        ("8080", "db:70000"),
        ("8080", "db"),
    ] {
        assert_eq!(
            parse(&args(listen, target)),
            Parsed::Invalid,
            "{listen} {target}"
        );
    }
    assert_eq!(parse(&["8080".to_owned()]), Parsed::Invalid);
}
