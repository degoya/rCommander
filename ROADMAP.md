# Roadmap

Where rCommander is heading. This is a plan, not a promise: milestones move, change shape or
split when daily use shows something more urgent, and nothing here is a date. rCommander is
pre-alpha — nothing below has shipped yet. What has shipped is in
[`CHANGELOG.md`](CHANGELOG.md).

## 0.0 — Foundation

Repository infrastructure taken over from rDownloader, a spike that confirms iced and `iced_term`
on Windows, the virtual file system with its local backend, the SQLite database, the command
registry every action goes through, and the MCP server around it.

## 0.1 — Two-panel core

Two panels with tabs, column and brief views, sorting, selection, quick search and filter, the
drive bar with drive letters and WSL distributions, the F-key operations through a background
queue with progress and conflict handling and a queue window to reorder it, several queue lists
saved and loaded, history and hotlist, settings with an About page, fonts per area, a light and a
dark theme, and native file icons on Windows.

## 0.2 — Tools

Multi-rename with Total Commander's placeholders, regular expressions, preview and undo; file
search by name, content, size, date and attributes; content comparison and directory
synchronisation; checksums, `.sfv` and other checksum files verified with Enter; split
and combine; properties and links; quick view and an internal viewer for text, hex and images,
with a Markdown preview.

## 0.3 — Archives

zip, 7z and tar archives opened like folders, extracted and packed.

## 0.4 — Terminal

Any panel can show a terminal instead of the file list: PowerShell, cmd and WSL shells, with the
working directory kept in step with the panel; Ctrl+Ä lays it over the whole window width.

## 0.5 — Connection manager and vault

A small SSH connection manager: a folder tree of servers, passwords and keys in an encrypted
vault that only asks for a master password once secrets are stored, ssh-agent and Pageant, import
from `~/.ssh/config` and from Total Commander (hotlist, tabs, connections), host-key trust and
login scripts.

## 0.6 — SSH and SFTP

SFTP panels and SSH terminals that run their login script, transfers local ↔ remote and
remote ↔ remote as a stream, and resume.

## 0.7 — FTP and FTPS

FTP and FTPS panels and transfers, with certificate trust.

## 0.8 — Beta hardening

Drag and drop with Explorer, the Windows shell context menu, design guidelines and a pass over
every dialog, a security review of vault, MCP and remote access, robustness and performance work,
and a beta package.

## After the beta

Linux and macOS packages, 1.0, and then translations — the i18n keys exist from the start. A
button bar like Total Commander's and RAR extraction also come after the beta.
