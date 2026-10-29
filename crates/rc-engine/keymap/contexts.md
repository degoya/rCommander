<!--
The keys inside dialogs, the viewer, the command line and the terminal: what their widgets
handle themselves, outside the keymap. scripts/keyboard-reference.sh appends this file to the
generated wiki page reference/keyboard-shortcuts.md; keep it in step with the code of the
dialog a change touches, then run the script.
-->
## Keys inside the window's parts

These keys belong to a part of the window while it has the keyboard; they are not in the keymap.
Total Commander's keys where it has them; *rC* marks rCommander's own.

### Command palette (Ctrl+Shift+P)

| Key | Action |
| --- | --- |
| Letters | Find an action by its name or its key (`ctrl+f3`); every word must match |
| Up / Down, PgUp / PgDn | Choose a row |
| Enter, click | Run it, as its key would |
| Esc | Close the palette |

### Quick search and command line

| Key | Action |
| --- | --- |
| Ctrl+Alt+letters | Quick search: jump to the first name that starts with them (Settings: Alt with letters, or letters alone; match anywhere; `*` and `?` work) |
| Down / Up (during a quick search) | Next / previous match |
| Letters | Type into the command line under the panels |
| Enter / Shift+Enter | Run the command line in the active panel's folder / and keep the console open |
| Up / Down (in the command line) | The previous command lines |
| Esc | Empty the command line, or lift the panel's filter |

### Prompts and popups

| Key | Action |
| --- | --- |
| Up / Down (in a mask prompt, Num+ and Num-) | The last 20 masks |
| Enter / Esc | Run the prompt / drop it |
| Arrows, Home, End, Enter or Space, Esc | The folder history (Alt+Down), the hotlist (Ctrl+D) and the drive menu (Alt+F1, Alt+F2; also a drive letter) |
| Esc (while folder sizes are counted) | Stop counting |
| Tab / Shift+Tab (in a dialog) | Next / previous text field of any dialog, such as the master password and its repeat |
| Enter (in the settings) | OK |
| F10 (in any dialog) | *rC* What Esc does there; without a dialog F10 quits (setting `keys.f10`) |
| Alt+underlined letter (in a dialog) | The button or checkbox with that letter; the text field, pick list or option group after the caption with it gets the keyboard; the letter alone while a button or checkbox has the focus ring (underlined while Alt is held, or always: `keys.underline_access_keys`) |

### Menus and lists

The context menus, the drive menu and the lists of the tabs work as Windows menus do.

| Key | Action |
| --- | --- |
| Shift+F10, Menu key | The context menu: of the selection or the file under the cursor in the file list; of the drive under the cursor in the drive menu (Alt+F1, Alt+F2); of the tab under the cursor in the list of tabs |
| Up / Down, Home / End, Enter or Space, Esc | Choose an entry, take it, close the menu |
| A letter | Take the one entry starting with it; with several, go to the next of them |
| Ctrl+Shift+A | The list of the panel's tabs, all of them, also those behind `»` (TC); while the panel shows its terminal, the list of the terminal's sessions, from inside the terminal too (*rC*) |
| Del (in the list of tabs or of sessions) | Close the tab or the session under the cursor |
| Ctrl+Up, Ctrl+Down (in the list of tabs) | Move the tab under the cursor one place left or right in its row (*rC*) |

The drive menu lists every drive, also those behind the drive bar's `»`.

### Lister (F3) and Quick View (Ctrl+Q)

| Key | Action |
| --- | --- |
| 1 / 2 / 3 / 4 | Text / binary / hex / image |
| 5 | A Markdown file: preview or text (TC's 5 shows HTML formatted) |
| 7 | A viewer plugin's document: its next view - coloured text, tree, ... (*rC*) |
| A / 6 / 8 | Windows-1252 / UTF-16 LE / UTF-8 |
| E | Next encoding (*rC*) |
| W | Wrap lines on or off |
| T | Tab width 8, 4 or 2 (*rC*) |
| N / P | Next / previous file of the panel |
| F2 | Read the file again |
| F7, Ctrl+F | Search text (with or without case) or hex bytes |
| F3 / Shift+F3 | Find next / previous |
| Ctrl+G | Go to an offset (*rC*) |
| Ctrl+C | Copy the selected text |
| Num `+` / Num `-` (also `+`, `=` / `-`) | Zoom an image in / out |
| Ctrl+`+` / Ctrl+`-` (also Ctrl+Num `+` / Ctrl+Num `-`, Ctrl+`=`), Ctrl+wheel | Text a step larger / smaller, 6 to 72 pt; an image zooms (*rC*) |
| Ctrl+0 | Text back to the viewer font size; an image fits the window (*rC*) |
| F | Image fit to the window or at 100 % |
| R / Shift+R | Turn the image right / left |
| Right / Left (tree) | Unfold a node or step to its first child / fold it or step to its parent, as in the Explorer |
| Up / Down, PgUp / PgDn, Home / End (tree) | Move through the tree's rows |
| Esc | Stop a search, close a bar, close the viewer |

### Connection manager (Ctrl+F)

| Key | Action |
| --- | --- |
| Arrows, Home, End, PgUp, PgDn | Move in the tree; Left and Right close and open a folder |
| Enter | Open a folder |
| F2 / F7 / Insert | Rename / new folder / new connection |
| F8, Del | Delete, after a question |
| Space | Show the connection in the drive bar, or not |
| Ctrl+X / Ctrl+V | Move a connection or folder |
| Tab / Shift+Tab | Into the form and through its fields; Shift+Tab from the tree reaches the filter |
| Enter / Esc (in the form) | Save / back to the tree |
| Esc (in the tree) | Close the manager |

### Queue window (Ctrl+Shift+Q)

| Key | Action |
| --- | --- |
| Up / Down | Choose a job |
| Ctrl+Up / Ctrl+Down, Ctrl+Home | Move a waiting job up / down / to the front |
| Space | Pause or resume the job |
| Del | Remove a waiting job; cancel a running one after a question |
| Enter | Answer the job's question |
| Esc | Close the window; the jobs go on |

F2 in the copy and move dialog (F5, F6) puts the job into the queue; F2 (or Esc) in a job's
progress window sends the running job to the background, where the queue window lists it.

### Multi-rename (Ctrl+M)

| Key | Action |
| --- | --- |
| Alt+N / Alt+X | Rename mask / extension mask |
| Alt+S / Alt+R | Search for / replace with |
| Alt+A, Alt+1, Alt+E, Alt+G, Alt+B | ^ Case, 1x, [E], RegEx, Subst. |
| Alt+C / Alt+T | Upper and lower case / counter |
| Alt+D / Alt+P, F2 | <Default> / preset name, save the preset |
| Alt+L | Write a log |
| Ctrl+Up / Ctrl+Down, Alt+O | Move a file in the list / sort by the old name |
| Tab / Shift+Tab | Through the fields |
| Enter / Alt+U / Esc | Start / undo the last multi-rename / close |

### Compare by content (Ctrl+Shift+F3) and synchronize (Alt+Shift+F2)

| Key | Action |
| --- | --- |
| Alt+Down / Alt+Up | Next / previous difference (compare; *rC*, as WinMerge) |
| Enter | Compare the folders (synchronize) |
| Space, click | Change the action of a row (synchronize) |
| F3, double click | Compare a pair of files by content (synchronize) |
| Esc | Stop a running comparison, then close |

### Terminal (only in rCommander)

| Key | Action |
| --- | --- |
| Ctrl+Shift+C / Ctrl+Shift+V | Copy / paste in the terminal (right click pastes on Windows) |
| Ctrl+C, right click (with selected text) | Copy the selection and clear it; without one Ctrl+C interrupts and a right click pastes (settings `terminal.copy_ctrl_c`, `terminal.copy_right_click`) |
| Ctrl+V (with text on the clipboard) | Paste it; without text Ctrl+V goes to the program (setting `terminal.paste_ctrl_v`) |
| Ctrl+Alt+V | The clipboard's image saved as a PNG the program can read, its path pasted (`terminal.paste_image`, setting `terminal.paste_image`; Windows) |
| Shift + mouse | Select text even while a program such as Claude Code or vim takes the mouse (setting `terminal.shift_selects`) |
| Shift+PgUp / Shift+PgDn, mouse wheel | Scroll back through the terminal's history |
| 1-9, Enter / Esc (in the shell choice) | Start a shell / cancel |

Every other key goes to the shell; only the terminal toggle (Ctrl+Ö, Ctrl+` on a US keyboard),
the terminal over the whole window (Ctrl+Ä, Ctrl+') and the list of the terminal's sessions
(Ctrl+Shift+A) leave it.

### Function-key bar and mouse

The function-key bar under the panels shows the keys and changes with Shift, Ctrl and Alt; a
click runs the key. A click on a tab shows it, a middle click closes it, a right click locks,
copies or closes it; a locked tab (`*` before its title) opens every new folder in a new tab. A
tab dragged within its bar goes where the accent bar marks it, a locked one too; Escape or
leaving the bar cancels (setting `tabs.drag_reorder`). A right click on a file opens its context
menu, beside the rows or on `..` the folder's menu.

In a file list a click puts the cursor on the row, Ctrl+click selects or deselects it,
Shift+click selects the rows from the last clicked one to it in place of the selection,
Ctrl+Shift+click adds them; `..` is never selected. A plain click keeps the selection, as in
Total Commander, unless *A click clears the selection* is on (Settings, page *Folders*).

### Planned keys

| Key | Action | Milestone |
| --- | --- | --- |
| Alt+letter | The main buttons of a dialog | 0.8 (RC-080-05) |
