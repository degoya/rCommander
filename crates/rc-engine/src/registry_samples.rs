//! One sample of every [`Command`] variant, for the registry's tests: the first commands here,
//! the later ones in `registry_samples_more.rs` (one file holds at most 500 lines).

use rc_ops::{Answer, ConflictChoice, DeleteMode, ErrorPolicy, JobOptions};

use crate::{
    Algorithm, Command, ConnectionDraft, ConnectionId, ConnectionPatch, CursorStep, CursorTarget,
    FolderId, HostKeyAnswer, ListMode, LogLevel, PanelId, REDACTED, SecretParam, SortKey, TermId,
    TermSize,
    compare::FileCompareRequest,
    sync::DirCompareRequest,
    viewer::{ReadHexRequest, ReadTextRequest, TextEncoding},
};

/// One command of every variant: a new variant gets a sample here or in
/// `registry_samples_more.rs`, which holds its id against the table.
pub(super) fn one_of_each() -> Vec<Command> {
    vec![
        Command::AnswerHostKey {
            question: 1,
            answer: HostKeyAnswer::ConnectOnce,
        },
        Command::ImportKnownHosts { path: None },
        Command::ListHostKeys {
            filter: Some("box".into()),
        },
        Command::RemoveHostKey { id: 1 },
        Command::ReplaceHostKey { question: 1 },
        Command::OpsAnswer {
            job: 1,
            answer: Answer::Conflict {
                choice: ConflictChoice::Rename,
                for_all: true,
            },
        },
        Command::OpsCancel { job: 1 },
        Command::MultiRenameApply(crate::MultiRenameRequest {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into(), "b.txt".into()],
            ..crate::MultiRenameRequest::default()
        }),
        Command::MultiRenamePreview(crate::MultiRenameRequest {
            dir: Some("photos".into()),
            names: vec!["IMG_1.jpg".into()],
            rule: crate::RenameRule {
                name_mask: "[P]_[C]".into(),
                ..crate::RenameRule::default()
            },
            ..crate::MultiRenameRequest::default()
        }),
        Command::MultiRenameUndo,
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec!["a".into()],
            target: None,
            options: JobOptions {
                on_conflict: Some(ConflictChoice::Skip),
                ..JobOptions::default()
            },
        }),
        Command::OpsDelete(crate::DeleteRequest {
            panel: Some(PanelId::Right),
            sources: Vec::new(),
            mode: DeleteMode::Permanent,
            on_error: ErrorPolicy::Skip,
            queue: None,
        }),
        Command::OpsList { job: None },
        Command::OpsMkdir {
            panel: None,
            path: "new".into(),
        },
        Command::OpsMove(crate::TransferRequest {
            panel: None,
            sources: Vec::new(),
            target: Some("/x".into()),
            options: JobOptions::default(),
        }),
        Command::OpsPause { job: 2 },
        Command::OpsResume { job: 2 },
        Command::GetPanelState { panel: None },
        Command::ListDir {
            panel: None,
            path: Some("/".into()),
        },
        Command::Navigate {
            panel: Some(PanelId::Left),
            path: "..".into(),
        },
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Index(0),
        },
        Command::SwitchActivePanel { panel: None },
        Command::ToggleSelection {
            panel: None,
            names: vec!["a".into()],
        },
        Command::CloseTerminal {
            terminal: TermId(1),
        },
        Command::ListTerminals {},
        Command::OpenTerminal {
            panel: Some(PanelId::Right),
            shell: Some("pwsh".into()),
            distro: None,
            cwd: Some("..".into()),
            size: Some(TermSize {
                rows: 30,
                cols: 100,
            }),
        },
        Command::ReadScreen {
            terminal: TermId(1),
            scrollback: 50,
        },
        Command::SendInput {
            terminal: TermId(1),
            text: "dir\r".into(),
        },
        Command::ListWslDistros,
        Command::RefreshWslDistros,
        Command::GetVaultStatus,
        Command::LockVault,
        Command::ListConnections,
        Command::GetConnection {
            id: ConnectionId(1),
        },
        Command::CreateConnection {
            folder_id: Some(FolderId(2)),
            connection: ConnectionDraft::new("web", "example.org"),
        },
        Command::UpdateConnection {
            id: ConnectionId(1),
            patch: ConnectionPatch {
                port: Some(2222),
                ..ConnectionPatch::default()
            },
        },
        Command::MoveConnection(crate::MoveConnectionRequest {
            id: ConnectionId(1),
            folder_id: None,
            sort_order: Some(3),
        }),
        Command::DeleteConnection {
            id: ConnectionId(1),
        },
        Command::DuplicateConnection {
            id: ConnectionId(1),
        },
        Command::ImportSshConfig(crate::SshConfigImportRequest {
            path: Some("/home/me/.ssh/config".into()),
            dry_run: true,
            hosts: Some(vec!["web".into()]),
            folder_id: Some(FolderId(2)),
        }),
        Command::CreateFolder {
            parent_id: None,
            name: "prod".into(),
        },
        Command::RenameFolder {
            id: FolderId(2),
            name: "staging".into(),
        },
        Command::MoveFolder(crate::MoveFolderRequest {
            id: FolderId(2),
            parent_id: Some(FolderId(3)),
            sort_order: None,
        }),
        Command::SearchFiles(crate::SearchRequest {
            panel: None,
            paths: Vec::new(),
            query: crate::SearchQuery::default(),
            limit: None,
            cursor: None,
        }),
        Command::DeleteFolder {
            id: FolderId(2),
            recursive: true,
        },
        Command::BackupDatabase,
        Command::BackupDatabaseTo(crate::BackupToRequest {
            path: "D:\\backups".into(),
            overwrite: false,
        }),
        Command::RestoreDatabase {
            path: "D:\\backups\\rcommander-20261017-120000.db".into(),
        },
        Command::ErrorLog(crate::ErrorLogRequest {
            level: Some(LogLevel::Error),
            contains: Some("denied".into()),
            limit: Some(10),
        }),
        Command::ClearErrorLog,
        Command::GetRetention,
        Command::SetRetention(crate::RetentionRequest {
            table: "error_log".into(),
            max_rows: Some(1_000),
            max_age_days: None,
        }),
        Command::MoveCursor {
            panel: None,
            step: CursorStep::PageDown,
        },
        Command::OpenEntry {
            panel: Some(PanelId::Right),
        },
        Command::ToggleCursorSelection { panel: None },
        Command::ToggleTerminal { panel: None },
        Command::Refresh { panel: None },
        Command::SelectAll { panel: None },
        Command::DeselectAll { panel: None },
        Command::InvertSelection { panel: None },
        Command::ListAgentIdentities,
        // A secret parameter serializes as the placeholder, so only the placeholder round-trips.
        Command::ImportKey(crate::ImportKeyRequest {
            connection_id: ConnectionId(1),
            path: "C:\\keys\\id_ed25519".into(),
            passphrase: Some(SecretParam::from(REDACTED)),
        }),
        Command::CreateVault {
            password: SecretParam::from(REDACTED),
        },
        Command::UnlockVault {
            password: SecretParam::from(REDACTED),
        },
        Command::ChangeMasterPassword {
            current: SecretParam::from(REDACTED),
            new: SecretParam::from(REDACTED),
        },
        Command::RemoveVault,
        Command::CompareFiles(FileCompareRequest {
            left: Some("a.txt".into()),
            unified_limit: 4096,
            ..FileCompareRequest::default()
        }),
        Command::CompareDirs(DirCompareRequest::default()),
        Command::ComputeChecksums(crate::ChecksumRequest {
            panel: None,
            paths: vec!["a.iso".into()],
            algorithm: Algorithm::Blake3,
        }),
        Command::CreateChecksumFiles(crate::ChecksumFilesRequest {
            panel: Some(PanelId::Left),
            dir: Some("sub".into()),
            names: vec!["a.iso".into()],
            algorithm: Algorithm::Sha256,
            per_file: true,
            file_name: None,
        }),
        Command::VerifyChecksumFile {
            panel: None,
            path: "a.iso.sha256".into(),
        },
        Command::SplitFile(crate::SplitRequest {
            panel: None,
            path: "a.iso".into(),
            part_size: 4_294_967_295,
            target_dir: Some("/media/usb".into()),
        }),
        Command::CombineFiles(crate::CombineRequest {
            panel: None,
            path: "a.001".into(),
            target_dir: None,
            target_name: Some("a.iso".into()),
        }),
        Command::ReadHex(ReadHexRequest {
            path: "a.bin".into(),
            panel: None,
            offset: 16,
            length: Some(64),
        }),
        Command::ReadText(ReadTextRequest {
            path: "a.txt".into(),
            panel: Some(PanelId::Left),
            offset: 0,
            max_bytes: None,
            encoding: Some(TextEncoding::Utf16Le),
        }),
        Command::OpenFile {
            panel: None,
            path: Some("a.txt".into()),
        },
        Command::Sort {
            panel: Some(PanelId::Left),
            key: SortKey::Size,
            descending: None,
        },
        Command::SetListMode {
            panel: None,
            mode: ListMode::Brief,
        },
        Command::ToggleHidden { show: Some(true) },
        Command::SelectByMask(crate::MaskRequest {
            panel: None,
            mask: Some("*.txt|a*".into()),
        }),
        Command::DeselectByMask(crate::MaskRequest {
            panel: Some(PanelId::Right),
            mask: None,
        }),
        Command::SelectExtension {
            panel: None,
            select: true,
        },
        Command::RestoreSelection { panel: None },
        Command::SelectRange {
            panel: None,
            step: CursorStep::Right,
        },
        Command::ListDrives { refresh: true },
        Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: "C:".into(),
        },
        Command::DriveMenu { panel: None },
        Command::MapNetworkDrive {
            remote: r"\\nas\media".into(),
            letter: "Z:".into(),
            reconnect: true,
        },
        Command::UnmapNetworkDrive {
            drive: "Z:".into(),
            force: false,
        },
        Command::GetSettings { key: None },
        Command::ListKeymap,
        Command::SetKeymap {
            overrides: "F5= Ctrl+K=copy".into(),
        },
        Command::SetSetting {
            key: "appearance.theme".into(),
            value: serde_json::json!("dark"),
        },
        Command::ShowFiles {
            panel: Some(PanelId::Left),
        },
        Command::ShowTerminal {
            terminal: TermId(1),
        },
    ]
    .into_iter()
    .chain(super::samples_more::more())
    .chain(crate::tabs::one_of_each())
    .collect()
}
