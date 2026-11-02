//! Real servers in this process for copies between servers (RC-060-06): SFTP on the SSH test
//! server of `rc-remote`, FTP on its FTP test server. An SFTP server serves a directory below the
//! build's own temporary directory (`CARGO_TARGET_TMPDIR`), never the system's, so a test can
//! watch the system's temporary directory for files a copy should not make; the FTP test server
//! keeps its files where it likes.

use std::{path::PathBuf, sync::Arc, time::Duration};

use rc_remote::{
    FtpVfs, SftpVfs, SshSession,
    ftp::{Credentials, Encryption, FtpConfig, test_server},
    sftp::SftpLimits,
    ssh::{SessionOptions, SshTarget},
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, SftpServe, TestServer, host_key_a,
    },
};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

/// A directory below the build's temporary directory.
pub fn build_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory to serve")
}

/// An SSH server with SFTP, and one session to it.
pub struct Sftp {
    pub server: TestServer,
    pub served: tempfile::TempDir,
    _session: SshSession,
    pub vfs: Arc<SftpVfs>,
}

impl Sftp {
    pub async fn start() -> Self {
        Self::start_with(SftpLimits::default()).await
    }

    /// A server whose backend waits at most what `limits` say (RC-080-04: a server that goes
    /// silent).
    pub async fn start_with(limits: SftpLimits) -> Self {
        let served = build_tempdir();
        let serve = SftpServe::new(served.path());
        Self::serving(served, serve, limits).await
    }

    /// A server that folds case on any disk, as OpenSSH on NTFS or APFS does (RC-080-60); its
    /// home `/home` is where the backend's probe finds that out.
    pub async fn folding() -> Self {
        let served = build_tempdir();
        std::fs::create_dir(served.path().join("home")).expect("home");
        let mut serve = SftpServe::new(served.path());
        serve.home = "/home".to_owned();
        serve.fold_case = true;
        Self::serving(served, serve, SftpLimits::default()).await
    }

    async fn serving(served: tempfile::TempDir, serve: SftpServe, limits: SftpLimits) -> Self {
        let server = TestServer::start(ServerOptions {
            sftp: Some(serve),
            ..ServerOptions::new(host_key_a())
        })
        .await;
        let target = SshTarget {
            host: "127.0.0.1".to_owned(),
            port: server.port,
            user: "tester".to_owned(),
        };
        let credentials = MemoryCredentials {
            password: Some(PASSWORD.to_owned()),
            ..MemoryCredentials::default()
        };
        let session = SshSession::connect(
            &target,
            &SessionOptions::default(),
            Arc::new(MemoryGate::trusting(&host_key_a())),
            &credentials,
            &CancellationToken::new(),
        )
        .await
        .expect("session");
        let names = session.open_channel().await.expect("channel");
        let files = session.open_channel().await.expect("channel");
        let vfs = SftpVfs::connect(names, files, limits).await.expect("sftp");
        Self {
            server,
            served,
            _session: session,
            vfs: Arc::new(vfs),
        }
    }

    /// The file the server keeps for its path `path`.
    pub fn on_disk(&self, path: &str) -> PathBuf {
        self.served.path().join(path.trim_start_matches('/'))
    }
}

/// An FTP server without TLS, and a backend logged in to it.
pub struct Ftp {
    pub server: test_server::TestServer,
    pub vfs: FtpVfs,
}

impl Ftp {
    pub async fn start() -> Self {
        Self::start_with(test_server::Options::default(), Duration::from_secs(10)).await
    }

    /// A server with `options` - `stall` for one that stops in the middle of a file - and a
    /// backend that waits at most `timeout` (RC-080-04).
    pub async fn start_with(options: test_server::Options, timeout: Duration) -> Self {
        let server = test_server::TestServer::start(options).await;
        let mut config = FtpConfig::new(
            "127.0.0.1",
            Encryption::None,
            Credentials::User {
                name: "tester".to_owned(),
                password: SecretString::from(test_server::PASSWORD),
            },
        );
        config.port = server.port;
        config.timeout = timeout;
        let vfs = FtpVfs::connect(config).await.expect("ftp");
        Self { server, vfs }
    }

    /// The file the server keeps for its path `path`.
    pub fn on_disk(&self, path: &str) -> PathBuf {
        self.server.local(path)
    }
}
