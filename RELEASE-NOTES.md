# Release notes

What each rCommander version changes for you, in a few plain points. Every detail, for
developers too, is in [CHANGELOG.md](CHANGELOG.md).

<!--
How a section is written: "## X.Y.Z", newest first, then one to eight "- " points in English of
what a user notices: what is new, what works better, what is fixed. At most 200 characters each,
ending with a full stop. No job numbers, no paths, no code, no internals (pipeline, tests,
refactoring). A version without anything a user notices says only: Maintenance release: internal
changes only, no change in behaviour.
-->

## 0.12.0

- rCommander looks like a Mac app on macOS: menus in the menu bar, Cmd works as Ctrl, macOS colours and fonts with your accent colour; the settings can switch the look and Cmd back.
- Plugins run on an updated WebAssembly engine that fixes three security flaws.
- Thanks to dogsventuring-sascha, who contributed the Mac look.

## 0.11.1

- SFTP servers on Windows or macOS that ignore upper and lower case are recognised: renaming a.txt to A.TXT works there, and copying a folder into itself under another case is refused.

## 0.11.0

- Select and copy in the Markdown preview: drag across paragraphs, lists, tables and code, double-click a word, Ctrl+A for all, Ctrl+C copies plain text.

## 0.10.0

- rCommander speaks German, French and Spanish beside English: choose the language in the settings, or leave it on Automatic to follow your system.
- The terminal keys stay on the same key on every keyboard: the key right of L (Ctrl+Ö, Ctrl+;) and the key left of 1 (Ctrl+^, Ctrl+`, Ctrl+²); menus show your layout's character.

## 0.9.1

- The first release you can download: 0.9.0 was announced but its packages never reached the download page. 0.9.1 brings everything 0.9.0 lists.

## 0.9.0

- rCommander is public: source, downloads and the handbook are on GitHub, and the website is rcommander.net.
- New packages for macOS on Apple Silicon and Intel, beside Windows and Linux.
- rCommander checks for updates, shows a notice when one is there, and installs a signed update on your word.
- Updates can also install by themselves when you quit or while rCommander is idle; a failed update is taken back.
- Closing rCommander now ends every terminal and its programs within about three seconds, so the program file can be replaced.
- Help > About links the source, the handbook and the changelog.

## 0.8.7

- Searching backwards in the viewer (F3) on a large file on an FTP server is much faster: a match near the start of a 256 MiB file is found in seconds instead of about two minutes.

## 0.8.6

- An assistant's call confirmed with a code, or allowed for the session, runs on exactly the files that were shown; if a panel has moved to another folder by then, nothing happens.
- Moving a file on the same drive never replaces a file that appeared under the target name in the meantime; you are asked instead.
- Pasting text with a control character such as Ctrl+C into a terminal asks first, as pasting several lines does.
- A file opened from an archive is unpacked into a folder only you can read.
- A plugin is installed only from the exact file its preview showed.
- The colour picker stays in front of rCommander's window, and Quick Connect refuses port 0.
- Many small speed-ups: selecting in large folders, file search page by page, the connection tree, the Lister and the settings dialog.

## 0.8.5

- FTP folders show their links at once; a link is looked up when you open, enter or copy it, and a link to a folder then shows as a folder.
- A new setting, Look up FTP links when listing (Transfers), looks up every link of a listing as before.

## 0.8.4

- Unpacking many files from a solid 7z or a compressed tar archive is much faster, and cancelling stops at once.
- Viewing a large file on an FTP server reads from where you jump to instead of from the start, also when searching backwards.
- Chinese, Japanese, emoji and accented characters are drawn correctly in the terminal, which also keeps up better with fast output.
- Large FTP folders fill in while they arrive instead of after a long wait.
- Masks separate by spaces as well as semicolons and keep quoted masks together, in selecting, searching, unpacking and synchronizing, as in Total Commander.
- The synchronize list keeps its rows filled when the window is resized, like the other lists.

## 0.8.3

- Copy, move and delete work on exactly what their dialog showed, even if the selection changes before you press OK.
- A second archive password question waits until the first is answered instead of pushing it away; none is lost or asked twice.
- Touchpads scroll smoothly in the panels, search results, compare, sync and multi-rename lists.
- A plugin update signed by another key than the installed plugin is refused until you remove the old one.
- cmd and Windows PowerShell always start from Windows itself, never a program of the same name in PATH.
- At most 5 agent keys are offered per SSH login, the connection's own key first (Settings, Terminal).
- Older automatic database backups are removed, the newest 10 are kept (Settings, Exchange).
- An AI assistant can no longer write Git hooks or VS Code tasks, and files it copies are checked again just before they are read.

## 0.8.2

- An AI assistant's question now runs exactly what it shows: changing the selection or folder while you read no longer changes what is deleted, copied or opened.
- An assistant's command that carries a password or token is refused instead of being shown to you masked.
- Unpacking or copying out of an archive over an existing file no longer loses that file when the entry is damaged or you cancel.
- On the .. line without a selection, pack, unpack, checksum and multi-rename do nothing instead of taking the first file.
- A file dropped from Explorer lands in the panel under the pointer, also with the divider moved or a panel maximized.
- The import pages read only the file you previewed, and Quick Connect asks twice for a new master password.
- xz and 7z archives that need too much memory to unpack are refused (Settings, Folders, Archive decoder memory).
- Cancelling on an FTP server no longer makes the next command read the wrong answer; security updates of libraries.

## 0.8.1

- In the connection manager, Shift+Enter opens a terminal on the selected SSH server and Ctrl+Enter connects to it, straight from the list or the filter.
- Fixed a one-pixel see-through line between the dark menu bar and the panels on Windows 11.

## 0.8.0

- The connection manager (Ctrl+F) opens with the cursor in its list, on the connection you used last; typing a letter there filters (Settings, Keyboard).
- Duplicate a connection with Shift+F5 or the Duplicate button: the copy keeps its password and key, and its name is ready to change.
- d: in the command line opens the other panel's folder even when this panel is already on that drive, as in Total Commander (Settings, Folders).

## 0.7.0

- Packing, unpacking, checksums, synchronizing and multi-rename start at once instead of waiting behind a running copy (Settings, Transfers).
- The Total Commander import also brings its colours, its colours by file type and the file lists' font.
- A package for Linux on 64-bit ARM.

## 0.6.1

- Ctrl+Alt+V pastes a screenshot into a terminal as the path of an image file, so Claude Code and similar programs can read it, also on an SSH server (Windows; Settings, Terminal).

## 0.6.0

- Copy and paste in a terminal as in Windows Terminal: Ctrl+C or a right click copies selected text, Ctrl+V pastes, and copying on select can be switched on (Settings, Terminal).
- Copying inside programs that take the mouse, such as Claude Code, now reaches the clipboard, also over SSH; hold Shift to select in the terminal itself.
- Alt with a letter in a terminal goes to the program instead of opening a menu.
- A new folder (F7) is made at once, even while a large copy or move runs in the background.
- The active panel's blue border now runs all round, also along the top.
- An AI assistant's connection to rCommander closes when rCommander does, so an update can replace the files.

## 0.5.2

- When an AI assistant runs commands in a terminal or on a server, you can allow it there for 30 minutes instead of confirming every single command.
- The status bar shows what an AI assistant may do without asking; a click takes it all back. The time can be changed or the offer switched off (Settings, MCP server).

## 0.5.1

- Shells and the programs they started now close together with rCommander on Windows, also after a crash; a new setting keeps started programs running (Settings, Terminal).
- The login-script editor is clearer: numbered steps joined by a line, the secret chosen from a list, arrow buttons to reorder, Save and Delete at the bottom.
- A warning marks a Send secret step that does not follow a wait for a password prompt; it can be switched off (Settings, Terminal).

## 0.5.0

- Linux now has the menu bar too: File, Mark, Commands, Net, View, Configuration and Help at the top, by mouse or Alt with the underlined letter.

## 0.4.7

- A new setting lets Space select an entry and keep the cursor on it instead of moving to the next line (Settings, Folders).

## 0.4.6

- Shared credentials: keep a user, password and key once in the connection manager and let many connections use them; change the password once for all.
- An AI assistant can list and organise shared credentials, but never sees their secrets or points a connection at one.

## 0.4.5

- A connection's password can come from the Windows Credential Manager, free and without a plugin: choose it under the connection, search, pick the entry.
- When the password manager no longer has the chosen entry, connecting says so and asks you to pick it again.

## 0.4.4

- A plugin's licence key is typed hidden on the Plugins page and never shown to an AI assistant.
- The Plugins list shows when a password manager plugin is locked, not answering or has no licence.

## 0.4.3

- A connection's password, private key or passphrase can come from a password manager plugin instead of rCommander's vault: choose the source under the connection, search, pick the entry.
- The secret is fetched from the password manager at every connect and never stored; a locked or unreachable password manager is named when you connect.

## 0.4.2

- Colours by file type now colour the extension column of the full view too, not only the name.
- The scroll bar of the settings pages stands at the window's right edge, as on the other pages.

## 0.4.1

- Pick colours in Windows' colour dialog: Choose... beside every colour of the Colours tab, and ... at each rule's colours.

## 0.4.0

- Choose your own colours for the file lists, as in Total Commander: text, background, cursor, marked names and the accent, for the light and the dark look.
- Colour file names by type: rules such as *.exe in red, with a preview and a warning when text gets hard to read.

## 0.3.1

- A graphics driver reset no longer fills the database with gigabytes of errors that slowed rCommander down; a database bloated that way shrinks back by itself.
- When the graphics card loses its device, rCommander asks whether to restart instead of freezing; switch it off under Settings -> Appearance.
- The queue window marks running jobs, lines up each file under its path and shows the speed of all running jobs together.
- F2 in a copy's progress window sends it to the background, as F2 in the copy dialog puts it into the queue.

## 0.3.0

- Alt with the underlined letter works in every dialog - properties, pack, checksums, find files, compare, connections, queue, settings - and ticks checkboxes or jumps to a field too.
- Recording a new key in Settings -> Keyboard -> Shortcuts no longer presses a button when you type its letter.

## 0.2.0

- Folder tabs as in Total Commander - Ctrl+T, Ctrl+W, Ctrl+Tab, locked tabs, drag to reorder - with a history per tab and a hotlist on Ctrl+D; » and Ctrl+Shift+A list them all; tabs survive a restart.
- F5 to F8 as in Total Commander: OK runs at once, F2 queues; the queue shows every job, the bar its first five; F7 offers the name under the cursor; cut copies resume; F8 deletes for good.
- Drag files between panels, in from or out to the Explorer, a mail or a browser; Ctrl+C/X/V exchange files with the Explorer; change any key in Settings -> Keyboard -> Shortcuts.
- Multi-rename on Ctrl+M with masks, preview, presets and undo; compare files and folders, synchronize them; checksum files made and verified, .sfv with Enter; split, combine; find on Alt+F7.
- F3 or Ctrl+Q show text, hex, pictures, SVG, Markdown, .nfo art; Ctrl+wheel sizes text; plugins colour JSON, XML, YAML, CSV, configs, scripts, diffs, logs, web code (7: tree, source); properties.
- Zip, 7z and tar archives open like folders: test, unpack, pack, F3 and F4 on a file, F5 copies folders out, Skip all; tabs inside them survive a restart; safe from archive bombs.
- A connection manager on Ctrl+F - type to filter - with secrets in a vault that locks on Win+L, sleep and after 8 hours, then carries on once unlocked; imports from ssh config, TC and RDM.
- SSH, FTP and FTPS servers in a panel, first files at once, plain FTP flagged, the terminal there is the server's shell; F5/F6 copy to drives and between servers, resume; Ctrl+Shift+F disconnects.

## 0.1.0

- Two file panels with Total Commander's keys: full and brief views, sort by column, Owner column; drag widths, divider, scroll bar; long names end in …, whole on hover; Explorer's icons and menu.
- Select as in Total Commander or with Ctrl/Shift+click as in the Explorer: Insert, Space, Shift with the arrows, Num+ and Num- by a mask, Num* inverts; marked rows keep their font.
- A Windows menu bar, dark in dark mode; Enter opens or starts a file; a command line, quick search and a filter; Copy name and full path; the path above a panel is clickable.
- A drive bar with drives, free space and WSL; a drive the other panel shows opens at its folder; Alt+F1/F2 map and disconnect network drives, which reconnect when opened; slow shares never freeze.
- Ctrl+Ö turns a panel into a terminal in its folder and back, Ctrl+Ä spreads it over the window; Ctrl+Shift+Ö picks the shell, Ctrl+Shift+A lists the sessions; it never moves a panel to a share.
- Every dialog and question in a movable window of its own, as in Total Commander; Browse... beside paths; in every dialog Tab, Alt+letter, Escape, F10 or X cancel.
- Back up rCommander's data from the menu bar or with "rcommander backup"; export settings, connections, host keys and hotlist, encrypted if you like, and import them on another PC.
- AI assistants drive rCommander over MCP, asking you first (Deny preselected) before anything risky; they never see secrets, write into rCommander's, SSH or autostart folders, or keep the vault open.
