"""The bulk smoke list of scripts/lib/testlist.py (`--smoke`): the open acceptance boxes grouped
by area, a few steps per area and one `Smoke passed` box that stands for every box of its area.

An area is found by the job's heading: the first area of AREAS one of whose words (regular
expressions) begins a word of it, in lower case. A job no area claims is under "Other", which has
no smoke box - its boxes are ticked one by one. --apply ticks every keyed box of an area whose
smoke box is ticked; a box ticked on its own is ticked as in any list.
"""

import re

# (id, title, words of a job heading, the smoke steps). The order decides: the first match wins.
AREAS = (
    ("mcp", "MCP", ("mcp",), (
        "Connect an MCP client (`rcommander mcp`), list the tools and read a folder.",
        "Run a writing tool and a command that starts a process: the window asks, shows exactly "
        "what runs, and Deny and Allow do what they say.",
    )),
    ("update", "Update, packages, release and portable mode",
     ("update", "release", "winget", "portab", "paket", "homebrew", "apt-"), (
        "Check for updates in Settings and install one; the program restarts on the new version.",
        "Start the portable package: its data stays beside the program.",
    )),
    ("terminal", "Terminal", ("terminal", "wsl", "alacritty", r"osc\b"), (
        "Open a terminal in a panel (cmd, PowerShell and a WSL distribution): type, scroll, copy "
        "and paste, run a full-screen program.",
        "Switch the panel between file list and terminal and use the full-width terminal; the "
        "folder follows `cd` both ways.",
        "Quit rCommander with a program running in the terminal: no process is left.",
    )),
    ("connections", "Connection manager and vault",
     ("verbindungsmanager", "verbindungsdialog", "vault", "tresor", "master-passwort", "secret",
      "zugangsdaten", "login-skript", "login-script", "import aus", "geheimnis",
      "anmeldeinformation"), (
        "Open the connection manager: add, edit, duplicate and delete a connection with the "
        "keyboard alone; import from ~/.ssh/config, Total Commander and Remote Desktop Manager.",
        "Set, change and lock the master password; a stored secret is asked for only after "
        "unlocking.",
        "Edit a login script and run it on a connection.",
    )),
    ("remote", "SSH, SFTP and FTP",
     ("ssh", "sftp", "ftp", "remote", "russh", "host-key", "totp", "einmalcode", "pageant",
      "übertragung", "server", "zertifikaten"), (
        "Connect to an SSH server with a key, with a password and through the agent; trust a new "
        "host key once.",
        "Browse it over SFTP; copy local to remote, remote to local and remote to remote; cancel "
        "one copy and resume it.",
        "Do the same over FTP and FTPS (accept the certificate once).",
    )),
    ("archives", "Archives",
     ("archiv", "entpack", "packen", r"zip\b", r"7z\b", "sevenz", r"tar\b", r"rar\b"), (
        "Enter a zip, 7z and tar archive like a folder; view and copy a file out of it.",
        "Pack a selection (Alt+F5), add to and delete from an archive, unpack it (Alt+F9) and "
        "test it.",
    )),
    ("rename", "Multi-rename", ("umbenenn", "rename"), (
        "Open Multi-rename (Ctrl+M) on a selection: masks, counters, search and replace, a "
        "preset; the preview matches the result.",
        "Rename, then undo; a rename that fails leaves nothing half done.",
    )),
    ("search", "Search and compare",
     ("suche", "dateisuche", "vergleich", "compare", "duplikat", "everything"), (
        "Search files (Alt+F7) by name and content, inside archives, and with Everything on "
        "Windows; feed the result to a panel.",
        "Compare two files and find duplicates.",
    )),
    ("viewer", "Lister, Quick View and the viewer plugins",
     ("viewer", "lister", "quick view", "schnellansicht", "markdown", "vorschau", ".nfo",
      "svg", "farbig"), (
        "Open text, Markdown, image, JSON, XML, YAML, CSV, SVG, source code and a certificate in "
        "the Lister (F3) and in Quick View (Ctrl+Q): each shows its viewer.",
        "Search, select and copy in the Lister; change the font size with Ctrl+mouse wheel.",
    )),
    ("plugins", "Plugins", ("plugin", "wasmtime", "wit-"), (
        "Open Settings, Plugins: the bundled plugins are listed and enabled, and a change of "
        "their settings takes effect.",
    )),
    ("settings", "Settings, themes and keys",
     ("einstellung", "theme", "schrift", "tastenkürzel", "export und import", "farb", "about",
      "datenbank"), (
        "Change theme, fonts and a shortcut in Settings; restart: all three are kept.",
        "Export the settings, change one, import them back.",
    )),
    ("bars", "Menus, button bar and drive bar",
     ("buttonleiste", "menüleiste", "menü", "laufwerks", "befehl", "symbol", "icons",
      "kontextmenü"), (
        "Use the menu bar, the context menu (right click) and the command palette.",
        "Add, edit and drag a button of the button bar; run an own command, once as "
        "administrator.",
        "Use the drive bar: drives, network and devices, the `»` list, open connections.",
    )),
    ("fileops", "File operations and the queue",
     ("dateioperation", "kopier", "verschieben", "lösch", "warteschlange", r"f5\b", "drag", "drop",
      "symlink", "überschreib", "eigenschaft", "checksum", "prüfsumme", "kommentar", "dateizeit",
      "batch"), (
        "Copy (F5), move (F6), rename, delete (F8, Shift+F8) and make a folder (F7), between "
        "panels and by drag and drop with Explorer.",
        "Overwrite, skip and rename on a conflict; queue several jobs (F2) and watch the queue "
        "window.",
        "Show properties, checksums and split and combine a file.",
    )),
    ("windows", "Windows, dialogs and the keyboard",
     ("dialog", "fenster", "tastatur", "fokus", "beendet", "f10", "taskleiste"), (
        "Open the large dialogs: each is a window of its own that moves, closes with the X and "
        "Esc and gives the focus back.",
        "Walk every dialog with Tab alone; every action has a key.",
    )),
    ("panels", "Panels and navigation",
     ("dateiliste", "panel", r"tabs?\b", "spalte", "scroll", "auswahl", "dateiauswahl", "ordner",
      "baum", "workspace", r"git\b", "statuszeile", "netzlaufwerk", "gerät", "mtp", "samba",
      "miniatur", "laufwerk", "navigation", "icon"), (
        "Browse both panels: views, sorting, columns, thumbnails, the tree, tabs and "
        "workspaces; select with the keyboard and the mouse.",
        "Connect and disconnect a network drive and a phone; the panels reread on change.",
    )),
)
OTHER = ("other", "Other", (), ())


def area_of(heading):
    lower = heading.lower()
    return next((a for a in AREAS if any(re.search(r"\b" + w, lower) for w in a[2])), OTHER)


def render_smoke(entries, head):
    """The smoke list's text from (job, heading, mark, status, path, boxes) entries."""
    grouped = {}
    for entry in entries:
        grouped.setdefault(area_of(entry[1]), []).append(entry)
    sections = []
    for area in AREAS + (OTHER,):
        found = grouped.get(area)
        if not found:
            continue
        count = sum(len(e[5]) for e in found)
        lines = [f"## {area[1]}", "", f"{count} box(es) of {len(found)} job(s)."]
        if area[3]:
            lines += ["", "Steps:", ""] + [f"{n}. {s}" for n, s in enumerate(area[3], 1)]
            lines += ["", f"- [ ] Smoke passed: every step above works - ticks the {count} "
                          f"box(es) below <!-- smoke {area[0]} -->"]
        else:
            lines += ["", "No smoke steps: tick each box on its own."]
        for job, heading, mark, status, path, wanted in found:
            lines += ["", f"### {heading}", "", f"`{path}`{mark} - {status}", ""]
            lines += [f"- [ ] {b.text} <!-- box {k} -->" for k, b in wanted]
        sections.append("\n".join(lines))
    return "\n\n".join([head] + sections) + "\n"


SMOKE = re.compile(r"^\s*[-*] \[([ xX])\] .*<!-- smoke ([a-z]+) -->\s*$")
