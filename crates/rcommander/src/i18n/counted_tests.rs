use super::{super::english, Counted};
use crate::locale::Lang;

const LANGS: [Lang; 4] = [Lang::En, Lang::De, Lang::Fr, Lang::Es];

/// Every text of [`Counted`] with each of its numbers `n`, named.
fn every_text(words: Counted, n: u64) -> Vec<(&'static str, String)> {
    let size = || "2 KiB".to_owned();
    vec![
        (
            "panel_status",
            words.panel_status(("1 KiB".to_owned(), size()), (n, n), (n, n)),
        ),
        ("files_and_folders", words.files_and_folders(n, n)),
        ("disk_usage_total", words.disk_usage_total(size(), n, n)),
        ("quick_dir", words.quick_dir(n, n, size())),
        ("delete_folder", words.delete_folder("Work", n, n)),
        (
            "progress_counts",
            words.progress_counts((n, n), ("1 KiB".to_owned(), size())),
        ),
        ("inventory_report", words.inventory_report(n, n, n, n, n)),
        (
            "checksum_verified",
            words.checksum_verified("sums.sha256", [n; 5]),
        ),
        (
            "host_keys_imported",
            words.host_keys_imported("known_hosts", [n; 4]),
        ),
        ("selection_loaded", words.selection_loaded(n, n)),
        ("archive_test", words.archive_test("a.zip", "done", [n; 3])),
        ("sync_found", words.sync_found([n; 4])),
        ("loaded_unloadable", words.loaded_unloadable(n, n)),
        ("attributes_partly", words.attributes_partly(n, n)),
        ("rename_changes", words.rename_changes(n, n)),
        ("rename_conflicts", words.rename_conflicts(n, n)),
        (
            "search_running",
            words.search_running(n, n, "/data".to_owned()),
        ),
    ]
}

/// Each text fills every placeholder of its frame and its parts, in every language and for 0, 1
/// and 2: a part's key never shows, nor a `{name}` left open.
#[test]
fn every_counted_text_is_whole_in_every_language() {
    for lang in LANGS {
        for n in 0..=2 {
            for (name, text) in every_text(Counted(lang), n) {
                assert!(!text.contains('{'), "{lang:?} {name} {n}: {text}");
                let key = english().keys().find(|key| text.contains(key.as_str()));
                assert_eq!(key, None, "{lang:?} {name} {n}: {text}");
                assert!(text.contains(&n.to_string()), "{lang:?} {name} {n}: {text}");
            }
        }
    }
}

/// The ten texts of the audit (RC-110-91) as they read with each of their numbers 0, 1 and 2, in
/// every language: French takes the singular for 0 as well.
const READS: &[(&str, Lang, [&str; 3])] = &[
    (
        "panel_status",
        Lang::En,
        [
            "1 KiB of 2 KiB in 0 of 0 files, 0 of 0 folders",
            "1 KiB of 2 KiB in 1 of 1 file, 1 of 1 folder",
            "1 KiB of 2 KiB in 2 of 2 files, 2 of 2 folders",
        ],
    ),
    (
        "panel_status",
        Lang::De,
        [
            "1 KiB von 2 KiB in 0 von 0 Dateien, 0 von 0 Verzeichnissen",
            "1 KiB von 2 KiB in 1 von 1 Datei, 1 von 1 Verzeichnis",
            "1 KiB von 2 KiB in 2 von 2 Dateien, 2 von 2 Verzeichnissen",
        ],
    ),
    (
        "panel_status",
        Lang::Fr,
        [
            "1 KiB sur 2 KiB dans 0 sur 0 fichier, 0 sur 0 dossier",
            "1 KiB sur 2 KiB dans 1 sur 1 fichier, 1 sur 1 dossier",
            "1 KiB sur 2 KiB dans 2 sur 2 fichiers, 2 sur 2 dossiers",
        ],
    ),
    (
        "panel_status",
        Lang::Es,
        [
            "1 KiB de 2 KiB en 0 de 0 archivos, 0 de 0 carpetas",
            "1 KiB de 2 KiB en 1 de 1 archivo, 1 de 1 carpeta",
            "1 KiB de 2 KiB en 2 de 2 archivos, 2 de 2 carpetas",
        ],
    ),
    (
        "files_and_folders",
        Lang::En,
        [
            "0 files, 0 folders",
            "1 file, 1 folder",
            "2 files, 2 folders",
        ],
    ),
    (
        "files_and_folders",
        Lang::De,
        [
            "0 Dateien, 0 Verzeichnisse",
            "1 Datei, 1 Verzeichnis",
            "2 Dateien, 2 Verzeichnisse",
        ],
    ),
    (
        "files_and_folders",
        Lang::Fr,
        [
            "0 fichier, 0 dossier",
            "1 fichier, 1 dossier",
            "2 fichiers, 2 dossiers",
        ],
    ),
    (
        "files_and_folders",
        Lang::Es,
        [
            "0 archivos, 0 carpetas",
            "1 archivo, 1 carpeta",
            "2 archivos, 2 carpetas",
        ],
    ),
    (
        "disk_usage_total",
        Lang::En,
        [
            "2 KiB in 0 files and 0 folders.",
            "2 KiB in 1 file and 1 folder.",
            "2 KiB in 2 files and 2 folders.",
        ],
    ),
    (
        "disk_usage_total",
        Lang::De,
        [
            "2 KiB in 0 Dateien und 0 Verzeichnissen.",
            "2 KiB in 1 Datei und 1 Verzeichnis.",
            "2 KiB in 2 Dateien und 2 Verzeichnissen.",
        ],
    ),
    (
        "disk_usage_total",
        Lang::Fr,
        [
            "2 KiB dans 0 fichier et 0 dossier.",
            "2 KiB dans 1 fichier et 1 dossier.",
            "2 KiB dans 2 fichiers et 2 dossiers.",
        ],
    ),
    (
        "disk_usage_total",
        Lang::Es,
        [
            "2 KiB en 0 archivos y 0 carpetas.",
            "2 KiB en 1 archivo y 1 carpeta.",
            "2 KiB en 2 archivos y 2 carpetas.",
        ],
    ),
    (
        "quick_dir",
        Lang::En,
        [
            "0 files, 0 folders, 2 KiB in files",
            "1 file, 1 folder, 2 KiB in files",
            "2 files, 2 folders, 2 KiB in files",
        ],
    ),
    (
        "quick_dir",
        Lang::De,
        [
            "0 Dateien, 0 Verzeichnisse, 2 KiB in Dateien",
            "1 Datei, 1 Verzeichnis, 2 KiB in Dateien",
            "2 Dateien, 2 Verzeichnisse, 2 KiB in Dateien",
        ],
    ),
    (
        "quick_dir",
        Lang::Fr,
        [
            "0 fichier, 0 dossier, 2 KiB dans les fichiers",
            "1 fichier, 1 dossier, 2 KiB dans les fichiers",
            "2 fichiers, 2 dossiers, 2 KiB dans les fichiers",
        ],
    ),
    (
        "quick_dir",
        Lang::Es,
        [
            "0 archivos, 0 carpetas, 2 KiB en archivos",
            "1 archivo, 1 carpeta, 2 KiB en archivos",
            "2 archivos, 2 carpetas, 2 KiB en archivos",
        ],
    ),
    (
        "delete_folder",
        Lang::En,
        [
            "Delete the folder Work with 0 folders and 0 connections inside it, and their stored secrets?",
            "Delete the folder Work with 1 folder and 1 connection inside it, and their stored secrets?",
            "Delete the folder Work with 2 folders and 2 connections inside it, and their stored secrets?",
        ],
    ),
    (
        "delete_folder",
        Lang::De,
        [
            "Den Ordner Work mit 0 Ordnern und 0 Verbindungen darin sowie deren gespeicherte Geheimnisse l\u{f6}schen?",
            "Den Ordner Work mit 1 Ordner und 1 Verbindung darin sowie deren gespeicherte Geheimnisse l\u{f6}schen?",
            "Den Ordner Work mit 2 Ordnern und 2 Verbindungen darin sowie deren gespeicherte Geheimnisse l\u{f6}schen?",
        ],
    ),
    (
        "delete_folder",
        Lang::Fr,
        [
            "Supprimer le dossier Work avec 0 dossier et 0 connexion qu'il contient, ainsi que leurs secrets enregistr\u{e9}s\u{202f}?",
            "Supprimer le dossier Work avec 1 dossier et 1 connexion qu'il contient, ainsi que leurs secrets enregistr\u{e9}s\u{202f}?",
            "Supprimer le dossier Work avec 2 dossiers et 2 connexions qu'il contient, ainsi que leurs secrets enregistr\u{e9}s\u{202f}?",
        ],
    ),
    (
        "delete_folder",
        Lang::Es,
        [
            "\u{bf}Eliminar la carpeta Work con 0 carpetas y 0 conexiones que contiene, y sus secretos guardados?",
            "\u{bf}Eliminar la carpeta Work con 1 carpeta y 1 conexi\u{f3}n que contiene, y sus secretos guardados?",
            "\u{bf}Eliminar la carpeta Work con 2 carpetas y 2 conexiones que contiene, y sus secretos guardados?",
        ],
    ),
    (
        "progress_counts",
        Lang::En,
        [
            "0 of 0 files, 1 KiB of 2 KiB",
            "1 of 1 file, 1 KiB of 2 KiB",
            "2 of 2 files, 1 KiB of 2 KiB",
        ],
    ),
    (
        "progress_counts",
        Lang::De,
        [
            "0 von 0 Dateien, 1 KiB von 2 KiB",
            "1 von 1 Datei, 1 KiB von 2 KiB",
            "2 von 2 Dateien, 1 KiB von 2 KiB",
        ],
    ),
    (
        "progress_counts",
        Lang::Fr,
        [
            "0 sur 0 fichier, 1 KiB sur 2 KiB",
            "1 sur 1 fichier, 1 KiB sur 2 KiB",
            "2 sur 2 fichiers, 1 KiB sur 2 KiB",
        ],
    ),
    (
        "progress_counts",
        Lang::Es,
        [
            "0 de 0 archivos, 1 KiB de 2 KiB",
            "1 de 1 archivo, 1 KiB de 2 KiB",
            "2 de 2 archivos, 1 KiB de 2 KiB",
        ],
    ),
    (
        "inventory_report",
        Lang::En,
        [
            "0 new, 0 changed, 0 unchanged, 0 no longer in the inventory, 0 left out",
            "1 new, 1 changed, 1 unchanged, 1 no longer in the inventory, 1 left out",
            "2 new, 2 changed, 2 unchanged, 2 no longer in the inventory, 2 left out",
        ],
    ),
    (
        "inventory_report",
        Lang::De,
        [
            "0 neu, 0 ge\u{e4}ndert, 0 unver\u{e4}ndert, 0 nicht mehr im Inventar, 0 ausgelassen",
            "1 neu, 1 ge\u{e4}ndert, 1 unver\u{e4}ndert, 1 nicht mehr im Inventar, 1 ausgelassen",
            "2 neu, 2 ge\u{e4}ndert, 2 unver\u{e4}ndert, 2 nicht mehr im Inventar, 2 ausgelassen",
        ],
    ),
    (
        "inventory_report",
        Lang::Fr,
        [
            "0 nouveau, 0 modifi\u{e9}, 0 inchang\u{e9}, 0 n'est plus dans l'inventaire, 0 ignor\u{e9}",
            "1 nouveau, 1 modifi\u{e9}, 1 inchang\u{e9}, 1 n'est plus dans l'inventaire, 1 ignor\u{e9}",
            "2 nouveaux, 2 modifi\u{e9}s, 2 inchang\u{e9}s, 2 ne sont plus dans l'inventaire, 2 ignor\u{e9}s",
        ],
    ),
    (
        "inventory_report",
        Lang::Es,
        [
            "0 nuevos, 0 cambiados, 0 sin cambios, 0 ya no est\u{e1}n en el inventario, 0 omitidos",
            "1 nuevo, 1 cambiado, 1 sin cambios, 1 ya no est\u{e1} en el inventario, 1 omitido",
            "2 nuevos, 2 cambiados, 2 sin cambios, 2 ya no est\u{e1}n en el inventario, 2 omitidos",
        ],
    ),
    (
        "checksum_verified",
        Lang::En,
        [
            "sums.sha256: 0 OK, 0 different, 0 missing, 0 cannot be read, 0 invalid lines",
            "sums.sha256: 1 OK, 1 different, 1 missing, 1 cannot be read, 1 invalid line",
            "sums.sha256: 2 OK, 2 different, 2 missing, 2 cannot be read, 2 invalid lines",
        ],
    ),
    (
        "checksum_verified",
        Lang::De,
        [
            "sums.sha256: 0 OK, 0 abweichend, 0 fehlen, 0 nicht lesbar, 0 ung\u{fc}ltige Zeilen",
            "sums.sha256: 1 OK, 1 abweichend, 1 fehlt, 1 nicht lesbar, 1 ung\u{fc}ltige Zeile",
            "sums.sha256: 2 OK, 2 abweichend, 2 fehlen, 2 nicht lesbar, 2 ung\u{fc}ltige Zeilen",
        ],
    ),
    (
        "checksum_verified",
        Lang::Fr,
        [
            "sums.sha256\u{a0}: 0 OK, 0 diff\u{e9}rent, 0 manquant, 0 illisible, 0 ligne non valide",
            "sums.sha256\u{a0}: 1 OK, 1 diff\u{e9}rent, 1 manquant, 1 illisible, 1 ligne non valide",
            "sums.sha256\u{a0}: 2 OK, 2 diff\u{e9}rents, 2 manquants, 2 illisibles, 2 lignes non valides",
        ],
    ),
    (
        "checksum_verified",
        Lang::Es,
        [
            "sums.sha256: 0 correctos, 0 distintos, 0 faltan, 0 no se pueden leer, 0 l\u{ed}neas no v\u{e1}lidas",
            "sums.sha256: 1 correcto, 1 distinto, 1 falta, 1 no se puede leer, 1 l\u{ed}nea no v\u{e1}lida",
            "sums.sha256: 2 correctos, 2 distintos, 2 faltan, 2 no se pueden leer, 2 l\u{ed}neas no v\u{e1}lidas",
        ],
    ),
    (
        "host_keys_imported",
        Lang::En,
        [
            "Imported from known_hosts: 0 new, 0 already trusted, 0 changed (the trusted keys stay), 0 skipped.",
            "Imported from known_hosts: 1 new, 1 already trusted, 1 changed (the trusted key stays), 1 skipped.",
            "Imported from known_hosts: 2 new, 2 already trusted, 2 changed (the trusted keys stay), 2 skipped.",
        ],
    ),
    (
        "host_keys_imported",
        Lang::De,
        [
            "Aus known_hosts importiert: 0 neu, 0 schon vertraut, 0 ge\u{e4}ndert (die vertrauten Schl\u{fc}ssel bleiben), 0 \u{fc}bersprungen.",
            "Aus known_hosts importiert: 1 neu, 1 schon vertraut, 1 ge\u{e4}ndert (der vertraute Schl\u{fc}ssel bleibt), 1 \u{fc}bersprungen.",
            "Aus known_hosts importiert: 2 neu, 2 schon vertraut, 2 ge\u{e4}ndert (die vertrauten Schl\u{fc}ssel bleiben), 2 \u{fc}bersprungen.",
        ],
    ),
    (
        "host_keys_imported",
        Lang::Fr,
        [
            "Import\u{e9} depuis known_hosts\u{a0}: 0 nouvelle, 0 d\u{e9}j\u{e0} approuv\u{e9}e, 0 modifi\u{e9}e (la cl\u{e9} approuv\u{e9}e reste), 0 ignor\u{e9}e.",
            "Import\u{e9} depuis known_hosts\u{a0}: 1 nouvelle, 1 d\u{e9}j\u{e0} approuv\u{e9}e, 1 modifi\u{e9}e (la cl\u{e9} approuv\u{e9}e reste), 1 ignor\u{e9}e.",
            "Import\u{e9} depuis known_hosts\u{a0}: 2 nouvelles, 2 d\u{e9}j\u{e0} approuv\u{e9}es, 2 modifi\u{e9}es (les cl\u{e9}s approuv\u{e9}es restent), 2 ignor\u{e9}es.",
        ],
    ),
    (
        "host_keys_imported",
        Lang::Es,
        [
            "Importado desde known_hosts: 0 nuevas, 0 ya de confianza, 0 cambiadas (se mantienen las claves de confianza), 0 omitidas.",
            "Importado desde known_hosts: 1 nueva, 1 ya de confianza, 1 cambiada (se mantiene la clave de confianza), 1 omitida.",
            "Importado desde known_hosts: 2 nuevas, 2 ya de confianza, 2 cambiadas (se mantienen las claves de confianza), 2 omitidas.",
        ],
    ),
    (
        "selection_loaded",
        Lang::En,
        [
            "0 selected, 0 not found",
            "1 selected, 1 not found",
            "2 selected, 2 not found",
        ],
    ),
    (
        "selection_loaded",
        Lang::De,
        [
            "0 markiert, 0 nicht gefunden",
            "1 markiert, 1 nicht gefunden",
            "2 markiert, 2 nicht gefunden",
        ],
    ),
    (
        "selection_loaded",
        Lang::Fr,
        [
            "0 s\u{e9}lectionn\u{e9}, 0 introuvable",
            "1 s\u{e9}lectionn\u{e9}, 1 introuvable",
            "2 s\u{e9}lectionn\u{e9}s, 2 introuvables",
        ],
    ),
    (
        "selection_loaded",
        Lang::Es,
        [
            "0 seleccionados, 0 no encontrados",
            "1 seleccionado, 1 no encontrado",
            "2 seleccionados, 2 no encontrados",
        ],
    ),
];

#[test]
fn the_texts_read_right_at_zero_one_and_two() {
    for (name, lang, wanted) in READS {
        for (n, wanted) in (0..).zip(wanted) {
            let text = every_text(Counted(*lang), n)
                .into_iter()
                .find(|(text_name, _)| text_name == name)
                .map(|(_, text)| text);
            assert_eq!(text.as_deref(), Some(*wanted), "{lang:?} {name} {n}");
        }
    }
}
