use super::types::{Failure, FailureKind, Http, HttpResponse, Protocol};
use super::{AUTHORIZATION, HOSTS, list};

/// A server that answers `/hosts.tsv` with `status` and `body`, and remembers the headers it got.
struct Server {
    status: u16,
    body: &'static str,
    headers: Vec<(String, String)>,
}

impl Server {
    fn answering(status: u16, body: &'static str) -> Self {
        Self {
            status,
            body,
            headers: Vec::new(),
        }
    }
}

impl Http for Server {
    fn get(&mut self, path: &str, headers: &[(String, String)]) -> Result<HttpResponse, Failure> {
        if path != HOSTS {
            return Err(Failure::of(FailureKind::Io));
        }
        self.headers = headers.to_vec();
        Ok(HttpResponse {
            status: self.status,
            body: self.body.as_bytes().to_vec(),
        })
    }
}

#[test]
fn hosts_are_read_one_per_line() {
    let mut server = Server::answering(
        200,
        "17\tweb-01\tweb-01.example.net\t22\tssh\tdeploy\tdc1/web\tprod,web\n\
         18\tfiles\t10.0.0.8\t2222\tsftp\t\t\t\n\
         not a host\n",
    );
    let hosts = list(&mut server).expect("hosts");
    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0].external_id, "17");
    assert_eq!(hosts[0].protocol, Protocol::Ssh);
    assert_eq!(hosts[0].tags, ["prod", "web"]);
    assert_eq!(hosts[1].user, None);
    assert_eq!(hosts[1].folder, "");
}

/// The token goes as a marker, never as a value: the plugin has none.
#[test]
fn the_token_is_a_marker() {
    let mut server = Server::answering(200, "");
    list(&mut server).expect("no hosts");
    assert_eq!(
        server.headers,
        [(AUTHORIZATION.0.to_owned(), AUTHORIZATION.1.to_owned())]
    );
    assert!(AUTHORIZATION.1.contains("{{secret:api_token}}"));
}

#[test]
fn a_refused_token_is_access_denied() {
    let failure = list(&mut Server::answering(403, "")).expect_err("refused");
    assert_eq!(failure.kind, FailureKind::AccessDenied);
}
