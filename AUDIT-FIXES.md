# Audit-Befunde: Erläuterungen und Fix-Vorschläge

Ergänzung zu [`AUDIT.md`](AUDIT.md), Stand Commit `b8a4154` (0.17.0).

Die Code-Vorschläge sind **Skizzen gegen den aktuellen Code**. Sie wurden nicht kompiliert und nicht getestet. Namen von Hilfsfunktionen, die es noch nicht gibt, sind als solche gekennzeichnet. Zu jedem Fix gehört ein Regressionstest; der jeweils passende ist unter „Test“ beschrieben.

Reihenfolge: nach Schweregrad, innerhalb davon nach Aufwand/Nutzen.

---

## H1 – Sparse-Tar umgeht den Bomben-Schutz

### Was passiert genau
`describe()` in `crates/rc-vfs/src/archive/tar_format.rs` behandelt GNU-Sparse-Einträge als normale Datei und setzt `raw.size = entry.size()`. Im `tar`-Crate (0.4.46, `archive.rs:539-545`) ist `entry.size` bei Sparse-Einträgen die **expandierte** Größe (`real_size`). Gespeichert sind im Archiv aber nur die Datenblöcke. Diese Größe liefert `header.entry_size()`.

Der Bomben-Schutz besteht aus zwei Teilen:
- `ArchiveLimits::check_entry(size, packed)` prüft das Verhältnis nur, wenn `packed` bekannt ist. Bei Tar ist `raw.packed` immer `None`.
- Der `decode::Guard` misst nur die Ausgabe des Kompressors (gz/xz/zst), also den Tar-Strom. Die Sparse-Expansion passiert erst danach, im `tar`-Crate.

Ergebnis: Ein unkomprimiertes `.tar` von 10 KB mit einem Sparse-Eintrag `real_size = 900 GiB` und einem einzigen 512-Byte-Datenblock wird gelistet und entpackt. `pump` schreibt dabei 900 GiB Nullen dicht auf die Platte. Das globale Limit `max_unpacked_size = 1 TiB` greift erst darüber.

### Fix
1. Für Sparse-Einträge die gespeicherte Größe als `packed` mitgeben. Damit greift die bestehende Ratio-Prüfung (`max_ratio = 500` ab 256 MiB):

```rust
// crates/rc-vfs/src/archive/tar_format.rs, in describe()
let mut raw = RawEntry::new(name, kind, position);
raw.size = entry.size();
if header.entry_type().is_gnu_sparse() {
    // Stored: only the data blocks; `size()` is the expanded size with the holes (tar 0.4).
    raw.packed = header.entry_size().ok();
}
```

2. Zusätzlich eine **Gesamtratio** über das ganze Archiv einführen. Die Summe der deklarierten Größen wird dabei gegen die Dateigröße des Archivs geprüft. Das fängt auch die Fifield-Zip-Bombe ab (M10, siehe unten):

```rust
// crates/rc-vfs/src/archive/safety.rs
impl ArchiveLimits {
    /// The sum of every entry's stated size against the archive file's own size.
    pub(crate) fn check_total(&self, unpacked_total: u64, archive_len: u64, path: &Path) -> Result<()> {
        if unpacked_total > self.max_unpacked_size || self.over_ratio(unpacked_total, archive_len) {
            return Err(limit(path, "total unpacked size"));
        }
        Ok(())
    }
}
```

Diese Prüfung gehört dorthin, wo `Budget` die Größen summiert, nachdem der Baum gebaut ist.

3. Optional: Sparse-Einträge beim Schreiben als Sparse-Datei anlegen (Seek über Löcher) oder ganz ablehnen. Ein Dateimanager braucht sie praktisch nie.

### Test
Ein Tar mit Sparse-Header bauen (`tar::Header::new_gnu()`, Typ `GNUSparse`, `real_size = 1 << 30`, ein Datenblock). Erwartet wird `limit("compression ratio")` beim Listing und beim Extract.

---

## H2 – MCP darf standardmäßig fast beliebige Dateien lesen

### Was passiert genau
`Policy::default()` (`crates/rc-mcp/src/policy.rs:41-47`) setzt `read: Rule::Allowed`. Einzige Schranke ist `guard::judge()` (`crates/rc-mcp/src/guard.rs:103-152`). Das ist eine Denylist aus SSH-Ordner, GnuPG, Schlüssel-Endungen und einigen Credential-Dateien.

Danach greift nur noch die Maske `rc_db::redact::redact_text`. Ihr Verhalten habe ich für dieses Dokument **mit einem kleinen Testprogramm gegen das echte `rc-db` geprüft**:

| Datei / Inhalt | Ergebnis |
|---|---|
| `~/.config/gh/hosts.yml`: `oauth_token: gho_…` | maskiert (`token` als Wort) |
| `~/.cargo/credentials.toml`: `token = "…"` | maskiert |
| `~/.npmrc`: `//registry.npmjs.org/:_authToken=npm_…` | **nicht maskiert** |
| nackter Token `ghp_…` (z. B. in Skripten, `.env.local` ist gesperrt, `deploy.sh` nicht) | **nicht maskiert** |
| `~/.vault-token`: `hvs.CAES…` (die Datei enthält nur den Token) | **nicht maskiert** |

Hinzu kommen Dateien, deren Inhalt **an sich** ein Geheimnis ist und die keine Name-Wert-Form haben:
- Browser-Profile (`Login Data`, `key4.db` + `logins.json`),
- `~/.local/share/keyrings/*`,
- Shell-Historien mit eingetippten Passwörtern,
- `~/.config/rclone/rclone.conf` (Passwörter nur obfuskiert).

Angriffsweg: Prompt-Injection über eine Datei oder Webseite, die der Agent liest. Das Ergebnis eines `files_read_text` landet beim Modellanbieter und eventuell in Tool-Aufrufen an Dritte.

### Fix (empfohlen: zweistufig)

**Stufe 1 – sofort, wenig Aufwand:** Denylist und Maske ergänzen.

```rust
// crates/rc-mcp/src/guard.rs
const CREDENTIAL_NAMES: &[&str] = &[
    ".env", ".git-credentials", ".netrc", ".pgpass", "_netrc", "winscp.ini",
    // neu
    ".npmrc", ".pypirc", ".vault-token", ".yarnrc.yml", ".bash_history", ".zsh_history",
    ".python_history", ".psql_history", ".mysql_history", "credentials.tfrc.json",
    "rclone.conf", "logins.json", "key4.db", "login data", "cookies",
];
const CREDENTIAL_PAIRS: &[(&str, &str)] = &[
    // … bestehend …
    (".cargo", "credentials.toml"), (".cargo", "credentials"),
    ("gh", "hosts.yml"),
];
/// Neu: Ordner, deren gesamter Inhalt Credentials sind.
const CREDENTIAL_DIRS: &[&[&str]] = &[
    &[".config", "gcloud"], &[".azure"], &[".local", "share", "keyrings"],
    &[".password-store"], &[".mozilla", "firefox"], &[".config", "google-chrome"],
    &[".config", "chromium"], &["appdata", "local", "google", "chrome", "user data"],
    &["appdata", "roaming", "mozilla", "firefox"],
];
```

Für die Maske in `crates/rc-db/src/redact.rs`:
- `SECRET_WORDS` um zusammengesetzte Formen erweitern, damit `_authToken` (Wort `authtoken`) greift. Alternativ eine Teilwort-Regel für `token` am Wortende einführen, also `authtoken`, `apitoken`, aber nicht `max_tokens`.
- Eine Erkennung bekannter Token-Präfixe ergänzen, die unabhängig vom Namen greift (`ghp_`, `gho_`, `ghs_`, `github_pat_`, `glpat-`, `npm_`, `xox[abpr]-`, `hvs.`, `AKIA[0-9A-Z]{16}`, `sk-…`). Das ist eine kleine `regex::RegexSet`, die vorhandene Crate ist linear.

**Stufe 2 – grundsätzlich:** Lese-Wurzeln einführen.
- Neue Policy-Option `read_roots: Vec<PathBuf>`, Default: die Ordner, die in den Panels offen sind, plus die Wurzeln, die die Person freigibt.
- Lesen außerhalb dieser Wurzeln fällt auf `Rule::Ask`.
- So muss eine Denylist nicht mehr vollständig sein. Die Denylist bleibt als zweite Linie bestehen.

```rust
// Skizze: crates/rc-mcp/src/server.rs, im Gate für CommandClass::Read
if class == CommandClass::Read && !policy.within_read_roots(&resolved_paths) {
    rule = rule.max(Rule::Ask);
}
```

### Test
`guard_tests.rs` um je einen Fall pro neuem Eintrag erweitern. `redact`-Tests mit den fünf Zeilen aus der Tabelle oben.

---

## M1 – `ssh_exec`-Freigabe überlebt einen Hostwechsel

### Was passiert genau
`exec_target()` (`crates/rc-mcp/src/ask.rs:205-213`) bindet „für 30 Minuten erlauben“ an den String `connection_id=5`. `connections_update` ist Klasse write und damit ohne Rückfrage erlaubt. `refuse_endpoint_move` (`crates/rc-engine/src/commands/connections.rs:79-89`) sperrt einen Endpunktwechsel nur, wenn `holds_secrets()` zutrifft. Bei einer Verbindung mit Agent-Auth oder Key-Datei ohne Passphrase ist das nicht der Fall.

Ablauf:
1. Die Person erlaubt `ssh_exec` auf „test-vm“ (id 5).
2. Der Agent ruft `connections_update {id:5, host:"prod-db"}`.
3. Der Agent ruft `ssh_exec {connection_id:5, command:"…"}`. Die Freigabe greift, das Kommando läuft auf `prod-db`. Voraussetzung: Der Host-Key von prod-db ist bekannt. Das ist typischerweise der Fall, wenn die Person sich dort schon einmal verbunden hat.

### Fix
Freigaben verwerfen, sobald sich der Endpunkt einer Verbindung ändert. Das gilt unabhängig davon, ob die Änderung von MCP oder aus der GUI kommt.

```rust
// crates/rc-engine/src/mcp_grants.rs
impl Grants {
    /// Drops every grant bound to `target` (e.g. "connection_id=5"); whether any went.
    pub(crate) fn revoke_target(&mut self, target: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|held| held.grant.target.as_deref() != Some(target));
        before != self.0.len()
    }
}

// crates/rc-engine/src/commands/connections.rs – im Update-Pfad, nach erfolgreichem Speichern
if moves_endpoint(&current, &patch) {
    self.revoke_mcp_target(&format!("connection_id={}", id.0)); // neu: ruft revoke_target, publiziert Grants-Event
}
```

Zusätzlich sinnvoll: `refuse_endpoint_move` auch dann greifen lassen, wenn für die Verbindung eine aktive `ssh_exec`-Freigabe besteht.

### Test
In `rc-mcp/tests`:
1. Freigabe setzen.
2. `connections_update` mit neuem Host ausführen.
3. `ssh_exec` aufrufen. Erwartet wird eine erneute Rückfrage (`headless_ask_denied` im Headless-Fixture).

---

## M2 – Schreib-Denylist umgehbar

### Was passiert genau
`guard_write.rs` kennt `.bashrc`, `.profile`, `.gitconfig` und `fish/config.fish`, aber nicht die Dateien, die diese **einbinden** oder die gleichwertig sind. Ein Agent, der nur write hat (Default: erlaubt), kann folgende Wege nutzen:
- **`~/.bash_aliases` schreiben:** Die Debian/Ubuntu-`.bashrc` sourct diese Datei. Das nächste Terminal führt den Inhalt aus.
- **`~/.config/git/config` schreiben** (XDG-Pfad, gleichwertig zu `.gitconfig`), z. B. mit `core.fsmonitor = sh -c '…'`. Das nächste `git status` in irgendeinem Repo führt den Befehl aus.
- **`<repo>/.git/config`:** dasselbe, nur lokal im Repo.
- **`~/.local/bin/git` anlegen plus `files_set_attributes mode=755`:** Die Datei verdeckt das echte `git`, weil `~/.local/bin` in Ubuntu vorne im PATH steht.

### Fix
1. Listen ergänzen:

```rust
// crates/rc-mcp/src/guard_write.rs
const RUN_BY_TOOLS: &[(&[&str], &str)] = &[
    (&[".git", "hooks"], "Git's hooks"),
    (&[".git", "config"], "Git's configuration"),
    (&["git", "config"], "Git's configuration"),          // ~/.config/git/config
    (&["fish", "config.fish"], "a shell's start-up file"),
    (&["fish", "conf.d"], "a shell's start-up file"),
    (&["fish", "functions"], "a shell's start-up file"),
    (&[".vscode", "tasks.json"], "an editor's tasks"),
    (&[".vscode", "settings.json"], "an editor's settings"),
    (&[".cargo", "config.toml"], "Cargo's configuration"),
    (&[".cargo", "config"], "Cargo's configuration"),
    (&[".local", "bin"], "a folder on the PATH"),
    // ~/bin nur direkt unter dem Home-Ordner prüfen, nicht jedes "bin" (eigene Regel)
    (&["applications"], "a desktop launcher"),              // ~/.local/share/applications
    (&["environment.d"], "the session's environment"),
    (&["plasma-workspace", "env"], "the session's environment"),
];
const SHELL_STARTUP: &[&str] = &[
    // … bestehend …
    ".bash_aliases", ".zlogout", ".xprofile", ".xsessionrc", ".xinitrc",
    ".inputrc", ".npmrc", ".pam_environment",
];
```

2. Grundsätzlich wie bei H2: **Schreib-Wurzeln** (`write_roots`). Schreiben außerhalb der Ordner, die in den Panels offen sind, wird zu `Ask`. Dann ist die Liste nur noch die zweite Linie.

---

## M3 – `files_set_attributes` mit Spezialbits und auf `~`

### Fix
Spezialbits über MCP ablehnen. Das Öffnen von Rechten für Gruppe oder alle als destructive einstufen:

```rust
// crates/rc-mcp/src/tools/properties.rs
/// Octal permission bits an agent may set: no set-uid, set-gid or sticky bit.
pub(crate) fn parse_mode(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 8).ok().filter(|mode| *mode <= 0o777)
}

/// Whether `mode` opens writing to group or others: judged as destructive (asks).
pub(crate) fn opens_write(mode: u32) -> bool {
    mode & 0o022 != 0
}
```

In `files_set_attributes` vor dem Dispatch:
- Ist `opens_write(mode)` erfüllt oder ist der Pfad das Home-Verzeichnis bzw. ein Elternordner eines geschützten Pfads (`~/.ssh`, `~/.config`), dann die Klasse auf `Destructive` heben. Dafür gibt es schon `server.rs:230-236`, das für `queue_load` argumentabhängig klassifiziert.

---

## M11 – Exec über Workspace-Autostart

### Fix
Einzeilig: `workspaces.on_start` als geschützten Schlüssel markieren.

```rust
// crates/rc-engine/src/settings/schema/workspaces.rs:45
    protected: true,
```

Damit wird `settings_set workspaces.on_start` über MCP abgelehnt, wie die anderen geschützten Schlüssel auch. Die GUI ist davon nicht betroffen.

---

## M4 – OSC 133 aus Programmausgabe fälschbar

### Was passiert genau
`CwdSync::on_payload` (`crates/rc-term/src/cwd_sync.rs:173-182`) behandelt **jedes** `ESC]133;A` im Ausgabestrom als „Shell steht am Prompt“. Wartet ein `cd`, weil das Panel gewechselt hat, während ein Programm lief, wird er sofort geschrieben. Die Quelle der Markierung spielt dabei keine Rolle: Remote-Shell, `cat` einer präparierten Datei oder ein Log mit Escape-Sequenzen.

Konkreter Ablauf:
1. In der lokalen bash (mit Integration) läuft `ssh winbox` (cmd.exe).
2. Die Person wechselt im Panel in den Ordner `x&calc&`. Der `cd` wartet.
3. Der Server gibt `\e]133;A\e\\` aus (z. B. aus einem `type`-Befehl).
4. rCommander tippt ` cd -- 'x&calc&'\r` in cmd. cmd führt `calc` aus.

### Fix: Session-Nonce in der Markierung
1. Beim Start der Shell eine Zufalls-Nonce erzeugen (16 Byte hex) und als Umgebungsvariable `RC_SHELL_NONCE` übergeben. Das passiert dort, wo `Integration.env` gebaut wird (`crates/rc-term/src/integration.rs`).
2. Die Snippets geben die Nonce mit aus: `printf '\e]133;A;rc=%s\e\\' "$RC_SHELL_NONCE"`. Für cmd entsprechend im `PROMPT`: `$E]133;A;rc=<nonce>$E\`.
3. Der Parser akzeptiert nur Markierungen mit passender Nonce:

```rust
// crates/rc-term/src/osc.rs – parse_osc bekommt die erwartete Nonce
let mut parts = text.strip_prefix("133;")?.split(';');
let kind = parts.next()?;
let nonce_ok = parts.any(|part| part.strip_prefix("rc=") == Some(expected_nonce));
if !nonce_ok {
    return None; // a mark from a program's output, not from our snippet
}
```

4. OSC 7 und `9;9` (Ordnermeldungen) genauso behandeln, also z. B. `\e]7;file://host/path;rc=<nonce>` oder eine eigene Sequenz `\e]1337;RcCwd=…;rc=…`.

Die Nonce ist für Programme lesbar, die die Person selbst in dieser Shell startet. Das liegt im Vertrauensmodell („die Person am Keyboard“). Für Remote-Server und Dateiinhalte ist sie nicht lesbar, und genau das ist der Angriffsweg.

Rückwärtskompatibilität: Handinstallierte Snippets aus dem Wiki ohne Nonce folgen dann nicht mehr. Das ist sicher, weil dann nur nichts getippt wird. Die Wiki-Seite entsprechend aktualisieren.

### Test
`cwd_sync_tests.rs`: Ein `133;A` ohne Nonce bei wartendem `cd` darf `changes.write` nicht setzen.

---

## M5 – SSH-Start-`cd` ohne Shell-Prüfung

### Fix
```rust
// crates/rc-engine/src/commands/ssh_terminal.rs
pub fn cd_command(folder: &str) -> Result<Vec<u8>, EngineError> {
    if folder.chars().any(char::is_control) { /* wie bisher */ }
    // A Windows server's shell is cmd or PowerShell, where single quotes do not hold
    // `& | < > ^ %`: such a folder is not typed at all.
    if folder.chars().any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '%' | '!' | '`' | '$')) {
        return Err(VfsError::InvalidPath {
            path: PathBuf::from(folder),
            reason: "a shell character cannot be typed safely into an unknown shell",
        }.into());
    }
    Ok(format!("cd -- {}\r", shell_quote(folder)).into_bytes())
}
```

Besser, aber aufwendiger: Statt zu tippen, die Shell per `exec`-Request starten: `cd -- '<folder>' && exec "$SHELL" -l`. Dann kommt `cd` nie als Tastatureingabe an. Ist die Remote-Shell nicht POSIX, schlägt der Befehl fehl, statt etwas auszuführen. Erkennung über eine erste `exec`-Probe `echo $0` ist möglich, kostet aber einen Roundtrip.

`--` ist auch auf POSIX nötig, weil sonst Ordner wie `-P` oder `-` als Option bzw. `OLDPWD` gelesen werden.

---

## M6 – Spezialbits von Remote-Servern

### Fix
Beim Übernehmen der Attribute nach der Herkunft unterscheiden. In `crates/rc-ops/src/staging.rs` `preserve()`:

```rust
let mut mode = entry.attributes.mode.filter(|_| capabilities.unix_mode);
if !self.src_is_local() {               // neu: Quelle ist SFTP/FTP/Archiv/Device
    // A server's word on set-uid, set-gid and sticky is not taken; nor world-write.
    mode = mode.map(|bits| bits & 0o777 & !0o002);
}
```

`src_is_local()` lässt sich aus der Vfs-Kennung der Quelle ableiten. `capabilities()` des Quell-Vfs oder dessen Typ ist im `Staging`-Kontext verfügbar. Alternativ: eine umask-Logik wie bei `extract.rs:344` an einer Stelle zentralisieren und für alle nicht-lokalen Quellen anwenden.

---

## M7 – Rekursives „Attribute ändern“ folgt Symlinks

### Fix
Links in der Rekursion für Modus und readonly überspringen und für die mtime die Link-Variante verwenden:

```rust
// crates/rc-ops/src/attributes.rs, in der Schleife
let is_link = matches!(entry.kind, EntryKind::Symlink { .. });
if is_link && (change.mode.is_some() || change.readonly.is_some()) {
    report.skipped += 1;   // neu: als übersprungen melden, nicht als Fehler
    continue;
}
```

```rust
// crates/rc-vfs/src/local/unix.rs, set_properties
if let Some(modified) = change.modified {
    let time = filetime::FileTime::from_system_time(modified);
    filetime::set_symlink_file_times(long_path(path), time, time)   // folgt dem Link nicht
        .map_err(|error| VfsError::from_io(error, path))?;
}
```

Hinweis: `set_symlink_file_times` setzt atime und mtime. Die atime sollte daher vorher per `symlink_metadata` gelesen und beibehalten werden.

Den Modulkommentar („Links are changed as such“) entsprechend korrigieren.

### Test
Baum mit `link -> ../outside/secret` (Modus 0600), rekursiv `mode=0644` setzen. Erwartet: `outside/secret` bleibt 0600.

---

## M8 – Entwicklungsschlüssel als Plugin-Root in Release-Builds

### Fix
```rust
// crates/rc-plugin-host/src/sign/roots.rs
pub const EMBEDDED_KEYS: &[EmbeddedKey] = &[
    EmbeddedKey { key_id: RELEASE_KEY_ID, public_key: "B/mK…", not_after: None },
    #[cfg(any(debug_assertions, feature = "dev-plugin-key"))]
    EmbeddedKey { key_id: DEVELOPMENT_KEY_ID, public_key: "B0zG…", not_after: None },
];
```

Alternativ, falls lokal signierte Pakete in Release-Builds weiter funktionieren sollen: dem Dev-Key ein festes `not_after` geben (z. B. 90 Tage ab Release). Dann muss ein kompromittierter Dev-Key spätestens zu diesem Datum ersetzt sein. Pakete mit Dev-Key sollten in Release-Builds zudem nicht automatisch über `sync_bundled` installiert werden, sondern nur mit TOFU-Dialog.

---

## M9 – Signierschlüssel während des Builds in der Umgebung

### Was passiert genau
In `.github/workflows/release.yml:63-72` stehen `RCOMMANDER_PLUGIN_SIGNING_KEY` (als `env`) und die Schlüsseldatei (`RC_PLUGIN_KEY`) bereit, während `scripts/build-plugins.sh` folgende Schritte ausführt:
- `cargo build -p rcommander` (Z. 115): baut alle Build-Skripte und Proc-Macros des Workspaces.
- `cargo rustc` pro Plugin (Z. 137).

Jedes `build.rs` einer transitiven Abhängigkeit kann `std::env::var("RCOMMANDER_PLUGIN_SIGNING_KEY")` lesen. Bei einem Supply-Chain-Angriff auf ein beliebiges Crate im Lockfile ist der Release-Plugin-Schlüssel weg.

### Fix
Bauen und Signieren in zwei Steps trennen. `build-plugins.sh` bekommt dafür zwei Modi:

```bash
# scripts/build-plugins.sh – neue Optionen
#   --build-only   baut Packager und Komponenten, signiert nichts
#   --sign-only    packt und signiert die Komponenten aus $COMPONENTS, ruft keinen Compiler
```

```yaml
# .github/workflows/release.yml, Job plugins
      - name: Build the bundled plugins (no key in this step)
        run: scripts/build-plugins.sh --build-only
      - name: Sign the bundled plugins
        env:
          RCOMMANDER_PLUGIN_SIGNING_KEY: ${{ secrets.RCOMMANDER_PLUGIN_SIGNING_KEY }}
        run: |
          key="${RUNNER_TEMP}/plugin-signing.pem"
          trap 'rm -f "${key}"' EXIT
          (umask 077 && printf '%s\n' "${RCOMMANDER_PLUGIN_SIGNING_KEY}" > "${key}")
          RC_PLUGIN_KEY="${key}" scripts/build-plugins.sh --sign-only
```

Restrisiko: Der Packager (`rcommander`, Debug-Profil) enthält selbst Code der Abhängigkeiten und läuft mit dem Schlüssel. Noch besser wäre ein schlankes Signier-Binary mit wenigen Abhängigkeiten (ed25519 + Paketformat), analog zu `rc-pack` für die Update-Manifeste.

---

## M14 – Listing folgt Symlinks auf UNC-Ziele (Windows, NTLM)

### Fix
```rust
// crates/rc-vfs/src/local/describe.rs
let (kind, shown) = if meta.file_type().is_symlink() {
    let target = fs::read_link(real).map(|t| strip_verbatim(&t)).unwrap_or_default();
    // A link to a share is not followed: following it signs in there (NTLM, RC-080-10).
    let to_network = cfg!(windows) && crate::network_path(&target.to_string_lossy()).is_some();
    let followed = if to_network { None } else { fs::metadata(real).ok() };
    match followed {
        Some(followed) => (EntryKind::Symlink { target, target_kind: Some(target_kind(&followed)) }, followed),
        None => (EntryKind::Symlink { target, target_kind: None }, meta.clone()),
    }
}
```

Relative Link-Ziele können nicht auf UNC zeigen. Bei absoluten genügt die Prüfung oben. Für Junctions bzw. Mount-Points gilt dasselbe, falls `read_link` dort ein `\??\UNC\…` liefert. Dann vor `strip_verbatim` prüfen. `network_path` erkennt `\??\` bereits als `Other`.

---

## M15 – `config.import` warnt nicht bei indirekten Secrets

### Fix
```rust
// crates/rc-engine/src/commands/exchange_connections.rs:123-128
let moved = endpoint(&draft) != endpoint(&entry.connection);
if moved && self.holds_secrets(connection).await {   // statt nur connection.secrets
    row.warning = Some("endpoint_with_secrets".to_owned());
}
```

`holds_secrets` (`commands/connections.rs:58`) deckt bereits ab: gespeicherte Secrets, `credential_id`, Secret-Sources und Login-Skripte mit `send_secret`. Die Vorschau-Funktion muss dafür `async` und auf der Engine sein. Sie hat ohnehin DB-Zugriff.

---

## M16 – Update kann auf ältere Version zurückrollen

### Fix
```rust
// crates/rc-engine/src/updates/handover.rs – in start_update_download und hand_over_update,
// direkt nach dem Auslesen von `offer`:
if !rc_update::is_newer(&offer.version, crate::updates::RUNNING) {
    return Err(UpdateRefusal::new("update.not_newer", "the offer is not newer than this version"));
}
```

Für den neuen Fehlercode `update.not_newer` eine Übersetzung ergänzen. Ein Test in `updates_tests.rs` speichert ein Angebot mit `RUNNING` als Version und erwartet `update.not_newer`.

---

## M17 – Kein `fsync` vor Rename / Quell-Löschen

### Fix
Eine `sync`-Methode im Write-Stream des Vfs einführen. Lokal ist das `File::sync_all`, remote ein No-op bzw. SFTP `fsync@openssh.com`, falls der Server das anbietet:

```rust
// crates/rc-vfs – trait WriteStream (Skizze)
async fn sync(&mut self) -> Result<()> { Ok(()) }   // default: nichts

// crates/rc-vfs/src/local – Implementierung
async fn sync(&mut self) -> Result<()> {
    self.file.sync_all().await.map_err(|e| VfsError::from_io(e, &self.path))
}
```

Aufrufen vor `settle` (`stream_pair.rs:land`, `staging.rs:167`) und, bei Move, vor `remove_source`. Optional danach das Zielverzeichnis fsyncen (Unix: `File::open(dir)?.sync_all()`).

Kosten: Auf langsamen Medien kostet das einige Millisekunden pro Datei. Wer das nicht will, bekommt eine Option „schnell (ohne Sync)“. Bei Move sollte der Sync aber immer erfolgen.

---

## M20 – Kein Panic-Hook

### Fix
```rust
// crates/rcommander/src/main.rs, erste Zeilen in main()
let previous = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
    let payload = info.payload().downcast_ref::<&str>().copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic>");
    let backtrace = std::backtrace::Backtrace::force_capture();
    tracing::error!(target: "panic", %location, payload, %backtrace, "panic");
    previous(info);
}));
```

Der Hook muss nach der Initialisierung des `tracing`-Subscribers gesetzt werden, der ins Fehlerlog (`rc-db/src/error_log`) schreibt. Sonst geht die Meldung ins Leere. Zusätzlich an den Stellen, an denen `JoinHandle`s abgewartet werden, `JoinError::is_panic()` auswerten und loggen.

---

## Abhängigkeiten / `deny.toml`

```toml
# deny.toml – entfernen (cargo deny meldet "advisory-not-detected"):
#   RUSTSEC-2026-0367, -0368, -0369   (zip ist bereits 9.2.2)
#   RUSTSEC-2026-0391, -0398, -0403   (zstd 0.13.4 / zstd-safe 7.4.0 nicht mehr betroffen)
```

```bash
cargo update -p async-executor   # yanked 1.14.0
```

`lru 0.16.4` (RUSTSEC-2026-0253, unsound, über `cryoglyph ← iced_wgpu`): Ein Fix ist ohne iced-Update nicht möglich. Den Eintrag mit Begründung in `deny.toml` aufnehmen, damit er sichtbar bleibt. Die Begründung: `LruCache::pop` mit panickendem `Drop` tritt im Glyph-Cache nicht auf. Beim nächsten iced-Release prüfen.

---

## Kleinere Fixes (Einzeiler / wenige Zeilen)

| Befund | Ort | Fix |
|---|---|---|
| Update-HTTP-Client fällt still auf Default zurück | `crates/rc-update/src/fetch.rs:148` | `HttpFetcher::new() -> Result<Self>`; Fehler weiterreichen statt `unwrap_or_default()` |
| Export-Passwort ohne Mindestlänge | `crates/rc-engine/src/commands/exchange_export.rs:59-65` | bei `secrets == true` dieselbe Prüfung wie `MIN_MASTER_PASSWORD_LEN` |
| Plugin-Key vor Komponentenprüfung vertraut | `crates/rc-engine/src/plugin_admin/commands.rs:133-146` | erst mit temporär vertrautem Verifier vollständig prüfen, dann `trust_plugin_key` |
| Multi-Rename-Rename ohne `no_replace` | `crates/rc-ops/src/rename.rs:50-64` | `vfs.rename_no_replace` statt `stat` + `rename` |
| Pack ohne Überschreiben nicht atomar | `crates/rc-ops/src/pack/mod.rs:361` | bei `!overwrite` `rename_no_replace` |
| SSH-Terminal: SIGHUP ohne Eskalation | `crates/rc-term/src/local_pty.rs:282-290` | Master droppen, nach 2 s `killpg(pgid, SIGKILL)` |
| OSC 52 standardmäßig an | `crates/rc-term-widget/src/terminal.rs:114` | Default `false` oder nur fokussiertes Terminal, Limit 100 KB |
| Restore übernimmt fremde Trigger | `crates/rc-db/src/restore.rs:355-421` | `SELECT type,name FROM sqlite_schema` gegen die erwartete Menge prüfen, Fremdobjekte ablehnen |
| MCP-Token über Neustarts gleich | `crates/rc-mcp/src/token.rs:71-80` | pro Start neu erzeugen (steht ohnehin in der Instance-Datei) |
| SFTP-Upload ohne Rechte beim Open | `crates/rc-remote/src/sftp/change.rs:27-34` | `FileAttributes { permissions: Some(0o600), .. }` beim `open`, Endrechte via `preserve` |
| `workspaces.on_start` ungeschützt | s. M11 | `protected: true` |

---

## Vorgeschlagene Reihenfolge

1. **Sofort (klein, hoher Nutzen):** M11, M16, M3 (`parse_mode`), H1 Schritt 1, M9, `deny.toml`, M20.
2. **Kurzfristig:** H2 Stufe 1, M2-Listen, M1, M5, M6, M7, M14, M15.
3. **Mittelfristig (Design):** H2/M2 Stufe 2 (Lese- und Schreib-Wurzeln), M4 (Nonce inkl. Snippets und Wiki), M17 (`sync` im Vfs), H1 Gesamtratio, Fuzzing der Parser.
