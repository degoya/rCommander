//! The app without a window, for the tests: a message goes through [`App::apply`], its jobs run
//! on the engine one after the other, and what the engine announces comes back as messages until
//! nothing is left - the loop the window's subscriptions run. The constructors below are the
//! engines and harnesses the tests share (RC-080-24), the folders they run on and the viewer's
//! fake document source (RC-080-32).

use std::{collections::VecDeque, fs, path::Path};

use rc_db::Db;
use rc_engine::{Chord, Engine, EngineEvent, Keymap, PanelId, VfsPath};
use tempfile::TempDir;
use tokio::sync::broadcast::{self, error::TryRecvError};

use super::{
    App, Effect, Job, Message,
    database_dialog::DatabaseMsg,
    properties::PropsMsg,
    run_job,
    viewer::document::{Answer, DocumentSource, Token, TreeNode},
};

/// A temporary folder holding `entries`, in order: a name ending in `/` is a folder, any other
/// a file with the text beside it; missing parents are created. The same as `rc-engine`'s
/// `test_support::tree`, which is the engine crate's own.
pub fn tree(entries: &[(&str, &str)]) -> TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    for (name, text) in entries {
        let path = root.path().join(name);
        if name.ends_with('/') {
            fs::create_dir_all(&path).expect("mkdir");
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&path, text).expect("write");
    }
    root
}

/// A document source that answers nothing - no tokens, no children, no text; the tests answer
/// for it.
#[derive(Debug)]
pub struct FakeSource;

impl DocumentSource for FakeSource {
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>> {
        Box::pin(async move { Ok(vec![Vec::new(); starts.len()]) })
    }

    fn children(&self, _node: Option<u64>) -> Answer<Vec<TreeNode>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

pub struct Harness {
    pub app: App,
    events: broadcast::Receiver<EngineEvent>,
    /// The panels whose terminal the app asked to show; no shell starts in a test.
    pub terminals_shown: Vec<PanelId>,
    /// Each Tab (`false`) and Shift+Tab (`true`) that walked a dialog's focus ring; the walk
    /// itself needs a window.
    pub fields_walked: Vec<bool>,
}

impl Harness {
    pub fn new(engine: Engine) -> Self {
        let events = engine.subscribe();
        Self {
            app: App::new(engine, Keymap::standard()),
            events,
            terminals_shown: Vec::new(),
            fields_walked: Vec::new(),
        }
    }

    /// A harness on a [`headless_engine`] on `root`.
    pub async fn headless(root: &Path) -> Self {
        Self::new(headless_engine(root).await)
    }

    /// [`Harness::headless`] with `db` attached to the engine first.
    pub async fn on_db(root: &Path, db: Db) -> Self {
        let engine = headless_engine(root).await;
        assert!(engine.attach_db(db));
        Self::new(engine)
    }

    /// [`Harness::headless`] with an in-memory database.
    pub async fn with_db(root: &Path) -> Self {
        Self::on_db(root, Db::open_in_memory().await.expect("db")).await
    }

    /// A harness whose engine shows `left` and `right`.
    pub async fn open(left: &Path, right: &Path) -> Self {
        Self::new(open_engine(left, right).await)
    }

    /// [`Harness::open`] with `db` attached to the engine first.
    pub async fn open_on_db(left: &Path, right: &Path, db: Db) -> Self {
        let engine = open_engine(left, right).await;
        assert!(engine.attach_db(db));
        Self::new(engine)
    }

    /// [`Harness::open`] with an in-memory database.
    pub async fn open_with_db(left: &Path, right: &Path) -> Self {
        Self::open_on_db(left, right, Db::open_in_memory().await.expect("db")).await
    }

    /// A temporary folder and a [`Harness::headless`] on it.
    pub async fn temp() -> (TempDir, Self) {
        let root = tempfile::tempdir().expect("temp dir");
        let harness = Self::headless(root.path()).await;
        (root, harness)
    }

    /// A temporary folder and a [`Harness::with_db`] on it.
    pub async fn temp_with_db() -> (TempDir, Self) {
        let root = tempfile::tempdir().expect("temp dir");
        let harness = Self::with_db(root.path()).await;
        (root, harness)
    }

    /// `message` and everything that follows from it.
    pub async fn send(&mut self, message: Message) {
        // Boxed: a debug build lays each awaited `send` out side by side in a test's future,
        // which outgrew the test thread's 2 MiB stack.
        Box::pin(self.run(message)).await;
    }

    async fn run(&mut self, message: Message) {
        let mut queue = VecDeque::from([message]);
        while let Some(message) = queue.pop_front() {
            for effect in self.app.apply(message) {
                match effect {
                    Effect::Dispatch(panel, commands) => {
                        let job = Job { panel, commands };
                        queue.push_back(run_job(self.app.engine(), job).await);
                    }
                    Effect::ShowTerminal(panel) => self.terminals_shown.push(panel),
                    Effect::WalkFields { back, .. } => self.fields_walked.push(back),
                    Effect::Database(job) => {
                        let result = job.run(self.app.engine().clone()).await;
                        queue.push_back(Message::Database(DatabaseMsg::Finished(Box::new(result))));
                    }
                    Effect::HostKey(job) => {
                        let answer = job.run(self.app.engine().clone()).await;
                        queue.push_back(Message::HostKey(answer));
                    }
                    Effect::Ops(submit) => {
                        let done = submit.run(self.app.engine().clone()).await;
                        queue.push_back(Message::FileOps(super::file_ops::FileOpsMsg::Submitted(
                            Box::new(done),
                        )));
                    }
                    Effect::Properties(job) => {
                        let result = job.run(self.app.engine().clone()).await;
                        queue.push_back(Message::Properties(PropsMsg::Finished(Box::new(result))));
                    }
                    Effect::Connections(job) => {
                        let answer = job.run(self.app.engine().clone()).await;
                        queue.push_back(Message::Connections(answer));
                    }
                    Effect::CertTrust(job) => {
                        let answer = job.run(self.app.engine().clone()).await;
                        queue.push_back(Message::CertTrust(answer));
                    }
                    Effect::MultiRename(call) => {
                        let answer = call.run(self.app.engine().clone()).await;
                        queue.push_back(Message::MultiRename(answer));
                    }
                    Effect::QueueList(call) => {
                        let answer = call.run(self.app.engine().clone()).await;
                        queue.push_back(super::queue_window::answered(answer));
                    }
                    Effect::TrustStore(call) => {
                        queue.push_back(call.run(self.app.engine().clone()).await);
                    }
                    Effect::Plugins(call) => {
                        queue.push_back(call.run(self.app.engine().clone()).await);
                    }
                    Effect::FocusTerminal(_) | Effect::Task(_) => {}
                }
            }
            queue.extend(self.pending());
        }
    }

    /// A key chord as the keymap writes it, such as `Tab` or ``Ctrl+` ``.
    pub async fn press(&mut self, keys: &str) {
        let chord: Chord = keys.parse().expect("a chord");
        self.send(Message::Key(chord)).await;
    }

    /// What the engine announced from elsewhere - an MCP call - reaches the app.
    pub async fn settle(&mut self) {
        for message in self.pending() {
            self.send(message).await;
        }
    }

    /// Drops what the engine announced so far, as a window that fell behind would miss it.
    pub fn miss_events(&mut self) {
        self.pending();
    }

    fn pending(&mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => messages.push(Message::Engine(event)),
                Err(TryRecvError::Lagged(_)) => messages.push(Message::EngineLagged),
                Err(TryRecvError::Empty | TryRecvError::Closed) => return messages,
            }
        }
    }
}

/// A headless engine on `root`, both panels there.
pub async fn headless_engine(root: &Path) -> Engine {
    Engine::headless(VfsPath::local(root))
        .await
        .expect("engine")
}

/// An engine whose panels show `left` and `right`.
pub async fn open_engine(left: &Path, right: &Path) -> Engine {
    Engine::open(VfsPath::local(left), VfsPath::local(right))
        .await
        .expect("engine")
}

/// `connections.save` storing `password` for the connection `id`, as the form does (RC-080-50
/// removed `secrets.set`); `None` removes the stored one.
pub fn password_change(id: rc_engine::ConnectionId, password: Option<&str>) -> rc_engine::Command {
    let kind = rc_engine::SecretKind::Password;
    let change = match password {
        Some(value) => rc_engine::SecretChange::Set {
            kind,
            value: value.into(),
        },
        None => rc_engine::SecretChange::Clear { kind },
    };
    rc_engine::Command::SaveConnection(rc_engine::SaveConnectionRequest {
        fields: rc_engine::SaveFields::Update {
            id,
            patch: rc_engine::ConnectionPatch::default(),
        },
        secrets: vec![change],
        login_script: None,
        favourite: None,
        credential: None,
    })
}
