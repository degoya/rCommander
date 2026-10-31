# Changelog

All notable changes to rCommander are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), the versioning follows [SemVer](https://semver.org/).

## [Unreleased]

## [0.13.0] - 2026-10-31

### Added

- **Package managers** (RC-100-52): a Homebrew tap with a cask for macOS and a formula for Linux
  (`degoya/homebrew-rcommander`) and a Scoop bucket (`degoya/scoop-rcommander`), rendered from
  `packaging/` by `scripts/package-managers.sh` (`--check` against the last release) and pushed by
  release.yml after each release. `rc_update::install_kind` knows `scoop`, `homebrew`, `winget`,
  `deb` and `rpm` (`RCOMMANDER_INSTALL_KIND`, the path, the `install-kind` marker,
  `VERSION.txt`): such an installation never replaces itself, reads the stable channel, has the
  automatic install off, and the Updates page shows the manager's command with "Copy command";
  `update.status` carries `install_command`. The hidden `rcommander install-kind` prints the kind
  for the channels' CI (`package-channels.yml`).
- **winget** (RC-100-53): `degoya.rCommander` as the portable zip, a pull request to
  `microsoft/winget-pkgs` per release (komac, `WINGET_TOKEN`), the older open one closed
  (`scripts/winget-supersede.sh`).
- **deb and rpm, a signed apt and dnf repository** (RC-100-54): `scripts/package-deb-rpm.sh`
  (`scripts/package-linux.sh --deb --rpm`) with nfpm, a desktop entry and hicolor icons; the
  packages on every GitHub release; `scripts/package-repo.sh` and `release.yml`'s `apt-dnf` job
  file them into `degoya/rcommander-packages`, signed with the key `rcommander-packages`, the
  newest three versions kept; `packages-repo.yml` installs and upgrades with apt and dnf.
- **Colour themes** (RC-100-50): a theme sets every colour of the window and the terminal's
  palette; one per light and dark mode (`appearance.theme_light`, `appearance.theme_dark`).
  Built in: Fluent (default), Mac (macOS), Classic, High contrast, Solarized and Nord, each light
  and dark with terminal colours. Own themes (table `themes`, migration 0020) are made, duplicated,
  renamed, deleted and edited on the Appearance page's tab *Themes* (formerly *Colours*), with a
  live preview Cancel takes back, and exported and imported as `.rctheme` files checked at the
  boundary; they go with the settings export. The list colours of RC-100-43 become a "Custom"
  theme at the first start. Commands `theme.list`, `theme.select` (MCP `theme_list`,
  `theme_select`) and the window's `theme.save`, `theme.delete`; open terminals take a new
  palette without a restart.
- **A theme per connection** (RC-100-51): a connection chooses a theme for the light and one
  for the dark mode (connection manager, Theme (light) and Theme (dark), Default the window's;
  `connections.theme_light`, `theme_dark`, migration 0021). Its panel - list, headers, status line,
  tab - and its SSH terminal draw with it, an open terminal recoloured without a new session;
  the window, dialogs, the other panel and local and WSL terminals stay global; a deleted theme
  falls back. Switch `appearance.connection_themes` (on). `connections.create`, `.update`, `.save`
  and their MCP tools take and check `theme_light`, `theme_dark` (`theme_unknown`,
  `theme_wrong_mode`); a theme-only change keeps the session; duplicate and the connections export
  carry them, an import without the theme says `theme_missing`.

### Fixed

- `scripts/public-ci.sh` no longer reads a timeout or network failure of `gh auth status` as
  "not signed in" (exit 2): it tries three times and then ends with exit 1, not finished; only
  gh's own not-signed-in or invalid-token answer is exit 2 (RC-100-31).

## [0.12.0] - 2026-10-30

### Changed

- **A Mac look on macOS**: the menus sit in the system's menu bar (muda's `NSMenu`, with the
  application menu - About, Settings on Cmd+, , Services, Hide, Quit - and the Window menu) instead
  of the bar rCommander draws (`DRAWN` is now Linux only); entries carry no key equivalent, so the
  keymap's keys and the text fields' Cmd+C still reach the window. Cmd acts as the keymap's Ctrl
  (Cmd+R refreshes, Cmd+Shift+P opens the palette; Ctrl still works) and keys show as the Mac
  writes them (Cmd+Shift+F as ⇧⌘F). The colours are macOS's greys for light and dark mode with the
  accent chosen in System Settings (`AppleAccentColor`), the interface font is San Francisco and
  the monospace font Menlo. The main window's title bar is transparent over the content, with the
  traffic lights on the window's own surface (Finder's look); the content keeps their height free.
  Two settings, both on, switch it (RC-100-49): `appearance.macos_look` (at the next start) gives
  macOS the look of the other systems - WinUI's colours with the default accent, the default fonts
  and the system's title bar - while the menus stay in the system's menu bar;
  `keys.macos_cmd_as_ctrl` (at once) leaves Cmd to the system, so only Ctrl is the keymap's Ctrl
  and keys show with the Control symbol (⌃⇧F). Contributed by dogsventuring-sascha (PR #2).

### Fixed

- **wasmtime 49.0.5**: the plugin host's Wasmtime is raised from 49.0.4 to 49.0.5 in
  `Cargo.lock` (cranelift 0.136.5), which fixes RUSTSEC-2026-0422 (typed functions with `i31` or
  `nullref` arguments may panic), RUSTSEC-2026-0423 (cross-engine type confusion with host-created
  struct types) and RUSTSEC-2026-0424 (unchecked memory use when borrowed resources move between
  components); `cargo deny` on GitHub refused the old version.

## [0.11.1] - 2026-10-30

### Fixed

- **An SFTP server that folds case is treated as one**: when the backend opens it `stat`s the
  home directory as named and with one name in the other case, both at once (one round trip,
  nothing written); the same object means the server folds case, as OpenSSH on NTFS or APFS
  does, and an error or a home without a letter keeps it case-sensitive. Copying `Data` into
  `data/sub` is refused there and a rename that only changes the case goes through a temporary
  name; the setting `sftp.case_sensitivity` (`auto`, `sensitive`, `insensitive`) overrides the
  probe. The SFTP conformance test runs its case check again on every disk, and the test server
  can fold case (RC-080-60).

## [0.11.0] - 2026-10-30

### Added

- **Selecting and copying in the Markdown preview** (RC-020-14): the mouse selects across
  paragraphs, headings, list items, table cells and code lines; Shift+click extends, a double
  click takes a word, a triple click a line; Ctrl+A selects all and Ctrl+C copies the selection
  as plain text, one line a block. iced's `rich_text` is drawn unchanged inside a `Selectable`
  wrapper (`viewer/selectable.rs`) that hit-tests a paragraph of its own; the blocks' text is
  collected with the parse by the same `markdown::view_with` pass (`viewer/select.rs`). The new
  setting `viewer.markdown_select` (on) turns it off.

## [0.10.0] - 2026-10-30

### Added

- **The interface in German, French and Spanish** (RC-100-47): `locales/de.json`, `fr.json` and
  `es.json` are compiled in beside English, a key a catalogue lacks reads in English. The new
  setting `appearance.language` (Appearance > Window; Automatic, English, Deutsch, Français,
  Español - each name in its own language) picks one; Automatic takes the system's language when
  rCommander has it, else English. It takes effect at the next start and is a schema row, so
  `settings.get`/`settings.set` reach it over MCP. The repo lint `catalogues` holds every
  catalogue to `en.json`'s keys and placeholders, without an empty text.

### Changed

- **The terminal keys sit on the physical key** (RC-100-48): the keymap can name a key of the
  main block by where it sits, `Ctrl+[Semicolon]` (the W3C code in brackets; `keymap/physical.rs`).
  The terminal toggle, the new terminal and the maximize are bound this way, so they are Ctrl+Ö /
  Ctrl+Ä on a German keyboard, Ctrl+; / Ctrl+' on a US one and Ctrl+Ñ / Ctrl+´ (a dead key) on a
  Spanish one, also after a layout switch. The toggle and the new terminal also sit on the key
  left of 1, `Ctrl+[Backquote]`: Ctrl+` on US, Ctrl+^ (a dead key) on German, Ctrl+² on French
  AZERTY, where Ctrl+M stays Multi-Rename (owner 2026-10-29). Menus, palette, context menus, the drive
  bar and the Shortcuts tab show the active layout's character (Windows: `MapVirtualKey` through
  winsafe, the menu bar again after a switch; elsewhere the US character). Keys in character form
  keep working and win over a key by place; the Shortcuts tab records a punctuation mark, an
  umlaut or a dead key by place. The keyboard reference prints such a key as `Ctrl+Ö / Ctrl+;`.

## [0.9.1] - 2026-10-29

### Fixed

- The release workflow found no package to publish: the three upload patterns share `artifacts/` as their root, so each package arrived in a subfolder; the publish job now gathers them into one folder first. v0.9.0 was tagged but never published (RC-100-32).
- The screenshot run's Lister scene waited for a Lister window of its own, which the run never opens; it now captures the Lister over the main window (RC-100-45).

### Changed

- The public repository and the wiki no longer carry the roadmap: `ROADMAP.md` joins `docs/` in `scripts/public-exclude.txt` and the README points at the changelog only (owner 2026-10-29).

## [0.9.0] - 2026-10-29

### Added

- **Public export to GitHub, repository and wiki** (RC-100-30, after rDownloader):
  `scripts/export-public.sh X.Y.Z` turns `git archive` of a release tag, without
  `scripts/public-exclude.txt` (`docs/`, `AGENTS.md`, `CLAUDE.md`, `.claude/`, the maintainer's
  workflow scripts), into one fresh commit "Release X.Y.Z" with tag `vX.Y.Z` in a local clone of
  `github.com/degoya/rCommander`; `scripts/export-wiki.sh X.Y.Z` does the same for the wiki,
  leaving out `<!-- private page -->` pages and `<!-- private -->` sections. Both refuse a link
  into what is left out, a private name outside `scripts/public-names-allow.txt` and anything
  gitleaks finds; nothing is pushed without `--push`. `CONTRIBUTING.md`, `SECURITY.md`,
  `CODE_OF_CONDUCT.md`, issue and pull-request templates and `dependabot.yml` for strangers; the
  README points to GitHub Releases, the wiki and rcommander.net. Help > About links the GitHub
  repository, wiki and changelog. The MCP coverage page is `crates/rc-mcp/mcp-coverage.md`, the
  dialog keys `crates/rc-engine/keymap/contexts.md`, the logo `crates/rcommander/resources/brand/`,
  the wiki page for plugin authors `develop/plugin-development.md`.

- **GitHub CI and release workflows, macOS packages** (RC-100-31, RC-100-44): `ci.yml` runs the
  preflight (shellcheck and gitleaks at their pins, cargo-deny online), clippy `-D warnings` and
  nextest on Linux x64 and arm64, Windows (MSVC) and macOS (Apple Silicon, Intel), the plugins'
  wasm32 lint and unsigned build, and packs `rCommander.app` for both Mac architectures;
  `workflow_dispatch` narrows the runner images (`platforms`). `release.yml` on `v*.*.*` signs
  the plugins once (`RCOMMANDER_PLUGIN_SIGNING_KEY`), packs all five targets, writes SHA256SUMS
  with a Sigstore bundle and the signed update manifests (`RCOMMANDER_UPDATE_SIGNING_KEY`) and
  publishes the release with the version's `RELEASE-NOTES.md` section.
  `scripts/public-ci.sh <branch> [--platforms ...]` exports a branch as `ci/...`, runs `ci.yml`
  there, waits, records the green per tree (`.git/rc-verified-ci`) and deletes the branch on
  green; `scripts/ci-log.sh <run>` prints a run's failures. A wave's integration branch runs it
  on Windows before the merge.
  `scripts/package-macos.sh` builds `rcommander-<v>-macos-<arch>.zip` (Info.plist from
  Cargo.toml, bundle id `net.rcommander.app`, icon, plugins in `Resources/`, ad hoc signature);
  the package scripts take `--plugins <dir>`, Windows packs natively under Git Bash, Linux arm64
  natively on arm64. On macOS the bundled plugins are read from the app's `Resources/plugins`,
  Delete moves to the Trash (`NSFileManager`) and the boot volume's link in `/Volumes` is no
  drive of its own.

- **A release pipeline with evidence, as rDownloader cuts its releases** (RC-100-32):
  `scripts/release-pipeline.sh X.Y.Z [--push] [--resume] [--plan]`, started detached by
  `scripts/release-start.sh`, runs preflight, the version bump, `check.sh --full`, the Windows and
  Linux (x64, arm64) packages as checks, their verification, the documents gate, the job archive,
  the release commit, the merge into `main`, the public CI, the tag, the push and the public
  export, wiki and website; every step's output, exit and size go into
  `artifacts/release-evidence-X.Y.Z.log` with the run's nonce, and `scripts/tag-release.sh`
  refuses a tag unless every earlier step of this run passed on the tree it tags. A beta
  (`X.Y.Z-beta.N`) stays off `main`. For the public repository's `release.yml`:
  `scripts/release-assets.sh sums <dir>` and `scripts/release-publish.sh update-manifest <dir>
  <tag>` (`rc-pack`, key from `RCOMMANDER_UPDATE_SIGNING_KEY`, a warning without it); the
  `release` skill.

- **Screenshots of the real window, taken on Windows** (RC-100-45): the hidden flag
  `rcommander --screenshots <scenes.json> --out <dir> [--demo-root <dir>]` starts the window on
  a throwaway profile (its own database under the temporary folder), a demo tree with a tar
  archive and four `example.com` connections without a secret - no MCP server, nothing
  restored, nothing connected - runs each scene's steps (keys through the keymap, read and write
  commands, the settings opened, a Lister closed, an MCP question shown), saves
  `iced::window::screenshot` as `<dir>/<scene>.png`, writes `screenshots.json` and removes the
  profile and the demo tree. The scene file is read strictly: an unknown field or parameter, a
  deleting, executing or trust command, a name that could leave `<dir>` or a limit passed ends
  the run before the window opens. `scripts/screenshots.sh [--release] [--deliver]` runs it
  through `run-windows.sh` with `scripts/screenshots/scenes.json` (eleven scenes, alt texts in
  `alts.json`), fetches the pictures to `artifacts/screenshots/<version>/` and with `--deliver`
  copies them to the website's `public/screenshots/` and the wiki's `images/screenshots/`.

- **`scripts/update-website.sh X.Y.Z [--push]` brings rcommander.net to a release** (RC-100-46),
  as rDownloader does it: it sets the website's `app/data/release.json` (version, release date
  kept on a re-run) and `site.json`'s version, checks the site's wiki links against the public
  wiki, runs the site's tests, generates it, checks the download page links the version's
  GitHub release assets and commits `Release X.Y.Z` in the website repository; a pre-release is
  refused, `--push` pushes the site's branch, nothing is deployed. Test
  `scripts/tests/update-website.sh`. The website itself (separate repository) now names
  `https://rcommander.net` and `degoya/rCommander`, offers the Windows, Linux and macOS archives
  with `SHA256SUMS`, shows the screenshot scenes of RC-100-45 and describes the viewer plugins.

- **Signed update manifest per channel** (RC-100-04): the new crate `rc-update` with the Ed25519
  envelope under `rcommander.update-manifest.v1`, the compiled-in root `rcommander-update-v1`
  (a placeholder that fails closed until the owner's key is in), the manifest checks in
  rDownloader's order, the `rc-pack` binary (`keygen`, `update manifest build|verify|bump`) and
  `scripts/sign-update.sh` with its script test; copied and adapted from rDownloader at 26c8592a6.

- **Update check, notice and verified download** (RC-100-05): background checks of the GitHub
  releases (stable, beta) or a mirror, the floors stored first, offers never below the running
  version, `update.status` and `update.check` for MCP, the status bar notice, the settings page
  Updates with the release notes, the download checked by size and SHA-256; all `update.*`
  settings protected.

- **Install, restart and roll back** (RC-100-06): `update.install` (window only) copies the
  database first, hands a journal to the hidden `rcommander apply-update`, switches the portable
  program files through `.previous/` and the trash, waits for the new version's proof and takes
  everything back otherwise; a start recovers an interrupted update before the database opens.

- **Automatic updates and the clean-up after them** (RC-100-07): `update.auto_install` off,
  `on_quit` or `when_idle` (no input, job, terminal, SSH session or dialog for
  `update.idle_minutes`), never again by itself for a version that failed; after a proven update
  only the newest database copy stays, for `update.backup_retention_days`.

### Fixed

- **rCommander ends whole when it closes** (RC-080-58): the exit is bounded - the MCP server
  gets 2 s (its graceful shutdown gives up open requests after 1.5 s), the app's runtime 1 s
  for its blocking tasks, then the process ends with `std::process::exit`; a guard thread armed
  as iced ends finishes and ends the process when iced's own runtime waits on a blocking task
  for more than 1 s. A process without a window no longer keeps the terminals' job objects, the
  shells and the locked exe. A step Windows refuses for a terminal's job object is a `warn` with
  the step and the OS error, never the command line; each exit phase writes a `debug` line.
- **A drive whose root lies below a drive letter is a drive of its own on Windows**: the drive of
  a folder is the longest drive root above it, then its letter or share, so the folder per drive
  and the other panel's folder go by that drive as on Linux (RC-080-59).
- **Total Commander's `wincmd.ini` from the root of a drive is found**: a registry path such as
  `\totalcmd\wincmd.ini` without an install folder is kept on the current drive instead of being
  dropped (RC-080-59).
- **Reading a folder as a file on Windows is "is a directory"**: the local backend turns
  Windows' "access denied" for a directory opened as a file into `is_a_directory`, as on Linux,
  so reading a folder fails by its kind, not as a missing permission (RC-080-59).

## [0.8.7] - 2026-10-29

### Changed

- **The viewer's backward search over FTP opens a data connection per span, not per MiB**
  (RC-080-57): on a backend that starts a read at an offset but cannot seek (FTP's `REST`) it
  reads a span before the offset forwards on one stream and keeps the last match, the span
  doubling from two chunks up to 32 MiB (`MAX_SPAN_CHUNKS`); memory stays one chunk and the
  overlap. Local files, SFTP and archive entries search as before. `scripts/perf.sh --cases`
  unpacks each format in a process of its own (`perf archive-pack`, `perf archive-unpack`), so
  an unpacking's peak memory holds nothing of the packing.

## [0.8.6] - 2026-10-29

### Changed

- **Database, MCP gate and repo lints tightened** (RC-080-54, code audit 2026-10-28): migration
  0019 adds plain indexes on `connection_folders(parent_id)`, `connections(folder_id)`,
  `connections(login_script_id)` and `credentials(folder_id)`, so a folder's members, a
  folder's delete cascade and a deleted script's trigger no longer scan; the MCP gate reads the
  policy, the headless-code switch and the exec-grant minutes in one query, the retention limits
  come from one `LIKE 'retention.%'` query, and a device or NT path is refused without listing
  the drives; the rekey shares its statement text per column; the instance file is read up to
  4 KiB; the server's instructions live in `rc-mcp/src/instructions.md`, held against the tools
  by a test; the test-only confirmation APIs left the build. The repo lints count focus stops per
  kind (pick lists, togglers, sliders, pressed `mouse_area`s; marker
  `// focus-ring: pointer only`), judge each spawn by its own chain, ignore comments, count no
  test file's key as used, fail on an unreadable file and skip only the workspace's `target`;
  arm64 Linux joins the licence list and cargo-deny's graph.
- **`scripts/perf.sh` makes its test folders on disk** (RC-080-54): `RC_PERF_DIR` defaults to
  `${XDG_CACHE_HOME:-~/.cache}/rcommander-perf` instead of the log directory under `/tmp`, which
  as a tmpfs ran out of space at 1 000 000 files.
- **The endpoint check of a connection with secrets runs inside the save's transaction**
  (RC-080-54 for RC-080-50): `Db::update_connection_with` and `Db::save_connection_with` take an
  `EndpointRule`; under `KeepWhileSecretsHeld` the change's own `BEGIN IMMEDIATE` refuses a new
  host, port, kind or user of a connection that holds a secret (`endpoint_change_needs_gui`);
  `connections.update` and `connections.save` use it for every MCP call, so a secret stored by
  another process between the look and the write stops the move too. A client's cancel of an MCP
  call reaches the engine command.

### Fixed

- **An MCP call confirmed without the window runs what was confirmed** (RC-080-55): a headless
  confirmation code is issued for the call frozen to the panel - its answer names the arguments
  with the selection or the cursor's entry written out - and redeeming it runs exactly that call
  with its panels pinned, whatever is selected by then; a call allowed for the session freezes
  and runs pinned too. A panel in another folder by then ends the call with `panel_changed`.
- **The engine's smaller findings of the code audit** (RC-080-50, code audit 2026-10-28): the
  dead commands `secrets.set`, `secrets.clear`, `panel.open_archive` and `terminal.resize` are
  gone (the form saves through `connections.save`, Ctrl+PageDown is `panel.open_entry`, a panel
  sizes its terminal itself); a copy, move or delete opens the backend of every source, so a
  local file and one at `sftp://` go in one call; comparisons by content stop with the token of
  their call (`Engine::dispatch_cancellable`, `dispatch_mcp_cancellable`); `file.open` shows a
  folder below a name that is no UTF-8; an MCP endpoint check and its write run under the
  secret writes, so no secret stored between them goes to the new server; a grant that ran out
  is announced when it goes, and one grant is kept once; `files.search` keeps its walk for the
  next page (5 minutes, at most 8 searches) instead of walking from the start for every page;
  small files are compared from one read; `settings.get` reads every row in one query; the
  hotlist order, an MCP question's context and the sync walk (both sides listed at once) cost
  less; the read guard of an MCP copy judges its paths in batches on one blocking task.
- **Remote and terminal, the smaller findings of the code audit** (RC-080-51, code audit
  2026-10-28): an SSH session once used for SFTP closes after its idle time again - the kept
  SFTP backend is dropped when unused for that long; a kept FTP session is no longer kept from
  a login that started before the connection changed, and the loser of two logins at once says
  `QUIT`; `sftp://<name>` of two quick connections with one name means the newest; a quick
  connection saved as a connection is removed again when its secret cannot be stored; its user
  may hold no control character; restoring the tabs waits for a folder at most as long as the
  drive bar waits for a drive (2 s), so a share that does not answer no longer holds the start;
  a file opened from an archive is unpacked under a folder of the user's own (Unix:
  `rcommander/unpacked` in the runtime or cache folder at 0700) with a name made by `tempfile`;
  a terminal's program goes into its job object by its process handle, not its id, and each
  start carries `terminal.end_programs_on_close` itself (`LocalCommand`); a paste with a control
  character but a tab (Ctrl+C, Ctrl+U, Ctrl+W, a backspace, an escape) asks first, as one with a
  line break does; a shortcut's COM calls run on a thread of their own; an OpenSSH private key
  is parsed once.
- **Smaller audit findings in rc-ops, rc-checksum and the plugins** (RC-080-52, code audit
  2026-10-28): combine refuses listed part sizes that overflow (`io`) instead of wrapping; a
  local move's rename no longer replaces a file that took the free name after the conflict
  check (`already_exists`); a pack finds the archive it replaces in its disk's case, so the old
  `Archive.zip` never lands inside `archive.zip`; a file that grew while it was packed goes in
  with its counted size, is a `verify_failed` failure, and "move to archive" keeps it; a rename
  job's steps are held below their directory when submitted; tar.zst writes whole buffers into
  its pipe, the archive pipe reads into one buffer, the UTF-8 decoder puts together only the
  character a window cut; a secret source's http answer and a refused secret are wiped, and its
  refused port is decided once when the grant is made; `ChecksumError::io` names write failures.
- **A move on one disk never replaces what took the free name** (RC-080-56): the rename onto a
  free name refuses an existing target in the one call (`Vfs::rename_no_replace`; Linux
  `renameat2(RENAME_NOREPLACE)`, Windows `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`, a
  look first where the file system has neither); a file that appeared since the conflict check
  is asked about again, a folder is merged into, and "rename the existing target" never moves it
  onto a numbered name taken meanwhile.
- **The rest of the code audit in the window** (RC-080-53, code audit 2026-10-28): a plugin is
  installed only from the file its preview read - the preview reports the package's SHA-256,
  `plugins.install` takes it (`digest`) and refuses a file swapped since with
  `plugin_package_changed`; the colour dialog is owned by rCommander's window rather than the one
  in front; a panel that moved while the window missed events drops the old folder's message;
  the ssh_config import asks the vault for whichever key met a lock and drops the hosts of a
  failed preview; the Lister drops a picture asked for before a reload; Quick Connect refuses
  port 0; the Total Commander and RDM imports name a connection's kind as the form does; the
  progress title says Packing and Unpacking; blank lines of the colour rules are no rule on the
  tab either. Per frame: the colour rule editor, the SSH command output (read-only editors built
  once), the login script's prompt check, the pick lists (no copied options, the font lists
  built once), the connection tree (borrowed rows, one lowercasing pass for a filter), the
  Lister's last page (kept per layout) and the status line's numbers on a selection change (by
  the names that changed) no longer cost a pass or a copy each. The font database is locked
  only to find a face, and a colour look is reused instead of leaked anew. The focus-ring lint's
  new findings: the plugin version and the multi-rename preset lists are pick stops, the rest
  pointer-only rows and layers, marked.

## [0.8.5] - 2026-10-28

### Changed

- **FTP links looked up when used, a setting for the old way, the measurements of the cases**
  (RC-080-46): an FTP listing shows its links at once instead of asking the server about up to
  64 of them first; entering, opening, viewing or copying a link looks it up, and a link to a
  folder shows as one from then on in that session. The new setting
  `ftp.resolve_links_in_listing` (Transfers page, off) looks them up in every listing, after the
  transfer, as before. `scripts/perf.sh --cases` measures unpacking 10 000 small files from a
  solid 7z and a `tar.xz` and the viewer on a large file over the in-process FTP server.

## [0.8.4] - 2026-10-28

### Changed

- **One copy of each helper in the window** (RC-080-48, GUI half, code audit 2026-10-28): the
  vault question and its command (`app/vault_ask.rs`) for the connection manager, the database
  dialog's exchange and Quick Connect; one import page state for the ssh_config, Total Commander
  and Remote Desktop Manager imports; one question queue and trust view for the host-key and
  certificate dialogs (`app/trust.rs`); one list window for find files, synchronize dirs, the
  diff and multi-rename (`app/list_window.rs`); `Chord::is_plain` for the ~40 inline modifier
  checks; `settings::stored_bool` for the switches; one task stores a recent-entries list; one
  conflict preset for the copy and the unpack dialog; `rc_ops::Meter` measures the window's
  operations too. Behaviour unchanged, but the synchronize-dirs rows now keep their view full
  on a resize, as the other lists do.
- **Copies from the audit merged, backend half** (RC-080-48, code audit 2026-10-28): one Total
  Commander mask tokenizer (`rc_ops::mask::split_line`) for the selection, unpack, search and
  synchronize masks - `;` and white space separate everywhere, double quotes keep a mask
  together, a trailing `\` marks a directory mask, also in the search's include masks; one wire
  path, child join, home and tree helper for FTP and SFTP (`rc-remote`'s `wire` and `tree`),
  removing a remote tree now stops at 256 levels; one location helper for `ftp://` and
  `sftp://`; in `rc-ops` one pump-and-land step for copy and resume, one cancel race
  (`Control::or_cancel`) and every temporary name scheme in `rc_ops::temp_name`
  (`is_temp_name` knows them all); one key derivation on the blocking pool in `rc-vault`; the
  registry's groups listed once (`command_groups!`, which `scripts/mcp-coverage.sh` reads); each
  engine error code written once; the MCP tools' `confirmation` argument from one schema
  transform instead of 31 unread fields.
- **Unpacking, the viewer over FTP and the terminal without quadratic costs** (RC-080-46, code
  audit 2026-10-28): an archive keeps a reading session between reads - the reader and the
  format's index on a producer thread that walks a solid 7z block or a tar stream on - and
  unpacking reads the files in the archive's order, so a solid 7z or a `tar.*` unpacks in one
  pass and a zip's central directory is read once; a cancel stops the producer also while it
  still unpacks towards its entry. The limits (decoder memory, entries, streams) and the
  `.rcpart` staging hold as before. The viewer starts a read on FTP at its offset (`REST`), and
  a backward search over a backend that only streams reads the file once forwards. The terminal
  copies what a frame shows under the lock and draws without it, centres a wide glyph over its
  two cells, keeps combining marks with their glyph and shapes a lone ASCII glyph the basic
  way; the engine looks for shells and starts a terminal on the blocking pool. An FTP listing
  reaches the panel in batches as it arrives, its links last.

## [0.8.3] - 2026-10-28

### Added

- **Acceptance status, test list from the jobs, a small local CI** (RC-080-49, code audit
  2026-10-28): the status qualifier `Partial (Abnahme offen ...)` for a job with only acceptance
  boxes open - `scripts/archive-jobs.sh --check` refuses it where other boxes are open,
  `--acceptance` lists the candidates, `scripts/session-state.sh` counts them apart.
  `scripts/testlist.sh` makes the Windows test list from the jobs' open acceptance boxes and
  `--apply` ticks them back into the job files. A pre-push hook (`check.sh --preflight`) and a
  nightly systemd user timer (`scripts/nightly.sh`: `check.sh --full` in nextest's `ci`
  profile), installed only by `scripts/install-hooks.sh`. `check.sh` lints the plugins' guest
  code for wasm32 and, in the Windows lane, the libraries without `--all-targets`; a green
  `--full` on a clean tree stamps HEAD and `scripts/worktree.sh finish` warns without that
  stamp (`RC_FINISH_REQUIRE_GREEN=1` refuses). `RC_LOCK_HELD` carries the holder's PID and
  counts only from an ancestor holding the lock; script tests for the lock, `jobs.sh`,
  `worktree.sh` and the installer. `release-test` keeps line tables (the Windows test package
  carries `rcommander.pdb`); the sccache size and `target/` upkeep are in
  `docs/development.md`.

### Changed

- **Large selections and large batches without quadratic costs** (RC-080-45, code audit
  2026-10-28): a panel's selection is one copy-on-write set (`rc_engine::Selection`) that a
  command's answer, a cursor step and the window's copy of the state share; a refresh, a toggle
  and Ctrl+Shift+B look names up through one index of the entries instead of one search per
  name; `Engine::panel_path` reads a panel's folder alone. A batch of N jobs reads both panels
  again once, when its last job is over, and its jobs are measured once per operation. A
  progress event patches the job's row in the bar and the queue window
  (`rc_ops::Queue::job_progress`), and the queue window builds only the rows around its scroll
  offset. A cursor step or a selection change rebuilds no tab row; the file icons are asked for
  only when the rows on screen change. In `rc-ops` a transfer finds a subtree by the index range
  of its walk, the stream pair's buffers are made once per job, and the progress goes into the
  job's snapshot when it is published, not per buffer.

### Fixed

- **Bounded import parsers and engine bookkeeping** (RC-080-47, code audit 2026-10-28): the
  ssh_config import reads each file once (before, `Include f f f` nested eight deep read `f` about
  10^8 times), names at most 256 includes and takes at most 1024 hosts and as many pattern names,
  each host with at most 100 key paths and 64 ignored keywords; aliases are found through a map, and
  a line that can change no host any more is not applied again
  (`import/ssh_config/{include,options}.rs`, warnings `include_repeated`, `too_many_includes`,
  `too_many_hosts`). Total Commander's INI reader finds sections and keys through a map. One reader
  of "a regular file of at most N bytes" (`import/bounded_read.rs`) serves the ssh_config, INI, RDM
  and settings-export imports and the plugin packages; a package path naming a pipe no longer blocks
  a thread. Every connection write caps a name, host and user name at 255 characters
  (`text_too_long`) and a key path and start directory at 4096 (`path_too_long`), in `rc-db`'s
  validation. The drive list is not asked again while a list question hangs, its late answer is
  announced; unused SSH session slots leave the pool; the plugin execution records wait in a bounded
  queue (1024); a secret seen again moves to the back of the fingerprint list.
- **One name rule, the plugin key kept, the shell from System32** (RC-080-43, code audit
  2026-10-28): every name rCommander creates from untrusted input keeps one rule,
  `rc_rename::check_plain` - rc-ops' typed names, copies to Windows and unpacked entries, and now
  rc-checksum's combined file (also a `.crc` file's `filename=`), split parts and checksum files,
  which refused only separators and colons; a checksum run checks every listed entry before it
  joins it; a multi-rename name may not turn the reading direction (`rename_name_direction`).
  One list of invisible and direction characters (`rc_rename::invisible`) and one table of key
  markers (`rc_remote::key_markers`). A package for an installed plugin signed by another key is
  `KeyStatus::Changed` and refused (`plugin_key_changed`) before its key is trusted; the person
  removes the old plugin first. `limits.max_concurrent` is enforced: that many calls of a plugin
  run at once, one beyond waits. `cmd.exe` and Windows PowerShell come from
  `%SystemRoot%\System32` before `PATH`, and only absolute `PATH` entries count. The SSH login
  offers the connection's own key first and at most `ssh.agent_max_keys` agent keys (default 5).
- **Dialog races, background work and silent errors in the window** (RC-080-44, code audit
  2026-10-28): an archive's password question no longer replaces an open archive dialog or
  another job's prompt - it waits in its job, and the prompt opens for the oldest waiting job
  once the dialog closed (`archive_ops/password.rs`); F5, F6 and F8 fix the selection when their
  dialog opens; a new sync dialog stops the comparison of the one it replaces; closing the Lister
  stops its search, Enter while one runs keeps the bar; saving the settings restarts the MCP
  server also when a later change fails, Escape while saving leaves the server's state and the
  failure to the window (`settings/saving.rs`), a plugin's typed settings stay until stored; a
  closed network drive dialog's late answer leaves the next one alone; a Lister match inside a
  character marks that character; a stored window size is at most 16000 pixels; a search date
  ending at the epoch is invalid instead of an overflow; the context menu opens in its row's
  column of the brief view; a touchpad's small steps add up to rows (`theme::wheel`). Swallowed
  failures at start-up, of the drive layout, the drag folder, the picture decoder, the MCP
  policy and token, and five engine commands are logged.

- **MCP classes and guards in the engine, open limits closed** (RC-080-42, code audit
  2026-10-28): the class a call really has comes from its typed parameters in the engine
  (`Command::effective_class`, the `ESCALATIONS` table; `Engine::effective_class` adds a lowered
  retention limit and a saved queue list that overwrites) instead of six functions on raw JSON in
  `rc-mcp`; `CommandClass` has no derived order any more but an explicit `stricter()`, and the
  coverage page shows the stricter class. A write-guarded tool runs with both panels pinned
  (`Engine::pin_mcp_panels`): guard and command resolve against the same folders, and `ops_copy`
  and `ops_move` judge `sftp://` and `ftp://` targets where the job writes. A checksum run's read
  guard judges on the blocking pool. `queue_load` passes every place through the write guard, an
  entry with a sensitive source is not loaded and a database error names its entry. A terminal
  has at most 1000 rows and columns, a session at most 16 open confirmation questions (all at
  most 256), argon2id parameters below 19 MiB and two passes are refused in a release build. The
  instance file goes through `rc_db::write_private_file`, the data folder is owner-only on Unix,
  and a search walks the backend of its start folders. Owner's decisions: the write guard also
  refuses `.git/hooks`, `.gitconfig`, `fish/config.fish`, `.bash_logout`,
  `.config/systemd/user` and `.vscode/tasks.json`; the mask finds `key = value`, `key: value`,
  `"key": "value"` and names like `DB_PASSWORD`; `db_backup` keeps the newest `backup.keep`
  (1-1000, default 10) automatic backups and never removes a named one. A job an MCP call
  submits or loads is judged by the read guard again while it runs, each file right before it
  is opened - copies, moves and their resumption, packing, unpacking, the archive test,
  checksums, also after a restart: a file that became sensitive fails as `sensitive_path`
  (`rc-ops/src/read_guard.rs`, `JobOptions::read_guarded`).

## [0.8.2] - 2026-10-28

### Changed

- **The check chain without silent greens** (RC-080-41, code audit 2026-10-28):
  `scripts/check.sh --preflight` also runs `cargo deny --offline check`, the migration pins,
  `set-version.sh --check` and `licenses.sh --offline --check`; a missing shellcheck, gitleaks or
  cargo-deny, or a shellcheck other than the pinned one, fails `--full` (a visible skip under
  `--preflight`); the secret scan fails when its copy of the tree does; the tests run with
  `--no-fail-fast`. The package scripts check the version first and stop when a package of the
  same version from another commit or a dirty tree exists (`RC_REPACK=1`; `scripts/lib/package.sh`,
  test `scripts/tests/package.sh`); the live runs build `--locked`. The Bash guard counts clippy,
  `cargo xwin` and `live-tests.sh`/`perf.sh`/`soak.sh` as runs and `awk`/`sed`/`wc` as filters,
  lets `--version` and `nextest list` pass and names the eight self-locking scripts; the
  migration-number hook reads the edited path's repository.

### Fixed

- **An FTP connection stays in step after a cancelled call** (RC-080-40): a listing, command or
  transfer dropped half-way (a cancelled folder load, a stream dropped while it waited for
  `226`) gave its connection back with the server's reply unread, and the next command read that
  reply as its own. A lease is now marked as owed a reply before every command's wait and while a
  transfer is open, and closes its connection when dropped so (`ftp/pool.rs`, `Lease::pending`;
  the plain commands moved to `ftp/pool/commands.rs`). The home from `PWD` takes the rules of an
  SFTP home: a path with `.`, `..` or `\` gives `/` (`home_of`, `sftp::path::from_wire`), and
  a `PWD` that timed out or failed no longer leaves the first connection in the pool.

- **Import pages, secret-source search and Quick Connect's master password** (RC-080-39): the
  ssh_config, Total Commander and Remote Desktop Manager imports keep the paths their preview was
  read from and import from those; a changed path drops the preview, and a late preview answer is
  left out (`connections/preview.rs`). A secret source's search takes only the answer of its last
  search in its own source. Quick Connect asks for a new master password twice. The export and
  import dialog carries typed passwords only as `SecretString`.

- **`..` takes nothing, drops follow the divider, one fetch job** (RC-080-38): with the cursor on
  `..` and nothing selected, packing, unpacking, the checksum dialogs and multi-rename no longer
  work on the first entry (the engine's cursor) - they do nothing, as in Total Commander; one
  helper `Pane::chosen_names` (selection in listing order, else the cursor's entry, none on `..`)
  serves them, the context menu, the drag and Ctrl+C. A file dropped from the Explorer lands on
  its side of the panel divider (`resize.left_share`), not of the window's middle, and always in
  a maximized panel. A second fetch of remote entries while the session's temporary folder is
  being made asks for no second folder, so the fetch is copied once; a folder made besides is
  removed. Drop and fetch dedup and look-ups are linear. The archive password field is a
  `SecretString` from the first key (`app/drag/fetch.rs` now holds the fetch's `impl App`).

- **No silent loss when unpacking, copying out of an archive, moving or renaming** (RC-080-37,
  code audit 2026-10-28): a file out of an archive - unpacked or copied - goes through
  `<name>.rcpart` onto a disk or a server like every other copy, so a damaged entry or a cancel
  leaves the file it would overwrite as it was (staged, but neither resumed nor kept across a
  restart: `staging::is_staged` and `is_resumable` apart). A move whose source stream ends early
  fails the file and keeps the source (`verify.rs`); a restored move removes the sources of files
  it finds complete at their targets; a case-only rename whose undo fails reports
  `rename_undo_failed` with the temporary name, and a retry goes on from there; "rename" in a
  conflict tries at most 10,000 numbered names and stops on a cancel; a part file or unfinished
  archive that cannot be removed is logged with its path.

- **xz and 7z headers can no longer take gigabytes; the Total Commander import resolves only TC's
  variables** (RC-080-35): an xz block or a 7z coder whose dictionary passes the new setting
  `archive.decoder_memory_mib` (Settings, Folders; 16-4096, 256 by default) is refused as
  `archive_decoder_memory` before anything is reserved - sevenz-rust2 has no limit, so a 7z's
  coders are read from its header first (`rc-vfs/src/archive/sevenz_memory.rs`); a `%name%` in
  `wincmd.ini` outside TC's own list (`COMMANDER_PATH`, `APPDATA`, ...) is skipped as
  `unknown_variable` instead of copying an environment secret into the report
  (`import/total_commander_path.rs`).

- **An MCP question shows exactly what the allowed call runs** (RC-080-34): an exec or destructive
  call an argument of which the mask would change (`password=$(curl ...)`, a cookie or
  authorization header, a bearer token, credentials in an address) is refused with
  `invalid_params` before anyone is asked, and a value is measured unmasked; a call that takes
  the selection, the cursor's entry or a panel's folder is frozen before the question - the panel
  and the names written into its arguments, the panels it reads pinned - so selecting or moving
  the cursor while the question waits changes nothing, and a panel in another folder by then
  ends the call with the new code `panel_changed` (`rc-mcp/src/freeze.rs`,
  `rc-engine/src/mcp_pins.rs`).

### Security

- **Dependencies with security advisories updated** (RC-080-36, code audit 2026-10-28):
  rustls-webpki 0.103.16 (RUSTSEC-2026-0382, certificate verification of FTPS), crypto-bigint
  0.7.10 and ctutils 0.4.6 (constant-time fixes under russh), clipboard-win 5.4.3 (yanked);
  `cargo deny --offline check` is green. zstd 0.13 stays under wasmtime's compile cache (no
  wasmtime release on zstd 0.14), ignored in `deny.toml` with its reason, as is the unmaintained
  rustybuzz of the SVG viewer plugin.

## [0.8.1] - 2026-10-27

### Added

- **Terminal and Connect from the connection manager's tree** (RC-050-15, owner 2026-10-27):
  Shift+Enter opens a terminal on the SSH server at the cursor, Ctrl+Enter connects to it, in the
  tree and in the filter, without walking to the buttons under the form (`connections/keys.rs`,
  `connections/remote.rs`).

### Fixed

- **No see-through line under the dark menu bar on Windows 11** (RC-080-18, owner 2026-10-27):
  the one-pixel line muda paints between the bar and the panels at `WM_NCPAINT`/`WM_NCACTIVATE`
  went onto the window with alpha 0, so the window behind showed through. The menu bar's subclass
  now paints it over opaque in the bar's colour (`platform/windows/menu_bar.rs`, `opaque_line`).

## [0.8.0] - 2026-10-26

### Added

- **`d:` in the command line follows the other panel on the panel's own drive too** (RC-010-29),
  on the owner's wish: with both panels on `D:`, `d:` opens the other panel's folder, as in Total
  Commander; the drive bar and the drive menu still leave a panel on its own drive where it is.
  `Engine::change_drive_to(panel, drive, typed)`, `cmdline.run` passes `typed`. New setting
  `drives.follow_on_own_drive` (Folders, on; it needs `drives.follow_other_panel` on; off: as in
  0.7.0).
- **The connection manager starts in its list, and duplicates a connection** (RC-050-15), on the
  owner's wish: Ctrl+F puts the keyboard in the tree on the connection used last - the row the
  cursor was on when the manager closed in this run, else the one the window logged in to last
  (`connections.last_used`, written by `Engine::login_connection` for the window's calls, never
  for MCP; `ConnectionTree::last_used` in `connections.list`), else the first row - with its
  folders opened; a character typed there goes into the filter. New setting
  `connections.focus_list` (Keyboard page, on; off: the filter, as before). New command
  `connections.duplicate` (`GuiOnly`, no MCP tool): a copy right under the original as
  `<name> (2)` (the next free number; the copy of `web (2)` is `web (3)`) with every field, the
  login script, the shared credential and its own secrets, decrypted and sealed again for the
  copy's row in `connections.save`'s one transaction; a locked vault asks the master password and
  duplicates again. Not copied: the drive-bar mark, session preferences, secret-source references.
  In the manager: Shift+F5 and a *Duplicate* button; the copy's name opens to change.

## [0.7.0] - 2026-10-26

### Added

- **Linux package for 64-bit ARM.** `scripts/package-linux.sh --arch arm64` cross-compiles for
  `aarch64-unknown-linux-gnu` with Debian's `gcc-aarch64-linux-gnu` (linker in
  `.cargo/config.toml`, target in `rust-toolchain.toml`) into
  `artifacts/linux/rcommander-<version>-linux-arm64.tar.gz`; `--arch x64` stays the default.
- **Total Commander's colours and list font come with its import** (RC-050-14), on the owner's
  wish: `import.total_commander` has a fourth part `look` (`TcParts::look`, on; MCP parameter
  `look`) that reads `[Colors]` into the light look and `[ColorsDark]` into the dark one (text,
  background, second background, cursor, cursor text, marked names; Total Commander's decimal
  `COLORREF`, `-1` and `ThemedCursor` left alone), its colours by file type (`ColorFilterN`,
  appended to the rules there, a known mask replaced in place, saved-search filters skipped, at
  most 64), and the file lists' font of `[AllResolutions]` (points times 4/3 as rCommander's
  size). The preview lists each as a row of its own (`TcLookRow`, keys `colour.<look>.<token>`,
  `rule.<N>`, `font.list`); the import writes through `settings.set`. New skip codes `inverse`,
  `colour_invalid`, `search_filter`, `too_many`.

### Changed

- **Packing, unpacking, checksums, synchronizing and multi-rename run at once** (RC-010-28), on
  the owner's wish after F7 waited behind a copy: `archive.extract`, `archive.pack`,
  `checksum.queue`, `multi_rename.apply`/`undo` and the sync jobs (`Engine::sync_jobs`) go in
  `AT_ONCE` - outside the queue's lists, in no lane - instead of the default list, where they
  waited behind every copy to the same drive. A sync's jobs run as one batch, one after the
  other, so its order holds. New setting `queue.tools_at_once` (on); off, they are queued as
  before (`Engine::tool_list`).

## [0.6.1] - 2026-10-25

### Added

- **Paste a clipboard image into a terminal as a file path** (RC-040-08), on the owner's wish to
  give Claude Code a screenshot, also on an SSH server, where Claude Code reads the server's
  clipboard: the new command `terminal.paste_image` (Ctrl+Alt+V, `"when": "always"`, changeable
  in Shortcuts; `GuiOnly`, no MCP tool) has the window read the clipboard's `CF_DIB`, turn it into
  a PNG (`platform::clipboard_image`, the `image` crate) and `Engine::place_terminal_image` write
  it where the terminal's program reads it - the temp folder for a local shell, the same file under
  `/mnt/...` for WSL, `/tmp` on the server over SFTP for an SSH shell, without asking - and the
  path is pasted (quoted when it holds a space). The files go when the terminal closes or its
  program ends; local ones a crash left go a day later. New setting `terminal.paste_image` (on),
  `EngineEvent::PasteImage`. Windows only for now. `/tmp` on a server is shared: an image's name
  holds 128 random bits, the file is made exclusively (a file or link put there first stops it)
  and set to `0600` before the picture goes in (`rand`, already in the workspace, now in
  `rc-engine`).

### Changed

- **Version 0.6.1**: pasting a clipboard image into a terminal as a file path (RC-040-08) gets a
  package of its own, for Windows and Linux. `scripts/set-version.sh 0.6.1`, the first version it
  cut a changelog section for; the packages are `rcommander-0.6.1-windows-x64.zip` and
  `rcommander-0.6.1-linux-x64.tar.gz`.
- **The changelog has version sections** (owner 2026-10-25): `scripts/set-version.sh X.Y.Z` cuts
  what `[Unreleased]` holds into `## [X.Y.Z] - <date>` below a new, empty `[Unreleased]`
  (`scripts/lib/changelog.sh`, test `scripts/tests/changelog.sh`); a version already cut is left
  alone. Cut by hand once for 0.6.0, which holds everything before it. Rule in `AGENTS.md`.

## [0.6.0] - 2026-10-25

Everything up to and including 0.6.0: the versions before it were not cut into sections of their
own; from here on `scripts/set-version.sh` cuts `[Unreleased]` at every version.

### Added

- **Copy from a terminal as in Windows Terminal** (RC-040-07), on the owner's report that
  neither Ctrl+C, a right click nor selecting copied anything (only Ctrl+Shift+C did): Ctrl+C
  copies selected text and clears the selection, without a selection it still interrupts; a
  right click copies a selection and clears it, without one it pastes (Windows); copy on select
  is optional. New settings `terminal.copy_ctrl_c` (on), `terminal.copy_right_click` (on) and
  `terminal.copy_on_select` (off), Terminal page, not protected; `rc_term_widget::CopySettings`,
  read by the window (`app/terminal_copy.rs`) and handed to every terminal. No new command or MCP
  tool: the settings go through `settings.get`/`settings.set`.
  Ctrl+V pastes the clipboard's text as Windows Terminal does, on the owner's report that Claude
  Code started in a terminal answered Ctrl+V with "No image found in clipboard": the widget sent
  it as `^V`, which PowerShell turns into a paste itself but Claude Code takes for an image
  paste. With no text on the clipboard Ctrl+V still goes to the program. New setting
  `terminal.paste_ctrl_v` (on), `CopySettings::ctrl_v`.
  Copying inside Claude Code (which takes the mouse) reached no clipboard: a program's OSC 52
  copy (`Event::ClipboardStore`) was dropped by the widget's change stream, and Shift with the
  mouse still went to the program. Now OSC 52 copies reach the clipboard (reading it that way
  stays refused) and Shift with the mouse selects, as in Windows Terminal. New settings
  `terminal.osc52_copy` (on) and `terminal.shift_selects` (on); `rc_term_widget::Event::Copy`.

- **MCP: executing calls allowed for a while on one terminal or connection** (RC-010-27), on the
  owner's wish not to be asked 20 times for 20 commands: the question of `terminal_send_input`
  and `ssh_exec` offers "Allow here for N min", bound to the MCP session, the tool and the
  `terminal` or `connection_id` the call names; another terminal, server or session asks again,
  other executing tools and destructive ones always ask (the engine refuses an offer the class
  does not allow). New setting `mcp.ask.exec_session_minutes` (MCP page, 30, 0-480, protected;
  0 never offers it). The grants moved from `rc-mcp` into the engine (`mcp_grants.rs`, at most
  256): `Engine::mcp_grants`, `EngineEvent::McpGrantsChanged`; the status bar shows
  "MCP: N allowed without asking", a click takes them all back with the new `GuiOnly` command
  `mcp.revoke` (also in the command palette; coverage: omitted). `McpQuestion` and `McpAsk` carry the offer
  (`McpSessionOffer`). `CursorView` moved to `state/panel_id.rs` (500-line limit).
  `Engine::dispatch` boxes the future of its one `match`: in a debug build it sat inside every
  caller's future, and the new command pushed a harness test past the 2 MiB stack of its thread
  (`a_key_removed_elsewhere_leaves_the_table_at_once` needed 1984-2048 KiB before, 1536-1792
  KiB boxed).

- **Terminal programs end with the terminal and with rCommander** (RC-040-06), on the owner's
  report that shells outlived a closed rCommander: on Windows a local or WSL terminal's program
  goes into a job object of its own (`rc-term/src/local_pty/job.rs`) with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing the terminal ends the job, and Windows closes the
  handle when rCommander ends - also after a crash or from the task manager. New setting
  `terminal.end_programs_on_close` (Terminal page, on): off, the job lets what the shell starts
  break away, so only the shell ends. New `unsafe` exception in `rc-term` (`windows-sys` job
  functions); no new command or MCP tool.

- **Login-script editor, clearer** (RC-050-13), on the owner's question where sudo's password
  goes: a Send secret step chooses its secret in a pick list (the connection's password or one
  stored with the script); a hint says where to store another one while none is; a warning marks a
  Send secret whose step before is no Wait for a password prompt (`Draft::secret_warning`,
  sample prompts in `PASSWORD_PROMPTS`) - the secret could be typed into the shell. New setting
  `login_scripts.warn_secret_prompt` (Terminal page, on, not protected). The steps are numbered
  cards joined by a line beside each card, their kinds in one column, "Press Enter after" a
  checkbox, Up/Down arrow icons (`layout::glyph_button`, two new glyphs in the icon font) disabled
  at the ends; Save, Delete and Close are one button row at the bottom (Delete at the
  left). No new command or MCP tool.

- **Menu bar on Linux** (RC-080-33), on the owner's report that Linux had no way to the settings:
  where no system menu is attached (Linux, macOS until its own), rCommander draws the menu bar
  itself - the same menus `menu_bar::build` makes for the Win32 menu, a choice the same
  `MenuBarMsg::Chosen`. Titles above the drive bar, the open menu a card like the context menu;
  Alt with a title's letter opens it, the arrows, Home/End, Enter or an entry's letter, Escape and
  F10 work as in a Win32 menu (Alt alone does nothing). Follows `appearance.menu_bar` and
  `keys.context_menu_shortcuts`; no new command or MCP tool.

- **Space keeps the cursor** (RC-020-13), on the owner's request: setting
  `selection.space_moves_down` (Folders page, on by default). Off, Space flips the selection of
  the entry under the cursor (and counts a folder) without moving on; Insert always moves on.
  `panel.select_and_size` reads it; no new command or MCP tool.
- **Shared credentials** (RC-050-12), after Remote Desktop Manager: an entry of the connection
  tree with a user, password, private key and passphrase, kept once; any number of connections
  point at it, so a password changed once is every one's. Migration 0018: table `credentials`
  (durable) with the secret columns of a connection, sealed for `"credentials/<column>/<id>"` and
  listed in `SECRET_COLUMNS`, so a new master password re-encrypts them in the same transaction;
  `connections.credential_id` (`ON DELETE SET NULL`). `resolve_secret` follows the link before
  the connection's own secrets and sources; a connection without a user of its own logs in as
  the entry's (an FTP password login still names its own). New commands `credentials.save` (fields and secrets in one
  transaction), `credentials.move`, `credentials.delete`; `connections.save` takes `credential`;
  `connections.list` lists the entries per folder; `TreeItem::Credential`; code
  `credential_not_found`. The manager shows them with a diamond, "New credential" under the
  tree, a form of name, user and secrets, a "Credential" choice in a connection's form and a
  delete question that counts the connections using it. Protected setting
  `vault.shared_credentials` (on). MCP: `credentials_save`, `credentials_move`,
  `credentials_delete`; an agent never stores an entry's secret, changes its user or points a
  connection at one (`gui_only`), and a connection on an entry keeps its endpoint from an agent
  (`endpoint_change_needs_gui`). Not yet: RDM's credential entries in the RDM import, and the
  link in the configuration export.

- **The Windows Credential Manager as a secret source** (RC-100-37), free and built into the
  core: under a stored connection's form "Secret source" offers "Windows Credential Manager"
  beside the vault and the plugins. A search lists the generic credentials by target, user and
  comment (never the blob, `CredEnumerateW`); a connect reads the mapped target's blob
  (`CredReadW`, UTF-16LE or UTF-8 told apart by its NUL bytes) into a `SecretString`, the
  buffer overwritten before `CredFree`. Read only; creating and changing entries stays with
  Windows. New protected setting `vault.wincred_source` (on); other platforms offer no such
  source and answer `secret_source_missing`. New code `secret_source_entry_missing` for a ready
  source that lacks the entry (also a plugin's `not_found`). `windows-sys` gains
  `Win32_Security_Credentials`.

- **Masked plugin settings and licence keys** (RC-100-36). A plugin's text setting can be
  `masked = true`: the Plugins page types it hidden, `plugins.list` over MCP answers `[redacted]`
  for its value (`PluginSettingInfo::masked`). The Plugins list marks a secret source that does
  not answer (locked, not answering, no licence, failed; `secrets.sources`). The licence check
  itself lives in the paid plugin: a private crate (Ed25519-signed offline keys, `RCL1.` lines,
  issued with `rclicence`); the Bitwarden plugin 0.2.0 checks it and answers `unlicensed`
  without a valid key. The core checks no licence.

- **The Bitwarden plugin** (RC-100-35), in a private repository of its own (not in this one,
  not GPL): a secret source over the Bitwarden CLI's local API (`bw serve` on `localhost:8087`) -
  passwords, SSH key items' private keys, TOTP codes; a search keeps ids, names, users and the
  first URI only. Tested against Vaultwarden in Docker and end to end through the engine's plugin
  host. No licence check yet (RC-100-36). Wiki `using/plugins.md`, section Bitwarden.

- **A connection's secret from a password manager** (RC-100-34). A stored connection's password,
  private key or passphrase can come from an installed secret-source plugin (RC-100-33) instead
  of the vault: under the form, "Secret source" offers "Vault" or each source with its state; a
  source opens a search (titles, users, addresses), a picked entry is mapped at once. Only the
  reference is stored (migration 0017, `secret_sources`, durable); `resolve_secret` asks it before
  the vault, at every connect, the value on the scrub list. A source that fails is named:
  `secret_source_locked`, `_unreachable`, `_unlicensed`, `_failed`, `_missing`. New commands
  `secrets.sources` and `secrets.source_search` (Read, MCP `credential_sources_list`,
  `credential_sources_search`, never a value) and `secrets.source_map`, `secrets.source_unmap`
  (`GuiOnly`). A mapped connection's endpoint changes only in the window; the MCP server's port is
  refused to every source. An export carries the references (`sources`), never a source's value;
  an imported row with references waits for a tick (`secret_sources`).

- **A rule by file type colours the extension too** (RC-100-43; owner, 2026-10-22: "bei einem
  colours by file type sollte auch der filetype/extension dann auch die farbe bekommen nicht nur
  der name selbst"). In the full view the extension column takes the rule's colour with the name;
  sizes, dates and a marked row keep theirs (`file_list/rows.rs`, `value_colour`).

- **The colour dialog on the Colours tab** (RC-100-43). On Windows "Choose..." beside every colour
  and "..." at each rule's light and dark colour open the system's colour dialog
  (`ChooseColorW` through winsafe, `platform/colour_picker.rs`) on the colour in force: the basic
  colours, the spectrum and sixteen custom colours kept for the session. It runs on a thread of
  its own, owned by the window in front; the colour chosen goes into the field or the rule. Other
  platforms keep typing `#rrggbb`. The tab's messages move to `colours_page.rs`.

- **Colours as in Total Commander** (RC-100-43). Settings, Appearance, tab *Colours*: per light
  and dark look the lists' text, background and second background, the cursor in both panels and
  its text, marked names and the accent, each a swatch, `#rrggbb` (empty: the look's own) and
  Reset, with a preview of the list and a warning for every pair below WCAG AA (4.5:1). Colours by
  file type: rules of Total Commander masks with a light and a dark colour, the first match
  colouring the name (`appearance.colour_rules`), moved up and down, added and removed in the tab.
  The colours apply to the panels and lists only (new tokens `list_fill`, `list_text`,
  `list_text_secondary`, `cursor_text`, `marked`), never to the dialogs. New setting kinds
  `colour` and `colour_rules`; the settings list moves to `schema/list.rs`; `rc_engine::Mask`
  re-exports the selection masks.

- **A more readable queue window, F2 for the background** (RC-010-26). A running job is marked by
  an accent stripe and its state in the accent; the file it is on stands under its path, and its
  bytes, speed and time left at the right edge. With more than one job running, the line under
  the list shows how many run and their speed together (setting `queue.total_speed`, on). The
  lane column is left out while every job shares one lane (`queue.hide_single_lane`, on). F2 in a
  job's progress window sends it to the background like Esc; the button reads "Background (F2)".
- **Roadmap: colours as in Total Commander** (RC-100-43): own colours for the file list, cursor and
  marks per theme, and colours by file type.

- **Secret sources in the plugin contract** (RC-100-33). Contract `rcommander:plugin@0.3.0` adds
  the world `secrets-plugin` (`status`, `search` without values, `get` of one secret) and the
  interface `http`, granted to a secret source as one port of this machine
  (`net_http = { loopback = <port> }`): plain HTTP to `localhost`, `127.0.0.1` or `[::1]` on that
  port only, never rCommander's MCP port, no redirect followed, the answer capped. The host's
  `SecretsPlugin` runs each call in a fresh store, validates entries and hands the value on as a
  `SecretString`; the install preview names the port, so an update that moves it asks again.
  `rcommander plugin new --type secrets` writes a working secret source, and conformance asks it
  `secrets_no_service`. The host still runs plugins built against 0.1.0 and 0.2.0; the bundled
  viewers are rebuilt against 0.3.0 with a patch version each and no other change. rCommander
  does not ask a secret source yet (RC-100-34).

- **Access keys in the remaining dialogs** (RC-080-16). Alt with the underlined letter now also
  works in the properties, pack and unpack, checksum, database, connection manager and import,
  login-script, queue, find files, compare and synchronize, map network drive, hotlist and
  quick connect dialogs, on the copy dialog's options and on the settings' MCP, About, Fonts,
  Plugins and Keyboard > Shortcuts pages (the search, Change key, Add key, Remove key, Reset,
  Reset all; while a key is recorded, every letter is the recording's). A checkbox toggles on its letter, and the letter of a caption gives its text
  field, pick list or option group the keyboard, as in Windows. Building blocks:
  `theme::layout::check`, `caption`, `keyed`, `marked_icon_button`, `focus::Stop::field` and
  `Stop::pick`; the connection manager follows the ring after an access key as after Tab.

- **Linux package script.** `scripts/package-linux.sh [--profile release-test]` builds the
  bundled plugins and `rcommander` natively and packs them like `package-windows.sh` into
  `artifacts/linux/rcommander-<version>-linux-x64.tar.gz`.
- **A plugin's picture and text in one document** (RC-100-29). A viewer plugin's document may
  offer `image` beside its text; **7** and the list in the title row switch between them, the
  picture with the built-in image view's zoom, fit, rotation and background. Each output has a
  temporary file of its own and its own limits; the first one is written before the Lister or
  Quick View shows the file, the other only on the first switch to it, off the UI thread, with
  the plugin's session kept open meanwhile. A second output that fails is a notice and its view
  goes; the first goes on working. A raw `rgba:<w>x<h>` raster is a picture now (its length
  checked against its size, scaled down beyond `viewer.picture_megapixels`), so the bundled SVG
  viewer shows its drawing first and the source on **7**. `PluginDocument::output_path` is
  `text_path`, with `picture`, `write_text` and `write_picture` beside it.

- **Keyboard shortcuts in the settings** (RC-100-21). The Keyboard page has two tabs: General
  (F10, access keys, the menus' keys) and Shortcuts, Total Commander's hotkey editor: every key
  in one table (action, key, where it applies, changed or locked) with a search by name or key;
  Change key and Add key take the next key pressed, Remove key, Reset and Reset all. A key
  another row holds asks before it moves; the keys the window keeps - Tab, Escape, Alt with a
  letter or digit, the `"window"` rows - are refused with the reason, shown in a tooltip.
  Only the deviations from the standard are stored, in the protected setting `keys.overrides`
  (`Ctrl+K=copy F5=`, no migration), so later standard keys still arrive and a row a newer
  version no longer has is left out; they go with the settings of `config.export` and come
  back unticked with `config.import`. The engine builds the keymap in force
  (`Keymap::with_overrides`, `Engine::keymap`); new commands `keymap.list` (Read, MCP
  `keymap_list`) and `keymap.set` (`GuiOnly`). The window reads it at the start and after a
  change: the menu bar is relabelled, the palette, the context menus and the function-key bar
  show the keys in force, a binding without a key stays in the palette, and a context-menu
  entry follows its moved key.

- **Plugin settings in the contract, `rcommander:plugin@0.2.0`** (RC-100-14). A new interface
  `settings` (`setting(name) -> option<string>`, after rDownloader's `destination-settings`) in
  the viewer world: a plugin reads its own `[[settings]]` - the person's value from the Plugins
  page while the manifest accepts it, else the default; `none` for an undeclared name. The
  engine hands the stored `plugin.<id>.<key>` values to each viewer when it loads them
  (`ViewerOptions::settings`), so a change holds from the next file opened. A component may
  import `settings` only under 0.2.0 with `[[settings]]` declared, refused otherwise when
  packaged, installed and loaded. `SUPPORTED_API_VERSIONS` is `0.1.0`, `0.2.0`: the host links
  the interfaces and finds the `viewer` export under the manifest's version, so a 0.1.0 package
  still installs and runs; the import check compares the version too. The JSON viewer 0.2.0
  reads `format` (`pretty` indents), the SDK's viewer template reads its new `separator`
  setting; `ViewerHost::setting` and `MemoryHost::settings` for native tests.

- **The bundled shell script viewer** (RC-100-18): `plugins/shell-viewer/`
  (`rc-plugin-shell-viewer`, contract 0.2.0 without `settings`) for `.sh`, `.bash`, `.zsh`,
  `.ksh`, the dot files `.bashrc`, `.bash_profile`, `.profile`, `.zshrc`, and scripts without an
  extension by 16 shebang magics, `probe` declining a shebang that names no shell. `highlighted`
  only: keywords and `[[ ]]`, builtins and function names, variables and assignment names,
  strings with their expansions coloured inside, heredocs (`<<EOF`, `<<-EOF`, `<<'EOF'`) with
  their marker, numbers, comments and the shebang, operators and redirections, brackets, and a
  quote left open at the end as `invalid`. The line index keeps the lexer's state per
  checkpoint - open quotes, `$(`/`${`/`$((`/backtick/`case` frames, the heredoc and its marker
  hash, a `\` continuation - so `tokens` starts at any of them; invalid UTF-8 is written as
  U+FFFD with token offsets counting the text written. One streamed pass, an index of at most
  8 MiB; tested natively up to 8 MiB, 100 MiB as an ignored test.
- **The bundled YAML viewer** (RC-100-16): `plugins/yaml-viewer/` for `.yaml` and `.yml`, built
  as the JSON viewer is (one streamed pass, a bounded line and structure index, no dependency).
  `highlighted` colours keys, strings and block scalars, numbers, literals, comments, document
  markers and directives, anchors, aliases and tags, indicators and errors; a lexer state per
  checkpoint (block scalar, quoted scalar, flow depth) lets `tokens` start at any checkpoint.
  `tree` unfolds block mappings and sequences by indentation, shows a flow collection, block
  scalar or alias as one shortened value, several documents as `document n`, never resolves
  anchors or aliases, and names the first indentation error or tab in the indentation with its
  line and column. Contract `rcommander:plugin@0.2.0` without `settings`.
- **The bundled CSV viewer** (RC-100-17): `plugins/csv-viewer/` (`rc-plugin-csv-viewer` 0.1.0)
  for `.csv`, `.tsv`, `.tab` and `.psv`, built as the JSON viewer is (one streamed pass, a
  bounded line and record index, no dependency). `highlighted` gives every column its own colour
  - the eight classes `key`, `string`, `number`, `keyword`, `type-name`, `function`, `literal`,
  `operator` in turn - separators `punctuation`, a broken field (text after the closing quote, a
  quote never closed) `invalid`; the line checkpoints keep the quoted state and column, so a
  field over several lines keeps its colour. `tree` lists `row <n>` per record with its first
  fields, unfolded into the fields named by the header (`column <n>` without one), in ranges past
  1000. Settings `separator` (`auto`: the extension, else the separator found equally often on
  every line of the first 32, else the most frequent), `header` (`auto`: no field of the first
  record a number) and `encoding` (`auto`: UTF-8 when valid, else Windows-1252, the table in the
  plugin); the text handed over is always UTF-8. Contract `rcommander:plugin@0.2.0` with
  `settings`.
- **XML viewer as a bundled plugin** (RC-100-15): `plugins/xml-viewer/` (`rc-plugin-xml-viewer`,
  contract 0.2.0) for `.xml`, `.xsd`, `.xsl(t)`, `.rss`, `.atom`, the MSBuild/NuGet files,
  `.resx`, `.xaml`, `.config`, `.manifest` and `.plist`. `highlighted`: element and attribute
  names, values, CDATA, comments, PIs/DOCTYPE, entities and punctuation as token classes, a
  construct over several lines coloured as one (the line index keeps the lexer's mode per
  checkpoint); the setting `format` (`as_is`/`pretty`, an element a line). `tree`: a node per
  element (start tag cut, the text of a leaf as its value), ranges of 1000, the first
  well-formedness error as a node with line and column. No DOM: one streamed pass, an index of at
  most 16 MiB; tested natively up to 8 MiB, 100 MiB as an ignored test.
- **Plugin SDK, `rcommander plugin ...` and the bundled JSON viewer in the package** (RC-100-12,
  with RC-100-11's guest binding; stage 1 of `docs/plugins.md`). `sdk/`: the viewer template (CSV
  and TSV as a Markdown table, text as the fallback, its tests run by `cargo test`), `README.md`
  and the CI workflow `ci/plugin.yml`, MIT. `rcommander plugin new --type viewer` writes the
  template - compiled into the executable - with its own UUIDv7, Ed25519 key pair and a
  `.gitignore` that keeps the key out; `package` (with `--defaults` for the shared fields of
  `plugins/metadata.toml`), `verify`, `conformance [--json]` (the package, the world and the
  question `viewer_empty_source`; exit 1 on a failure) and `keygen`, over `rc-plugin-host`'s new
  `check_package` and `complete_manifest`. The JSON viewer's `src/guest.rs` (wasm32 only,
  wit-bindgen 0.62); its failure code moved under its id. Scripts `build-plugins.sh` (component,
  import check, signed package with the development key from outside the repository,
  conformance), `check-sdk-templates.sh`, `wit-reference.sh`, `check-plugin-imports.sh`,
  `plugin-manifest-defaults.sh`; the three compile-free checks in `check.sh --preflight`;
  `package-windows.sh` puts the signed packages into `plugins/` of the zip; `set-version.sh` keeps
  the SDK's `min_app_version` and CI pin.
- **Plugin packages, signatures, installation and the Plugins page** (RC-100-09; stage 1 of
  `docs/plugins.md`). `rc-plugin-host` reads and writes `.rcplug` packages (manifest, component,
  Ed25519 signature over the length-prefixed digest, locales), verifies them against the
  compiled-in development key `rcommander-dev-v1` and keys confirmed on first use (a `key_id`
  pinned to its key), refuses revoked keys, withdrawn digests and imports the manifest does not
  grant, installs atomically into `<data dir>/plugins/<id>/<version>/`, installs the bundled
  packages from `plugins/` beside the executable, sweeps leftovers at the start, keeps versions
  with rollback and holds a version an open viewer uses (`installed::list`, `installed::hold`);
  library functions for `rcommander plugin keygen|package|verify`. Migration 0016: `plugins`,
  `plugin_keys`, `plugin_digest_revocations` (durable) and `plugin_executions` (retained, written
  by every plugin call). Commands `plugins.list` (MCP `plugins_list`) and the GUI-only
  `plugins.preview`, `install`, `enable`, `disable`, `rollback`, `resume`, `remove`. Settings ->
  Plugins: the plugins with their state, Install... with a preview, versions, own settings and
  log; protected settings `plugins.enabled`, `crash_limit`, `viewer_output_limit_mib`,
  `associations`, `development_mode` (and `viewer_token_colors`, moved here). Threat model
  `docs/security/plugins.md`.
- **Lister and Quick View open files with their viewer plugin** (RC-100-13; stage 1 of
  `docs/plugins.md`). `rc-engine`'s new `plugins/` adapter (after rDownloader's
  `PluginSet`/`ClaimedPlugins`) loads the installed, enabled viewer plugins
  (`rc_plugin_host::installed`, with the engine's execution log and each plugin's shared health)
  when a viewer first needs them and again after a `plugins.*` setting changed, holds a plugin's
  version folder while its document is open, claims a file by `[match]` without
  running a plugin (`plugins.associations` first, images and Markdown built in unless assigned,
  then the first plugin by name), and opens it off the UI thread: `probe`, `open` with the
  window's theme, the text or picture into a session file (capped at
  `plugins.viewer_output_limit_mib`, removed with the document) that the Lister reads as any
  file, its lines counted while written so the window's line starts become the line numbers
  `tokens` asks by. The source streams from any backend through the viewer's file window.
  Sensitive files - what `rc_mcp::sensitive_path` refuses, or a private key in the head - never
  reach a plugin. Each Lister carries a stop flag raised when it goes, so Quick View moving on
  or closing the Lister ends the plugin's running call (`should-stop`, then the epoch). A
  crashed, timed-out or suspended plugin leaves the built-in viewer with a notice; F2 opens a
  plugin's document again through its plugin. `error.plugin_*` texts for every host code.
  `rc-plugin-host` exposes its test bed behind a `testbed` feature (with a `source.read-at`
  echo) for the adapter's tests. No new command; `docs/mcp-coverage.md` unchanged.
- **Viewer: coloured text, a tree and several views per document** (RC-100-10; owner,
  2026-10-20). Lister and Quick View draw a viewer plugin's document against a GUI-side model
  (`app/viewer/document.rs`) that RC-100-13 fills from the plugin runtime: views `Text`,
  `Highlighted`, `Markdown`, `Image`, `Tree`, the first shown; `7` and a list of views in the
  title row switch them. Coloured text lays token spans (WIT `token-class`, UTF-8 bytes in their
  line) on the Lister's own text through an iced highlighter, so wrap, search, selection, copy
  and the text size stay; spans are asked only for the lines on screen, once each, never waited
  for, and invalid ones are dropped. The tree unfolds by click and by Right/Left as in the
  Explorer, its children asked once, bounded in depth, nodes and text. Theme tokens `syntax.*`
  (Light+/Dark+, 4.5:1 on `surface`); setting `plugins.viewer_token_colors` (on, Appearance) in
  the new protected schema `plugins.*` - `settings.set` refuses every `plugins.*` and
  `plugin.*` key. No new command; `docs/mcp-coverage.md` unchanged.
- **Text size in the viewer with Ctrl+wheel, Ctrl++ and Ctrl+-** (RC-020-12; owner,
  2026-10-20). Lister and Quick View make the text, binary, hex, DOS and Markdown views a step
  larger or smaller (6 to 72 pt; 1 pt up to 16, wider above), Ctrl+0 goes back to the viewer font
  size; Ctrl++ is also Ctrl+=, Ctrl+Shift+= and Ctrl+NumAdd, Ctrl+- also Ctrl+NumSubtract. The
  size stays with its window through N and P and with Quick View as its cursor moves; the top
  line stays on top and the preview keeps its place. In the image view the same keys and
  Ctrl+wheel zoom the picture, Ctrl+0 fits it. The status line names the size. Settings
  (Appearance): `viewer.text_zoom` (on) and `viewer.text_zoom_keep` (`window`, or `setting` to
  store the size as `viewer.font_size`, which now reaches 6 to 72). The `+` key of the main
  block is the chord key `Plus`, so Ctrl++ works on a German layout. No new command; MCP reaches
  the settings through `settings_get`/`settings_set`.
- **Plugin interface design, approved** (RC-100-01; owner, 2026-10-19).
  `docs/plugins.md` designs rCommander's plugins for every Total Commander category (lister,
  packer, file system, content) and beyond (connection types, secret sources, commands, compare)
  on rDownloader's plugin system, copied and adapted: signed WebAssembly components, one WIT
  world per kind, no WASI and no file system, capabilities only from the manifest, bytes through
  host-provided sources and sinks; a native host process only for TC's DLLs. With what is taken
  over from where, the answers to the limits of components, signing, lifecycle, MCP, the SDK with
  one working scaffold per kind, the new dependencies and a staged plan with its tests.
  `docs/adr/0003-plugins-wasm-and-a-tc-host.md` records the owner's choice (accepted). No code yet.
- **Names sort like Windows, like Linux or by the panel's target** (RC-010-02; owner,
  2026-10-19). `sort.names` (Settings, Folders): `windows` - Windows' word sort as Total
  Commander shows it, the default -, `linux` - the byte order of `ls` in the C locale, capitals
  first -, or `by_target` - Windows' on this Windows machine's drives, Linux' on SSH and FTP
  servers, inside WSL and in an archive lying there (`sort::names_for`). `sort.numbers_by_value`
  (on by default) compares runs of digits by their value in both. The engine holds both for the
  process (`sort::set_names`), reads them at the start and sorts both panels again when one
  changes.
- **Tabs are dragged to another place, and the tab menu shows its keys** (RC-010-24; owner,
  2026-10-19). A tab dragged within its tab bar goes where an accent bar marks it, as in Total
  Commander; a click still shows the tab, Escape or leaving the bar cancels, locked tabs move too,
  and Ctrl+Up/Ctrl+Down move a tab in the list of tabs (Ctrl+Shift+A). The order is the engine's
  (`tabs.move`, MCP `tab_move`) and survives a restart. A tab's menu shows each entry's key from
  the keymap on the right ("Close tab  Ctrl+W"), through the helper the file list's and the
  queue's menus now share (`app/context_menu/keys.rs`). Both can be switched off:
  `tabs.drag_reorder` (Settings, Folders) and `keys.context_menu_shortcuts` (Settings, Keyboard),
  on by default.
- **Access keys in the dialogs** (RC-080-16; owner, 2026-10-19). The buttons of the copy, move,
  delete and progress dialogs, the conflict and error questions, the quit question and the
  settings dialog (Apply, the Import/Export and Host keys pages) underline one letter, as in
  Windows and Total Commander: Alt with the letter presses the button, the letter alone does
  while a button or checkbox has the focus ring. The letters are marked with `&` in
  `locales/en.json`; the underline shows while Alt is held, or always with
  `keys.underline_access_keys` (Settings, Keyboard; off by default). `theme::mnemonic`, the
  window-wide `mnemonic::Scope`, `theme::focus::Stop::letter`.
- **Hidden and system entries carry a `!` on their icon** (RC-010-02; owner, 2026-10-19), as in
  Total Commander: while hidden files are shown, the icon of a hidden or system file or folder is
  faded with a red `!` over it (`appearance.mark_hidden`, Settings, Appearance, on by default).
- **The quick search can filter** (RC-010-08; owner, 2026-10-19). With `quick_search.filters`
  (Settings, Folders; off by default) the quick search hides every file and folder its text does
  not find instead of only moving the cursor, as Total Commander's quick filter does: its text
  becomes the filter of the names it would find (`quick_search::quick_pattern`, start or
  anywhere), a character no name continues with is not taken, Enter keeps the filter and Escape
  lifts it.
- **Project foundation** (RC-000-01, RC-000-03). Repository infrastructure taken over from
  rDownloader (toolchain, `scripts/check.sh`, worktrees, Windows packaging from WSL, agent setup,
  repo lints), the architecture and development documentation, ADR 0001 (UI toolkit, proposed)
  and ADR 0002 (storage and secrets), the feature list against Double Commander, the roadmap with
  job files for milestones 0.0 to 0.8, and the wiki skeleton.
- **File system and database** (RC-000-04). `rc-vfs` defines the async `Vfs` trait (list, stat,
  read and write streams, mkdir, rename, delete, set attributes), `VfsPath` and typed errors with
  stable codes, and the `local` backend (long paths, UNC, `\\wsl.localhost\<distro>`, links,
  hidden files). `rc-db` opens `rcommander.db` over sqlx with checksum-pinned migrations
  (`scripts/migration-pin.sh`), one serialized writer and typed settings (migration 0001).
- **UI spike** (RC-000-02). An iced 0.14 window with two virtualised panels, keyboard
  navigation, a Fluent-like light and dark theme following the system, and an `iced_term`
  terminal per panel (Ctrl+T); results and measurements in ADR 0001 (accepted on Linux).
- **Engine core and MCP server** (RC-000-05). `rc-engine` holds the two panels and the command
  registry every action goes through (`panel.list_dir`, `panel.get_state`, `panel.navigate`,
  `panel.set_cursor`, `panel.toggle_selection`, `panel.switch_active`) and announces each change
  as an event. `rc-mcp` serves them as MCP tools over stdio (`rcommander mcp`, headless or
  forwarded to a running app) and Streamable HTTP on 127.0.0.1 (bearer token, Host/Origin check),
  with a per-class policy, single-use confirmation codes and credential masking on every answer.
  `docs/mcp-coverage.md` is generated by `scripts/mcp-coverage.sh`, checked in the preflight.
- **WSL distribution detection** (RC-010-04). `rc_vfs::wsl` lists the installed distributions
  from `wsl.exe --list --quiet` (UTF-16LE with or without BOM, NUL padding, CRLF or LF, UTF-8),
  marks the default and the WSL version from the registry, roots each at
  `\\wsl.localhost\<distro>\` (`\\wsl$` before build 21354) and lists nothing - no error - when
  WSL is missing, switched off or does not answer within 3 s. The list is kept; commands
  `wsl.list_distros` and `wsl.refresh`, MCP tools `wsl_list_distros` and `wsl_refresh`.
- **Terminal sessions** (RC-040-01). `rc-term` runs a local shell in a pseudo-terminal (ConPTY on
  Windows; pwsh, Windows PowerShell and cmd detected there, the login shell, bash, zsh and sh
  elsewhere) behind a `TermBackend` trait that WSL and SSH will share, feeds its output into
  alacritty_terminal and reads the screen back as text. The engine gains `terminal.open`,
  `terminal.close`, `terminal.send_input`, `terminal.resize`, `terminal.read_screen` and
  `terminal.list`; MCP gains `terminal_open` and `terminal_send_input` (exec),
  `terminal_close` (destructive), `terminal_list` and `terminal_read_screen` (masked), whose
  confirmation codes are bound to the terminal, or the shell and folder, they name.
- **Vault** (RC-050-01). `rc-vault` encrypts single secret fields in `rcommander.db`:
  argon2id key from the master password (parameters and salt stored in `vault_meta`, migration
  0002), XChaCha20-Poly1305 per value bound to its table, column and row, a lock that drops the
  zeroized key, an idle lock (15 minutes by default), a growing delay after failed attempts, and a
  master-password change that re-encrypts every secret in one transaction. No secret stored, no
  vault, no password prompt. Engine commands `vault.status` and `vault.lock`, MCP tools
  `vault_status` and `vault_lock`; the vault cannot be unlocked over MCP.
- **Connection tree** (RC-050-02). Folders and SSH connections in `rcommander.db` (migration
  0003, no secret columns): `rc-db` validates every write (blank names, empty hosts, ports outside
  1-65535, names unique per folder, no folder moved into its own subtree, 32 levels at most) and
  answers stable error codes; `rc-engine` adds `connections.*` and `folders.*` commands with
  change events; `rc-mcp` serves them as the `connections` tool group. No tool takes or returns a
  secret, a call naming a password is refused (`secret_not_accepted`), deletes need a
  confirmation code.
- **SSH host-key trust, engine side** (RC-050-07). New crate `rc-remote` with the trust store in
  `rcommander.db` (migration 0004 `known_hosts`): fingerprints as `ssh-keygen -lf` prints them,
  the verdict trusted, unknown or changed per host, port and key type, hashed `known_hosts`
  names checked by HMAC-SHA1, and the import of `~/.ssh/known_hosts` (markers, patterns and
  broken lines reported). `Engine::check_host_key` stops an unknown or changed key with
  `host_key_unknown` / `host_key_changed` and files a question for the GUI; commands
  `hostkeys.list`, `hostkeys.remove`, `hostkeys.import_known_hosts`, `hostkeys.answer` and
  `hostkeys.replace` (the only way to swap a changed key). MCP gets `hostkeys_list` and
  `hostkeys_remove` (with a confirmation code) and no tool that trusts a key. The dialogs follow
  with the app shell.
- **GUI-only commands** (RC-050-07). A command class `gui_only` for human decisions about trust
  or secrets (`hostkeys.answer`, `hostkeys.replace`, `hostkeys.import_known_hosts`):
  `Engine::dispatch_mcp`, the only entry the MCP server uses, refuses them with `gui_only`, the
  MCP policy denies the class whatever the setting, and the coverage table omits them.
- **Import from `~/.ssh/config`** (RC-050-06). `rc_engine::import::ssh_config` reads an
  OpenSSH client configuration into plain connection candidates: literal `Host` names with
  `HostName`, `User`, `Port`, `IdentityFile` (as a path, `~` and `%` tokens expanded) and
  `ProxyJump`; pattern and negated names skipped with their reason, `Match` blocks skipped,
  `Include` with globs, cycle and depth (8) protection; anything else a warning, never a value.
  Storing into the connection tree, the preview dialog and the MCP tool follow.
- **Storage discipline** (RC-010-11). `rcommander.db` holds durable state only: every table that
  grows with use has a row and an age limit (`rc_db::RETAINED_TABLES`, settings
  `retention.<table>.*`, never unlimited), every other one a reason (`DURABLE_TABLES`), and a test
  fails on a table in neither. A sweep at start and every hour deletes in batches and gives the
  space back (`auto_vacuum = INCREMENTAL`, older files switched once). Warnings and errors are
  stored in the new `error_log` table (migration 0005; 5 000 lines and 30 days by default),
  masked and capped; `info` and `debug` never are. MCP tools `diagnostics_error_log`,
  `diagnostics_clear_error_log`, `diagnostics_get_retention`, `diagnostics_set_retention`. The
  credential redaction moved from `rc-mcp` to `rc_db::redact`.
- **Operations queue** (RC-010-06). The new crate `rc-ops` copies, moves, deletes and creates
  directories as queued jobs that work only against `Vfs` (a copy streams between any two
  backends): counting first, throttled progress with speed and time left, pause, resume and
  cancel inside a file (a cancelled file is removed), conflict and error questions with
  presets and "for all" (Total Commander's answers: overwrite, overwrite older, larger or
  smaller, skip, rename the new file automatically or to a typed name, rename the existing
  target, compare, cancel), BLAKE3 verify, kept times and attributes, one job per target backend at
  a time, and the recycle bin on Windows. Engine commands `ops.*`; MCP tools `ops_copy`,
  `ops_move`, `ops_mkdir`, `ops_delete`, `ops_list`, `ops_pause`, `ops_resume`, `ops_cancel`
  (deleting and overwriting need a confirmation code). `rc-vfs` gains `MemoryFs` for tests.
- **Archives as a file system** (RC-030-01). `rc-vfs` reads zip (zip64, CP437 or a configured
  code page for names without the UTF-8 flag), 7z (solid blocks streamed, never unpacked into
  memory), tar plain or with gzip, xz, bzip2 or zstd, and single `.gz`/`.xz`/`.bz2`/`.zst` files
  as the read-only backend `ArchiveFs` (`BackendId::Archive`): format by magic bytes, implied
  directories, a listing cache per archive. Escaping names (`..`, absolute, drive letter, UNC)
  are cleaned and marked, links shown but never followed, encrypted entries marked and refused
  with `password_required`, and limits on entry count, unpacked size and compression ratio stop
  archive bombs (`archive_limit`). The `Vfs` trait gains `open_read_seekable`; the conformance
  suite a read-only part.
- **Archives in the panel** (RC-030-02). Enter opens a zip, 7z or tar archive in the panel like a
  folder (by extension; setting `archive.enter_opens`), Ctrl+PageDown any file whose content is
  one, and `..` at its root returns onto the archive file; a typed path through an archive file
  opens it there. The path bar runs through the archive, the file list shows the packed size and
  marks encrypted entries and unsafe stored names. Archives are read-only: changes inside are
  refused (`archive_read_only`), F5 copies out through the queue, F3 reads entries through the
  `Vfs`, and Enter on a file inside hands the system a read-only copy in a temporary folder. The
  archive test (Alt+Shift+F9, `archive.test`) is a background `rc-ops` job that reads every entry
  to its end and lists damaged and encrypted ones. MCP: `archives_list`, `archives_test`;
  `navigate` opens an archive by its path; the sensitive-path guard also judges the archive file.
- **Multi-rename engine** (RC-020-01, logic part). `rc-rename` renders Total Commander's mask
  placeholders (`[N]`, `[E]`, `[A]`, `[P]`, `[G]` with ranges such as `[N2-5]`, `[C10+5:3]`,
  `[YMD]`, `[hms]`, `[U]`/`[L]`/`[F]`/`[n]`, `[[]`/`[]]`), the five case modes and plain (with
  `|` terms) or regex search and replace, optionally substituting the whole name, into a preview that flags duplicates, taken names, names the file system refuses and
  case-only changes (Windows naming rules on every file system, rule texts bounded); it orders
  the renames through temporary names (swaps, rings, case-only on NTFS) in linear time and
  derives the undo steps and the bounded undo-log record. Carrying them out, the undo log and the MCP tools follow with
  `rc-ops` (RC-010-06).
- **Multi-rename apply and undo** (RC-020-01). `multi_rename.preview` plans a rule against a
  fresh listing of any `Vfs` folder; `multi_rename.apply` plans again, renames nothing while the
  plan has a conflict, logs every step in the retained undo log `rename_log` (migration 0011,
  100 000 rows and 30 days by default) and runs them as one `rc-ops` rename job that never
  overwrites an entry and marks each step before the next; `multi_rename.undo` reverses the
  latest run as far as it got - also after a cancel or a crash - and refuses as a whole when an
  entry no longer fits. MCP tools `multi_rename_preview`, `multi_rename_apply` and
  `multi_rename_undo`; apply and undo refuse with `sensitive_path` when an old or a new name is a
  key or credential file, so no rename carries one past the guard.
- **App shell** (RC-010-01). The window is a client of `rc-engine`: two panels on the start
  directory, every key a registry command, the keymap as data
  (`crates/rc-engine/keymap/default.json`) with Total Commander 11's defaults - Tab, Enter,
  Backspace and Ctrl+PgUp, Ctrl+\ to the root, Ctrl+R to reread, Insert, Space, Ctrl+A, Ctrl+Num+,
  Ctrl+Num- and Num* for the selection - and the keys of later features reserved. A `..` row
  heads every listing with a parent, as in TC. An MCP `navigate` over stdio or HTTP moves the
  panel on screen. The MCP server runs inside the app (`mcp.http.enabled`, `mcp.http.port`) and
  the status bar shows its address. Window size, position and maximized state survive a
  restart. Ctrl+` toggles a panel's terminal (was Ctrl+T). Every visible string goes through
  `locales/en.json`, the language follows the system with English as fallback, and the repo lint
  `i18n_keys` holds keys and catalogue together.
- **WSL terminal** (RC-040-02). `terminal.open` with `shell: "wsl"` (and `distro`, a name from
  `wsl.list_distros`; WSL's default without it) starts `wsl.exe -d <distro> --cd <path>` in the
  local PTY back end, arguments passed one by one. `rc_term::windows_to_wsl_path` translates the
  panel's folder: `C:\x` to `/mnt/c/x`, the distribution's own `\\wsl.localhost` or `\\wsl$`
  share to its Linux path; another distribution's share, a network share or a relative path
  starts in `~` with a status hint (`notice`). `terminal.list` adds one "WSL: <name>" profile per
  detected distribution (none outside Windows). Without WSL the answer is `wsl_unavailable`, an
  unknown distribution `wsl_distro_not_found`; the MCP confirmation code of `terminal_open` is
  bound to the distribution as well.
- **SSH agents** (RC-050-05). `rc_remote::agent` finds the running agents - `SSH_AUTH_SOCK` on
  Unix; on Windows the OpenSSH agent service (`\\.\pipe\openssh-ssh-agent`) and Pageant, in the
  order of the setting `ssh.agent_sources` (`both`, the default, `openssh`, `pageant`) - and
  lists their keys (agent, key type, comment, SHA256 fingerprint; never key bytes) within one
  second, through russh's agent client. `agent.list_identities` and the MCP tool
  `ssh_agent_identities` (read) answer the list, or `agent_not_found`, `agent_timeout`,
  `agent_failed`. `rc_remote::agent::connect` opens an agent as the signer for the SSH login. No
  secret is stored and the vault is never touched.
- **Secrets and the master password** (RC-050-03). A connection's password, private key and
  passphrase are stored encrypted in `rcommander.db` (migration 0006). A key file (OpenSSH, PEM,
  PKCS#8, PuTTY `.ppk` v2/v3) is taken into the vault once, checked and opened with its
  passphrase, and stored with its public half in clear, so a backup of the database carries
  every key; the file is left as it is. The master password is set when the first
  secret is stored and asked for only when a connection needs one, never at start-up; removing
  the last secret offers to remove the vault. New `gui_only` commands `secrets.set`,
  `secrets.import_key`, `secrets.clear`, `vault.create`, `vault.unlock`, `vault.change_password`
  and `vault.remove`, the events `SecretNeeded`, `VaultUnlocked`, `VaultUnused` and
  `VaultRemoved`, and `has_password`, `has_key`, `has_passphrase` and `key` (type, fingerprint,
  comment) on every connection. MCP masks
  every secret known in the unlocked session word for word; a canary test finds none in the log,
  the answers, the events or the database file. The dialogs follow with the connection manager.
  `connections.import_ssh_config` (MCP `connections_import_ssh_config`, a dry run unless told
  otherwise) previews each host as new, update, unchanged or refused and imports the selected
  ones into "Imported from ssh config" in one transaction; migration 0007 marks imported
  connections with `import_source`/`import_key`, so a second import updates instead of
  duplicating. Only `~/.ssh/config` and `~/.ssh/config.d/*` are read (canonical path, also a
  WSL distribution's), a key file or key text anywhere stops the import, and the report never
  repeats text of the file beyond keywords and line numbers. The preview dialog follows.
- **File search, engine side** (RC-020-03, engine part). `rc_engine::search` walks any `Vfs`
  for Total Commander's Alt+F7 search: name masks (`*.rs;*.toml | *.bak;node_modules\`) or a
  regular expression, text or regex inside files (UTF-8, UTF-16 LE/BE, Windows-1252 or BOM
  detection, several in one pass; case, whole words, not containing, skip binary) streamed in
  64 KiB chunks with matches across chunk boundaries, size, date, age, attribute and kind
  filters, a depth limit, links followed only on request and then with a cycle guard. Hits come
  in a stable order; `Engine::start_search` streams hits and progress and stops on cancel. The
  command `files.search` and the MCP tool `files_search` (read) answer it page by page with a
  continuation cursor (100 hits by default, 1000 at most) and stop when the MCP call is
  cancelled. The find-files dialog, the result panel and search templates follow.
- **Compare by content and synchronize dirs, engine side** (RC-020-04). `rc_engine::compare`
  compares two files over any `Vfs`: streamed byte by byte (first differing offset, files of any
  size in two 256 KiB buffers) and, for text up to 16 MiB, line by line with the `similar` crate
  (UTF-8/UTF-16/Latin-1, ignore white space, case and line endings, side-by-side rows, jumps
  between changes, unified diff with a size limit). `rc_engine::sync` compares two trees by name,
  size, time (2 s FAT tolerance, optional one-hour DST shift) and optionally content, with
  Total Commander masks and optional subdirectories, cancellable; it proposes per row a copy
  either way or, in the asymmetric mirror modes, a delete, and turns the plan into copy, mkdir
  and delete operations - deletes only once confirmed. Commands `compare.files` and
  `compare.dirs`, MCP tools `files_compare` and `dirs_compare` (read only). Running a plan
  through `rc-ops`, the diff window and the synchronize dialog follow.
- **Checksums, split and combine, engine side** (RC-020-05, engine part). The new crate
  `rc-checksum` hashes over any `Vfs` with CRC32, MD5, SHA-1, SHA-256, SHA-512 and BLAKE3 through
  one 256 KiB buffer, writes checksum files as Total Commander does (`.md5`, `.sha`, `.sha256`,
  `.sha512`, `.blake3`, `.sfv`; one per file or one for all; `<hex> *<name>` lines that
  `sha256sum -c` checks) and verifies them per line (ok, mismatch, missing, unreadable, invalid),
  reading the GNU, BSD-tag and SFV forms by content; a name that leaves its folder is reported,
  never opened. Split writes `<stem>.001`, ... with Total Commander's `.crc` file; combine checks
  every part, the size and the CRC before it keeps the result. Every run is cancellable and
  removes what it created when it does not finish; nothing existing is overwritten. Commands
  `checksum.compute`, `checksum.create`, `checksum.verify`, `file.split`, `file.combine` and the
  MCP tools `files_checksum`, `files_checksum_create`, `files_checksum_verify`, `files_split`,
  `files_combine`. The dialogs follow.
- **Checksum, split and combine dialogs** (RC-020-05, window part). Ctrl+Shift+F9 to F12 open
  "Create checksum files" (algorithm, one file per file or one named file), "Verify checksums"
  (starts at once for the selected checksum files), "Split file" (target folder from the other
  panel, part size typed or chosen: 100 MB, 1 GB, FAT32's 4 GB minus one byte, DVD) and "Combine
  files" (`checksum.dialog`; Total Commander binds no key, rCommander has no menu bar yet). The
  work runs as an `rc-ops` job (`checksum.queue`; new `JobKind`s `ChecksumCreate`,
  `ChecksumVerify`, `Split`, `Combine`, the snapshot's `report`) in the progress window with
  pause, background and cancel; `rc_checksum::Control` can now pause and counts files and bytes.
  A check lists every line with its status and reason, a create the digests it wrote; split and
  combine report in the status bar. MCP keeps its five tools; `checksum.queue` is refused under
  MCP and both new commands are omitted in the coverage table.
- **Viewer core** (RC-020-07, engine part). `rc_engine::viewer` is the GUI-free model of F3 and
  Quick View over any `Vfs`: windows of a file through a bounded block cache (4 MiB, never the
  whole file; `Vfs::open_read_seekable` where the backend seeks), encoding detection (byte-order
  mark, UTF-16 without one, UTF-8, else Windows-1252; Latin-1 by choice), pages of lines from a
  byte offset (LF, CRLF, lone CR; long lines cut), a line index counted in steps in at most
  512 KiB, hex rows as `xxd` prints them with go-to-offset, text and hex-byte search forward and
  backward with progress and cancel, and image sniffing (PNG, JPEG, GIF, WebP, BMP, ICO). New
  commands `files.read_text` and `files.read_hex` with MCP tools `files_read_text` (256 KiB by
  default, at most 4 MiB, says whether it was cut) and `files_read_hex` (at most 64 KiB). The MCP
  side gains the sensitive-path guard `rc_mcp::sensitive_path` (rCommander's database, instance
  file and backups, private keys, key stores, `.ssh` but public keys, `known_hosts` and `config`,
  credential files; canonical path first, case ignored) refusing with `sensitive_path` - also in
  `files_checksum`, `files_checksum_create` (a folder whose files would be refused too),
  `files_checksum_verify`, `files_split` and `files_combine`, while a content search of
  `files_search` leaves such files out of its hits and counts them in `withheld` - and the mask
  replaces every private key block with `[private key redacted]` in every answer. The viewer
  window and Quick View follow.
- **FTP/FTPS file system, core** (RC-070-01). `rc_remote::ftp::FtpVfs` implements `Vfs` on
  suppaftp 12 with rustls: plain FTP, explicit (`AUTH TLS`) and implicit FTPS with `PROT P`,
  passive (`EPSV` before `PASV`, data always to the control peer) or active mode, `MLSD`/`MLST`
  with a `LIST` fallback (Unix and DOS/IIS parsers, fixtures of real answers), `SIZE`/`MDTM`/`CWD`
  for `stat`, streamed `RETR` (`REST` to resume), `STOR` and `APPE`, `MKD`, `RNFR`/`RNTO`, `DELE`,
  `RMD`, `MFMT`, `SITE CHMOD`, a small connection pool and a timeout on every step. Names follow
  `OPTS UTF8 ON` or a Latin-1 fallback. Certificates are checked against the system roots; any
  other goes to a trust hook that refuses by default (`certificate_unknown`, with the SHA-256
  fingerprint). Credentials come in as a struct; the connection tree, vault storage and MCP
  follow.
- **File list** (RC-010-02). Full view (Ctrl+F2) with a column header that sorts on a click and
  brief view (Ctrl+F1) in columns, per panel and both virtualised; sorting by name, extension,
  size or date (Ctrl+F3 to F6, again turns it around) with folders on top and names in natural
  order (`panel.sort`, `panel.set_view`). Total Commander's selection keys: Num+ "Select group"
  and Num- "Unselect group" ask for a mask (`*.txt;*.md|draft*`, files only unless it ends in
  `\`) and keep the last 20 masks for ArrowUp (`panel.select_by_mask`,
  `panel.deselect_by_mask`), Alt+Num+ and Alt+Num- by extension, Num / restores the
  selection the last file operation took, Shift with the cursor keys selects ranges. Hidden and
  system files are switched for both panels from the status line and the choice survives a
  restart (`panel.toggle_hidden`, no default key, as in TC). The status line sums the selection
  against the folder; sizes read in KiB/MiB, dates in the system locale's order. The path bar
  has clickable segments and takes a typed path. Enter and a double-click open a file with its
  system association or start a program (`file.open`, class `exec`); F2 rereads like Ctrl+R.
  MCP tools `panel_sort`, `panel_set_view`, `panel_show_hidden`, `panel_select`,
  `panel_select_by_mask`, `panel_deselect_by_mask`, `file_open`.
- **Drive bar** (RC-010-05). A bar of drive buttons above each panel, or one above both
  (`drive_bar.layout`, toggled at the bar's end, kept across restarts): drive letters with a
  symbol per kind (fixed, removable, network, optical, RAM), the installed WSL distributions and
  the saved servers of the connection tree's root level (a hint until servers open, RC-060-02).
  Each panel has a drive line with its drive's volume name and free space; Alt+F1 and Alt+F2
  open the drive menu (arrows, Home, End, Enter, a drive's letter, Escape). A drive change
  returns to the directory the panel last showed on that drive. Volume names and space are asked
  off the UI thread with a 2 s limit, a hanging drive is never asked twice at once, and network
  drives are only asked on request, so a refresh never wakes a sleeping share. Commands
  `drives.list`, `panel.change_drive`, `panel.drive_menu`; MCP tools `drives_list`,
  `panel_change_drive`.
- **Settings and theme** (RC-010-09). A settings dialog after Total Commander's Configuration
  (Settings in the status bar; pages Appearance, Folders, Terminal, Vault, Retention, MCP server;
  OK, Cancel, Apply) over a typed schema in `rc-engine` (`rc_engine::SETTINGS`): theme
  system/light/dark, the panels' start folders, the default shell of new terminal sessions, the
  vault's idle lock, the retention limits, the MCP switch and port, the rights per tool class and
  the access token (copy, new token). A stored value that does not fit counts as its default and
  is logged. The theme changes at once and follows Windows' light or dark mode while it runs; the
  accent colour is Windows' own. `settings.get` and `settings.set` with the MCP tools
  `settings_get` and `settings_set`; every `mcp.*` and `vault.*` setting is refused there for
  every caller, so an agent never loosens its own permissions. Switched off, the MCP server stops
  at once and `rcommander mcp` says so; a new token or port restarts it at once.
- **Database backup and restore** (RC-010-12). `rcommander backup <target> [--overwrite]` and the
  status bar's Database dialog back `rcommander.db` up with `VACUUM INTO` - consistent while the
  window runs and writes - into a temporary file beside the target that must pass `PRAGMA
  integrity_check` before it takes the name; an existing file is replaced only on request, a
  folder gets `rcommander-YYYYMMDD-HHMMSS.db`, Unix files are `0600`. On Windows the command
  prints to its caller's console. A restore (window only) checks the file (sound, an rCommander
  database, no newer schema), backs the current state up and swaps it in at the next start.
  Commands `db.backup` (MCP tool `db_backup`, no path: `<data dir>/backups` only), `db.backup_to`
  and `db.restore` (both `gui_only`).
- **Terminal panel** (RC-040-03). Each panel shows its files or one of its terminal sessions:
  Ctrl+Ö (Ctrl+` on a US layout, the same key as in VS Code) starts the default shell in the
  panel's folder (or returns to the running one) and back to the files, and is the only key the
  app keeps while a terminal has the keyboard - Tab, the F-keys, Ctrl+C, Ctrl+Tab go to the
  shell. Ctrl+Shift+Ö (Ctrl+Shift+`) and the `+` of the session bar ask for the shell; the bar switches and closes sessions; `exit` in the last one returns to the files
  with the exit code in the status line. Ctrl+Shift+C/V copy and paste (a right click pastes on
  Windows), text with a line break asks first, Shift+PgUp/PgDn and the wheel scroll the history.
  The widget is `rc-term-widget`, iced_term 0.8 forked to draw the engine's `rc-term` session
  (no PTY of its own; draws inside clipped panels; no panic when a session ends). Engine:
  `panel.show_files`, `terminal.show`, `panel.toggle_terminal` starts the shell itself, and
  `terminal.open` shows the new terminal in its panel; MCP tools `show_files` and `terminal_show`.
- **Sessions and login scripts in the window** (RC-050-03, RC-050-04, RC-050-08; integration of
  wave 010-w3). The connection manager opens the login-script editor ("Login scripts..."), a
  layer over the manager; an SSH connection's form shows its session (none, connecting,
  connected with its channels) with Connect or Disconnect, and Test runs `connections.test` for
  every kind. Ctrl+Shift+F in the file list closes the active panel's connection;
  `connections.disconnect` also drops the session an FTP connection keeps for its panels.
- **SFTP file system and panel** (RC-060-02). `rc_remote::SftpVfs` implements `Vfs` on
  russh-sftp 3 over two channels of a connection's pooled SSH session (names and file streams),
  passes the conformance suite against the testkit server's new `sftp` subsystem, and treats the
  server as hostile: names with `/`, `\`, NUL, CR, LF or `.`/`..` are dropped, a listing is
  capped in entries (500 000) and time, every request has a timeout (`sftp_timeout`), a closed
  channel is `connection_lost`. Links are resolved a few at a time; reads stream, seek and start
  at an offset (for RC-060-07). `BackendId::Sftp { connection_id }`; the engine opens it for
  `sftp://<connection>/<path>` (`<connection>` alone: start or home directory), keeps it, registers
  it with the operations queue and replaces it after a lost session. An SSH server in the drive
  bar opens in its home directory; the path bar reads `sftp://<name>/...`, `panel.navigate` takes
  written locations, and panels on a server show a rights column. MCP `list_dir`, `files_stat`
  and `navigate` take `sftp://` paths; new codes `sftp_unavailable`, `sftp_timeout`,
  `sftp_protocol`, `sftp_listing_too_large`, `sftp_connection_not_found`,
  `sftp_connection_ambiguous`. Ctrl+N opens the quick connection: host, port, user, password or
  key file, optionally saved as a connection (secrets into the vault, the master password asked
  only then); unsaved, it lives in memory for the run (`connections.quick_connect`, GUI only)
  until `connections.disconnect` closes it and drops its secrets; MCP never reaches one by id.
  The drive bar lists every server of the connection tree; a folder is a button that opens the
  drive menu at its servers (`Drive::folder`).
- **SSH terminal with login scripts** (RC-060-03). A panel's terminal runs a shell on an SSH
  server: `rc-term` drives a channel of the connection's pooled session (one TCP connection with
  its SFTP panel) with a PTY (`xterm-256color`, the widget's size, `window-change` on a resize)
  and ends with the channel or the connection, the exit status in the status line. The
  connection's login script runs on the shell first - its secrets read from the vault before
  anything reaches the server, typed input held until it ends (Ctrl+C stops it), its progress in
  the status line - and a run that times out or fails says so in the terminal and leaves the
  shell usable. A start folder (the SFTP panel's, `cwd`, else the connection's start directory)
  is a POSIX-quoted `cd` after the script. Command `terminal.open_ssh` (an SFTP panel's terminal
  toggle opens it); MCP `terminal_open` takes `connection_id` (exec, code bound to the
  connection), `terminal_list` shows the run, an echoed password leaves `terminal_read_screen`
  masked, and a locked vault answers `vault_locked` without a dialog.
- **Remote command** (RC-060-04). `ssh.exec` runs one command on a connection's pooled SSH
  session in an `exec` channel without PTY or login script (`rc_remote::ssh::exec_command`):
  stdout and stderr apart, each capped (1 MiB by default, at most 4 MiB, `*_truncated`), the
  exit code or signal, a time limit (60 s by default, at most 3600 s; `timed_out` keeps what came
  so far), optional text on stdin. A run that ends early or is dropped sends `TERM` and closes
  its channel; the session stays. The command goes to the server unchanged and is logged only
  masked (`exec_invalid` for an empty or overlong one or a NUL). MCP tool `ssh_exec` (class
  `exec`): asked in the window with the connection and the full command, headless a
  confirmation code bound to connection, command and input; output masked, the vault's secrets
  included. A "Run command" dialog in the window, opened for a connection.
- **FTPS certificate trust** (RC-070-02). A certificate the system does not trust (self-signed,
  unknown CA, wrong name) opens a dialog with its SHA-256 fingerprint, subject, issuer, validity
  and the reason - accept once, trust and save, cancel - instead of only failing; "trust and save"
  pins it per host and port (`tls_certificates`, migration 0013). A certificate other than the
  pinned one stops the connection with `certificate_changed`, even a validly signed one, until
  the pinned one is removed (`certificates.remove`); a pinned certificate past its validity warns
  (`certificate_expired`, accept once only). The trust hook now decides after the system roots
  on every handshake; no password is sent before it passes. `certificates.answer` and
  `certificates.remove` are GUI only; MCP tool `certificates_list`.
- **Servers in the window** (RC-050-04, RC-060-02, RC-060-03, RC-060-04, RC-070-02; integration
  of wave 4). In the connection manager Enter or "Open in panel" shows an SSH connection in the
  active panel, "Terminal" starts `terminal.open_ssh` there and "Run command..." opens the
  command dialog; an FTPS connection's form lists the certificate trusted for its server with
  "Forget". A command line on an SFTP panel opens the command dialog with the line (`cd` still
  moves the panel), and Ctrl+Shift+F also disconnects an SFTP panel's server. The drive bar
  shows marked servers and quick connections as buttons of their own and each top-level folder
  of the tree as a button that opens the menu at all its servers (`drives.list` lists every
  server with `Drive::favourite`). `rc-db` refuses control characters in a connection's name,
  user and start directory (`control_character`), and an MCP call reaches no quick
  connection's session or open backend (`connection_not_found`).

- **Login scripts** (RC-050-08). A login script types after an SSH login what a person would:
  wait for a prompt (a size-limited regular expression on the output, colour codes removed),
  send text, send a stored secret, send Enter, pause. `rc-remote` plays it against any async
  byte stream with per-step and overall timeouts and cancellation, and reads every secret before
  it sends anything (`vault_locked` sends nothing); templates for `sudo -i`, `su - root` and a
  second password, and a test run against sample output. Scripts live in `rcommander.db`
  (migration 0009, `login_scripts` and the encrypted `login_script_secrets`, re-encrypted with a
  new master password); a connection names the one it runs. Engine commands `login_scripts.*`;
  MCP tools `login_scripts_list`, `_get`, `_create`, `_update`, `_delete` (confirmation code) and
  `_assign`; a create or update with steps and an assign are exec. A `send_secret` step, the
  steps before it, assigning a script with one and storing a script's secret are the window's
  only. The editor module is written;
  the run on an SSH terminal follows with RC-060-03.
- **SSH sessions** (RC-060-01). `rc_remote::ssh::SshSession` connects on russh with timeouts
  for TCP, handshake and login and a cancel token, checks the server key through the trust store
  before any secret is read, and logs in in a per-connection order (default agent/Pageant, the key
  from the vault with its passphrase, the password from the vault; `keyboard-interactive` when the
  server only asks by prompt), logging the method but never a secret. `rc-engine` pools one
  session per connection (every channel shares its TCP connection, keepalive 30 s, idle close,
  `connection_lost` and a reconnect on next use); `connections.connect`, `.disconnect`, `.test`,
  `.session_prefs` and `.set_session_prefs` with their MCP tools, the sessions in
  `connections_list`. Over MCP a locked vault answers `vault_locked` without an unlock dialog.
  Session settings in migration 0010; an in-process russh test server (`testkit`).
- **FTP/FTPS connections** (RC-070-01). The connection tree takes the kinds `ftp`, `ftps`
  (explicit) and `ftps_implicit` (port 990) and the auth method `anonymous`; every write checks
  that the kind can log in with the method (`auth_unsupported`), that an FTP password login
  names a user (`username_empty`) and that FTP opens as files (`view_unsupported`). The password
  is the connection's vault secret. `ftp://<connection name>/<path>` reaches a connection from
  `panel.list_dir` and the new `files.stat` (`BackendId::Ftp`): the session is opened on first use
  with the stored password (`vault_locked`, `password_not_stored`) and dropped when the connection
  or its secret changes. `Engine::test_ftp_connection` logs in once (the FTP part of
  `connections.test`, RC-060-01); MCP tool `files_stat`, FTP kinds in the `connections_*` tools. suppaftp's
  `trace` lines, which carry `PASS`, never reach the console log. Each FTP connection keeps its
  data mode (passive, active) and name encoding (Latin-1, UTF-8) (migration 0012). Active mode
  listens only on the control connection's address and takes exactly one connection, from the
  server. Fixed on the way: an FTPS upload to a server that sends TLS 1.3 session tickets lost
  its tail (data connections now close with `close_notify` and are drained first); listings are
  capped at 64 MiB and listed names with `/`, `\`, NUL, CR, LF, `.` or `..` are refused; a data
  connection that stalls for the timeout fails.
- **Host-key dialogs** (RC-050-07). An unknown SSH host key opens a dialog over the window with
  server, key type, `SHA256:` fingerprint and randomart: "Connect once", "Trust and save",
  "Cancel". A changed key opens a red warning with the stored and the presented fingerprint and
  only "Replace stored key..." and "Cancel"; replacing is a second step of its own. The keyboard
  starts on Cancel (Back), Escape cancels; the answers go through the GUI-only
  `hostkeys.answer` and `hostkeys.replace`, and the result shows in the status bar.
- **MCP question in the window** (RC-010-13). A tool the MCP policy asks about now waits for the
  person: the window comes to the front and shows the tool, its kind, its masked arguments and
  the panel it acts on; Allow, Allow for this session (write tools only) or Deny (Escape), and no
  answer within two minutes is a no (`denied_by_user`, `timed_out`). The answer is the `gui_only`
  command `mcp.answer` (`Engine::ask_mcp`, `EngineEvent::McpQuestion`), so no tool can give it,
  and a confirmation code counts for nothing while the window runs; the status bar counts the
  questions waiting. Headless, an `ask` call is refused (`headless_ask_denied`) unless the new
  protected setting `mcp.ask.headless_code` allows the confirmation code.
- **File icons** (RC-010-10). Every row of the file list shows an icon before its name: on
  Windows the one Explorer shows (`SHGetFileInfoW` by extension without touching the file, per
  file for `.exe`, `.lnk`, `.ico`, `.url` and folders with their own icon, at the display's
  scaling), loaded on a thread of its own for the rows on screen only and kept in a bounded LRU
  cache; generic file and folder icons, drawn in code, stand in until then, on Linux and macOS,
  and inside archives and on servers (by extension only). The setting `appearance.file_icons`
  (Native, Generic, Off; MCP through `settings_get` and `settings_set`) chooses.
- **Tabs, folder history and hotlist** (RC-010-03). Folder tabs per panel as in Total Commander:
  Ctrl+T opens the panel's folder in a new tab, Ctrl+Up the folder under the cursor, Ctrl+Shift+Up
  in the other panel; Ctrl+W closes (the last tab stays), Ctrl+Tab and Ctrl+Shift+Tab step round;
  a click shows a tab, a middle click closes it, a right click locks, copies or closes it. A
  locked tab (`*` before its title) opens a new folder in a new tab. Each tab keeps its folder,
  sort, view and cursor and a history of 50 folders (Alt+Left, Alt+Right, Alt+Down for the list).
  Ctrl+U swaps the panels' folders, Ctrl+Shift+U their tab rows; Ctrl+Left and Ctrl+Right show
  the folder under the cursor in the left or right panel. The directory hotlist (Ctrl+D) adds the
  current folder and opens entries, with hotlist folders and an optional folder for the other
  panel, edited in its own dialog. Tabs are stored when the window closes and come back at the
  start; a vanished folder opens its nearest parent and the status bar says so. Migration 0008
  (`tabs`, `dir_history` as a retained table, `hotlist`); commands `tabs.*`, `history.*`,
  `hotlist.*`, `panel.swap`, `panel.transfer`, each with its MCP tool except `tabs.next`.
- **File operations on the function keys** (RC-010-07). F5 copies and F6 moves from the active
  panel into the other one through a dialog with an editable target (relative to the source
  folder; a folder to copy into, or for one entry its new name), a preset for existing targets
  and verify after copying; OK runs in a compact progress window (pause, cancel, to the
  background), "In the background" in a collapsible bar where a second job starts beside it.
  Shift+F5 copies beside the file under a new name, Shift+F6 renames in the list, F7 makes
  nested folders with the cursor on the new one, F8/Delete ask before the recycle bin and
  Shift+F8/Shift+Delete warn before deleting for good, Shift+F4 makes a text file and opens it
  in the editor; Shift+F6 changes only the case of a name too. Conflicts show both files and every answer of the queue (also "rename to" with
  an editable proposal), a failing file offers retry, skip, skip all and cancel; afterwards both
  panels are read again and copied files deselected. F4 opens the editor of the new Programs page
  in the settings (default Notepad); F3 an external viewer only when one is set there, else
  rCommander's own viewer (both settings protected). The
  function-key bar follows Shift, Ctrl and Alt, runs a key on a click and can be hidden. Engine:
  `ops.dialog`, `ops.copy_to`, `ops.move_to`, `file.open_external` (all omitted from MCP), and
  `rc-ops` copies or moves one entry to a path of its own (`CopyAs`, `MoveAs`).
- **Quick search, quick filter and command line** (RC-010-08). Ctrl+Alt with letters jumps to
  the first name that starts with them (setting `quick_search.keys`: Alt with letters, or letters
  alone; `quick_search.anywhere`, `quick_search.case_sensitive`; `*` and `?` as wildcards),
  ArrowDown and ArrowUp step between the matches. Ctrl+S filters the panel as typed
  (`panel.filter`, MCP tool `panel_filter`): the filter shows in the path bar, stays after Enter,
  goes with the folder or with Escape, and deselects what it hides; Ctrl+Shift+S applies the last
  one again. Letters alone go into the command line under the panels: Enter runs it in the
  active panel's folder (`cmdline.run`, class `exec`, no MCP tool) - `cd`, `chdir`, `cd..`,
  `cd /d` and a drive (`d:`) move the panel without a process, any other line opens a console
  window (`cmd /s /c`, Shift+Enter `/k`; `sh -c` elsewhere); Ctrl+Enter, Ctrl+Shift+Enter and
  Ctrl+P add the name, the full path and the folder, quoted. The last 30 lines are kept in the
  database (bounded, out of MCP's reach), ArrowUp and ArrowDown in the field and Alt+F8 bring
  them back. A line is checked first (no control characters, at most 8000 characters) and never
  repeated in an error; outside a local folder the line is disabled.
- **Branch view and the find-files dialog** (RC-020-03). Ctrl+B shows every file below the
  panel's folder flat, each named by its path relative to it (`sub\file.txt`), folders left out;
  Ctrl+Shift+B only the selected folders and files; Ctrl+B again leaves into the folder of the
  entry under the cursor. The list fills in the background, follows the panel's sort and hidden
  switch, stops at 100 000 files, ends when the panel moves on, and a refresh reads it again;
  copy, move, delete and open act on the real files. Alt+F7 opens the find-files dialog (General
  and Advanced tabs as in Total Commander, hits as they are found, Stop, Go to file, F3/F4 at the
  hit); "Feed to listbox" shows the hits flat in the panel the same way. Commands
  `panel.branch_view`, `panel.feed_paths` and `files.find_dialog`; MCP tools `panel_branch_view`
  and `panel_feed_paths` (read). The searched masks are kept (`search.history`).
- **Viewer window and Quick View** (RC-020-07, window part). F3 (`view.open`) opens Lister over
  the window on the file under the cursor: text, binary (80 columns), hex (rows as `xxd`) and
  image, switched with 1 to 4; A, 6 and 8 choose Windows-1252, UTF-16 LE and UTF-8 as Lister
  does, E steps through all five encodings, W wraps, T changes the tab width, F2 reads the file
  again, N and P step to the next and previous file of the panel. A page at a time is read
  through `rc_engine::viewer` off the UI thread; the scrollbar works by byte position. Text is
  selected with the mouse and copied with Ctrl+C. F7 or Ctrl+F searches text (with or without
  case) or hex bytes, forward or backward, over the whole file with progress in the status line
  (Escape stops it); F3 and Shift+F3 search on, Ctrl+G goes to an offset. Images (PNG, JPEG,
  GIF, WebP, BMP, ICO) decode on a blocking thread within limits (256 MiB file, 64 Mi pixels,
  32768 per side) and show fit or zoomed (Num+/Num-, F) and turned (R, Shift+R); a damaged one
  says so. Ctrl+Q (`panel.toggle_quick_view`) shows the entry under the cursor in the other
  panel and follows the cursor - a file as in Lister, a folder as its counts and the bytes of its
  files. Both commands are omitted from MCP (purely visual). New dependency: `image` (decoders
  for these six formats only) and iced's `image-without-codecs`.
- **Unpacking archives** (RC-030-03). Alt+F9 (also Alt+F6) unpacks the selected zip, 7z and tar
  archives in a dialog (target, file mask, a folder per archive, with or without paths, conflict
  preset) as an `rc-ops` job (`archive.extract`). Entries whose names would leave the target (zip
  slip, absolute, drive, UNC, names Windows cannot hold) and stored links are skipped and listed;
  a link already in the target is never written through; set-user-id bits are dropped. Encrypted
  zip (ZipCrypto, AES) and 7z ask for their password (`archive.password`, GUI only; a wrong one is
  `password_wrong`), which lives for the job only. A cancel removes the file being written. F6 out
  of an archive is refused. MCP: `archives_extract` (overwriting needs a confirmation code;
  encrypted entries report `password_required`).
- **Packing archives** (RC-030-04). Alt+F5 packs into zip, 7z (solid or not), tar, tar.gz, tar.xz,
  tar.bz2 or tar.zst (pure-Rust ruzstd: level 0 stores, 1 to 9 is its fastest level) at levels 0
  to 9, Alt+Shift+F5 with "delete the packed files" preselected (`archive.pack`). Options: subfolders,
  paths, one archive per entry, links stored or followed. The archive is written under a temporary
  name and renamed when complete, so a cancel leaves nothing under the target's name; a panic in an
  archive crate fails that archive with `pack_failed` instead of ending the app; the sources
  are deleted only after a complete archive. MCP: `archives_pack` (overwrite and delete sources need
  a confirmation code; a source that is or holds keys or credentials is refused).
- **Panel and terminal in the same folder** (RC-040-04). Turning a panel to its terminal, or
  moving the panel while it shows the terminal, sends the shell to the panel's folder; turning
  back to the files shows the folder the shell was last in (settings `terminal.follow_panel` and
  `terminal.panel_follows`, both on). The `cd` is typed only while the shell waits at an empty
  prompt (OSC 133), never into a running program or a half-typed line; otherwise it waits for the
  next prompt and the panel's status line says why. The shell reports its folder with OSC 7 or
  OSC 9;9 (Windows, WSL share and drive paths, Unix); folder names are quoted for PowerShell,
  POSIX shells and cmd so that nothing in a name runs. New shells get the integration at their
  start (`terminal.shell_integration`, on): PowerShell through `-EncodedCommand`, bash through
  `PROMPT_COMMAND` (also in WSL via `WSLENV`), zsh through `ZDOTDIR`, cmd through `PROMPT`.
  New command `panel.open_terminal_here` (a shell in a panel's folder, in that or the other
  panel); `panel.get_state` reports a terminal panel's shell folder, prompt and follow status.
- **Properties, attributes, links and occupied space** (RC-020-06). Alt+Enter shows the
  properties of the entry under the cursor or of the selection (kind and link target, size and
  size on disk, created, modified and accessed, owner and mode on Unix, comment; sums for
  several) with Total Commander's "Change attributes" in the same dialog: read-only, hidden,
  system, archive, the Unix mode and both times, optionally down the folders for files, folders
  or both. Ctrl+Shift+F5 creates a `.lnk` shortcut (Windows, `IShellLinkW`), a symbolic link, a
  hard link or a junction; kinds the file system cannot make are greyed out, and a symbolic link
  without Developer Mode says so and points to the junction. Space counts the folder under the
  cursor into the size column, Alt+Shift+Enter all of them, in the background (Escape stops),
  Ctrl+L shows the occupied space of the selection; hard links count once. Ctrl+Z edits the
  comment in the folder's `descript.ion` (Total Commander's format), Ctrl+Shift+C copies the full
  paths of the selection. `Vfs` gains defaulted `property_capabilities`, `details`,
  `list_details`, `set_properties` and `create_link`; `rc-ops` counts space and applies
  attributes down a tree. Commands `file.properties`, `file.set_attributes`, `file.create_link`,
  `file.dir_size`, `file.set_comment`, `clipboard.copy_names`, `panel.select_and_size`,
  `panel.count_dir_content`, `panel.cancel_dir_sizes`; MCP tools `files_properties`, `files_dir_size`
  (read), `files_set_attributes`, `files_create_link`, `files_set_comment` (write; attribute
  changes and links refuse sensitive paths).
- **Connection manager** (RC-050-04, with the dialog of RC-050-06). Ctrl+F (`connections.manager`)
  or the status bar opens a dialog with the connection tree (filter by name or host; arrows,
  Enter, F2 rename, F7 new folder, F8/Delete delete after a question, Insert new connection,
  Ctrl+X/Ctrl+V and dragging move) and the form of a connection (protocol, host, port, user,
  login method, key file, password and passphrase, login script, view, start folder, notes;
  errors at the field). Secrets are typed only and never shown back; the master password is set
  or asked only when a secret is about to be stored. `connections.set_favourite` (MCP
  `connections_set_favourite`) chooses the drive bar's servers, by default the root level. The
  ssh_config import is a page of the dialog: preview, choice per host, target folder, key files
  into the vault only on request. Ctrl+Shift+F stays reserved for disconnecting (RC-060-01).
- **Shell context menu** (RC-080-02). A right click on a file, Shift+F10 or the context-menu key
  opens the Explorer's context menu for the selection in a local folder under Windows, with every
  shell extension, "Send to" and "Open with" filled (`IContextMenu2/3`) and rCommander's copy,
  rename and properties on top; a right click beside the rows or on `..` opens the folder's menu
  ("New", "Paste"). On servers, in archives, on Linux and macOS, and where the shell has no menu
  for a path, rCommander draws its own menu of the commands whose keys are bound. The selection
  works as in the Explorer: a right click outside it takes only the clicked entry. New command
  `panel.context_menu` (MCP: omitted, the menu is the window's).
- **Copy name and full path from the context menu** (follow-up of wave 5, RC-080-02,
  RC-020-06). The context menu of entries - above the Explorer's entries and in rCommander's own
  menu - has "Copy name" and "Copy full path", one line per selected entry. Full paths, also
  those of Ctrl+Shift+C, are the ones the panel shows: inside an archive the archive's path with
  the path inside, on an FTP or SFTP connection `ftp://` or `sftp://<name>/<path>`.
- **Offer to remove an unused vault** (follow-up of wave 5, RC-050-03). When the last stored
  password or key is deleted, the window asks whether the master password should go too:
  Remove runs `vault.remove`, Keep or Escape leaves the vault.
- **Multi-rename dialog** (RC-020-02). Ctrl+M (`multi_rename.open_dialog`) opens Total
  Commander's multi-rename tool on the selection: name and extension masks with placeholder
  buttons and a menu of the others, search and replace (`^`, `1x`, `[E]`, RegEx, Subst.), case,
  counter, a virtualised preview table with each conflict and its reason (Start is blocked while
  there is one), the order of the files changeable for the counter, presets with `<Default>`
  and "Last used", an optional log of the pairs carried out, Undo; usable without a mouse. The
  preview is debounced and made off the UI thread, the last input winning. A `\` in a new name
  moves the entry into a subfolder, created when missing, never out of the folder (`..`, roots,
  drives, shares and empty parts are conflicts). Presets are settings values with commands
  `multi_rename.preset_list|save|delete|rename` and their MCP tools.
- **Import from Total Commander** (RC-050-09). A page of the connection manager reads
  `wincmd.ini` (`%APPDATA%\GHISLER`, Total Commander's registry key, or a typed path) and
  `wcx_ftp.ini` (where `wincmd.ini` names it or beside it): the directory hotlist with its
  submenus, the folder tabs of both panels (locked ones stay locked, missing folders only on
  request, appended or replacing) and the FTP, FTPS and SFTP connections with their folders, all
  into folders named "Imported from Total Commander"; a preview with a box per row first, a second
  import adds nothing twice. Programs, internal commands, server and plugin paths of the hotlist
  are skipped with their reason. Its own INI reader takes UTF-16, UTF-8 and ANSI, at most 1 MiB
  per file, and never runs anything. Obfuscated FTP passwords are read only when "Take the
  passwords" is ticked, then decoded in zeroized memory straight into the vault; one behind Total
  Commander's master password is never read. Commands `import.total_commander` (MCP tool
  `import_total_commander`: a read for the preview, a write for the import, never a password) and
  `import.total_commander_passwords` (GUI only).
- **Drag and drop, files on the clipboard** (RC-080-01). Entries dragged from one panel onto the
  other open the copy dialog (with Shift the move dialog) for them, as in Total Commander; dragged
  out of the window under Windows they go to the Explorer, a mail or a browser upload through the
  shell's own data object and drag loop (`SHDoDragDrop`, drag image included). Files the Explorer
  drops onto a panel are copied into its folder (moved with Shift) by the operations queue, with
  progress and conflict questions; Ctrl+C and Ctrl+X put the panel's files on the clipboard as
  the Explorer does (`CF_HDROP`, "Preferred DropEffect"), Ctrl+V copies (after a cut: moves) the
  Explorer's files into the panel. Remote and archive entries are fetched into a temporary folder
  of the session first (a question above 256 MiB), which is removed when rCommander closes.
  Dropped and pasted paths are taken only as plain absolute paths without `.` or `..` parts, at
  most 10,000. No new engine command (MCP copies with `ops_copy`); Ctrl+C, Ctrl+X and Ctrl+V are
  `"window"` rows of the keymap.
- **Command palette and keyboard reference** (RC-080-05). Ctrl+Shift+P opens a palette that finds
  every action by its name or key and runs it as the key would, plus the commands without a key
  that need no parameters and the settings and database dialogs. The keys the window handles
  itself (Ctrl+Enter, Ctrl+Shift+Enter, Ctrl+P, Alt+F8, Ctrl+Shift+F, Ctrl+Shift+P, and Ctrl+C,
  Ctrl+X, Ctrl+V for files on the clipboard) are
  `"window"` rows of the keymap instead of stale reservations; every row has a name
  (`keys.<name>` in `en.json`). Every command without a key has a row in
  `app/palette/reach.json` saying how the keyboard reaches it or why not, and a test fails for a
  command with neither; `keymap/tc.json` lists rCommander's own keys, its deviations from Total
  Commander and the TC keys left free. `scripts/keyboard-reference.sh` generates the wiki page
  `reference/keyboard-shortcuts.md` from this data, its `--check` runs in the preflight;
  `docs/accessibility.md` states what iced 0.14 offers (no screen-reader support). The palette
  is a window key, not a registry command, so MCP has nothing new.
- **Diff window and synchronize-dirs dialog** (RC-020-04, GUI and queue part). Ctrl+Shift+F3
  (`compare.open_diff`) shows two files side by side in one windowed list (only the rows on screen
  are drawn), changed, deleted and inserted lines tinted, Alt+Down/Alt+Up to the next and
  previous difference, ignore white space, case and line endings; binary and large files show
  both sizes and the first differing byte. Alt+Shift+F2 (`sync.open_dialog`) compares the
  panels' folders in the background (Stop/Escape cancel), lists the rows with filters
  (differences, all, equal, left only, right only) and an action per row to click through, and
  asks before running: copies each way, deletes that can be unticked, permanent delete, ask
  before replacing. The plan runs as `rc-ops` copy, mkdir and delete jobs with the file
  operations' progress window and questions (`SyncPlan::jobs`, `Engine::sync_jobs`). New command
  `sync.apply`, MCP tool `dirs_sync_apply`: compares again, starts from the proposal or from
  nothing, sets actions by row path; `allow_deletes` or `overwrite` make the call destructive
  (confirmation code), without them nothing is deleted and existing targets are skipped; copies
  of keys and credential files are refused. New error code `sync_row_not_found`. Shift+F2
  (`compare.select_differences`, MCP `panel_select_differences`) selects in both panels the files
  the other side lacks, has older, or has in another size (by content: equal bytes are equal
  whatever their times); the conflict dialog of the file
  operations gains "Compare", which opens the diff window over the question.
- **Queue window like Total Commander** (RC-010-14). Each target backend of the queue keeps its
  waiting jobs in an order that can be changed: up, down, to the front; a free lane starts the
  first waiting job that is not paused. The queue window (Ctrl+Shift+Q - in Total Commander the
  Quick View window, which rCommander does not plan - or "Queue" on the background bar) lists every job with kind, source and target, lane, state and progress; it
  moves and removes waiting jobs, pauses and resumes one job or all, cancels a running one after
  a yes, clears the finished ones, and brings up a job's question. F2 in the F5/F6 dialog queues
  the job. New registry group `queue.*` (`list`, `move`, `remove`, `pause_all`, `resume_all`,
  `clear_finished`, `window`) with MCP tools `queue_list`, `queue_move`, `queue_remove`,
  `queue_pause_all`, `queue_resume_all`, `queue_clear_finished`; new error code
  `job_not_waiting`.
- **Transfers local ↔ SFTP** (RC-060-05). F5/F6 between a local and an SFTP panel copy and move
  in both directions through `rc-ops`: on a server a file is written as `<name>.rcpart` and
  renamed when complete, so a cancel or a lost connection never leaves a half file under the
  target's name, and an overwritten file stays until the new one is there; a move checks each
  copy's size on the target before it removes the source. A download waits in the server's lane:
  two jobs per connection run at once, four in all, the rest wait in the queue (settings
  `transfers.per_connection`, `transfers.max_running`, and `transfers.buffer_kib`, default 256
  KiB, on a new "Transfers" page). The F5/F6 dialog gains "Keep time stamps and permissions"
  (`JobOptions::keep_attributes`, on: the modification time, the Unix mode only between Unix
  sides); SFTP keeps 64 writes of 32 KiB in flight per file. `ops.copy`, `ops.move`,
  `ops.copy_to`, `ops.move_to` and the MCP tools `ops_copy`/`ops_move` take
  `sftp://<connection>/<path>` sources and targets (MCP: `keep_attributes`).
- **FTP in panels** (RC-070-03, without resume). An FTP or FTPS server opens in a panel like an
  SSH server: from the drive bar, Enter or "Open in panel" in the connection manager, and
  `ftp://<connection>/<path>` (also `ftps://`) in the path prompt; it starts in the connection's
  start directory or where the login starts (`PWD`), the path bar writes `ftp://<name>/`, the
  rights column shows when the server lists rights, Ctrl+Shift+F disconnects. The session is
  registered with the operations queue, so F3, F5 and F6 reach it: local and SFTP to and from FTP
  through the generic copy. `rc_remote::ftp` pools the connections per server: listings and
  commands share one control connection, transfers run on up to `FtpConfig::transfers` more
  (default 2) and one past the limit waits; idle connections get a `NOOP` every 30 s and close
  after 5 minutes unused. Listed names with a control character or a drive prefix (`C:x`) are
  dropped, so no download leaves its folder; SFTP listings judge names by the same rule
  (`rc_remote::listed_name`). The FTP test server joins rc-remote's `testkit`. `ops.copy` and
  `ops.move` open an `ftp://` target or source for the queue as they do an `sftp://` one, and an
  `sftp://<user@host>` location names a quick connection when no saved connection has that name
  (F5 into a quick connection's panel).
- **Network drives** (RC-010-16). `drives.map_network` maps a share `\\server\share[\...]` to a
  free drive letter through `WNetAddConnection2W` with `CONNECT_INTERACTIVE | CONNECT_PROMPT`
  (Windows asks for the credentials and keeps them; `CONNECT_UPDATE_PROFILE` for "Reconnect at
  sign-in"), `drives.unmap_network` disconnects one through `WNetCancelConnection2W`, forced only
  on request when files are open (`network_drive_in_use`). Folder and letter are checked before
  Windows sees them, Win32 answers become stable codes, the blocking calls run on the blocking
  pool (`rc-engine/src/drives/wnet.rs`, `windows-sys`, `unsafe` in that module only; elsewhere
  `network_drives_unsupported`). `drives.list` names a network drive's folder (`remote`). The
  window offers both under the drive menu's drives (Alt+F1, Alt+F2), in a drive's new context menu
  (right click on a drive button or menu row) and in the command palette, with no key of their
  own; a network drive's button names its folder. Mapping is GUI only; MCP disconnects with
  `drives_unmap_network`, destructive, its confirmation code bound to the drive and to `force`.
- **Resume of interrupted transfers** (RC-060-07). A copy or move with an SFTP or FTP/FTPS
  server on either side writes `<name>.rcpart` (downloads now on the local disk too) and keeps it
  when the connection breaks: the file is retried after 5, 10, 20 s on a reopened session and
  continues where the part ends (`Vfs::open_read_at`, `REST`, `APPE`), only while the source
  keeps its size and time and the part's last 64 KiB match - otherwise, and on a server without
  `REST`, it starts over. Spent retries pause the job as interrupted for Resume (settings
  `transfers.retry_count`, `transfers.retry_delay_secs`). The conflict dialog offers "Resume" for
  a part an earlier copy left (MCP `on_conflict: resume`). Such jobs are kept in `rcommander.db`
  (migration 0014, `transfer_jobs`, retained) and come back paused after a restart; `ops_list`
  shows them `resumable`, `ops_resume` continues, `ops_cancel` discards with the part.
- **Transfers between two servers** (RC-060-06). F5/F6 between two server panels - SFTP and
  FTP/FTPS in any pairing, two servers or the same one - stream the file from one to the other
  through rCommander, with no local file: the queue now reads and writes every copy at once
  through four buffers of its buffer size, so a slow target stops the source from reading on
  and memory stays at those buffers. Within one SFTP server the copy uses the panel's one
  connection; copies within one FTP server open their streams one at a time, so two of them
  never wait on each other's transfer connections. A failing target ends both streams at once
  and is named in the job's failure (`target`); resume, conflicts, pause and cancel work as for
  local transfers.
- **Design guidelines** (RC-080-08, stage 1). `docs/design.md` is the binding source for UI
  work (named in `AGENTS.md`): colour tokens, type, spacing, the small, medium and large dialog
  sizes, one button row at the bottom right in Windows order, text that never leaves its
  container, focus and selection states, tables, keyboard help lines, and the list of dialogs
  that still deviate. New theme building blocks: `theme::ellipsis::middle_ellipsis` (paths and
  names shortened in the middle by estimated font width, never inside a character) and
  `theme::layout` (`dialog_card` in the small and medium sizes, `button_row`, `primary_button`,
  `secondary_button`); the dialogs move onto them in stage 2.
- **Security review before the beta** (RC-080-03). Threat models `docs/security/vault.md`,
  `mcp.md` and `remote.md` (assets, actors, trust boundaries, each threat with its countermeasure
  and test, residual risks, findings). New rc-mcp tests over the whole tool list: every
  destructive and exec tool and every escalating call is refused headless, asks with codes, and
  a code answers its own call once (`tool_gate`); no tool unlocks the vault or trusts a host key
  or certificate, and an unknown host sees no login (`no_unlock_or_trust`); a canary sweep calls
  every tool unlocked and locked after real logins and finds no secret in answers, events, logs
  or data files, and planted leaks prove each check fails (`secret_canary_sweep`). A headless
  confirmation code is now bound to every argument of its call, not only to the terminal,
  connection or command it names; a call with more arguments than the question can show is
  refused; FileZilla's and WinSCP's password stores are sensitive paths; a stored vault row may
  name at most 1 GiB and 16 passes and must be version 1; an rc-db test holds every
  `_enc` column against `SECRET_COLUMNS`. `deny.toml`: every ignored advisory carries its reason,
  wildcard versions are denied.
- **About page and app icon** (RC-010-19). The settings' last page, About, shows the owner's
  logo (light or dark), version, commit, build time, platform, the addresses (marked "not yet
  published" while the repository is private), author and licence, with "Copy version info" for
  bug reports; its Licences tab lists every crate the binary links with its licence, a filter
  and a count per licence. The command palette opens it ("About rCommander"); MCP reads the same
  facts with `app.about` (`app_about`). The list is generated by `scripts/licenses.sh` from
  `cargo metadata` and `cargo tree` and held to `Cargo.lock` by tests. The exe, the window,
  Alt+Tab and the taskbar show the owner's icon (`docs/assets/`). The window title is fixed as
  Total Commander's, `rCommander <version> - © <author>`, instead of the active path; name,
  version and author come from `rc_engine::about`, the version only from the workspace manifest.
- **Fonts per area** (RC-010-17). Settings, Appearance chooses a font and a size (8 to 24) for
  the file lists (both panels, their tabs, the path line), the terminal and the viewer, from the
  families installed (iced's font database, no new dependency), with a "Monospaced fonts only"
  filter and a sample line; a change takes effect at once. A family no longer installed draws in
  the area's default. A list row's height and the fixed columns' widths follow the size, the
  paging counts rows of that height, the middle ellipsis measures in a monospaced font's
  advance. Six unprotected settings (`appearance.list_font[_size]`, `terminal.font[_size]`,
  `viewer.font[_size]`, type `font` or `int`) that `settings_get` and `settings_set` reach; at
  most 64 chosen names are kept for the program's life.
- **Browse... beside path fields** (RC-010-18). Every field that takes a local file or folder
  has "Browse...", which opens Windows' own open, save or folder dialog with the field's file
  types and its folder: the private key file of the connection manager and the quick
  connection, the database backup and restore file, the start folders, viewer and editor of the
  settings, the ssh_config and Total Commander files of the imports, the multi-rename log and
  the target folder of split and combine. The dialog runs on a thread of its own with the window
  as owner, so the window keeps drawing; the path chosen goes into the field as typed text, and
  Cancel changes nothing. Window only - no command, no MCP tool; hidden on Linux and macOS.
- **Folders in the selection** (RC-020-08). The setting `selection.include_folders` (Folders
  page, off by default, unprotected, so `settings_set` reaches it) lets Num+ and Num- select and
  deselect folders by a mask without a directory part too (`*.*` every folder; exclusions count
  for folders; `*\` works as before), and Shift+F2 also select the folders only one side has
  (`rc_engine::sync` gives such a folder a row without listing it). Alt+Num+/Alt+Num- stay with
  files. The MCP descriptions of `panel_select_by_mask`, `panel_deselect_by_mask` and
  `panel_select_differences` name the setting.
- **Terminal over the whole window width** (RC-040-05). `terminal.maximize`, Ctrl+Ä (Ctrl+' on
  a US layout, the same key `VK_OEM_7`), lays the active panel's terminal over the area of both
  panels and back; a panel showing its files turns to its terminal first (a shell starts when
  none fits, as Ctrl+Ö). The other panel is hidden, not closed; the drive bars, the folder tabs,
  the command line, the function-key bar and the status bar stay. The session runs on and its
  grid and PTY follow the new width. Both panels come back with the same key, when the panel
  turns to its files or its last terminal closes, and when the other panel becomes active. Kept
  from the shell like Ctrl+Ö; not stored, a start shows two panels. MCP: `omitted` (the
  window's layout only).
- **Icons on the dialogs' buttons** (RC-080-07). Save, Close/Cancel, New, New folder, Browse,
  Delete, Rename, Connect, Disconnect, Terminal, Test, Import, Copy, Move, Pause/Resume, Queue,
  Search, Stop and OK carry a Fluent icon before the label, in the button's text colour (light,
  dark, dimmed when disabled); the OK of the file dialogs shows their action (Copy, Move, New
  folder, Delete, Rename). The icons are a 4.5 KB subset of Fluent UI System Icons (MIT, in
  `NOTICE`, shipped in the package), compiled in as a font that `scripts/icon-font.sh` writes;
  `theme::layout::icon_button` is the one helper. Display only - no command, no MCP tool.
- **Several queue lists, saved and loaded** (RC-010-15). The queue holds named lists ("Backup
  NAS", "Upload web server"), each filled, ordered, started and stopped on its own, while the
  lanes and limits stay shared: lists on different targets run side by side, two lists on one
  disk or server take turns. A new or loaded list starts stopped and holds its jobs queued; the
  queue window shows one list at a time with New, Rename, Delete, Start/Stop, Save and Load, and
  moves a job to another list; F2 in the F5/F6 dialog puts the job into the chosen list and
  remembers it (`ops.copy_to`/`ops.move_to` take `queue`). A saved list keeps its copies and
  moves (paths and connection ids, no secret) in migration 0015 (`saved_queues`,
  `saved_queue_entries`, durable), optionally loaded again at the start; an entry whose
  connection was deleted is named as not loadable. New commands `queue.lists`, `create`,
  `rename`, `delete`, `start`, `pause`, `assign`, `save`, `load`, `delete_saved` (destructive)
  with MCP tools of the same names; new error codes `queue_list_not_found`, `queue_list_fixed`,
  `queue_name_invalid`, `saved_queue_not_found`, `queue_list_empty`.
- **Column widths and the panel divider draggable** (RC-010-20). The gap left of a fixed
  column's title in the file lists' header and the gap between the panels are grips (resize
  pointer): dragged, they set the column's width (the name takes the rest) and the panels'
  shares; a double-click restores the column's default width and the 50:50 split, as does the
  palette entry "Divider to 50 %" (no key). Widths are shared by both panels as in Total
  Commander, scale with the list font and give way down to their least widths in a narrow panel
  so the name keeps 80 pixels; each panel keeps 240. Stored on release as the unprotected
  settings `columns.{ext,size,packed,date,rights}_width` and `panels.split_permille`
  (Appearance), so MCP reads and sets them with `settings_get`/`settings_set`; no new command.
- **Archives across a restart, F3 and F4 inside an archive** (RC-030-02). A tab inside an
  archive comes back after a restart with the archive's columns; when the archive file is gone
  it opens the folder that held it (or the nearest one above) and the status line says so, with
  the path written through the archive file. Tabs selected or opened inside an archive keep the
  archive's columns too. F3 shows a file inside an archive in Lister; F4 hands the editor a
  read-only copy under `%TEMP%\rcommander\open-<pid>-<n>` instead of refusing.
- **F5 out of an opened archive** (RC-030-03). The copy dialog's `ops.copy_to` opens the archive
  backend for the queue, so a folder copies out with its structure, implied folders included;
  `ops.copy_to` into an archive and `ops.move_to` out of one are refused with
  `archive_read_only` like `ops.copy`/`ops.move`. Conflicts are the queue's question in the known
  dialog; "skip all" answers every later one (engine and window tests).
- **Long server listings shown as they arrive, owner column** (RC-060-02). `Vfs::list_batches`
  sends a listing in batches (SFTP one per `READDIR` answer, every other backend all at once);
  a panel whose listing runs longer than 200 ms shows what came, the status line counts it, and
  the rest follows sorted in the panel's order with cursor, selection and counted folders kept
  (`panel.navigate` answers `loading: true` meanwhile). Another folder, a refresh, a flat view
  or a tab ends the running listing, and SFTP stops reading after the block in flight; a listing
  that breaks off keeps what arrived and says why. `Entry` has an optional `owner` (user and
  group): from the SFTP `longname` (the attributes' numbers for `stat`), by name for local
  folders on Unix (`nix`), none on Windows, FTP and in archives; `list_dir`, `files_stat` and
  the search hits report it. The file lists get an "Owner" column, off by default (settings
  `columns.owner`, `columns.owner_width`); no new command.
- **Robustness and performance, stage 1** (RC-080-04). Every call on this machine's drives has a
  time limit (`rc_vfs::TimeoutFs`: 20 s for a call on one path and for a stream's step, 300 s for
  a listing once its folder answered) and ends with the new code `timed_out`; Escape stops a
  folder that keeps the panel waiting (`panel.cancel_io`, code `cancelled`; MCP omitted). Fault
  injection on the test servers: a silent SFTP server during an upload and a listing, an FTP
  server that stops sending, an SSH terminal whose server ends or goes silent. `scripts/perf.sh`
  measures large local folders through the engine (`crates/rc-engine/examples/perf.rs`), method
  and tables in `docs/performance.md`.
- **Import from Remote Desktop Manager** (RC-050-11). "Import RDM export..." under the
  connection manager's tree reads an export written without a master key (`.rdm`, XML, at most
  16 MiB, DTDs refused) and imports its SSH entries (SSH shells, PuTTY SSH, SFTP, SCP) into
  "Imported from Remote Desktop Manager", RDM's folders inside; preview first, FTP and FTPS
  listed as not supported yet (they follow later), RDP, VNC and the like as not supported, a
  protected export refused (`rdm_encrypted`). A second import finds its entries by RDM's ID and
  updates host, port, user and key path. Passwords are not taken: RDM's `SafePassword` is
  undocumented obfuscation (stage 2 No-Go). MCP tool `import_remote_desktop_manager` (preview by
  default, only `.rdm` files, no password); the MCP reading tools refuse `.rdm` files like
  `wcx_ftp.ini` (`rc_engine::RDM_EXPORT_PASSWORDS`).
- **Menus, tabs and terminal sessions by keyboard** (RC-080-16, part B). Ctrl+Shift+A (Total
  Commander's key, a window key of the keymap) lists the active panel's tabs, also those behind
  `»`; while the panel shows its terminal it lists the terminal's sessions instead and works from
  inside the terminal (a reserved key of the widget). In the lists Enter shows, Delete closes, and
  Shift+F10 or the Menu key open a tab's menu; Shift+F10 in the drive menu (Alt+F1, Alt+F2) opens
  the context menu of the drive under the cursor. A letter takes the one entry of a menu starting
  with it, or walks several (file and drive context menus, the tabs' popups). Ten of the 18
  commands without a way in got one in `reach.json` (checksums, split and combine through their
  dialogs, `compare.files` through the diff window, `compare.dirs` through the synchronize dialog,
  the host-key commands through the settings' Host keys page of RC-050-07); a test pins the eight
  that keep none to their written reasons. No new engine command, nothing for MCP.
- **Host keys page in the settings** (RC-050-07). One table of the trusted SSH host keys and FTPS
  certificates (server, port, type, SHA-256 fingerprint, trusted since), a search that also finds
  a hashed name by its host, Remove after a question (Delete, Enter, Escape keeps it) and the
  import of the user's own `known_hosts` with a note of new, known, changed and skipped entries;
  all through the existing `hostkeys.*` and `certificates.*` commands. Nothing on the page trusts
  a key; MCP still only lists them (`hostkeys_list`); no new command.
- **Every dialog by keyboard: the focus ring** (RC-080-16, part A). Tab and Shift+Tab go round
  every control of the dialog on top - text fields, checkboxes, option groups, pick lists, lists
  and trees, buttons - in reading order and come round, as in a native Windows dialog; the
  control with the keyboard shows WinUI's focus ring (2 px in the text colour). Enter presses the
  focused button, else the default one; Space presses a button or toggles a checkbox; the arrows
  choose in a pick list or option group. One building block, `theme::focus::Stop`, wraps a
  control; `theme::layout`'s buttons are stops, every dialog's checkboxes, pick lists, option
  groups and lists are wrapped (repo lint `focus_ring`), `app/field_walk.rs` walks them with the
  text fields. Dialogs without Tab got it (database, vault offer, SSH command, MCP question, drag
  question, queue window, diff window, file-operation questions and progress, hotlist editor);
  the connection manager follows the ring (filter, tree, buttons, form); multi-rename walks the
  ring instead of its fields; the SSH command and login-script editor take Enter as their default
  button. The MCP question opens with the ring on Deny and keeps Allow click-only. No new
  command.
- **F10 closes and quits; deleting for good where there is no recycle bin** (RC-080-17, owner
  2026-10-18). With a dialog open F10 does what Escape does, also from a text field; without one
  it quits rCommander through the new command `app.quit` (`GuiOnly`, MCP `omitted`: an agent
  never closes the person's window), and so do the main window's X and Alt+F4. With jobs running
  or waiting it asks first (Cancel the default; the queue is kept for the next start). In a
  terminal panel F10 stays the shell's. `keymap/tc.json` lists F10 as differs. New settings,
  readable and settable over MCP: `keys.f10` (`close_and_quit`, `close_dialogs`, `off`) on the
  new Keyboard page, `app.confirm_quit_with_transfers` and `delete.permanent_without_trash` (off
  by default) on the Transfers page. With the last one on, F8 and Delete where the folder has no
  recycle bin (a server, a network share or drive, WSL; Linux and macOS) open Shift+F8's
  question, which says it deletes for good, and delete permanently after OK; `ops.delete` and
  MCP's `ops_delete` are unchanged.
- **A native menu bar** (RC-080-18, owner 2026-10-18). The main window has the system's menu bar
  on Windows - File, Mark, Commands, Net, View, Configuration, Help, grouped as in Total
  Commander - built with `muda` 0.21 (MIT or Apache-2.0, no GTK). Every entry is a row of the
  command palette by its name and runs what that row runs; its key is shown from the keymap, no
  second binding table. Alt highlights the bar and Alt with an underlined letter opens a menu
  (a window subclass posts `SC_KEYMENU`, which winit never asks Windows for); F10 stays the close
  and quit key (RC-080-17). The bar, its popups and the Explorer's context menu are drawn dark
  with the app's dark look (the popups through uxtheme's undocumented `SetPreferredAppMode` and
  `FlushMenuThemes`, from Windows 10 1903 on, light where they are missing; owner 2026-10-18); the
  bar is greyed out while a dialog is open and attached without shrinking the window. The
  setting `appearance.menu_bar` hides it. The connections, database and settings links left the
  status bar for the menus. No new engine command; macOS (the system's menu bar) and Linux (no
  GTK) are written down in the job file.
- **Keys into the vault from the ssh_config import** (RC-050-06). A host ticked "Key into the
  vault" has its first `IdentityFile` taken in after the import (`secrets.import_key`), the master
  password asked only then. The result stands under the host's row; an encrypted key gets a
  masked passphrase field and "Take key", a wrong passphrase is said there, and one key that
  fails leaves the others going. `secrets.import_key` now reads only a key file on this computer
  (a drive or absolute path, or a WSL distribution's share): a network share, a device path, a
  URL or a relative path is refused with `key_path_refused` before the vault or the file is
  asked, so an `IdentityFile` cannot make the window open a share; the same rule holds at
  connect time for a connection's key path and a quick connection's key (a key in the vault
  still comes first). Over MCP the import never takes a key (`secrets.import_key` stays
  `gui_only`). No new command.
- **Drive line as in Total Commander** (RC-010-22). Each panel's drive line stands out (its own
  tone and a hairline, the active panel's in the accent), its drive button carries an arrow, the
  volume name follows in brackets (a server's or WSL distribution's name for those), and the line
  ends with `\` and `..`, which run the keymap rows `root_folder` and `parent_folder`
  (`panel.navigate`), disabled at a root; name and free space are shortened in the middle when the
  panel is narrow. New settings `panels.drive_line_volume` and `panels.drive_line_buttons`
  (Appearance, on by default). The line moved into `app/drive_bar/line.rs`. No new command.
- **Archives against 7-Zip and GNU tar** (RC-030-04). An opt-in test,
  `rc-ops/tests/external_tools.rs` (`--run-ignored only`), packs every format and has 7-Zip and
  GNU tar test, list and unpack it byte for byte, and opens and unpacks what those tools pack;
  a missing tool skips its half.
- **Live SFTP tests against real servers** (RC-080-14, SFTP part). `scripts/live/sftp/run.sh`
  starts OpenSSH servers in Docker (Alpine pinned by digest, 127.0.0.1 only, dummy credentials),
  runs the opt-in suite `crates/rc-engine/tests/live_sftp.rs` (`#[ignore]`, skipped without the
  servers) and removes every container afterwards. The engine without a window checks host keys
  (no login attempt before trust, a changed key refused with both fingerprints and replaced only
  by `hostkeys.replace`), the SFTP panel, a big file up and down byte for byte with `.rcpart`,
  time stamps, cancel and move, a quick connection, server to server, resume after dropped
  sessions, a hanging network, spent retries and a restart, the `sudo -i` login script,
  `ssh.exec` and a locked vault.
- **Export and import of settings and connections** (RC-080-19). The database dialog gains
  Export and Import (Configuration menu, palette, the settings' Import/Export page): settings,
  connections with their folders and login scripts, trusted host keys and FTPS certificates,
  hotlist and tabs, each part chosen, into one `.rcexport` file - unencrypted, or sealed with an
  export password of its own (argon2id, XChaCha20-Poly1305; `rc_vault::seal_export`), never the
  vault's key. Passwords, keys and passphrases go only on request, into an unencrypted file only
  after its warning. The import reads the file as hostile input (8 MiB, format, version, seal,
  caps per field), previews every row as new, already there, changed or not imported, writes
  secrets straight into the vault and recognises connections by their mark, so a second import
  adds nothing. A changed host key or certificate, a protected setting and a connection whose
  server changes while it has stored secrets are written only when ticked; a wrong password or
  a damaged file changes nothing. `config.export` and `config.import` are GUI only (no MCP
  tool); the last export's parts and encryption are the settings `exchange.export.*`.
- **One entry for every live suite** (RC-080-14). `scripts/live-tests.sh [sftp] [ftp]` runs the
  SFTP suite and the FTP suite under one build lock, every suite even after one failed, removes
  every `rc-live-*` container (the FTP suite's unnamed test runners too) on success, failure and
  Ctrl+C, and fails when one is left; a script test (`scripts/tests/live-tests.sh`) holds that
  with a stand-in `docker`. New in the suites: `rcommander mcp` served headless by the binary
  itself (`crates/rcommander/tests/live_mcp.rs`: connection tree, listing, a copy both ways,
  `ssh_exec` refused without a person and run with a code, an untrusted server refused), and FTP
  server to server (`live_ftp_s2s_*`: FTP to FTP, FTP to SFTP and back, on vsftpd, pure-ftpd and
  ProFTPD, with an OpenSSH container in each FTP server's network namespace), and vsftpd on
  2128 listing `EPSV` in `FEAT` and refusing it - ProFTPD's denied `EPSV` leaves `FEAT`, so the
  fallback to `PASV` was never tried and a fault planted there stayed green; now it fails 4 of 4.
  The FTPS runs - explicit and implicit, the certificate questions - now run green on all three
  servers.
- **Several queue jobs at once** (RC-010-15, owner 2026-10-18). The queue window selects as the
  Explorer and Total Commander do (click, Ctrl+click, Shift+click, Space/Insert, Shift+arrows,
  Ctrl+A); the Reorder keys and buttons, *Move to list* and Delete take the selection, which keeps
  its order when moved and may split a batch. A context menu (right click, Shift+F10, the menu
  key) adds *Move to the end*, *Move to a new list...*, *Select all*, *Clear this list* and
  *Clear all lists*; one question names the started or unfinished jobs, which are cancelled as a
  single cancel does. Pause and resume moved from Space to P. Commands and MCP tools
  `queue.move_many`, `queue.assign_many`, `queue.discard`, `queue.clear` (Write, as
  `ops.cancel`); `queue.create` takes jobs, `queue.move` a step `back`.
- **Selecting files and folders with the mouse, as in the Explorer** (RC-010-02). Ctrl+click
  selects or deselects one row, Shift+click selects the rows from the last clicked one (else the
  cursor) to the clicked one in place of the selection before, Ctrl+Shift+click adds them; the
  cursor follows, `..` is never selected, the brief view works the same. A plain click moves the
  cursor only, as in Total Commander; the setting `mouse.click_clears_marks` (off) makes it clear
  the selection as in the Explorer, after a press on a selected row only when no drag follows. A
  click with Ctrl or Shift starts no drag. The marks are the engine's `panel.toggle_selection` and
  `panel.deselect_all`, so MCP sees them.
- **Checksum files verified with Enter** (RC-020-10). Enter or a double click on a `.sfv`,
  `.md5`, `.sha`, `.sha1`, `.sha256`, `.sha512`, `.b3` or `.blake3` file starts Ctrl+Shift+F10's
  check for exactly that file (the same `rc-ops` job and result list, bad lines first); the
  context menu of such files offers *Verify checksums*. The setting `checksum.enter_verifies`
  (Folders, on) turned off lets Enter open the file as before. The extensions are
  `rc_checksum::Algorithm::of_file_name`; `file_open` over MCP is unchanged.
- **Markdown preview in Lister and Quick View** (RC-020-09). A `.md` or `.markdown` file opens
  formatted - headings, emphasis, lists, quotes, code blocks in the viewer's font, tables, links -
  through iced's own `markdown` widget, in the theme's colours; `5` (Total Commander's formatted
  HTML view) switches to the text and back. Setting `viewer.markdown_preview` (Appearance, on).
  The file is foreign input: images are never loaded (alt text only), raw HTML is shown as text,
  links open only on a click and only as `http`, `https` or `mailto` through the system, as
  `file.open` hands a file; files over 4 MiB open as text with a note; reading and parsing run
  off the UI thread.
- **Two bars in the progress window, file and total** (RC-010-07, owner 2026-10-19). As in Total
  Commander's copy dialog: *File* shows the bytes of the file at work (from 0 at each file, from
  the offset for a continued one), *Total* the whole operation - every job of its batch, the
  finished ones kept, a job not counted yet estimated by the counted ones - with counts, speed
  and time left. Speed, time left and the time elapsed cover the whole operation too: measured
  over the batch's bytes and files in a 5 s window that goes on from job to job, the time left
  the longer of bytes and files left at their rates, "about" while a job is not counted yet; MCP
  keeps each job's own speed and time left. `rc_ops::Progress` carries `file_bytes_done` and `file_bytes_total` (copy,
  move, server to server, unpack, pack, archive test), and so does the MCP `ops_list` answer.
  Background rows and the queue window keep their one bar.
- **Robustness and performance, stage 2: budgets and a soak run** (RC-080-04). `scripts/perf.sh`
  also measures the start until the first panel's listing is there (a new database and one that
  is there) and the idle memory; `--check` fails when the idle memory, the peak at 1 000 000
  entries or the start time is over its budget (`scripts/lib/perf.py`). `scripts/soak.sh --hours
  H` runs the queue's copies, moves, deletes and folder creations for hours
  (`crates/rc-engine/examples/soak.rs`), samples memory, descriptors, threads and jobs, and fails
  on growth (`scripts/lib/soak.py`). The live 100 000-entry SFTP listing prints its memory.
- **`.nfo` and `.diz` files in the viewer, in code page 437** (RC-020-11; owner, 2026-10-19).
  `TextEncoding::Cp437` (DOS, a table of its high half - `encoding_rs` has none) joins the one
  text decoding; Lister and Quick View open a file whose extension is in `viewer.dos_extensions`
  (Settings, Appearance; default `nfo diz`, a new `extensions` setting kind) in CP437 and without
  wrapping, unless a byte-order mark, UTF-16 or non-ASCII UTF-8 says otherwise. `S` switches to
  CP437 as in Total Commander's Lister, `A` back to ANSI, `E` cycles through six encodings; the
  binary view shows CP437. MCP `files_read_text` detects it the same way and takes
  `encoding: "cp437"`.
- **Plugin contract and runtime, stage 1** (RC-100-08; owner, 2026-10-20). Three crates copied
  and adapted from rDownloader (26c8592a6): `rc-plugin-types` (`Failure`), `rc-plugin-api`
  (`rcommander:plugin@0.1.0` with `types`, `host`, `source`, `sink` and the viewer world exactly
  as `docs/plugins.md` shows it, the native `Viewer` trait) and `rc-plugin-host` (Wasmtime 49:
  shared engine, checksummed compile cache, imports only as the kind and manifest allow, manifest
  version 1, plugin locales; `ViewerPlugin` and a `ViewerSession` whose store lives for one
  document with fuel and deadline per call, fuel credited per byte read or written, cancellation
  through `should-stop` and the epoch, every answer validated, an in-memory execution log and
  suspension after `plugins.crash_limit` failures in five minutes). Tests run small components
  that crash, hang, flood the log and answer hostile output. Not in the window yet (RC-100-13);
  no new command, so no MCP tool. `wasm32-unknown-unknown` joins the toolchain, `wasm-tools`
  1.261.0 the pinned tools.
- **The bundled JSON viewer plugin, its logic** (RC-100-11; owner, 2026-10-20).
  `plugins/json-viewer/` (MIT, after rDownloader's `plugins/` at `26c8592a6`, shared fields in
  `plugins/metadata.toml`) offers `highlighted` and `tree` for `.json` and `.geojson`: tokens
  per line (`key`, `string`, `number`, `literal`, `punctuation`, `invalid`, at most 4096 a line),
  the text as in the file or indented (`[[settings]] format`, `as_is`/`pretty`), and a lazy tree
  whose nodes are value offsets, with containers over 1000 children in ranges and the first
  error as a node with line and column. `open` streams the file once in 1 MiB windows into an
  index bounded at 16 MiB (line checkpoints, ends, counts and anchors of large values);
  `tokens` and `children` read from the nearest checkpoint or anchor. Native unit tests, the
  100 MiB case `#[ignore]`d; the guest binding follows with the contract crate.
- **The bundled batch viewer** (RC-100-19): `plugins/batch-viewer/` (`rc-plugin-batch-viewer`
  0.1.0) for `.bat` and `.cmd`, built as the YAML viewer is (one streamed pass, a bounded line
  index, no dependency), output `highlighted` only. Commands, `if`'s words and switches as
  `keyword`; labels (also after `goto`/`call`) as `function`; `%x%`, `%~dp0`, `%%i`, `!x!` and
  the `%x:a=b%`/`%x:~0,3%` forms as `key`; strings, numbers, `rem`/`::` comments, operators,
  redirections and comparisons; `echo`'s text uncoloured but for variables. A `^` continuation
  and open `( ... )` blocks carry from line to line. The setting `encoding` (`auto`, `utf-8`,
  `cp850`, `cp437`, `windows-1252`; `auto` is UTF-8 when the file is, else cp850) with the three
  code pages as tables of the plugin; the text is always UTF-8 and token offsets refer to it.
  Contract 0.2.0, importing `settings`; native unit tests, the 100 MiB case `#[ignore]`d.

- **Plugins in the Configuration menu, tabs on the Plugins and Appearance pages** (RC-100-20).
  Configuration > Plugins... opens the settings on the Plugins page, tab Installed. One tab
  strip for a dialog page, `theme::page_tabs` (WinUI's pivot: accent bar under the chosen tab,
  one stop of the focus ring, the arrows and a click choose, Ctrl+Tab and Ctrl+Shift+Tab turn
  round in the settings window). The Plugins page has Installed (install with the preview, the
  table, the selected plugin) and Global settings (the `plugins.*` rows); a package's preview
  turns to Installed. Appearance has Theme and window, Fonts, Panels and lists, Viewer, each
  schema row in exactly one (`settings/tabs.rs`); the About page uses the same strip. The tab
  each page shows is kept for the session, never stored. No new command, no MCP tool.
- **Shared text encodings for the plugins, `crates/rc-plugin-text`** (RC-100-22). `Encoding`
  (UTF-8, UTF-16 LE and BE, Windows-1252, cp850, cp437; `FromStr` for the setting values
  `utf-8`, `utf-16le`, `utf-16be`, `windows-1252`, `cp850`, `cp437`), `detect(prefix, fallback)`
  (a byte-order mark of UTF-8 or UTF-16 and its length, else UTF-8 when the prefix is valid, a
  character cut at its end included, else the fallback) and the streaming `Decoder`
  (`push`/`finish`: a UTF-8 character or UTF-16 pair over a window boundary stays whole, what is
  invalid becomes U+FFFD as the standard library's lossy conversions write it). No dependency,
  MIT, builds for wasm32; a plugin takes `rc-plugin-text.workspace = true`. The code-page tables
  of the batch viewer moved here.
- **The bundled config viewer** (RC-100-22): `plugins/config-viewer/`
  (`rc-plugin-config-viewer` 0.1.0, contract 0.2.0 with `settings`) for INI and its kin (`.ini`,
  `.inf`, `.cfg`, `.conf`, `.editorconfig`, `.gitconfig`), registry exports (`.reg`), `.env`,
  Java `.properties`, TOML (`.toml`, `Cargo.lock`; `probe` declines other `.lock` files),
  `.gitignore` and `.gitattributes`, the format chosen by name. `highlighted`: sections, tables,
  arrays of tables and registry keys as `keyword`, keys, strings, numbers, literals (booleans,
  TOML dates), registry types, comments, punctuation, `invalid`; the state per checkpoint knows a
  `\` continuation, a quoted `.env` value, TOML multi-line strings, arrays and inline tables over
  lines. `tree`: the keys before the first section, then the sections; TOML tables nested by
  dots, `[i]` per array element; `.env`, properties and the git files flat; more than 1000
  children in ranges, at most 16384 sections kept. Setting `encoding` (`auto`: byte-order mark,
  else UTF-8 when the first 64 KiB are, else Windows-1252; `utf-8`, `utf-16le`,
  `windows-1252`), decoded through `rc-plugin-text`. One streamed pass, an index of at most
  16 MiB; tested natively up to 8 MiB, 100 MiB as an ignored test.

- **The bundled diff viewer** (RC-100-24): `plugins/diff-viewer/` (`rc-plugin-diff-viewer`,
  contract 0.2.0 without `settings`) for `.diff`, `.patch` and `.rej`, and for files starting
  with `diff ` or `From <commit> ` (`git format-patch`; `probe` declines any other `From `).
  `highlighted` first, with the fixed token classes: added lines `number`, removed lines
  `invalid`, hunk headers `keyword` with git's function context `function`, file and extended
  headers (`diff`, `index`, `---`, `+++`, modes, renames, copies, binary) `type-name`,
  `\ No newline at end of file` and the text outside the diff (a mail header, a signature)
  `comment`. Unified, combined (`@@@`, a column per parent) and context diffs (`***`/`---`,
  `!`); a hunk is counted by its header, so a removed `--- x` inside it stays a removed line.
  `tree`: a node per file (path, `+n -m`, new/deleted/renamed/copied/binary), its hunks under it
  (header, line count), ranges of 1000, at most 32768 files and 131072 hunks with a last node
  counting the rest. One streamed pass, index and tree within 8 MiB, the line state at every
  checkpoint; invalid UTF-8 as U+FFFD. `--- ` is no magic: a declining `probe` would take YAML
  documents starting so from the YAML viewer. Native unit tests, the 100 MiB case `#[ignore]`d.

- **The bundled PowerShell viewer** (RC-100-23): `plugins/powershell-viewer/`
  (`rc-plugin-powershell-viewer` 0.1.0) for `.ps1`, `.psm1` and `.psd1`, built as the batch
  viewer is (one streamed pass, a bounded line index of checkpoints with the lexer's state),
  output `highlighted` only. Keywords; `Verb-Noun` cmdlets and the names after
  `function`/`filter` as `function`; variables (`$x`, `$env:PATH`, `${a b}`, `$_`) and splatting
  `@args` as `key`, `$true $false $null` as `literal`; `[types]` and attributes as `type-name`;
  parameters, `-eq`-style operators, `| ; && || = += ..` and redirections as `operator`; strings
  with their variables and `$( )` coloured inside, here-strings of both kinds, numbers with
  `kb mb gb` and `0x`, `#` and `<# #>` comments. Strings, here-strings, block comments, `$( )`
  depth and a backtick continuation carry from line to line; a string left open at the end is
  `invalid`. The setting `encoding` (`auto`, `utf-8`, `utf-16le`, `windows-1252`; `auto` follows
  a byte-order mark, else UTF-8 when the file is, else Windows-1252) decodes through
  `rc-plugin-text`; the text is UTF-8 and token offsets refer to it. Contract 0.2.0, importing
  `settings`; native unit tests, the 100 MiB case `#[ignore]`d.

- **The bundled log viewer** (RC-100-25): `plugins/log-viewer/` (`rc-plugin-log-viewer` 0.1.0)
  for `.log`, `.out` and `.err` up to 1 GiB, output `highlighted` only, built as the batch viewer
  is (one streamed pass, a bounded line index). Levels in any of three cases - errors (also
  syslog's `err crit alert emerg`) `invalid`, warnings `function`, `INFO NOTICE` `type-name`,
  `DEBUG TRACE VERBOSE` `comment` - also as `[ERROR]`, `level=error`, `"level":"error"`, logcat's
  `E/Tag:` and glog's `E1020`; ISO 8601, clock, syslog, Apache, logcat and Unix-seconds
  timestamps `number`; IPv4/IPv6, URLs and Windows/Unix paths `literal`; `key=value` and
  `"key":` `key`; quoted strings; Java, .NET, JavaScript and Python stack-trace lines
  `type-name`; brackets. Settings `level_colour` (`word`, `line`: the whole line of an error or
  warning and its stack trace in the level's colour, the one state carried over lines) and
  `encoding` (`auto`, `utf-8`, `utf-16le`, `windows-1252`) through the shared `rc-plugin-text`,
  lines split by UTF-16 unit; the text is UTF-8 and token offsets refer to it. With `word` the
  pass lexes nothing; a line is lexed once from its start. Contract 0.2.0, importing `settings`;
  native unit tests, 8 MiB in the normal run, the 100 MiB case `#[ignore]`d.

- **The bundled web code viewer** (RC-100-26): `plugins/web-viewer/` (`rc-plugin-web-viewer`,
  contract 0.2.0 without `settings`) for `.html`, `.htm`, `.xhtml`, `.vue`, `.css`, `.scss`,
  `.less`, `.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.mts`, `.cts`, `.tsx`, `.php`, `.phtml`;
  `highlighted` only, no setting. One lexer module per language (HTML, CSS with SCSS and LESS,
  JavaScript with TypeScript and JSX, PHP) under a stack of frames that is the line state at
  every checkpoint: `<script>` and `<style>` (dialect by `lang`/`type`), `style` and `on...`
  attributes, PHP's `<?php`/`<?=` anywhere in its page, a template string's `${...}`, JSX
  elements and their `{...}`; at most 64 deep, the rest of a deeper line `invalid`. An embedded
  language ends where a browser ends it (`</script` inside a string, an attribute's quote).
  JavaScript's regular expression against division by the token before it, across lines; TS
  type arguments and annotations. Invalid UTF-8 is written as U+FFFD, token offsets count the
  text written. One streamed pass, an index of at most 8 MiB; native tests up to 8 MiB, 100 MiB
  as an ignored test.

- **The bundled SVG viewer** (RC-100-28): `plugins/svg-viewer/` (`rc-plugin-svg-viewer`,
  contract 0.2.0 with `settings`) for `.svg` and `.svgz`, `probe` taking `<svg` in the first
  64 KiB or a gzip stream. `image` first - drawn with resvg 0.45 (no default features: `text`,
  `raster-images`) as `rgba:<w>x<h>`, straight alpha, the longest side the setting `size` (`512`,
  `1024`, `2048`), the background the setting `background` (`transparent`, `white`, `black`), an
  SVG without a size at its `viewBox` - then `highlighted`, the source coloured by the XML
  viewer's lexer and line index taken as a library (`rc-plugin-xml-viewer`). Text in Lato Regular
  (OFL-1.1, `fonts/`), the only font, for every `font-family`. Only `data:` images; an `href` to a
  file or `http(s)` resolves to nothing, a gzip stream in a `data:` URL is not inflated. A file
  and an inflated `.svgz` at most 64 MiB; a document nested deeper than 256 elements is refused
  by a byte scan before anything parses it (roxmltree recurses per level), also inside a
  `data:` URL; one whose `<use>` references expand beyond 2^20 elements before usvg builds it
  (one iterative pass, cycles counted once).
  Limits 256 MiB, 30 s, 10^10 fuel.
- **Settings reference generated from the schema** (RC-100-09). `scripts/settings-reference.sh`
  writes the wiki page `reference/settings.md`: every row of `SETTINGS`, page by page in the
  settings dialog's order (Appearance by its tabs), with label, key, values, default, help text
  and whether it is protected or needs a restart (labels and titles without their `&`
  access-key marks, as the window draws them); plugins' own settings link to
  `using/plugins.md`. It compiles nothing: `scripts/lib/settings_reference_rust.py` evaluates the
  schema's `const` items and `const fn` helpers, following constants into `rc_ops`, `rc_vault`
  and `rc_plugin_host`. `--check` (a stage of `check.sh --preflight`) exits 1 on a stale page and
  2 when a row does not read (named), a spec constant is missing from `SETTINGS`, or a text, page
  or kind is unknown; `scripts/tests/settings-reference.sh` covers it.

### Changed

- **Version 0.6.0**: copy and paste in terminals as in Windows Terminal (RC-040-07: Ctrl+C, a
  right click and Shift with the mouse copy, Ctrl+V pastes, a program's OSC 52 copy reaches the
  clipboard), Alt with a letter left to a terminal (RC-080-18), a new folder made at once
  (RC-010-14), the active panel's border all round and the MCP bridge ending with rCommander
  (RC-000-05), for Windows and Linux. `scripts/set-version.sh 0.6.0`; the packages are
  `rcommander-0.6.0-windows-x64.zip` and `rcommander-0.6.0-linux-x64.tar.gz`.
- **Version 0.5.2**: executing MCP calls allowed for a while on their terminal or connection
  (RC-010-27) get a package of their own, for Windows and Linux. `scripts/set-version.sh 0.5.2`;
  the packages are `rcommander-0.5.2-windows-x64.zip` and `rcommander-0.5.2-linux-x64.tar.gz`.
- **Version 0.5.1**: terminal programs ending with rCommander (RC-040-06) and the clearer
  login-script editor (RC-050-13) get a package of their own, for Windows and Linux.
  `scripts/set-version.sh 0.5.1`; the packages are `rcommander-0.5.1-windows-x64.zip` and
  `rcommander-0.5.1-linux-x64.tar.gz`.
- **Version 0.5.0**: the menu bar drawn on Linux (RC-080-33) gets a package of its own, for
  Windows and Linux. `scripts/set-version.sh 0.5.0`; the packages are
  `rcommander-0.5.0-windows-x64.zip` and `rcommander-0.5.0-linux-x64.tar.gz`.
- **Version 0.4.7**: the setting whether Space moves the cursor on (RC-020-13) gets a package
  of its own, for Windows and Linux. `scripts/set-version.sh 0.4.7`; the packages are
  `rcommander-0.4.7-windows-x64.zip` and `rcommander-0.4.7-linux-x64.tar.gz`.

- **Version 0.4.6**: shared credentials for several connections (RC-050-12) get a package of
  their own. `scripts/set-version.sh 0.4.6`; the Windows package is
  `rcommander-0.4.6-windows-x64.zip`.

- **Version 0.4.5**: the Windows Credential Manager as a secret source (RC-100-37) gets a
  package of its own. `scripts/set-version.sh 0.4.5`; the Windows package is
  `rcommander-0.4.5-windows-x64.zip`.

- **Version 0.4.4**: masked plugin settings and the secret-source marks in the Plugins list
  (RC-100-36) get a package of their own; the Bitwarden plugin 0.2.0 needs it.
  `scripts/set-version.sh 0.4.4`; the Windows package is `rcommander-0.4.4-windows-x64.zip`.

- **Version 0.4.3**: a connection's secret from a secret source (RC-100-34) gets a package of
  its own. `scripts/set-version.sh 0.4.3`; the Windows package is
  `rcommander-0.4.3-windows-x64.zip`.

- **Version 0.4.2** (owner, 2026-10-22: "das 0.4.2-Paket mit TESTLIST neu bauen"): the
  extension in the rule's colour and the settings' scroll bar at the right edge get a package of
  their own. `scripts/set-version.sh 0.4.2`; the Windows package is
  `rcommander-0.4.2-windows-x64.zip`.

- **Version 0.4.1** (owner, 2026-10-22: "halt paket 0.4.1"): the colour dialog of the Colours
  tab and the flate2 fix get a package of their own. `scripts/set-version.sh 0.4.1`; the Windows
  package is `rcommander-0.4.1-windows-x64.zip`.

- **Version 0.4.0** (owner, 2026-10-22: "RC-100-43 umsetzen und in version 0.4 builden"): the
  colours as in Total Commander get a minor version. `scripts/set-version.sh 0.4.0`; the Windows
  package is `rcommander-0.4.0-windows-x64.zip`.

- **Version 0.3.1** (owner, 2026-10-22): the fix for the lost graphics device and the flooded
  error log gets a package of its own. `scripts/set-version.sh 0.3.1`; the Windows package is
  `rcommander-0.3.1-windows-x64.zip`.

- **Version 0.3.0** (owner, 2026-10-22: "wir gehen mir dem nächten build auch auf version
  0.3"). `scripts/set-version.sh 0.3.0`: the workspace, `Cargo.lock`, the SDK template's
  `min_app_version` and the CI template's release; the Windows package is
  `rcommander-0.3.0-windows-x64.zip`.

- **MCP tools dispatch through one helper** (RC-080-27). `RcMcpServer::dispatch_mcp` (and
  `dispatch_as` with a view) replaces the 36 hand-written `respond(dispatch_mcp(..))`
  expressions and the three private wrappers in `rc-mcp`'s tool modules; the confirmation code
  takes its hex from `rc_engine::exchange::hex`. Tool names, schemas, answers, policy and
  masking are unchanged.
- **Two dependencies fewer: `fs4` and `dunce`** (RC-080-31). The free and total space of a drive
  and `Vfs::free_space` come from one function, `rc_vfs::disk_space` (`GetDiskFreeSpaceEx`
  through `winsafe` on Windows, `statvfs` through `nix` elsewhere); the `\\?\` prefix of a
  symlink's target is stripped by `rc-vfs`'s own `strip_verbatim`, also for `\\?\UNC\` paths.
- **Quick search, filter and masks compare without allocating per name** (RC-080-30).
  `rc_ops::mask::wildcard` takes the name as any cloneable iterator of characters and
  `rc_ops::mask::lowercase` lowers a name on the fly, so a keystroke in a folder of 100 000
  entries no longer makes two heap allocations per entry; `registry::spec` looks a command up in
  a map instead of walking the table. `TermSession::open_local` (no caller) is gone, the
  duplicate `rc-plugin-types` dev-dependency of `rc-engine` too, and eight functions only tests
  call are built for tests only.

- **One definition per GUI helper** (RC-080-26). The seventeen local copies of `theme::fit::line`
  and `label` in `crates/rcommander/src/app` are gone: `fit::label`, `fit::label_or_glyph` and
  `fit::hint` join `fit::line` and `fit::prose`; `app/escape.rs` exports `enter()` beside
  `escape()`, and the four copies and the hand-written `Enter`/`Escape` comparisons use them;
  `layout::dialog_title` draws a head's title, `Message::titled_head` and `Message::overlay`
  build the head and the card the simple dialogs share. No text size, font, wrapping or width
  changes.
- **One helper each for the small conversions every crate repeated** (RC-080-29). `rc-vfs`
  now carries `unix_time` and `unix_seconds` (seconds since 1970 to a time and back, never a
  panic on a server's or an archive's number), `Entry::modified_unix`, `Attributes::from_mode`
  (the read-only rule of a Unix mode), `drive_letter` and `split_name` (a name's stem and
  extension as Total Commander splits them); the archive readers, the SFTP and FTP listings, the
  conflict and tar writers, the MCP answers, the sort key, the command line, the imports and the
  key-path guard call them. `rc-plugin-host` encodes hex in one place. A search date bound past
  what the clock can hold no longer panics the engine. Call sites whose edge cases differ keep
  their own code.

- **One helper per test set-up** (RC-080-32). The hand-written folder trees of the engine's and
  the app's tests are tables for `test_support::tree` and the app harness's `tree`; the app's
  `harness()` copies call the `Harness` constructors (`open_on_db` and `open_with_db` are new);
  `rc-term`'s three fake back ends are one `test_support::FakeBackend`, the viewer's three fake
  document sources one `harness::FakeSource`. No test and no assertion changed.
- **One set-up for the MCP server's tests** (RC-080-28). `rc-mcp`'s `test_support::Fixture`
  builds what 46 test files built by hand - a temporary root, a headless engine, a database and
  the context - with `with_db`, `with_codes`, `with_vault`, `with_created_vault` and
  `with_agent_sources` for the deviations; the integration tests include the module by `#[path]`.
  No test checks anything else than before.
- **A file goes to the next plugin when `probe` declines** (RC-100-27). `ViewerPlugins::claim`
  becomes `claims`, the plugins asked in turn: the one `plugins.associations` assigns (within its
  `max_size`; too large, the file goes on to the others), then those whose extension matches,
  then those whose magic matches, each by name, no plugin twice, at most four; images and
  Markdown stay built in unless assigned. `open` asks them in turn: only a `false` from `probe`
  moves on, a failure stays the notice over the built-in viewer and counts towards suspension,
  the stop flag is checked before each `probe`; all declining leaves the built-in viewer. The
  diff viewer 0.1.1 has the magic `--- ` back, `probe` wanting a `+++ ` line after it: a YAML
  document starting `--- ` goes to the YAML viewer first, a `diff -u` saved without an extension
  to the diff viewer.

- **The preflight's 500-line rule reads `plugins/` and `sdk/`** (owner, 2026-10-20). `scripts/check.sh
  --preflight` looked only at `crates/` and `scripts/`, so a long plugin source surfaced first in
  the compiled `repo_lints/file_length` test - after the merge, for a wave agent who compiles
  nothing. The stage now walks `crates plugins sdk scripts`.

- **Version 0.2.0** (owner, 2026-10-20: "ja, heb die app-version mit scripts/set-version.sh auf
  0.2.0 an"). `scripts/set-version.sh 0.2.0`: the workspace, `Cargo.lock`, the SDK template's
  `min_app_version` and the CI template's release; the Windows package is
  `rcommander-0.2.0-windows-x64.zip`.

- **The queue window shows every job, and the background bar is its preview** (RC-010-25;
  owner, 2026-10-20). The jobs F5, F6 and F8 confirmed with OK run at once now appear in the
  queue window, on the default list, as a group of their own above the queued jobs, marked
  "Runs at once" - the finished ones with the finished. Pause, resume and cancel (per job and
  for all) work on them; moving them, putting them into a list and saving them are dimmed in the
  window and refused by the engine (`job_at_once`). They keep running at once. The bar under the
  panels shows the queue window's jobs in the window's order (one shared filter), the first
  `queue.strip_rows`, the job in the progress window aside; "N more in the queue" counts the
  rest. New setting `queue.show_at_once` (Settings, Transfers; on): off, the window lists only
  the queued jobs, the bar still shows every job and says "N more in the background". MCP:
  `queue_list` entries carry `at_once`, and the queue tools name the new refusal.
- **The background bar lists at most five jobs and steers them all at once** (RC-010-14; owner,
  2026-10-19). Opened, it lists the first `queue.strip_rows` jobs (Settings, Transfers; 1 to
  50, 5 by default), the running ones first, and how many more the queue window has; each row
  keeps Pause and Cancel. Its header has *Pause all* / *Resume all* and *Cancel all* for every
  background job, and one *Show* beside *Queue* instead of one per row - every job of a batch
  showed the same operation. The bar is `app/file_ops/strip.rs` now.
- **F6 deselects its sources when it starts** (RC-010-07; owner, 2026-10-19), not only when they
  are gone: a long or queued move left them marked until it ended.
- **Copies from disk to disk survive a killed process** (RC-080-04). A local copy or move is
  written as `<name>.rcpart` and renamed when complete, like a transfer to a server (Total
  Commander writes in place), and every copy or move is kept in `transfer_jobs` from its
  submission - a job still waiting too -, so after a crash every job is back, paused, and
  continues its part; an overwritten file stays whole until the new one is. A share that stops
  answering counts as a lost connection: the part is kept and the job pauses as resumable.
- **Ctrl+R leaves a flat view** (owner 2026-10-18). In the branch view (Ctrl+B) or a find
  result shown in the panel, Ctrl+R (and F2) returns to the folder the view was made from, read
  again; Ctrl+B again still leaves into the folder under the cursor. A view change (hidden files)
  still reads a flat view again (`panel.refresh` command vs. `Engine::refresh`).
- **A tooltip only for a shortened name** (owner 2026-10-18). Table cells in the dialogs (find
  files, synchronize, queue, the connection tree, multi-rename's conflicts) show the whole value
  on hover only when it does not fit, as the file list already did (`theme::fit::overflows`);
  a folder tab shows its path on hover only when its title is shortened.
- **No visible text names Total Commander outside its import** (owner 2026-10-18). The About
  page's thanks and the help of the external viewer, quick search keys and Enter on archives no
  longer mention it; `docs/design.md` makes it a principle.
- **Command registry** (integration of wave 010-w3). Each `registry/<group>.rs` lists its own
  `Command` variants after their ids (`<group>_commands!`); `command.rs` builds the enum, the
  serde names and `Command::id` from the groups' lists, so no file holds every command. The Rust
  API and the JSON form `{"command": id, "params": ...}` are unchanged. `file.properties` answers
  over MCP as `files_properties` (`files_stat` stays the one-entry stat of `files.stat`).
- **One wildcard matcher** (follow-ups of wave 5, RC-010-02, RC-030-03). `rc_ops::mask` holds
  the only `*`/`?` matcher and Total Commander's selection mask (`;` lists, `|` exclusions, `\`
  for folders); the panels' Num+/Num- masks and the unpack mask share it (re-exported as
  `rc-engine`'s `mask`), and the search, synchronize, quick search/filter and ssh_config
  `Include` globs compare through it. No behaviour changes.
- **The import from Total Commander is in the settings** (RC-050-10). A new settings page
  Import has "Import from Total Commander..."; it opens the same import (hotlist, tabs and
  connections) over the settings, and Back, Escape or Close return to them. The connection
  manager's "Import TC..." button is gone; "Import ssh config..." stays. The command palette
  reaches the import through the settings.
- **Every dialog on the design guidelines** (RC-080-08, stage 2). The dialogs take their card
  (small 480, medium up to 680, large), their one button row at the bottom right (one primary
  button in the accent colour, extra actions at the left, unavailable ones disabled) and their
  danger colour (`theme::DANGER`) from the theme; the function-key look stays with the bottom
  bars. Paths and file names are shortened in the middle at the width they get, the whole value
  in a tooltip (`theme::fit`: `path_line`, `path_cell`, and `cell`, `cut`, `table_row` moved
  from the connection manager); prose, errors and keyboard help lines wrap. The copy progress
  window's path no longer runs out of its card; the queue window shows one-line rows with column
  shares, the selected job's counts under the list and one button row; find files, synchronize,
  diff, run over SSH and the login-script editor move their buttons into that row; settings and
  the login-script editor are large dialogs. Host-key and certificate dialogs draw the button the
  keyboard is on as the primary instead of a function-key button with an accent border.
- **Panels: tabs, drive bars, columns, stripes** (RC-080-08, owner 2026-10-18). Folder tabs look
  like tabs: the active one in the list's colour with rounded top corners, joined to its panel,
  the others dimmed with a hover fill; long titles shortened in the middle, the path in a
  tooltip. A drive bar too narrow for its drives shows those that fit, in order, and `»` for the
  drive menu instead of cutting them off. File-list cells are clipped to their column, a long
  name shortened in the middle with the whole name in a tooltip; the whole column header cell
  sorts. Every other row of the file lists and the dialogs' tables is tinted (`stripe_fill`),
  switchable with the new setting `appearance.alternate_rows` (on; Settings, Appearance; MCP
  `settings.get`/`settings.set`).
- **A close X in every dialog** (RC-080-15, owner 2026-10-18). Every dialog card starts with one
  head - its title at the left, a close X at the top right (`theme::layout::Head`); the X sends
  what Escape sends, so it cancels or closes exactly as Escape does in that dialog. The cards take
  the head as their first argument, so none is without one. Display only - no command, no MCP
  tool.
- **The settings in a window of their own** (RC-080-15). The app runs as an iced daemon; the
  settings open as a system window with title bar, X, minimum size and the app's icon, moved and
  resized freely and opened where they were left (`window.dialog.settings`, JSON like
  `window.geometry`; the first time centred over the main window). The window is modal: on
  Windows it is owned by the main window (stays in front, minimizes with it, own Alt+Tab entry),
  the main window is disabled while it is open, and a stored place off every monitor is moved
  back onto one before it shows. MCP, host-key, certificate and vault questions draw in the
  settings' window while it is open and bring it to the front; the file dialog of "Browse..."
  belongs to it. The title bar's X and Alt+F4 do what Escape does. The other large dialogs follow
  in this job.
- **Escape cancels every dialog, always** (RC-080-15, owner 2026-10-18). An Escape a text field
  captured counts as Escape while a dialog is open (`app/escape.rs`), so no dialog needs a second
  Escape any more; a job's conflict or error question now answers Escape and its X with Cancel,
  and the connection manager closes on Escape from its form and its filter too. The layers and
  what Escape does in each are in `docs/design.md` ("Keyboard first").
- **The connection manager opens in its filter** (RC-080-15, owner 2026-10-18). Ctrl+F puts the
  keyboard into the filter: typing filters at once, the arrows, PageUp and PageDown move the
  tree's cursor, Enter connects, Tab gives the tree the keys.
- **One queue job per file or folder** (RC-010-14, owner's Windows test 2026-10-18). A copy, move
  or delete of several sources - F5, F6, F8, a drop, `ops_copy`, `ops_move`, `ops_delete` - is now
  one job per source, submitted together so they stand side by side in their lane and start in
  the order selected (`rc_ops::per_item`, `Queue::submit_batch`); the commands answer with the
  jobs. The jobs of one operation share a batch: an answer "for all" holds for each and answers a
  sibling's open conflict, `Cancel` answered to a question (or preset) stops the siblings, the
  progress window passes from one to the next and its Pause, Resume and Cancel steer them all,
  while the queue window steers each on its own. The store keeps a batch's jobs in order with
  their batch, and a restart puts them back as siblings (`Queue::restore_batch`). New setting
  `queue.one_job_per_item` (Transfers, on; off: one job as before). The queue window's "Pause
  all" and "Resume all" are one button that follows the queue's state. No new command.
- **The queue in a window of its own, modeless** (RC-010-14, RC-080-15, owner 2026-10-18).
  Ctrl+Shift+Q opens the queue as a native window owned by the main window (`DialogWindow::Queue`,
  place and size kept in `window.dialog.queue`), which - unlike the settings' window - stays
  enabled: `DialogWindow::modal`, and `platform::dialog_window::show`/`close` lock and release
  the main window only for a modal one. Keys go to the window that has the keyboard (focus events,
  `queue_window/own.rs`); Escape, F10 and the X close the queue window, Ctrl+Shift+Q brings it to
  the front. Each running job shows its current file, bytes, speed and time left under its row.
  New setting `queue.window_own` (Transfers, on; off: the overlay as before); the `transfers.*`
  rows moved with the queue's into `settings/schema/transfers.rs`. No new command.
- **The large dialogs in windows of their own** (RC-080-15 stage 2, owner 2026-10-18). The
  connection manager, find files, multi-rename and compare/synchronize (the diff window over the
  synchronize dialog, one window) open as native windows with title bar and X, moved and resized,
  place and size kept in `window.dialog.<name>`. Multi-rename is modal, as in Total Commander;
  the others are modeless: their window takes the keys only while it has the keyboard
  (`dialog_windows/own.rs`), the panels keep theirs beside them. The title bar's X and Alt+F4 are
  Escape in that window, Ctrl+F, Alt+F7 or a new comparison bring an open one to the front, the
  small questions draw over the window that has the keyboard, "Run command" over the manager's
  window. New setting `appearance.dialog_windows` (Appearance, on; off: the cards over the main
  window as before). No new command. Also in windows of their own, modeless: each Lister (F3) -
  one window per open Lister, not owned by the main window as in Total Commander, all at one kept
  place (`window.dialog.lister`); Escape, F10 and the X close that Lister only, Quick View stays
  in its panel -, the checksum results and the login-script editor (beside the manager). A file
  operation's question that appears while a modeless dialog's window has the keyboard (the
  queue's too) brings the main window to the front (`App::question_to_front`).
- **A drive change follows the other panel** (RC-010-05, owner 2026-10-19). Changing to the drive
  the other panel shows - `d:` in the command line, the drive bar, the drive menu,
  `panel_change_drive` - opens the other panel's folder, as in Total Commander; otherwise the
  folder this panel last showed there, else the root, a folder that is gone passed over. The
  panel's own drive is not followed. Setting `drives.follow_other_panel` (page Folders, on).
- **F7 offers the name under the cursor** (RC-010-07, owner 2026-10-19). The new folder dialog
  starts with the name of the file or folder under the cursor, all of it selected, as in Total
  Commander; on `..` it starts empty. Setting `mkdir.prefill_cursor_name` (page Transfers, on).
- **File list names are cut at their end, their tooltip waits** (RC-080-08, owner 2026-10-19). A
  file or folder name too long for its column is shortened at its end (`Board.Games.2019.10…`)
  instead of in the middle, and the whole name shows only after the pointer rests on it for
  500 ms. Settings `appearance.name_cut` (`end`, `middle`) and `appearance.name_tip_delay`
  (0 to 5000 ms), page Appearance. Other tooltips still show at once.
- **Dependencies trimmed, every lint exception reasoned** (audit 2026-10-19). rc-mcp drops
  `thiserror`, rc-ops takes `secrecy` for its tests only; the workspace drops `iced_term`, `toml`
  and the unused features iced `lazy`, clap `env` and tokio `signal`; `blake3` and rc-checksum's
  `sha2` come from `[workspace.dependencies]`. Clippy's `allow_attributes_without_reason` is on
  and every `allow` has its `reason`; AGENTS.md lists the modules that allow `unsafe_code`.
- **One size parser, one type for the recent-entries lists** (audit 2026-10-19).
  `format::parse_size` reads the split dialog's part size and the find-files size fields, which
  now take a decimal too (`1.5M`, `1,5 GiB`); `app/mru.rs` keeps the command line, mask and
  search histories under their settings and limits as before.
- **One question desk for host keys and certificates** (RC-080-22; audit 2026-10-19). The SSH
  host-key and FTPS certificate questions share `rc-engine`'s `QuestionDesk`: at most 32 open,
  the oldest dropped, ids never reused, one question per server, key and kind, and "once" grants
  void 300 s after the answer - at that instant already. Behaviour, events and the GUI-only
  answers are unchanged; the expiry edge and the full queue have tests.

- **`Engine::dispatch` is split by registry group** (RC-080-20; code audit, 2026-10-19). Its arms
  moved unchanged from the one 479-line `match` in `rc-engine/src/dispatch.rs` into
  `dispatch/arms/<group>.rs`, one `<group>_arms!` list for each `registry/<group>.rs`, which
  `dispatch_match!` folds back into one `match` without a catch-all arm: a command without an arm
  still does not compile. No behaviour changes.
- **One text decoding for the viewer, compare, comments, the imports and the content search**
  (RC-080-21; audit 2026-10-19). `viewer/encoding.rs` detects and decodes for all of them
  (`detect_whole`, `decode_text`: a byte order mark, UTF-16 without one, valid UTF-8, else
  Windows-1252), on `encoding_rs` instead of its own Windows-1252 table (identical for all 256
  bytes). `descript.ion` and Total Commander INI files now also read UTF-16; the content
  search's automatic encoding detects as the viewer does and so finds text in ANSI files and
  UTF-16 without a mark. Deliberately different, with the reason in the code: TC passwords,
  Remote Desktop Manager exports and `wsl.exe` output.
- **The standalone dialogs open in modal windows of their own** (RC-080-23, stage 1; owner,
  2026-10-19). While `appearance.dialog_windows` is on, backup, restore, export and import, the
  properties (occupied space, link, comment), quick connect, map and disconnect a network drive,
  pack and unpack, the checksum forms (create, split, combine), the hotlist dialog and "Run
  command" are windows of the system as in Total Commander: title bar, X, moved and resized,
  their place kept (`window.dialog.<name>`), the first field with the keyboard; the main window
  waits while one is open, and its file-operation question, drag question and queue card take
  no key meanwhile. The archive password stays a card; the database dialog from the settings
  still takes their place. The unlock prompt a connection asked for alone now shows over the
  settings' or multi-rename's window and has the keys there (it drew nowhere). `app.rs`'s dialog
  key arms moved to `app/dialog_keys.rs`, what each window draws to `dialog_windows/kinds.rs`.
- **Copy, move, new folder, delete, the job questions and the progress window open in windows
  of their own** (RC-080-23, stage 2a; owner, 2026-10-19). While `appearance.dialog_windows` is
  on, the dialogs of F5, F6, Shift+F5, F7, Shift+F4, F8 and Shift+F8, a job's conflict and error
  questions and its progress window are modal windows as in Total Commander, each with its own
  place (`window.dialog.transfer`, `new_item`, `delete`, `conflict`, `job_error`, `progress`)
  and a short title; the access keys and the focus ring work inside. A job's question is a
  window on top - of the progress window, and of another modal dialog's window for a background
  job, no longer waiting for it - and comes to the front for each new question. Escape, F10 and
  the title bar's X act in the window they belong to (cancel the question, close the dialog,
  send the progress to the background strip). The small dialogs' windows are hidden when they
  close and shown again at the next opening, so F7 and F8 open without a window to make. Every
  dialog window now shows when it is ready and closes only once shown, so a dialog closed
  during its opening cannot leave the main window locked; the debug log has each window's time
  to the screen. Off, the file operations stay cards (`dialog_windows/file_ops.rs`,
  `file_ops/window.rs`, `DialogWindowMsg`).
- **The small questions open in windows of their own** (RC-080-23, stage 2b; owner,
  2026-10-19). While `appearance.dialog_windows` is on, the MCP call's confirmation, a host key,
  a certificate, the offer to remove the unused vault, quitting while jobs run, the unlock prompt
  a connection asked for alone, an archive's password, the question before a large fetch and the
  synchronization's confirmation are windows of the system (`DialogWindow::Question`, place
  `window.dialog.<question>`), one at a time - the top one in the order their cards take the
  keys, the others wait. Each is owned by and locks the window it would hide behind - the front
  modal dialog's, else the main window (restored first); the compare window for the
  synchronization's confirmation, which leaves the panels working -, closes before that window
  does and opens again over a modal window that appears after it; it comes to the front with
  its taskbar button flashing. Its keys are its own while it, the main window or the window it
  locks has the keyboard; Escape, F10 and the X answer as on the card. The MCP window opens on
  Deny and only a click allows; a changed host key or certificate keeps the red border. Off, the
  cards stay (`dialog_windows/questions.rs`, `question_window.rs`).
- **Saving a connection is one engine command, `connections.save`** (RC-080-25; code audit
  2026-10-19, finding e). The connection manager's form - fields, secret changes (set, take a
  key file in, remove), login script and drive-bar mark - is stored in one transaction of
  `rcommander.db` instead of up to seven commands in a row; everything that can be refused
  without writing (fields, a blank secret, the vault, the key file and its passphrase) is asked
  first. New MCP tool `connections_save` for the fields, login script and mark in one call (no
  secrets: `secret_not_accepted`, `gui_only`; naming `login_script_id` is exec).

- **The tests share their set-up** (RC-080-24; code audit 2026-10-19, finding d). Test
  backends that wrap another implement `rc_vfs::testing::Forward` and override only what they
  change (the `memory` feature, now also a dev feature of rc-engine and rc-checksum); the two
  copies of `Trickle` are one (`rc_vfs::testing::Trickle`). `rc-engine` and `rc-mcp` have a
  `#[cfg(test)]` `test_support` module (engines, folder trees, fast vaults, expected outcomes;
  the rmcp client and tool call), the app's `Harness` has `headless`, `with_db`, `on_db`, `open`,
  `temp` and `temp_with_db`. No test changed what it asserts; 184 test files, 41837 lines
  before, 40453 after.

### Fixed

- **A new folder (F7) waited behind copies** (RC-010-14), on the owner's report that F7 did
  nothing while a large copy or move ran, unless the job was put into the background and moved to
  the front of the queue: `ops.mkdir` went into the default list and waited in the drive's lane.
  It now runs at once, outside the lists, as OK runs a copy, move or delete; F2 in the delete
  dialog still queues. Test `mkdir_does_not_wait_behind_a_copy_in_the_lane`; queue tests that used
  a new folder as a waiting job use a copy instead.

- **The active panel's accent border was hidden at the top**, on the owner's report: the path
  bar's own background lay flush on the card's 1 px border and covered it there and at the sides
  beside it; only a trace of colour showed at the rounded corners. The card now keeps its content
  1 px inside the border (`app/view.rs`).

- **Alt with a letter in a terminal opened a menu on Windows** (RC-080-18), on the owner's report
  that Alt+V - Claude Code's image paste on Windows - opened View while a terminal had the
  keyboard (Alt+F opened Files). The app had already told the Win32 bar that Alt's letters were
  the terminal's, and the bar's own Alt handling held back, but a menu still opened by its letter.
  The window's subclass now stops `WM_SYSCOMMAND`/`SC_KEYMENU` with a letter the bar may not take,
  wherever it comes from; Alt alone still opens the bar. Tests that the look and the drawn bar
  leave Alt's letters to a terminal.

- **A forwarding `rcommander mcp` ends with rCommander** (RC-000-05; owner, 2026-10-25: two
  `rcommander.exe mcp` started by the Claude app stayed after rCommander was closed and kept
  `rcommander.exe` in use, so a new build could not be copied over it). rmcp's HTTP client does
  not notice an instance that went away, so `proxy_io` waited for the client alone; it now probes
  the instance every 3 s (`/health`, as at the start) and ends after two misses in a row. It
  could reach nothing anyway: a restarted rCommander has a new port and token. The agent
  reconnects once rCommander runs again.

- **The Windows zip's `VERSION.txt` names rCommander's version** (owner, 2026-10-25).
  `scripts/package-windows.sh` read each bundled plugin's version into `version`, so the file
  said the last plugin's (0.1.2) instead of rCommander's; the loop now uses `plugin_version`, as
  `scripts/package-linux.sh` does. The program, its About and the zip name were never affected.
- **The settings' scroll bar stands at the window's right edge** (owner, 2026-10-22). A page of
  plain settings (Panels and lists, among others) and the About tab sized their scroll area to
  the widest line, so the bar stood in the middle of a wide window; the scroll area now fills the
  width, as the shortcut and host-key tables do.
- **`rc-plugin-host` builds on its own** (owner, 2026-10-22). zip's `deflate-flate2` names no
  backend for flate2, so `cargo test -p rc-plugin-host` failed in flate2 ("You need to choose a
  zlib backend"); the whole workspace built only because rc-ops and rc-vfs bring flate2 with
  `rust_backend`. rc-plugin-host now takes the workspace's flate2 too: miniz_oxide as before, no
  new crate. Alone: 164 tests run, 164 passed.

- **A lost graphics device no longer floods the database; rCommander offers a restart**
  (RC-010-11; owner, 2026-10-22: dialogs took long to open and close with 0.3.0). When the
  graphics driver lost its device, iced 0.14 logged "Error Other when presenting surface." on
  every frame and drew again at once: 15 000 error lines a second, 42.8 million rows and a
  13 GB `rcommander.db` in one evening, and a retention sweep that read the whole table per
  batch held the writer for hours at every start. Now the error log drops a line that repeats
  the one before it and sweeps after every 5 000 stored lines; every delete of the sweep walks
  an index and stops after its batch (no `COUNT(*)`, no `SCAN`); free pages go back in
  `incremental_vacuum` chunks of 2 048 with the writer released between two, also when the
  sweep deleted nothing (the owner's file: 3 million free pages, 13 GB to 4.5 MB). A storm of
  100 failed presents within a second asks in the system's message box whether to restart;
  yes stores tabs and places as a quit does and starts the program again. Setting
  `appearance.gpu_loss_notice` (Appearance, on); `rc_db::install_diagnostics_with` takes the
  window's watch as an extra layer.

- **DOS art without gaps between its lines** (RC-020-11; owner, 2026-10-20: "beim nfo viewer sind
  irgendwie ... zeilenabstände wohl zu groß"). DOS text (CP437 in the text view) drew every line
  1.4 times the font size, so the block characters of an .nfo stood apart. Its lines are now as
  tall as the viewer font's full block (U+2588, measured from the font when the settings are
  read), floored so the blocks touch; other text keeps its spacing. Setting
  `viewer.dos_tight_lines` (Appearance, on).

- **A very large picture opens in the viewer, scaled down** (RC-020-07; owner, 2026-10-19). A
  5360 x 32667 JPEG (175 million pixels) was refused as too large, while Total Commander showed
  it. The image view now allows up to 256 million pixels (was 64) and scales a picture with more
  than `viewer.picture_megapixels` (Settings, Appearance; 4 to 256, 32 by default) down before
  it reaches the window, in its decoded colour type so the full picture is never held twice;
  zoom and the status line keep the picture's own size.
- **Names sort as in Total Commander, hyphens included** (RC-010-02; owner, 2026-10-19). The
  hyphen and the apostrophe ranked last among the symbols, so `--- incoming ---` came after
  `$RECYCLE.BIN` and `_fix`; Total Commander, with Windows' word sort, passes over them and lets
  them decide only between names that are otherwise equal - `--- incoming ---` sorts by its
  leading blank, first. `sort::natural_cmp` now compares without them first and with them, last
  of all characters, after.
- **A waiting job of an operation confirmed with OK ends at once when cancelled** (RC-010-14).
  The jobs of such a batch ran in one loop, one after the other, so a job cancelled while it
  waited ended only when its turn came, after the ones before it. Each batch run at once now
  waits in a lane of its own with one place (`rc-ops` `Batch::turns`): a cancelled waiting job
  ends at once, a paused one lets the ones behind it go first, and resuming it starts it when
  the place is free.
- **A network drive mapped with *Reconnect at sign-in* shows and opens without Total Commander
  first** (RC-010-16). Windows connects such a drive only on its first use - Explorer and Total
  Commander ask it to (`WNetRestoreSingleConnectionW`) - so until then its letter was missing
  from `GetLogicalDrives` or the drive answered "not ready", and rCommander showed nothing or
  refused it. The drive bar now also lists the drives Windows remembers (`WNetOpenEnumW`,
  `RESOURCE_REMEMBERED`), a letter without a kind that has a remembered folder counts as a
  network drive, and `panel.change_drive` connects a network drive that answered "not ready"
  and asks it again; Windows asks for credentials it does not have.
- **In the drive menu a folder's servers stand indented under the folder's heading**
  (RC-010-05). The heading (`SSH & FTP / SWP`) stood in by 16 px while its servers stood at the
  menu's edge, so they looked like part of the folder above. The heading now lines up with the
  group headings, and a server in a folder stands in by 16 px.
- **A drive that does not answer and an MCP question nobody answered show their own messages**
  (RC-080-12). Both errors had the code `timed_out`, and `locales/en.json` held
  `error.timed_out` twice, so the window showed one text for both. A drive or share that times
  out now answers `drive_timed_out` (`rc-vfs` `VfsError::TimedOut`), with its own key
  `error.drive_timed_out`; the MCP question keeps `timed_out`. The repo lint `i18n_keys`
  rejects a key given twice in `en.json`, which a JSON map would keep only once.
- **A cancelled FTP download no longer costs the connection a transfer slot** (RC-080-14). On
  a server with a session limit such as pure-ftpd's "2 users (the maximum)", the login right
  after a cancelled download was refused while the server still counted the closed session;
  twice in a row, the pool gave up seats the server has and the next transfer waited for another
  one to end. While the pool holds fewer sessions than the server held for it before, a refused
  login is tried again after 0.25, 0.5, 1 and 2 s (`rc-remote` `ftp/pool/seats.rs`). Found by
  the live suite against pure-ftpd 1.0.50.
- **The live FTPS certificate change runs on pure-ftpd and ProFTPD** (RC-080-14): the test
  runner gets the server's capabilities to write the new certificate, and `recert.sh` waits
  until ProFTPD's old master is gone before it looks at the port.
- **Marked rows and the active path keep the chosen font** (RC-010-17). Both were drawn
  semibold; a list font without a semibold face then fell back to another font and the text
  jumped when a panel became active or a row was marked. Now only their colour changes (the
  accent), as in Total Commander. Owner, Windows report 2026-10-18.
- **The file list's scroll bar can be grabbed** (RC-010-23). The thumb was drawn only; it now
  drags as in Windows and Total Commander: the list follows live until the button is released,
  also with the pointer off the bar, and a click on the track above or below pages a screen. The
  thumb is stronger under the pointer and while held (`theme::thumb_fill`); the brief view moves
  by whole columns. A small widget (`app/file_list/scrollbar.rs`) sends the first row through
  `Pane::scroll_to`, the pane's position the wheel and the keys move too. Owner, Windows test
  2026-10-18.
- **OK runs a copy, move or delete at once; only F2 queues it** (RC-010-15, owner 2026-10-18: a
  delete went into the queue without F2). As in Total Commander, confirming the F5, F6 or delete
  dialog runs the job at once in the progress window - in no lane and in none of the queue's
  lists (`rc_ops::AT_ONCE`), also while queued jobs write to the same place; the jobs of one
  operation still run one after the other. Only F2 puts it into the chosen queue list, and the
  delete dialogs now have F2 and the list choice as F5 and F6 have. `ops.copy_to`, `ops.move_to`
  and `ops.delete` take `queue` (`ops.delete` newly); `None` is at once. Pause all and Resume
  all leave jobs run at once alone. MCP's `ops_copy`, `ops_move` and `ops_delete` stay jobs of
  the queue. No new command.
- **The dark menu bar is opaque again** (RC-080-18). On the newest Windows 11 the bar was fully
  see-through in the dark look: muda paints it with GDI, which leaves each pixel's alpha at 0.
  muda now paints into a buffered-paint DIB whose alpha is set to 255 before it reaches the
  window (`platform/windows/menu_bar.rs`). The environment variable `RC_MENU_BAR_THEME`
  (`opaque`, `muda`, `system`, `light`, `nosubclass`) turns the parts off one at a time for
  diagnosis. Owner, Windows report 2026-10-18.
- **Names sort as in Windows and Total Commander** (RC-010-02). Symbols and punctuation (`_`, `$`,
  `[`, ...) now come before digits and digits before letters, so `_drafts` stands before `2024`;
  before, `_` sorted after the digits by its character code. Owner, Windows test 2026-10-18.
- **A new terminal on a server panel opens without the shell picker** (RC-060-03). Ctrl+Shift+Ö
  (or the session bar's "+") on an SFTP panel offered the local shells (PowerShell, cmd, WSL),
  though the panel's terminal is always a shell on its server; it now opens that SSH shell in
  the panel's folder at once. Owner, Windows test 2026-10-18.
- **A connection goes on after the master password** (RC-050-03). Opening a server whose
  password sits in the locked vault asked for the master password and then did nothing; the
  connection had to be started again. The command the locked vault refused (a window job:
  manager Enter, Terminal, the drive bar, a server panel's terminal; the manager's Connect and
  Test) now runs once more after a successful unlock (`Message::VaultRefused`,
  `AfterVault::Retry`); Escape or a wrong password runs nothing. Owner, Windows test 2026-10-18.
- **A copy to or from a server goes on after the master password** (RC-050-03). F5 or F6 into
  or out of a server whose password sits in the locked vault (also server to server) asked for
  the master password and then dropped the copy. The refused job is now submitted once more
  after a successful unlock (`Submitted::refused`, `Retry::Ops`); Escape drops it.
- **A long branch view no longer floods the panel with updates** (RC-020-03). Ctrl+B paced its
  updates from the start of the last one, so a sort slower than the interval (a debug build)
  ran them back to back; the interval now counts from the end of the last update, as the panel
  listing's has since wave 15.
- **The checksum dialog says at once why a file name will not do** (RC-020-05). A name with a
  folder in it (`sub\sums.sha256`) was refused only after Create, as a failed job with a generic
  message. The name stays a plain file name on every panel - the checksum file lies in the
  folder of the files it lists, and its check reads `\` as a separator - but the dialog now
  checks it with the job's own rule (`rc_checksum::plain_name`), shows the reason under the field
  and keeps Create disabled.
- **A checksum file made for a folder with one file checks out** (RC-020-05). With no name given
  (an MCP call, an emptied field), a single selected folder holding one file got its checksum
  file inside that folder (`docs/a.txt.sha256`) while the line named `docs/a.txt`, so the check
  looked for `docs/docs/a.txt`. Such a folder now gets `checksums.<ext>` beside it, as any folder
  does and as the dialog already proposed; `<file>.<ext>` is kept for one selected file.

- **An SSH server that goes away is handled at once** (RC-060-01, RC-080-04, live tests
  against OpenSSH in Docker). An SFTP panel's backend watches its session
  (`SftpVfs::ending_with`): the first listing after a loss the keepalive or the server announced
  connects again instead of answering `connection_lost` once more, and a listing whose session
  the server ends fails at once with `connection_lost` instead of `sftp_timeout` after 30 s.
- **A transfer paused after a lost connection says why** (RC-080-04). While it waits to be
  resumed, the lost connection is among the job's failures (file, code, message); it goes once
  the job is resumed.
- **A long server listing completes in a debug build** (RC-060-02). The panel's next showing
  of the arrived entries is paced from the end of the last one, so a slow sort no longer runs
  them back to back, one block apart (100 000 entries never completed; now 6.5 s).
- **An FTP connection the server dropped while idle, and servers with few sessions**
  (RC-070-03, live tests against vsftpd, pure-ftpd and ProFTPD). An idle pooled connection the
  server closed is seen on its socket before reuse and replaced, instead of failing the next
  step with "connection closed before the end of the response". A login refused as one session
  too many (`421`, ProFTPD's `530 ... maximum number`) lowers the pool's limit for that server
  and the call waits for a free connection; only a server that takes no session at all reports
  `ftp_too_many_sessions`.
- **F5 from an FTP or SFTP panel into a drive or a share** (owner's report 2026-10-18). A target
  with a drive letter or a share (`D:\`, `\\nas\share`) typed or offered in the copy dialog
  of a server panel is a place on this machine; it was taken as a server path of that name and
  failed with "This path is not valid here" (`resolve_path`, as archives already did).
- **A full tab row is no longer cut off** (RC-010-21). A panel's folder tabs that do not fit its
  width show as many as fit, the active tab always among them (the row moves along with it), and
  `»` after them lists every tab with its full path and lock mark; picking one shows it. The
  widths follow the list font (`app/tabs/overflow.rs`, `visible_range`).
- **No SMB login to a server's or an agent's share** (RC-080-10). A shell on an SSH server never
  moves the local panel any more, and a folder on a network share in an OSC 7 or OSC 9;9 report
  is dropped from every terminal (WSL shares pass), so one line of a server's output cannot make
  Windows send the person's NTLM hash. Every MCP tool refuses a path argument on another
  machine's share - and `settings_set` a start folder there - with `network_path`, unless a
  network drive the person mapped reaches it (`rc_vfs::network_path`, `rc-mcp/src/unc.rs`).
- **MCP writes stay out of rCommander's data, `.ssh` and autostart; a restore file is checked
  before it is swapped in** (RC-080-09, findings 1 and 4 of RC-080-03). Every MCP write tool -
  copy, move, mkdir, pack, unpack, checksum files, split, combine, attributes, comments, links,
  multi-rename, synchronization - refuses with `protected_target` a place in the data folder,
  under rCommander's own file names, in any `.ssh` folder, in an autostart folder or on a shell's
  start-up file, also for what lands below a harmless target (`rc-mcp/src/guard_write.rs`). The
  start swaps in `rcommander.db.restore` only with the BLAKE3 digest the window's restore stored
  inside the current database, and after the restore check again; anything else is set aside as `rcommander.db.restore-rejected`
  and the database stays (`restore_rejected`). The engine's read guard knows a renamed copy of
  the database by its content.
- **Vault lock and password change hardened** (RC-080-11). Only the person's own use defers the
  idle lock - a secret an agent reads over MCP no longer keeps the vault open - and the new
  protected setting `vault.max_unlocked_hours` (8, `0` none) locks it that long after the unlock
  however busy it is. Windows' session lock (Win+L) and sleep or hibernation lock the vault
  (`vault.lock_on_session_lock`, on, protected; a hidden window in `rc-engine`'s
  `session_lock/windows.rs`). The MCP mask keeps each secret as its length and a keyed hash
  (per-process key, at most 1024) instead of its value and keeps masking after the lock, so a
  password still on an SSH screen does not leave through `terminal_read_screen`. A second
  rCommander on the same database whose key went stale after a password change elsewhere is
  refused the write in the write's own transaction (`vault_changed`) and locks, instead of
  storing a secret that never opens again. The database runs with `secure_delete` and truncates
  its WAL after a password change, so old ciphertexts, salt and verifier leave the file.
- **An FTP server whose login test passes opens in a panel** (owner's Windows report on
  development 2cf4812). A server that refuses `EPSV` (listed in `FEAT` or not, or any server
  over IPv6) or the `-a` of `LIST -a` showed "This is not supported here." on opening, because
  the listing's data connection failed with `500`. Passive data connections are now opened by
  `rc-remote` itself (`EPSV`, then `PASV`, or the reverse; the working one is kept per server),
  `LIST -a` falls back to `LIST`, and a command a server does not implement is named in the
  error (`ftp_unsupported_passive`, `ftp_unsupported_retr`, ...; never with its argument).
- **`.zst` decompression bomb** (RC-080-13). `ruzstd` 0.9.1 (RUSTSEC-2026-0344): a small crafted
  `.zst` or `.tar.zst` no longer exhausts memory when it is opened, listed, tested or unpacked;
  `cargo deny --offline check` passes (advisories, bans, licences, sources). A test writes the
  bomb - a frame asking for a 2 GiB window - byte by byte and sees it refused at once.
- **MCP questions, guards and remote trust hardened** (RC-080-12, the remaining findings of
  RC-080-03). MCP: a question names the entry `file_open` really opens and shows the panel's
  names spelled out; a value longer than a question shows is refused before anyone is asked;
  `wcx_ftp.ini` is refused to every reading tool (the Total Commander import still reads it);
  `rcommander mcp`'s probe proves the token with a keyed hash instead of sending it; lowering a
  retention and deleting a multi-rename preset are destructive; an answer the mask cannot walk
  fails closed; `hostkeys.remove` is GUI only (no `hostkeys_remove` tool) and
  `import.total_commander` is a write. Engine: a login script that sends a secret holds its
  connection's endpoint like a stored secret; under MCP an ssh_config `Include` stays inside
  `.ssh`, every config file is capped at 1 MiB. Remote: names a copy gives a local Windows folder
  (top-level sources and sync rows too) and names out of an archive pass unpacking's strict
  check, which now refuses direction characters such as U+202E; archives list only permission
  bits; an unknown key of another type for a known host says so in the dialog and stored key
  types lead the key exchange; `send_secret` must follow a `wait_for`; a plain-FTP session warns
  in the status bar; key-file KDF parameters are bounded; a `@revoked` key is imported from no
  line; part files are created anew, never truncated through a link.
- **A terminal on a WSL folder is that distribution's shell; Alt+F9 and Alt+F5 show their
  work** (owner's Windows test of wave 4, RC-040-02, RC-040-03, RC-030-03). Ctrl+Ö,
  `panel.open_terminal_here` and `terminal.open` without a shell on a panel in
  `\\wsl.localhost\<distro>\...` or `\\wsl$\<distro>\...` start `wsl.exe -d <distro> --cd
  <linux path>` instead of the default local shell (`rc_term::choose_shell`); a shell picked in
  the picker still wins. The unpack and pack dialogs submit their job as a copy does: progress,
  conflict and error questions, failures in the status line, both panels read again - before, the
  job ran unseen, a question waited for nobody and the unpacked files never showed.
- **Quick search with AltGr and a clean connection manager layout** (Windows test 2026-10-18,
  RC-010-08, RC-050-04). AltGr with a letter it types nothing for is Ctrl+Alt with it, as on
  Windows and in Total Commander, so it starts the quick search on a German layout; a character
  AltGr types (`@`) still goes into the command line. The connection manager takes the window's
  height, so a form scrolls only in a window too small for it; Test connection and Save sit with
  Close in one bottom row; the tree's list fills its column and its buttons wrap onto a second
  row instead of running into the form.
- **Terminals stay on their own system; keyboard lists scroll** (owner's Windows test of wave 5,
  RC-040-03, RC-040-04, RC-010-05, RC-010-03). The
  terminal toggle shows a panel's shell again only in a folder of its system and otherwise opens
  a fitting one beside it; the cwd sync never sends a local shell into a WSL share or a WSL shell
  out of its distribution's share and says why in the status line. The drive menu, the folder
  history, the hotlist, the command line's Alt+F8 list and the connection manager's tree keep
  their cursor in view.
- **Large dialogs use the window's size** (Windows test 2026-10-18, RC-050-04, RC-050-09). The
  connection manager and its import pages, multi-rename, find files, the queue window, the
  checksum results, synchronisation and compare fill the window but for a margin
  (`theme::large_dialog`); the multi-rename table and the find-files hit list show as many rows as
  fit. The import pages (Total Commander, ssh config) show one-line table rows with proportional
  columns, cut at the column with the whole value in a tooltip, a short password status, the list
  over the remaining height and one bottom row of options and buttons; the manager's import
  buttons are shorter ("Import ssh config...", "Import TC...").
- **Tab moves between the fields of a dialog** (Windows test 2026-10-18, RC-050-03, RC-010-09).
  In the master-password prompt Tab did nothing; now Tab and Shift+Tab walk the text fields of
  the dialog on top - the master password and its repeat, quick connect, the settings, find
  files, the login-script editor and the import pages - never into the panels or a dialog
  beneath (`app/field_walk.rs`). Enter in the settings is OK, as in Total Commander. The
  archive, checksum, properties, copy/move/new-folder and sync dialogs walk their fields too
  (RC-080-05).
- **The connection tree's rows are one line** (Windows test 2026-10-18, RC-050-04). Name and
  host sit in columns, cut with the whole value in a tooltip; a narrow list shows the names
  alone; the tree and the form share the manager's width.
- **Total Commander's connection folders** (RC-050-09). A `|` in a `wcx_ftp.ini` connection's
  name now puts it into folders, as Total Commander keeps them; a backslash stays in the name.
- **F5/F6 into an SFTP panel** (RC-060-05). The dialog offered the other panel's bare path,
  which the engine read on the source's backend: a copy from a local panel into an SFTP panel
  went to a local folder of that name. It now offers `sftp://<connection>/<path>`.
- **Temporary copies of files opened from archives are cleaned up** (follow-up of wave 5,
  RC-030-02). The read-only copies under `%TEMP%\rcommander\open-<pid>-<n>` go when the app
  (or `rcommander mcp`) exits; at start, those left by runs that have ended are removed. Only
  folders of exactly that name are touched, never a file, link or junction.
- **Security audit, wave 2 round 1.** MCP dispatches run under the sensitive-path guard
  (`Engine::attach_read_judge`), so the engine judges every file a command finds by itself:
  - `files_compare` refuses a key or credential file, also one picked from the panels (H1).
  - `ops_copy` and `ops_move` refuse sources that are or hold one, every entry below them judged
    (H2).
  - A content search never reads one - no hit, counted as skipped, no `withheld` count - judges
    followed links where they lead and refuses a sensitive start folder (M1).
  - A text comparison caps a file at 1,000,000 line breaks; more is not compared line by line
    (M2).
  - A copy or move rejects a listed name that is not one plain name (`../x`, `/abs`; on Windows
    `a\b`, `a:b`) with `invalid_name` instead of writing outside its target (M4).
  - A viewer window inside a key's body is masked, judged by 8 KiB around it (L1).
  - Checksum runs never hash a guarded file inside a folder or named in a checksum file, and a
    directory comparison by content never reads one (L2).
  - A key file is read only as a regular file, capped while reading; the guard judges names also
    as Windows opens them (`key.pem.`, `rcommander.db::$DATA`) (L5).
  - Logs and MCP answers redact `aws_secret_access_key`, `aws_session_token`, `github_token` and
    `refresh_token` values.
  - Stored secrets are bound to their endpoint: through MCP, `connections_update` refuses to
    change the host, port, kind or user of a connection with a stored password, key or
    passphrase, and an ssh_config import does not update such a connection
    (`endpoint_change_needs_gui`); the window still changes them (H3).
- **A copy whose target refuses the part file names the target** (RC-060-06). A target folder
  that refuses `<name>.rcpart` (read-only) or the rename of the complete part now records the
  target as the side that failed (`failure.target`), so the error card shows "On the target:"
  for it as for a write that broke off.
- **`ops.delete`, `ops.mkdir`, `files.read_text`/`read_hex` and `view.open` take server
  locations** (RC-060-05). `sftp://<connection>/<path>` and `ftp://...` were joined onto the
  panel's folder (`<panel>/sftp:/...`); these commands now locate their paths as `ops.copy` does
  and open the server's backend first. Over MCP the path read is judged by the read guard on the
  server too (`sensitive_path`), and `ops_mkdir`'s write guard judges the place on the server.
- **Ctrl+Shift+F closes an FTP connection and leaves the server** (RC-070-03).
  `connections.disconnect` only dropped the engine's references to the FTP session; its pooled control connections stayed
  open (and kept alive) while anything still held the backend, and the panel kept the server's
  listing. Now, as in Total Commander: every idle connection says `QUIT`, nothing is kept, the end
  is announced like an SSH session's, and every panel on the server - FTP or SFTP, either panel -
  goes back to the last local folder of its tab's history (else the other panel's local folder,
  else the home folder); going to the server again logs in again. A changed or deleted connection
  closes its FTP session the same way.
- **The Host keys page's table joins the focus ring** (RC-080-16). Tab now stops on the table
  of trusted keys and certificates; there the arrows select a key and Delete asks to remove it,
  as the page promised - before, the arrows only reached the table while a button had the
  keyboard. The harness test of the focus ring now walks every dialog kind (file operations,
  their questions and progress window, multi-rename, properties, archives, compare and
  synchronize, checksums, the queue window, the database, the hotlist editor, the network drive
  dialogs, the fetch question, the host-key, certificate and MCP questions, the login-script
  editor, "Run command", every settings page, the master password and the three imports).
- **The ssh_config import names its WSL buttons and takes a WSL distribution's keys into the
  vault** (RC-050-06). The buttons that fill in a distribution's `config` showed no text on
  Windows, where the root of a share (`\\wsl.localhost\<distro>\`) has no file name; they now
  carry the distribution's name from the drive bar. A key below a distribution's home was listed
  as `\\?\UNC\wsl$\<distro>\...` - the home was taken from the canonical path - and "Key into the
  vault" refused it as a network share; the home now loses the verbatim prefix
  (`import::ssh_config_path::plain_wsl`), so the key reads `\\wsl$\<distro>\...` and is taken.
  Other shares and verbatim paths stay refused.
- **The File bar of a checksum job fills** (RC-010-07). Creating or verifying checksums, split
  and combine left the progress window's *File* bar empty: their runs reported only the totals.
  `rc_checksum::Control` now names the file at work with its size and its own bytes
  (`current_file`, `file_bytes_done`, `file_bytes_total`, from 0 at each file; the source of a
  split, each part of a combine), and the checksum jobs of `rc-ops` read them into
  `Progress.current`, `file_bytes_done` and `file_bytes_total` at the queue's interval, as a copy
  does; MCP's `ops_list` carries them too.
- **Settings that cannot be stored are logged, slow file system calls leave the window's
  thread** (audit 2026-10-19). The tabs, the window and dialog places and the three histories
  log a warning with their setting key when their write fails, instead of nothing. The drag's
  temporary folder is made, swept and removed on the blocking pool (the app ends once it is
  gone), and the MCP read guard resolves links and probes for a database copy there too, so a
  network drive cannot stall the window or a runtime thread.
- **Compare reads ANSI files as Windows-1252 and UTF-16 without a mark as text** (RC-080-21).
  Bytes 0x80-0x9F showed as control characters (Latin-1) instead of the euro sign and typographic
  quotes, and a UTF-16 file without a byte order mark counted as binary and got no line diff.
  `files_compare` answers `windows1252` where it said `latin1`.
- **A refused save no longer leaves a half-saved connection** (RC-080-25). A wrong key
  passphrase, an unreadable key file, a locked vault or a missing login script after the fields
  had been stored left the connection changed - a new one even created without its secret. Now
  nothing is stored, and the message stands at its field as before.
- **Colours of the JSON, XML and YAML viewers on a file that is not valid UTF-8** (RC-100-22;
  owner, 2026-10-20). The viewers wrote the file's bytes as they were and placed the tokens on
  them, so after an invalid sequence the Lister's U+FFFD (three bytes) shifted the colours of
  the line, or the host dropped its tokens. The text now goes through `rc-plugin-text`'s decoder
  (each invalid part one U+FFFD, `as_is` and `pretty` alike) and a line's tokens are moved onto
  that text (`src/utf8.rs` in each plugin; the YAML viewer reads a line's bytes again up to its
  last token). JSON viewer 0.2.1, XML and YAML viewers 0.1.1. The CSV and batch viewers decode
  through `rc-plugin-text` too; their own tables and decoders are gone, their behaviour is the
  same.
