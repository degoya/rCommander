//! The samples of `registry_samples.rs` that no longer fit there.

use rc_ops::{ApplyTo, JobOptions};
use rc_vfs::{LinkKind, PropertyChange};

use crate::{
    Command, ConnectionId, LoginScriptDraft, LoginScriptId, LoginScriptPatch, NameFormat, PanelId,
    REDACTED, SecretParam, Step,
};

/// The later commands' samples, in the order they came.
pub(super) fn more() -> Vec<Command> {
    vec![
        Command::ListLoginScripts,
        Command::GetLoginScript {
            id: LoginScriptId(1),
        },
        Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "sudo".into(),
                steps: vec![Step::SendLine],
                step_timeout_ms: None,
                total_timeout_ms: Some(30_000),
            },
        },
        Command::UpdateLoginScript {
            id: LoginScriptId(1),
            patch: LoginScriptPatch {
                name: Some("su".into()),
                ..LoginScriptPatch::default()
            },
        },
        Command::DeleteLoginScript {
            id: LoginScriptId(1),
        },
        Command::AssignLoginScript {
            connection_id: ConnectionId(2),
            script_id: Some(LoginScriptId(1)),
        },
        Command::SetLoginScriptSecret {
            script_id: LoginScriptId(1),
            label: "root".into(),
            value: SecretParam::from(REDACTED),
        },
        Command::ClearLoginScriptSecret {
            script_id: LoginScriptId(1),
            secret_id: 3,
        },
        Command::ConnectSession {
            id: ConnectionId(1),
        },
        Command::DisconnectSession {
            id: ConnectionId(1),
        },
        Command::TestConnection {
            id: ConnectionId(1),
        },
        Command::GetSessionPrefs {
            id: ConnectionId(1),
        },
        Command::SetSessionPrefs {
            id: ConnectionId(1),
            prefs: crate::SessionPrefs::default(),
        },
        Command::Stat {
            panel: None,
            path: "ftp://nas/pub".into(),
        },
        Command::QuickConnect(crate::QuickConnectRequest::default()),
        Command::AnswerCertificate {
            question: 1,
            answer: crate::CertificateAnswer::AcceptOnce,
        },
        Command::ListCertificates { filter: None },
        Command::RemoveCertificate { id: 1 },
        Command::DisablePlugin { id: "p".into() },
        Command::EnablePlugin { id: "p".into() },
        Command::InstallPlugin {
            path: "/tmp/csv.rcplug".into(),
            fingerprint: None,
            digest: "00".repeat(32),
        },
        Command::ListPlugins { id: None },
        Command::PreviewPlugin {
            path: "/tmp/csv.rcplug".into(),
        },
        Command::RemovePlugin {
            id: "p".into(),
            version: "1.0.0".into(),
        },
        Command::ResumePlugin { id: "p".into() },
        Command::RollbackPlugin { id: "p".into() },
        Command::SshExec(crate::SshExecRequest {
            connection_id: ConnectionId(1),
            command: "uptime".into(),
            stdin: None,
            timeout_secs: None,
            max_output_bytes: None,
        }),
        Command::OpenSshTerminal {
            connection_id: ConnectionId(3),
            panel: None,
            cwd: Some("/srv/my site".into()),
            size: None,
        },
        Command::AnswerMcpQuestion {
            question: 1,
            answer: crate::McpAnswer::AllowForSession,
        },
        Command::RevokeMcpGrants {},
        Command::FileDialog {
            panel: None,
            dialog: crate::FileDialog::CopyHere,
        },
        Command::OpsCopyTo {
            panel: Some(PanelId::Left),
            sources: vec!["a.txt".into()],
            target: "b.txt".into(),
            options: JobOptions::default(),
            queue: Some(2),
        },
        Command::OpsMoveTo {
            panel: None,
            sources: Vec::new(),
            target: "../out/".into(),
            options: JobOptions {
                verify: true,
                ..JobOptions::default()
            },
            queue: None,
        },
        Command::OpenExternal {
            panel: None,
            path: Some("new.txt".into()),
            program: crate::ExternalProgram::Editor,
            create: true,
        },
        Command::RunCommandLine {
            panel: None,
            line: "dir".into(),
            keep_open: true,
        },
        Command::Filter {
            panel: Some(PanelId::Left),
            pattern: Some("*.txt".into()),
            last: false,
        },
        Command::BranchView {
            panel: None,
            selection: true,
            show: None,
        },
        Command::FeedPaths {
            panel: Some(PanelId::Right),
            root: None,
            paths: vec!["a.txt".into()],
        },
        Command::FindFilesDialog { panel: None },
        Command::ViewFile {
            panel: None,
            path: Some("notes.txt".into()),
        },
        Command::ToggleQuickView { panel: None },
        Command::MaximizeTerminal {
            panel: Some(PanelId::Right),
        },
        Command::PasteImage { terminal: None },
        Command::ArchiveList {
            panel: Some(PanelId::Right),
            path: None,
            dir: Some("/docs".into()),
        },
        Command::ArchiveTest {
            panel: None,
            paths: vec!["backup.7z".into()],
        },
        Command::ArchiveExtract {
            panel: None,
            archives: Vec::new(),
            target: None,
            settings: rc_ops::ExtractSettings::default(),
            options: JobOptions::default(),
        },
        Command::ArchivePack {
            panel: Some(PanelId::Left),
            sources: vec!["docs".into()],
            target: Some("docs.7z".into()),
            settings: rc_ops::PackSettings::default(),
            options: JobOptions::default(),
        },
        Command::ArchivePassword {
            job: 1,
            password: None,
        },
        Command::OpenTerminalHere {
            panel: None,
            other: true,
        },
        Command::CopyNames {
            panel: None,
            format: NameFormat::FullDetails,
        },
        Command::CreateLink {
            panel: None,
            target: Some("a.txt".into()),
            link: Some("/b/a.lnk".into()),
            kind: Some(LinkKind::Junction),
        },
        Command::DirSize {
            panel: Some(PanelId::Left),
            paths: vec!["sub".into()],
        },
        Command::GetProperties {
            panel: None,
            paths: Vec::new(),
            count: true,
        },
        Command::SetFileAttributes {
            panel: None,
            paths: vec!["a".into()],
            change: PropertyChange {
                readonly: Some(true),
                mode: Some(0o640),
                ..PropertyChange::default()
            },
            recursive: true,
            apply_to: ApplyTo {
                files: true,
                dirs: false,
            },
        },
        Command::SetComment {
            panel: None,
            path: None,
            comment: Some("note".into()),
        },
        Command::CancelDirSizes { panel: None },
        Command::CountDirContent {
            panel: Some(PanelId::Right),
        },
        Command::SelectAndSize { panel: None },
        Command::ConnectionManager {},
        Command::SetFavourite {
            id: ConnectionId(1),
            favourite: true,
        },
        Command::ContextMenu { panel: None },
        Command::ChecksumDialog {
            panel: None,
            dialog: crate::ChecksumDialog::Create,
        },
        Command::QueueChecksumJob(crate::ChecksumJob::Verify {
            panel: Some(PanelId::Left),
            path: "a.sfv".into(),
        }),
        Command::MultiRenameOpenDialog { panel: None },
        Command::MultiRenamePresetList,
        Command::MultiRenamePresetSave {
            name: "Photos".into(),
            rule: crate::RenameRule {
                name_mask: "[Y]-[M]-[D]_[C:3]".into(),
                ..crate::RenameRule::default()
            },
        },
        Command::MultiRenamePresetDelete {
            name: "Photos".into(),
        },
        Command::MultiRenamePresetRename {
            name: "Photos".into(),
            new_name: "Pictures".into(),
        },
        Command::ImportTotalCommander(crate::TcImportRequest {
            wincmd_ini: Some("C:/Users/me/AppData/Roaming/GHISLER/wincmd.ini".into()),
            dry_run: true,
            only: Some(vec!["menu1".into(), "ftp:nas".into()]),
            ..crate::TcImportRequest::default()
        }),
        Command::ImportTotalCommanderPasswords(crate::TcPasswordsRequest {
            only: Some(vec!["ftp:nas".into()]),
            ..crate::TcPasswordsRequest::default()
        }),
        Command::ExportConfig(crate::ConfigExportRequest {
            path: "C:/Users/me/Documents/rcommander.rcexport".into(),
            parts: crate::exchange::ExportParts::ALL,
            password: Some(crate::SecretParam::from(crate::REDACTED)),
            secrets: true,
            plain_secrets: false,
            overwrite: false,
        }),
        Command::ImportConfig(crate::ConfigImportRequest {
            path: "C:/Users/me/Documents/rcommander.rcexport".into(),
            dry_run: true,
            only: Some(vec!["setting:appearance.theme".into()]),
            ..crate::ConfigImportRequest::default()
        }),
        Command::OpenDiff {},
        Command::SelectDifferences { by_content: true },
        Command::SyncApply(crate::sync::SyncApplyRequest::default()),
        Command::OpenSyncDialog {},
        Command::QueueClearFinished {},
        Command::QueueList {},
        Command::QueueMove {
            job: 3,
            step: rc_ops::Reorder::Front,
        },
        Command::QueuePauseAll {},
        Command::QueueRemove { job: 4 },
        Command::QueueResumeAll {},
        Command::QueueWindow {},
        Command::About {},
        Command::Quit {},
        Command::GetUpdateStatus {},
        Command::CheckForUpdates {},
        Command::DownloadUpdate {},
        Command::InstallUpdate {},
        Command::QueueAssign { job: 5, list: 1 },
        Command::QueueCreate {
            name: "Backup NAS".into(),
            jobs: vec![5, 6],
        },
        Command::QueueAssignMany {
            jobs: vec![5, 6],
            list: 1,
        },
        Command::QueueMoveMany {
            jobs: vec![5, 6],
            step: rc_ops::Reorder::Back,
        },
        Command::QueueDiscard { jobs: vec![5, 6] },
        Command::QueueClear {
            list: 1,
            all_lists: false,
        },
        Command::QueueDelete { list: 2 },
        Command::QueueDeleteSaved { saved: 3 },
        Command::QueueLists {},
        Command::QueueLoad { saved: 3 },
        Command::QueuePause { list: 1 },
        Command::QueueRename {
            list: 1,
            name: "Disk G".into(),
        },
        Command::QueueSave {
            list: 1,
            name: Some("Disk G".into()),
            restore_on_start: true,
        },
        Command::QueueStart { list: 1 },
        Command::CancelIo { panel: None },
        Command::ImportRemoteDesktopManager(crate::RdmImportRequest {
            path: "C:/Users/me/Documents/connections.rdm".into(),
            dry_run: true,
            only: Some(vec!["0f3c1a52-7d4e-4b8a-9c61-2e5b8d7a4f10".into()]),
        }),
        // A secret parameter serializes as the placeholder, so only the placeholder round-trips.
        Command::SaveConnection(crate::SaveConnectionRequest {
            fields: crate::SaveFields::Update {
                id: ConnectionId(1),
                patch: crate::ConnectionPatch {
                    notes: Some("prod".into()),
                    ..crate::ConnectionPatch::default()
                },
            },
            secrets: vec![
                crate::SecretChange::Clear {
                    kind: crate::SecretKind::Passphrase,
                },
                crate::SecretChange::Set {
                    kind: crate::SecretKind::Password,
                    value: SecretParam::from(REDACTED),
                },
                crate::SecretChange::ImportKey {
                    path: "C:\\keys\\id_ed25519".into(),
                    passphrase: None,
                },
            ],
            login_script: Some(None),
            favourite: Some(true),
            credential: Some(Some(crate::CredentialId(2))),
        }),
        Command::SaveCredential(crate::SaveCredentialRequest {
            fields: crate::CredentialFields::Create {
                folder_id: Some(crate::FolderId(3)),
                credential: crate::CredentialDraft {
                    name: "ops".into(),
                    username: Some("deploy".into()),
                },
            },
            secrets: vec![crate::SecretChange::Set {
                kind: crate::SecretKind::Password,
                value: SecretParam::from(REDACTED),
            }],
        }),
        Command::MoveCredential {
            id: crate::CredentialId(2),
            folder_id: None,
        },
        Command::DeleteCredential {
            id: crate::CredentialId(2),
        },
        Command::MapSecretSource(crate::SecretSourceRef {
            connection_id: ConnectionId(1),
            kind: crate::SecretKind::Password,
            source: "plugin:019d0000-0000-7000-8000-0000000000b1".into(),
            entry_id: "4f1c".into(),
            entry_label: "web (root)".into(),
        }),
        Command::SearchSecretSource {
            source: "plugin:019d0000-0000-7000-8000-0000000000b1".into(),
            query: "web".into(),
        },
        Command::UnmapSecretSource {
            connection_id: ConnectionId(1),
            kind: crate::SecretKind::Key,
        },
        Command::ListSecretSources {
            connection_id: Some(ConnectionId(1)),
        },
        Command::ListThemes {},
        Command::SelectTheme {
            mode: crate::settings::themes::ThemeMode::Dark,
            id: "nord-dark".into(),
        },
        Command::SaveTheme {
            theme: crate::settings::themes::ThemeDraft {
                id: Some("own-1f2e3d4c".into()),
                name: "Night".into(),
                dark: true,
                accent: "system".into(),
                tokens: [("window".to_owned(), "#101010".to_owned())].into(),
            },
        },
        Command::DeleteTheme {
            id: "own-1f2e3d4c".into(),
        },
    ]
}
