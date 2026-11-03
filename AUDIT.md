# Code-Audit rCommander

**Stand:** Commit `b8a4154` (Release 0.17.0), Default-Branch  
**Datum:** 2026-11-03  
**Umfang:** gesamtes Repository (~408k Zeilen Rust in 20 Crates, 13 Plugins, SDK, Skripte, CI)  
**Methode:** Code gelesen (statisch), Querprüfung gegen Quellen von Abhängigkeiten (`tar`, `portable-pty`, `vte`, `alacritty_terminal`), `cargo audit` 0.22.2 und `cargo deny` gegen die aktuelle Advisory-DB. Tests und Clippy wurden in diesem Audit **nicht** ausgeführt (Build von ~400k Zeilen inkl. wasmtime/iced). Produktivcode wurde nicht verändert.

Schweregrade: **kritisch** (Fernausnutzung ohne Nutzerzutun / Schlüsselverlust), **hoch** (Ausnutzung durch präparierte Eingabe mit realem Schaden), **mittel** (Ausnutzung braucht Vorbedingungen oder Schaden begrenzt), **niedrig** (Härtung, Randfall, Wartbarkeit).
Jeder Befund wurde am Code nachvollzogen; wo ein Teil nicht verifiziert werden konnte, steht **[unverifiziert]**.

---

## 1. Zusammenfassung

Die Codebasis ist für ein Projekt dieser Größe außergewöhnlich diszipliniert: `unsafe_code = "deny"` mit Freigaben nur in Windows-FFI-/PTY-/Device-Modulen (jeweils mit `// SAFETY:`), keine `.unwrap()` in Produktivcode von `rc-engine`/`rcommander`, Dateilängen-Limit, `allow` nur mit Begründung, CI mit gepinnten Actions auf fünf Plattformen, `cargo deny` online. Kryptografie (Vault, Update-Signatur, Plugin-Signatur), SSH-Host-Key-Prüfung, FTPS-Pinning und Zip-Slip-Abwehr sind korrekt umgesetzt.

**Es gibt keinen kritischen Befund.** Die wichtigsten Punkte:

| # | Schweregrad | Bereich | Kurzbeschreibung |
|---|---|---|---|
| H1 | hoch | Archive | GNU-Sparse-Tar umgeht den Bomben-Schutz – wenige KB ⇒ bis 1 TiB Nullen auf Platte |
| H2 | hoch | MCP | Lese-Denylist lückenhaft, `read` standardmäßig ohne Rückfrage ⇒ Abfluss von Tokens (gh, npm, cargo, gcloud, Browser …) |
| M1 | mittel | MCP | `ssh_exec`-Freigabe an Verbindungs-ID gebunden; Agent kann danach den Host der Verbindung umbiegen |
| M2 | mittel | MCP | Schreib-Denylist umgehbar (`~/.bash_aliases`, `~/.config/git/config`, `~/.local/bin` …) ⇒ write wird zu exec |
| M3 | mittel | MCP | `files_set_attributes` setzt ohne Rückfrage Modus bis `0o7777` auch auf `~` |
| M4 | mittel | Terminal | OSC 133 aus Programmausgabe fälschbar ⇒ Auto-`cd` wird in fremde Programme/Shells getippt |
| M5 | mittel | Terminal | SSH-Start-`cd` nimmt POSIX-Shell an, ohne `--` ⇒ Command-Injection auf Windows-OpenSSH (`cmd.exe`) |
| M6 | mittel | Remote | setuid/setgid/sticky vom Server werden auf heruntergeladene Dateien übernommen |
| M7 | mittel | Dateiops | Rekursives „Attribute ändern“ folgt Symlinks (Unix) |
| M8 | mittel | Plugins | Entwicklungs-Signaturschlüssel ist in Release-Builds vollwertige Root ohne Ablauf |
| M9 | mittel | CI | Plugin-Signierschlüssel liegt während `cargo build` (alle Build-Skripte der Abhängigkeiten) in der Umgebung |

Insgesamt: **0 kritisch, 2 hoch, 20 mittel, ~40 niedrig**.

---

## 2. Sicherheit

### 2.1 Archive / Entpacken (rc-vfs, rc-ops)

**H1 – [hoch] Sparse-Tar-Bombe umgeht das Ratio-Limit**
- Ort: `crates/rc-vfs/src/archive/tar_format.rs:149` (Sparse ⇒ `RawKind::File`), `:157` (`raw.size = entry.size()`), `crates/rc-vfs/src/archive/safety.rs:89-94` (`check_entry` greift nur mit `packed`).
- Verifiziert in `tar-0.4.46/src/archive.rs:539-545`: für GNU-Sparse setzt das Crate `entry.size` auf die **reale** (expandierte) Größe; Löcher werden beim Lesen mit Nullen aufgefüllt. Tar kennt kein `packed`, der Stream-Guard von tar.gz misst nur den Tar-Strom, nicht die Expansion.
- Szenario: Ein Tar von wenigen KB deklariert Sparse-Dateien mit zusammen knapp 1 TiB (Default `max_unpacked_size = 1 << 40`, `safety.rs:55`). Entpacken schreibt die Nullen **dicht** ⇒ Platte voll.
- Fix: Für Sparse-Einträge die Summe der Datenblöcke als `packed` an `check_entry` geben; zusätzlich Gesamtverhältnis „Summe entpackt / Archivgröße“ prüfen; Sparse-Dateien sparse schreiben (oder ablehnen).

**M10 – [mittel] Kein globales Ratio-Limit, Default 1 TiB**
- Ort: `crates/rc-vfs/src/archive/safety.rs:55`, `zip_format.rs:48,129`.
- Ratio wird nur pro Eintrag und erst ab 256 MiB geprüft; überlappende Zip-Einträge (Fifield-Bombe) oder viele Einträge knapp unter der Schwelle werden nicht erkannt.
- Fix: Gesamtratio gegen Archivgröße; überlappende lokale Header erkennen; Default an freien Platz des Ziels koppeln.

**N – [niedrig] Extrahierte Modi ignorieren die umask** – `crates/rc-ops/src/extract.rs:344` maskiert auf `0o777`, `staging.rs:201-205` setzt per chmod ⇒ Archiv kann world-writable Dateien erzeugen. Fix: `mode & !umask` bzw. Obergrenze 0755/0644.

Positiv: `clean_name` (`safety.rs:159`) lehnt `..`, absolute Pfade, Laufwerksbuchstaben, UNC und `\` ab und **überspringt** solche Einträge; `target_path::below` prüft jede Komponente (ADS, Gerätenamen, Punkt am Ende); Links/Hardlinks aus Archiven werden nie angelegt; `make_dirs` lehnt Links/Junctions im Ziel ab; SUID/SGID werden entfernt. Die in `deny.toml` genannten `zip::extract`-APIs werden tatsächlich nicht benutzt.

### 2.2 MCP-Server (rc-mcp, rc-engine dispatch)

**H2 – [hoch] Lese-Denylist mit Lücken; Lesen standardmäßig erlaubt**
- Ort: `crates/rc-mcp/src/guard.rs:33-67, 103-152`; Default `read: Rule::Allowed` in `crates/rc-mcp/src/policy.rs:43`.
- Nicht erfasst u. a.: `~/.config/gh/hosts.yml`, `~/.npmrc`, `~/.pypirc`, `~/.cargo/credentials.toml`, `~/.config/gcloud/*`, `~/.azure/*`, `~/.vault-token`, `~/.terraform.d/credentials.tfrc.json`, `~/.config/rclone/rclone.conf`, Browser-Profile (`logins.json`/`key4.db`, `Login Data`), Shell-Histories.
- Szenario: Ein per Prompt-Injection gesteuerter Agent ruft `files_read_text ~/.npmrc` oder `~/.vault-token` – keine Rückfrage. Es bleibt nur die namensbasierte Maske (`crates/rc-db/src/redact.rs`). Nachträglich per Testprogramm gegen `rc-db` verifiziert: `oauth_token:` (gh) und `token = …` (cargo) werden maskiert, **`_authToken=…` (npm), nackte Tokens (`ghp_…`, `hvs.…`) und Dateien, deren Inhalt selbst das Geheimnis ist, nicht**.
- Fix: Allowlist-Modus („nur unter diesen Wurzeln“) oder `read: ask` außerhalb freigegebener Wurzeln; Denylist deutlich erweitern; Maske um zusammengesetzte Namen (`authtoken`) und bekannte Token-Präfixe erweitern. Details und Code-Skizzen: [`AUDIT-FIXES.md`](AUDIT-FIXES.md).

**M1 – [mittel] `ssh_exec`-Freigabe hängt an der Verbindungs-ID, nicht am Ziel**
- Ort: `crates/rc-mcp/src/policy.rs:361-369` (`exec_target` ⇒ `connection_id=N`), `crates/rc-mcp/src/ask.rs:206-214`, `crates/rc-engine/src/commands/connections.rs:57-91`.
- Szenario: Person erlaubt `ssh_exec` auf Verbindung 5 (Agent-/Key-Datei-Auth, kein gespeichertes Secret ⇒ `holds_secrets` false) für 30 min. Agent ruft `connections_update id=5 host=prod…` (Klasse write, erlaubt) und danach `ssh_exec connection_id=5` – ohne Rückfrage auf Produktion, mit den Keys des SSH-Agents. Voraussetzung: Host-Key von prod bereits vertraut.
- Fix: Freigabe an (Host, Port, User, Auth-Art) binden oder bei jedem `connections.update/save` alle Freigaben der Verbindung verwerfen.

**M2 – [mittel] Schreib-Denylist umgehbar ⇒ write wird exec**
- Ort: `crates/rc-mcp/src/guard_write.rs:41-79, 90-145`.
- Ungeschützt u. a.: `~/.bash_aliases`, `~/.config/fish/conf.d/*.fish`, `~/.config/git/config` (`core.pager`/`core.fsmonitor`), `<repo>/.git/config`, `~/.local/bin/*`, `~/.local/share/applications/*.desktop`, `~/.xprofile`, `~/.config/environment.d/`, `.cargo/config.toml`, `.vscode/settings.json`. Inhalte kommen per `ops_copy` (auch von SFTP), `archives_extract`, `files_combine`; Ausführungsbit per `files_set_attributes` – alles write, Default erlaubt.
- Fix: write außerhalb freigegebener Wurzeln als `ask`; gepflegter Katalog von Autostart-/Konfigurationspfaden.

**M3 – [mittel] `files_set_attributes` ohne Rückfrage, bis `0o7777`, auch auf `~`**
- Ort: `crates/rc-mcp/src/tools/properties.rs:168-171, 264-304`.
- Szenario: `paths=["/home/u"], mode="777"` ist erlaubt ⇒ andere lokale Benutzer können Einträge in `~` ersetzen; sshd (StrictModes) verweigert danach Key-Login.
- Fix: Bits `0o7000` ablehnen; Öffnen von Rechten (group/world-write) als destructive einstufen; `~` und Eltern geschützter Pfade sperren.

**M11 – [mittel] Exec-Rückfrage per Workspace-Autostart umgehbar**
- Ort: `crates/rc-engine/src/settings/schema/workspaces.rs:39-47` (`workspaces.on_start` nicht geschützt), `crates/rc-engine/src/workspaces/apply.rs:111-124`.
- Szenario: Agent legt via `workspaces_create` (write) einen Workspace mit Terminal-/SSH-Ansicht an und setzt `settings_set workspaces.on_start=<id>` (write). Beim nächsten Start öffnen sich Shells/SSH-Sessions, obwohl `workspaces_apply` exec ist.
- Fix: `workspaces.on_start` zu den geschützten Schlüsseln; Terminal-Workspaces nur nach Bestätigung als Autostart.

**M12 – [mittel, Testlücke] `tests/secret_canary_sweep.rs` deckt die riskantesten Pfade nicht ab**
- Fixture ohne `with_codes()` ⇒ alle destructive/exec-Tools (`ssh_exec`, `terminal_*`, `workspaces_apply` …) enden in `headless_ask_denied`; Argumente werden nur für `id/connection_id/script_id/panel/path` synthetisiert (`:241-255`); nur In-Process-Transport; Kodierungen ohne Hex-Groß, Base64 mit Versatz, URL-Encoding, UTF-16 (`:70-79`); `mcp.token` ist kein Canary.
- Fix: Durchlauf mit eingelösten Codes, Argumente aus dem Schema, HTTP-Transport, weitere Kodierungen, Token als Canary.

**Niedrig (MCP):**
- Token über Neustarts konstant (`crates/rc-mcp/src/token.rs:71-80`); stdio-Proxy sendet nach App-Absturz bis ~6 s weiter `Bearer <token>` an den Port (`stdio.rs:118-121,157-163`) ⇒ lokaler Port-Übernehmer erhält dauerhaft gültigen Token. Fix: Token pro Start rotieren, Proxy beim ersten Sendefehler stoppen.
- TOCTOU Klassenbestimmung `queue_load` (`server.rs:230-236` vs. `dispatch/mcp.rs:79-91`) – theoretisch.
- Kein Audit-Log der MCP-Aufrufe/Entscheidungen.
- `terminal_read_screen` ist Klasse read (liest Scrollback eigener Terminals der Person) – Design-Hinweis, `ask` empfohlen.

Positiv: Bind nur `127.0.0.1`; Host-, Origin- (inkl. `null`) und Bearer-Prüfung (`http.rs:248-323`); Token 32 Byte CSPRNG, konstantzeitiger Vergleich; Instance-Datei 0600 atomar; Health-Probe ohne Token mit BLAKE3-MAC; Body-Limit 4 MiB; lineare Regex; zentrales, fail-closed Gate; destructive/exec nicht auf `allowed` stellbar; Bestätigungscodes an Session+Tool+Argumente gebunden, einmalig, ablaufend; Maske auch über Fehlermeldungen; Vault-/MCP-/Plugin-/Update-Settings geschützt.

### 2.3 Terminal, Prozesse, WSL (rc-term, rc-process, rc-engine)

**M4 – [mittel] Prompt-Zustand (OSC 133) aus Programmausgabe fälschbar**
- Ort: `crates/rc-term/src/osc.rs:137-147`, `cwd_sync.rs:173-182`, `session.rs:387-398`.
- Szenario: In einer lokalen bash mit Integration läuft `ssh winhost` (Gegenstelle `cmd.exe`) oder `python`/`mysql`. Panel wechselt den Ordner ⇒ `cd` wartet. Beliebige Ausgabe mit `ESC]133;A` (Remote-Server, `cat datei`) löst das Tippen von ` cd -- '<pfad>'\r` in das laufende Programm aus; auf cmd führt ein Ordnername wie `a&calc&` Befehle aus.
- Fix: Integrations-Snippets mit Session-Nonce (`133;A;rc=<nonce>`, analog VS Code OSC 633); Marken ohne Nonce ignorieren – ebenso für OSC 7 und 9;9.

**M5 – [mittel] SSH-Start-`cd` nimmt POSIX-Shell an und lässt `--` weg**
- Ort: `crates/rc-engine/src/commands/ssh_terminal.rs:155-170` (`format!("cd {}\r", shell_quote(folder))`), Aufrufe Z. 56/98.
- Szenario: Windows-OpenSSH-Server mit Standard-Shell `cmd.exe`, Ordner `/C:/Users/u/x&calc&` (z. B. aus importierter Verbindungsdatei als `start_directory`) ⇒ `calc` läuft beim Öffnen des Terminals. Ordner `-P`/`-` werden als Option/`OLDPWD` gelesen.
- Fix: `cd -- '…'`; nur senden, wenn Shell als POSIX erkannt; sonst `&|<>^%!` ablehnen; oder Start per exec-Kanal `cd … && exec $SHELL -l`.

**M13 – [mittel, nur Unix] Terminal endet nicht, wenn Programm SIGHUP ignoriert**
- Ort: `crates/rc-term/src/local_pty.rs:166-178, 240-245, 282-290`; verifiziert in `portable-pty 0.9 lib.rs:325-333` (nur `SIGHUP`, keine Eskalation; Master bleibt über Waiter/Reader offen).
- Folge: Prozess plus Reader-/Waiter-Thread leben weiter (Leck); `close_terminal` gibt nach 3 s auf.
- Fix: Master in `shutdown` explizit droppen; nach Gnadenfrist `SIGKILL` an die Prozessgruppe.

**Niedrig (Terminal):**
- OSC 52 schreibt standardmäßig ohne Rückfrage und ohne Größenlimit in die Zwischenablage (`crates/rc-term-widget/src/terminal.rs:114,276-278`) – auch aus Hintergrund-/Remote-Terminals. Fix: Default aus oder nur fokussiertes Terminal, Limit, Statushinweis.
- Unbegrenzte OSC-Puffer in `vte` (Feature `std`), unbegrenzte Titel (`session.rs:380-383`) und unbegrenzte Eingabekanäle (`local_pty.rs:160`, `backend/ssh.rs:83`) ⇒ Speicher-DoS. Fix: Überlange OSC im eigenen Scanner verwerfen, Titel kürzen und Bidi-Zeichen entfernen, gebundene Kanäle.
- Panel-Folgen (OSC 7 / 9;9) aus Ausgabe lokaler Shells fälschbar (`osc.rs:150-169`, `terminal_sync.rs:71-91`). Fix: Nonce wie M4.
- Fallback auf bare Programmnamen `wsl.exe`/`powershell.exe`/`cmd.exe` (`wsl.rs:81-89`, `shells.rs:160-166`); `wsl_program` ohne `is_absolute()`-Filter auf `SystemRoot`; `rc-vfs/src/wsl/windows.rs:51` `Command::new("wsl.exe")`. Fix: absolute System32-Pfade, sonst Fehler.
- WSL-Rückübersetzung prüft erzeugten UNC-Pfad nicht erneut auf `..` (`wsl_path.rs:166-185`) **[Ausbruch unverifiziert]**.
- WSL-Profile gelten stets als POSIX (`quote.rs:28`); tcsh expandiert `!` in Single-Quotes.
- Login-Skript-Tap verwirft bei >256 Chunks still (`tap.rs:66-70`) – als Skriptfehler behandeln.

Positiv: Keine Shell-Strings beim Prozessstart; Distro-Namen nur aus installierter Liste; lokales `cd_line` korrekt (POSIX mit `--`, PowerShell `-LiteralPath`, cmd lehnt `" % !` ab); PowerShell-Integration via `-EncodedCommand`; Windows-Job-Objekt mit `KILL_ON_JOB_CLOSE`; keine Secrets in Kind-Umgebungen; OSC 8 entfernt; Bracketed Paste filtert ESC; `ssh_exec` mit Zeit-/Größenlimits.

### 2.4 SSH / SFTP / FTP (rc-remote, rc-vfs)

**M6 – [mittel] Spezialbits vom Server werden auf lokale Dateien übertragen**
- Ort: `crates/rc-remote/src/sftp/listing.rs:66` (`& 0o7777`), `crates/rc-remote/src/ftp/listing.rs:89` und `:238-239`; übernommen in `crates/rc-ops/src/staging.rs:201-205`; gesetzt in `crates/rc-vfs/src/local/unix.rs:95` (`& 0o7777`); `keep_attributes` Default `true` (`crates/rc-ops/src/job.rs:204`).
- Szenario: Bösartiger Server meldet `04755`/`0777` ⇒ setuid-Binary bzw. world-writable Datei des Opfers. (OpenSSH `sftp get -p` maskiert auf `0777`.)
- Fix: Bei Remote-Quelle `& 0o777 & !umask`; Spezialbits nur per expliziter Option.

**M14 – [mittel, Windows] Lokales Listing folgt Symlinks auf UNC-Ziele (NTLM-Leak)**
- Ort: `crates/rc-vfs/src/local/describe.rs:86-89` (`read_link` + `fs::metadata(real)` für jeden Eintrag).
- Szenario: Ordner enthält `x -> \\attacker\share\y` ⇒ bloßes Anzeigen löst SMB-Auth aus. Widerspricht dem eigenen Modell RC-080-10 (`network_path` wird sonst überall geprüft). Erzeugung des Links braucht Dev-Mode/Admin oder Fremdtool.
- Fix: Link-Ziel mit `rc_vfs::network_path()` prüfen; bei Netzwerkziel nicht folgen.

**Niedrig (Remote):**
- `@revoked` aus `known_hosts` wirkt nur beim Import, wird nicht persistiert (`known_hosts_import.rs:84-97,117-125`; `hostkey.rs:250-266`).
- Anderer Key-Typ ⇒ `Unknown` statt `Changed` (`hostkey.rs:250-257`) – TOFU-Dialog statt hartem Stopp.
- FTPS `RejectionSlot` wird von Datenverbindungen gesetzt, aber nur bei Control-Handshakes geleert (`ftp/tls.rs:212,306-316`, `connection.rs:411-414`) ⇒ falsch zugeordnete Zertifikatsfrage (fail-closed bleibt).
- SFTP-Upload legt Datei ohne Rechte an (`sftp/change.rs:27-34`) ⇒ `0600`-Datei während des Transfers mit Server-umask lesbar.
- Klartext-FTP-Warnung erst nach gesendetem Passwort (`rc-engine/src/remote.rs:177`).
- FTP-Keepalive `NOOP` seriell mit vollem Timeout blockiert Pool (`ftp/pool.rs:234-256`).

Positiv: Host-Key-Entscheidung außerhalb des russh-Callbacks, ungeprüfte Sessions abgelehnt (`session.rs:314-323`); geänderter Key = harter Stopp; gehashte `known_hosts` mit konstantzeitigem HMAC; Zertifikate/`@cert-authority` übersprungen; keinerlei Forwarding, alle Server-Kanäle abgelehnt; FTPS ohne „unsicher akzeptieren“, kein Downgrade, Pinning auch bei CA-gültigem Wechsel; **PASV-IP wird ignoriert**; CR/LF/NUL in FTP-Pfaden abgelehnt; `safe_name` gegen Traversal in Listings; Listing-Limits; keine Panics auf Serverdaten.

### 2.5 Vault und Secrets (rc-vault, rc-engine, rc-db)

**M15 – [mittel] `config.import` warnt nicht bei Endpunktwechsel, wenn Secrets über Credential/Secret-Source/Login-Skript hängen**
- Ort: `crates/rc-engine/src/commands/exchange_connections.rs:123-128` prüft nur `connection.secrets` (eigene `*_enc`-Spalten); die vollständige Prüfung `holds_secrets` (`crates/rc-db/src/connections/endpoint.rs:63`) wird nicht genutzt.
- Szenario: Präparierte Nachfolge-Exportdatei mit gleicher Markierung setzt `host` auf Angreifer-Server; Zeile ohne Warnung vorausgewählt; beim nächsten Verbinden geht das Passwort aus Credential Manager/geteiltem Credential an den Angreifer.
- Fix: `holds_secrets` verwenden oder Update mit `EndpointRule::KeepWhileSecretsHeld`.

**Niedrig (Vault):**
- Export-Passwort ohne Mindestlänge (`exchange_export.rs:59-65`), Master-Passwort hat 8 (`secrets.rs:46`).
- Fremde Exporte dürfen KDF bis 1 GiB / t=16 / p=16 verlangen (`rc-vault/src/kdf.rs:46-50`) ⇒ Ressourcen-DoS beim Import.
- Debug-Builds akzeptieren beim **Lesen** KDF bis 8 KiB/t=1 (`kdf.rs:64-74`) – nur auf `#[cfg(test)]` bzw. `new_key` beschränken.
- Klartext-JSON beim Export ohne Zeroize der Zwischenpuffer (`exchange_export.rs:124,143`).
- Passwortwechsel lässt alte Backups mit altem Passwort lesbar (`rc-db/src/vault.rs:181-243`) – UI-Hinweis.
- DB-Datei ohne explizites `0600` (Verzeichnis ist 0700, `private_dir.rs:11-24`).

Positiv: XChaCha20-Poly1305 mit Zufalls-Nonce (192 Bit) pro Schreibvorgang; AD `table/column/row_id` verhindert Vertauschen; argon2id 64 MiB/t=3, Parametergrenzen beim Lesen; `VaultKey` nicht `Clone`, geschwärztes `Debug`, Zeroize; Verifier per AEAD-Tag (konstante Zeit); Passwortwechsel atomar in `BEGIN IMMEDIATE` mit `secure_delete` und WAL-Truncate; Vault-Befehle `GuiOnly`; Export atomar 0600; TC-Passwortimport in `Zeroizing`; wincred-Blob vor `CredFree` überschrieben; Test auf Vollständigkeit von `SECRET_COLUMNS`.

### 2.6 Update, Plugins, Datenbank (rc-update, rc-plugin-host, rc-db)

**M8 – [mittel] Entwicklungs-Plugin-Schlüssel ist Root in Release-Builds**
- Ort: `crates/rc-plugin-host/src/sign/roots.rs:45-49` (`rcommander-dev-v1`, `not_after: None`); automatisch installiert über `installed/bundled.rs:53-55`.
- Szenario: Kompromittierter Entwicklerrechner ⇒ Angreifer signiert Plugins, die jede Installation ohne Rückfrage vertraut. Wirkung auf WASM-Sandbox begrenzt (Secret-Sources mit `net_http` erreichen Loopback).
- Fix: Dev-Key nur unter `cfg(debug_assertions)`/Feature einkompilieren oder sofort `not_after` setzen.

**M16 – [mittel] Kein Versions-Check beim Installieren eines gespeicherten Update-Angebots**
- Ort: `crates/rc-engine/src/updates/handover.rs:56-70, 158-185` – nur die Anzeige filtert (`updates.rs:123`).
- Szenario: Gespeichertes Angebot v1.2, Nutzer ersetzt Portable-Version manuell durch v1.3 mit derselben DB ⇒ `update.install` (oder Auto `on_quit`) rollt auf signiertes, älteres v1.2 zurück inkl. DB-Migrationsrisiko.
- Fix: `rc_update::is_newer(&offer.version, RUNNING)` in beiden Funktionen erzwingen; `Plan::validate` prüft `from < target`.

**Niedrig:**
- `fetch.rs:148` `builder.build().unwrap_or_default()` ⇒ Fallback-Client ohne `https_only`, Redirect-Policy, Timeouts (Integrität bleibt durch Signatur). Fehler weiterreichen.
- Plugin-Key wird vor der Komponentenprüfung dauerhaft vertraut (`plugin_admin/commands.rs:133-146`).
- Restore übernimmt fremde Trigger/Views (`rc-db/src/restore.rs:355-421`); `tokio::fs::copy` übernimmt Quellrechte (`:109`).
- TOCTOU Hash-Prüfung vs. `rename` in `apply_pending_restore` (`restore.rs:164,202-217`).
- DB-Rollback während laufendem `rcommander mcp` (kein Instance-Lock, `rc-update/src/install/locks.rs:6-8`) ⇒ verlorene Schreibvorgänge (Unix) bzw. `rollback_failed` (Windows).
- Zeitgutschrift für Host-HTTP-Aufrufe unbegrenzt (`rc-plugin-host/src/runtime.rs:80-84`, `http.rs:211-214`).
- `Plan.executable` nicht validiert (`rc-update/src/install/journal.rs:62-93`).
- `plugins/state.json` (`development_mode`, `trusted_keys`) ungeschützt im Datenordner – akzeptabel, solange MCP-Schreibschutz `<data>/plugins/**` abdeckt **[nicht geprüft]**.
- SVG-Viewer: Tiefe nur auf Rohbytes geprüft, `allow_dtd: true` (`plugins/svg-viewer/src/render.rs:94`, `budget.rs:31`) **[unverifiziert]**, höchstens Sandbox-Trap.

Positiv: Update-Manifest Ed25519 über rohe Bytes mit Domain-Separation, Widerrufsliste, Replay-Floor, `not_after` ≤ 366 Tage, signierter Kanal, Artefakt-URL-Bindung, `https_only`, Größen- und SHA-256-Abgleich, Entpacken vom gehashten Handle, journalisierter Switch mit Rollback. WASM: kein WASI, Imports doppelt erzwungen, Fuel/Epoch/Speicher-Limits, HTTP nur Loopback. Plugin-Pakete: Member-Allowlist, Key-Pinning pro ID. rc-db: durchgehend gebundene Parameter, `migrations.sha384` stimmt mit allen 22 Migrationen überein.

---

## 3. Korrektheit / Bugs

**M7 – [mittel] Rekursives „Attribute ändern“ folgt Symlinks (Unix)**
- Ort: `crates/rc-ops/src/attributes.rs:96-104` (auch Symlinks bekommen `set_properties`), `crates/rc-vfs/src/local/unix.rs:94` (`fs::metadata`), `:101` (`set_permissions`), `:143` (`filetime::set_file_mtime`) – alle folgen dem Link.
- Szenario: Im Baum liegt `key -> ~/.ssh/id_rsa`; rekursives `chmod 0644` macht den Schlüssel lesbar. Modulkommentar „Links are changed as such“ trifft unter Unix nicht zu; kein Test mit Links.
- Fix: Symlinks bei Modus/readonly überspringen, mtime via `filetime::set_symlink_file_times`, oder `O_NOFOLLOW` + `fchmod`.

**M17 – [mittel] Kein `fsync` vor Rename bzw. vor Löschen der Quelle (Move)**
- Ort: `crates/rc-ops/src/stream_pair.rs:242-270`, `staging.rs:167`, `copy.rs:367-369` (nur `pack/write.rs:257` synct).
- Szenario: Move auf USB-Stick, Quelle gelöscht, Daten im Page-Cache ⇒ Stromausfall/Abziehen = Datenverlust; `verify` liest aus dem Cache.
- Fix: `sync_all` vor `settle`, bei Move zusätzlich vor `remove_source`; optional Verzeichnis-fsync.

**M18 – [mittel] Move/Copy eines Symlinks auf ein Verzeichnis schlägt immer fehl**
- Ort: `crates/rc-ops/src/copy.rs:74-83, 228-239` – `Symlink{target_kind: Dir}` ⇒ `NotCopyable`, auch beim Rename innerhalb eines Dateisystems.
- Fix: Bei `rename_ok` Link umbenennen, sonst per `create_link` neu anlegen.

**M19 – [mittel] Multi-Rename ohne automatisches Rollback; geparkte Dateien bleiben versteckt**
- Ort: `crates/rc-ops/src/rename.rs:86-90`, `crates/rc-engine/src/commands/multi_rename.rs:214-250`.
- Bei Fehler mitten in Swap/Ring bleibt eine Datei als `.rc-rename-<tag>-n` (unter Unix unsichtbar). Undo existiert, muss aber manuell ausgelöst werden.
- Fix: Undo automatisch einreihen oder geparkte Namen im Fehler melden.

**M20 – [mittel] Kein Panic-Hook; Panics verschwinden unter Windows spurlos**
- Ort: `crates/rcommander/src/main.rs:8` (`windows_subsystem = "windows"`); workspaceweit kein `std::panic::set_hook` (per grep verifiziert).
- Folge: Abgestürzte Hintergrund-Tasks bleiben unbemerkt, Fehlerlog (`rc-db/src/error_log`) erfährt nichts.
- Fix: Früh in `main` Hook setzen, der via `tracing::error!` ins Fehlerlog schreibt; `JoinError::is_panic` loggen.

**Niedrig:**
- Poison-Recovery an 79 Stellen (`lock().unwrap_or_else(PoisonError::into_inner)`, z. B. `rc-engine/src/lib.rs:474-478`) verdeckt halbe Zustände – mindestens `warn!` loggen.
- Globaler `FTP_PAIR`-Mutex wird während Pause gehalten (`stream_pair.rs:44,287,332`) ⇒ pausierter Job blockiert alle FTP-internen Kopien auf allen Servern.
- Device-Timeout bricht `spawn_blocking` nicht ab (`rc-vfs/src/device/fs.rs:88-89`) ⇒ Löschen läuft nach `TimedOut` weiter, Retry parallel.
- TOCTOU: `stat` dann überschreibendes `rename` statt `rename_no_replace` (`rc-ops/src/rename.rs:50-64`); Pack ohne Überschreiben (`pack/mod.rs:312,361`); Delete/Copy nach Planung über volle Pfade (`delete.rs:73-100`, `copy.rs:419`); Resume hängt durch Symlink-`.rcpart` an (`resume.rs:222`).
- Namensschlüssel ohne Unicode-Normalisierung (`rc-rename/src/names.rs:59-65`) ⇒ Abbruch statt Kollisionserkennung auf macOS/NTFS.
- TC-Import: `%` im Pfad als `UnknownVariable`, `\\.\…` als Hotlist-Ziel, Menüschleife bricht bei Nummernlücke ab (`rc-engine/src/import/total_commander.rs:179-185,236,254-256`).
- `InvokeCommand` des Shell-Kontextmenüs synchron im UI-Thread ohne `CMIC_MASK_ASYNCOK` (`rcommander/src/platform/windows/context_menu.rs:225-248`) **[Wirkung unverifiziert]**.

---

## 4. Architektur & Wartbarkeit

- **Stark:** klare Crate-Grenzen (vfs/ops/remote/vault/db/mcp/engine/gui), stabile Fehlercodes (`error/codes.rs`), Dispatch nach Domänen (`dispatch/arms/*`, `registry/*`), 500-Zeilen-Limit pro Datei per Lint, Begründungspflicht für `allow`, `unwrap_used = warn` mit 0 Treffern in Produktivcode der beiden großen Crates.
- **[niedrig] Gott-Objekte:** `App` mit 52 Feldern (`crates/rcommander/src/app.rs:113ff`), `Engine::Inner` ~27 Felder (`rc-engine/src/lib.rs:238-290`), `EngineError` flach mit 107 Varianten. Das Dateilimit verschiebt Komplexität in Kopplung über `&mut self`. Empfehlung: Dialogzustände als Sub-Structs mit eigenem `update → Effects`; `EngineError` nach Domäne schachteln (Code bleibt stabil).
- **[niedrig] Monolithischer „Kern“:** `rc-engine` hängt direkt an wasmtime (`rc-plugin-host`), `rc-update`, `rc-vault`, `rc-remote` ⇒ lange Inkrementalbuilds. Feature-Gates/Trait-Grenzen erwägen.
- Lange Funktionen: `themes_page/actions.rs:130` (194 Zeilen), `app.rs:298` `apply` (188), `startup.rs:116` `prepare` (172).
- `unsafe`: nur in Windows-FFI-, PTY-, Device- und einem Plugin-Guest-Modul, jeweils Modul-`allow` mit `reason`; `SAFETY`-Kommentare vollständig (Stichprobe `platform/windows/*`). Restrisiko: undokumentierte `UAHMENU`-Strukturen (`menu_bar.rs:83-95`).

## 5. Performance

- **[niedrig]** Listing: jeder Flush klont und sortiert die gesamte Liste auf einem tokio-Worker (`rc-engine/src/commands/panel_listing.rs:108,112,247-248`) ⇒ bei >500k Einträgen Hunderte ms Blockade. Fix: `spawn_blocking` ab Schwelle oder Merge des neuen Batches.
- **[niedrig]** `archive/pipe.rs:227` alloziert pro 64-KiB-Chunk neu; `rc-engine/src/commands/archive.rs:179` `tokio::io::copy` mit 8-KiB-Puffer über tokio-fs.
- **[niedrig]** Quadratische Suchen in `rc-ops/src/delete.rs:77` und `pack/members.rs:110`; Papierkorb-Delete traversiert vorab den ganzen Baum (`delete.rs:39`).
- **[niedrig]** Unbounded `Job`-Kanal (`rcommander/src/app.rs:214`). Übrige Kanäle sind begrenzt, Lag führt zu `reload()`.
- Caches (Icons, Viewer, Fingerprints, Historien) sind begrenzt; `PanelState.entries` als `Arc<[Entry]>`.

## 6. Abhängigkeiten

Ergebnis `cargo audit` (0.22.2) und `cargo deny check advisories` gegen die aktuelle DB:

| Advisory | Paket | Bewertung |
|---|---|---|
| RUSTSEC-2023-0071 (Marvin) | `rsa 0.10.0-rc.18` über russh | in `deny.toml` begründet ignoriert; nur lokales Signieren – vertretbar |
| RUSTSEC-2026-0253 (unsound, UAF in `LruCache::pop`) | `lru 0.16.4` über `cryoglyph ← iced_wgpu` | **von `cargo audit` gemeldet, nicht in `deny.toml`**; `cargo deny` schlägt nicht an. Fix erst ab `lru >= 0.18.2`, hängt an iced. **[niedrig]** – Ausnutzbarkeit im Glyph-Cache gering, aber dokumentieren/beobachten |
| yanked | `async-executor 1.14.0` | **[niedrig]** `cargo update -p async-executor` |
| unmaintained | `paste`, `ttf-parser`, `zbus_names`, `zvariant`, `zvariant_derive` | begründet ignoriert, transitive über iced |

**[niedrig] Veraltete Ignore-Einträge in `deny.toml`:** `RUSTSEC-2026-0367/0368/0369` (zip – der Lockfile hat bereits `zip 9.2.2`, der Kommentar sagt „Leaves with the bump to zip 9“) und `RUSTSEC-2026-0391/0398/0403` (zstd, Lockfile `zstd 0.13.4`/`zstd-safe 7.4.0`) werden von `cargo deny` als `advisory-not-detected` gemeldet. Entfernen, damit Ignores nicht künftige Treffer verdecken.

`cargo deny` meldet ~40 doppelte Crate-Versionen (u. a. `hashbrown` ×4, `syn`, `thiserror`, `nix`, `rustix`, `sha2`) – Build-Zeit/Binary-Größe, kein Sicherheitsproblem. Quellen nur crates.io, keine Git-Abhängigkeiten, keine Wildcards.

## 7. Testabdeckung

- 5 377 Tests (`#[test]`/`#[tokio::test]`), 960 Testdateien; Verhältnis Test-/Produktivcode: `rc-engine` 0,76, `rcommander` 0,52. Nextest mit Timeout und Flaky-Retry, JUnit-Report.
- **Lücken:**
  - Keine Coverage-Messung in CI (`llvm-tools` ist installiert, aber kein `cargo llvm-cov`). **[niedrig]**
  - Windows-Plattformcode (~1,9k Zeilen `platform/windows/`, sämtlicher `unsafe`-Code) ohne Unit-Tests; kein Miri. **[niedrig]**
  - Keine Tests für Symlinks in rekursivem Attribut-Ändern (M7), Sparse-Tar (H1), Spezialbits aus Remote-Listings (M6).
  - Canary-Sweep deckt exec/destructive-Pfade und HTTP-Transport nicht ab (M12).
  - Kein Fuzzing der Parser (FTP-LIST/MLSD, `known_hosts`, Tar/Zip-Metadaten, TC-INI, OSC-Scanner). Empfehlung: `cargo-fuzz`-Ziele für diese Parser. **[niedrig]**

## 8. Build / CI

- **Stark:** `permissions: {}` als Default, Actions auf Commit-SHA gepinnt, benannte Runner-Images, `cargo deny` online, gitleaks und shellcheck, `--locked` überall, Clippy `-D warnings` auf allen Plattformen, Release-Artefakte mit SHA256SUMS + Sigstore, Packager aus separatem Artefakt (`release.yml`), Concurrency-Gruppen.
- **M9 – [mittel] Plugin-Signierschlüssel während des gesamten Builds in der Umgebung**
  - Ort: `.github/workflows/release.yml:63-72` – `RCOMMANDER_PLUGIN_SIGNING_KEY` steht als Step-`env` und als Datei (`RC_PLUGIN_KEY`) bereit, während `scripts/build-plugins.sh:115,137` `cargo build -p rcommander` und `cargo rustc` ausführt.
  - Szenario: Ein kompromittiertes Crate (Build-Skript oder Proc-Macro einer der Hunderte transitiven Abhängigkeiten) liest `env::var("RCOMMANDER_PLUGIN_SIGNING_KEY")` oder die Datei und exfiltriert den Release-Signierschlüssel ⇒ Angreifer kann Plugins signieren, denen jede Installation als Root vertraut.
  - Fix: Bauen und Signieren in getrennte Steps (bzw. Jobs) aufteilen; der Signier-Step bekommt nur die fertigen `.wasm`-Dateien und den Schlüssel, ohne Compiler-Aufruf.
- **[niedrig]** Gleiches Muster prüfen für `RCOMMANDER_UPDATE_SIGNING_KEY` (`release.yml:244`): hier läuft nur das vorgebaute `rc-pack` – unkritisch, aber Schlüssel nur an diesen einen Aufruf binden.
- **[niedrig]** Keine Coverage, kein Fuzzing, kein Miri in CI (siehe §7).

---

## 9. Priorisierte Empfehlungen

1. **H1** Sparse-Tar im Bomben-Schutz berücksichtigen + globales Ratio-Limit (M10).
2. **H2/M2/M3** MCP: Allowlist-Modus bzw. `ask` außerhalb freigegebener Wurzeln für read und write; Denylists erweitern; `token` in Maske; `files_set_attributes` ohne Spezialbits/World-Write.
3. **M1/M11** Exec-Freigaben an das tatsächliche Ziel binden; `workspaces.on_start` schützen.
4. **M9** Signierschlüssel aus dem Build-Step entfernen.
5. **M4/M5** OSC-Nonce für Shell-Integration; SSH-`cd` mit `--` und Shell-Erkennung.
6. **M6/M7** Spezialbits von Remote-Quellen maskieren; Symlinks beim rekursiven Attribut-Ändern nicht folgen.
7. **M8/M16** Dev-Plugin-Key aus Release-Builds; Versions-Check vor Update-Installation.
8. **M17/M20** fsync vor Rename/Quell-Löschen; Panic-Hook ins Fehlerlog.
9. `deny.toml` aufräumen, `async-executor` aktualisieren, `lru`-Advisory dokumentieren.
10. Fuzzing für Parser, Coverage-Messung in CI.
