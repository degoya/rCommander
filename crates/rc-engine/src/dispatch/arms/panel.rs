//! `Engine::dispatch` for the `panel` group (`registry/panel.rs`).

/// The `panel` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::GetPanelState { panel } => Ok(Outcome::Panel($self.panel_state(panel))),
            Command::ListDir { panel, path } => $self.list_dir(panel, path.as_deref()).await,
            Command::Navigate { panel, path } => $self.navigate(panel, &path).await,
            Command::SetCursor { panel, target } => $self.set_cursor(panel, target),
            Command::SwitchActivePanel { panel } => Ok($self.switch_active(panel)),
            Command::ToggleSelection { panel, names } => $self.toggle_selection(panel, &names),
            Command::MoveCursor { panel, step } => Ok($self.move_cursor(panel, step)),
            Command::OpenEntry { panel } => $self.open_entry(panel).await,
            Command::ToggleCursorSelection { panel } => Ok($self.toggle_cursor_selection(panel)),
            Command::ToggleTerminal { panel } => $self.toggle_terminal(panel).await,
            Command::Refresh { panel } => $self.refresh_command(panel).await,
            Command::SelectAll { panel } => Ok($self.select_all(panel)),
            Command::DeselectAll { panel } => Ok($self.deselect_all(panel)),
            Command::InvertSelection { panel } => Ok($self.invert_selection(panel)),
            Command::Sort {
                panel,
                key,
                descending,
            } => Ok($self.sort(panel, key, descending)),
            Command::SetListMode { panel, mode } => Ok($self.set_list_mode(panel, mode)),
            Command::ToggleHidden { show } => $self.toggle_hidden(show).await,
            Command::SelectByMask(crate::MaskRequest { panel, mask }) => {
                Ok($self.select_by_mask(panel, mask.as_deref(), true).await)
            }
            Command::DeselectByMask(crate::MaskRequest { panel, mask }) => {
                Ok($self.select_by_mask(panel, mask.as_deref(), false).await)
            }
            Command::SelectExtension { panel, select } => Ok($self.select_extension(panel, select)),
            Command::RestoreSelection { panel } => Ok($self.restore_selection(panel)),
            Command::SelectRange { panel, step } => Ok($self.select_range(panel, step)),
            Command::CancelIo { panel } => Ok($self.cancel_io(panel)),
            Command::ChangeDrive { panel, drive } => $self.change_drive(panel, &drive).await,
            Command::DriveMenu { panel } => Ok($self.drive_menu(panel)),
            Command::ShowFiles { panel } => Ok($self.show_files(panel).await),
            Command::OpenTerminalHere { panel, other } => {
                $self.open_terminal_here(panel, other).await
            }
            Command::ContextMenu { panel } => Ok($self.context_menu(panel)),
        } }
    };
}
pub(crate) use arms;
