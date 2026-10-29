//! `import_total_commander`: the thin wrapper over `import.total_commander` (RC-050-09).
//!
//! A write tool, as `import.total_commander` is registered (RC-080-12), its preview (`dry_run`, the
//! default) included, as `connections_import_ssh_config` is. It reads
//! only files called `wincmd.ini` and `wcx_ftp.ini` that the read guard lets through, and never a
//! password: the answer says per connection only whether Total Commander keeps one
//! (`password_state`), and a connection is always created without one. Taking the passwords into
//! the vault is the window's alone (`import.total_commander_passwords`, GUI only).

use rc_engine::{
    Command, Outcome, TcImportRequest,
    import::total_commander_report::{TcImportReport, TcParts, TcTabsMode},
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ImportTotalCommanderParams {
    /// Path of wincmd.ini; omit it for %APPDATA%\GHISLER\wincmd.ini or where Total Commander's
    /// registry key says. Only a file named wincmd.ini is read.
    #[serde(default)]
    pub wincmd_ini: Option<String>,
    /// Path of wcx_ftp.ini; omit it for the one wincmd.ini names or the one beside it. Only a
    /// file named wcx_ftp.ini is read.
    #[serde(default)]
    pub ftp_ini: Option<String>,
    /// Import the directory hotlist (default true).
    #[serde(default)]
    pub hotlist: Option<bool>,
    /// Import the folder tabs of both panels (default true).
    #[serde(default)]
    pub tabs: Option<bool>,
    /// Import the FTP/FTPS/SFTP connections (default true).
    #[serde(default)]
    pub connections: Option<bool>,
    /// Import the list colours ([Colors] for the light look, [ColorsDark] for the dark one), the
    /// colours by file type (appended) and the file lists' font into the settings (default true).
    #[serde(default)]
    pub look: Option<bool>,
    /// true (the default): only the preview, nothing changes. false imports.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// true puts the imported tabs in place of the panel's own; false (the default) adds them
    /// after them.
    #[serde(default)]
    pub replace_tabs: Option<bool>,
    /// The keys of the rows to import, from a preview (menu<N>, left<N>, right<N>,
    /// ftp:<section>, colour.<light|dark>.<token>, rule.<N>, font.list); omit it for everything
    /// new or changed.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// The report itself, without the outcome's `kind`.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Report(Box<TcImportReport>),
    Other(Box<Outcome>),
}

#[tool_router(router = tc_import_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Import Total Commander's directory hotlist, folder tabs and FTP connections (wincmd.ini, wcx_ftp.ini) into rCommander's hotlist, panels and connection tree, and its list colours, colours by file type and list font into the settings. dry_run defaults to true: the answer is the preview and nothing changes; call again with dry_run=false to import. A write tool, the preview included. Answers dry_run, wincmd_ini, ftp_ini, tabs_mode, hotlist (key, name, folders, folder, path, target, selected, status), tabs (key, panel, path, caption, locked, active, selected, status), connections (key, name, folders, kind, host, port, user, directory, password_state: none, present, master_password or unreadable, selected, status), look (key, source, value, selected, status) and warnings. Statuses: new, created, exists, update, updated, missing (a tab's folder is gone; imported only when only names it), skipped with a reason (program, internal_command, server_path, plugin_path, no_command, relative_path, unknown_variable, invalid_path, too_deep, name_invalid, inverse, colour_invalid, search_filter, too_many), and for connections new, created, update, updated, unchanged or refused with a code. A second import adds nothing twice. Everything goes into folders named Imported from Total Commander. Passwords are never read or returned; connections come without one, and only the person can take them into the vault in the window. Errors: tc_ini_path_refused, tc_ini_unreadable, tc_ini_too_large, tc_ini_not_found."
    )]
    pub(crate) async fn import_total_commander(
        &self,
        Parameters(params): Parameters<ImportTotalCommanderParams>,
    ) -> ToolResult {
        let defaults = TcParts::default();
        let command = Command::ImportTotalCommander(TcImportRequest {
            wincmd_ini: params.wincmd_ini,
            ftp_ini: params.ftp_ini,
            parts: TcParts {
                hotlist: params.hotlist.unwrap_or(defaults.hotlist),
                tabs: params.tabs.unwrap_or(defaults.tabs),
                connections: params.connections.unwrap_or(defaults.connections),
                look: params.look.unwrap_or(defaults.look),
            },
            dry_run: params.dry_run.unwrap_or(true),
            tabs_mode: if params.replace_tabs.unwrap_or(false) {
                TcTabsMode::Replace
            } else {
                TcTabsMode::Append
            },
            only: params.only,
        });
        self.dispatch_as(command, |outcome| match outcome {
            Outcome::TcImported(report) => View::Report(Box::new(report)),
            other => View::Other(Box::new(other)),
        })
        .await
    }
}

#[cfg(test)]
#[path = "tc_import_tests.rs"]
mod tests;
