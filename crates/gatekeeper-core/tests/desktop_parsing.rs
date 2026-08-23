//! Parser-Tests gegen die Fixtures in `tests/fixtures/`.
//!
//! Bewusst gegen echte Dateien statt gegen erfundene Minimalbeispiele. Die interessanten
//! Fehler stecken in dem, was Distributionen tatsächlich ausliefern.

use std::path::{Path, PathBuf};

use gatekeeper_core::{DesktopFile, Locale, ParseError};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn parse(relative: &str) -> DesktopFile {
    let path = fixtures().join(relative);
    DesktopFile::parse_file(&path).unwrap_or_else(|err| panic!("{relative} sollte parsen: {err}"))
}

// --------------------------------------------------------------------------------------
// Grundlagen an einer echten Datei
// --------------------------------------------------------------------------------------

#[test]
fn reads_plain_keys_from_a_real_entry() {
    let chromium = parse("native/chromium.desktop");

    assert_eq!(chromium.id, "chromium.desktop");
    assert_eq!(chromium.entry.string("Name").as_deref(), Some("Chromium Web Browser"));
    assert_eq!(chromium.entry.raw("Exec"), Some("/usr/bin/chromium %U"));
    assert_eq!(chromium.entry.string("Type").as_deref(), Some("Application"));
    assert_eq!(chromium.entry.bool("Terminal"), Some(false));
    assert_eq!(chromium.entry.bool("StartupNotify"), Some(true));
}

#[test]
fn parses_semicolon_lists_without_trailing_empty_item() {
    let chromium = parse("native/chromium.desktop");
    let mime = chromium.entry.list("MimeType");

    assert!(mime.contains(&"x-scheme-handler/http".to_string()));
    assert!(mime.contains(&"x-scheme-handler/https".to_string()));
    // Die Zeile endet auf ';', das darf kein leeres Element erzeugen.
    assert!(!mime.iter().any(String::is_empty), "leeres Element in {mime:?}");
}

#[test]
fn missing_key_is_none_not_empty() {
    let chromium = parse("native/chromium.desktop");

    assert_eq!(chromium.entry.string("NoDisplay"), None);
    assert_eq!(chromium.entry.bool("NoDisplay"), None);
    assert!(chromium.entry.list("Actions").is_empty());
}

// --------------------------------------------------------------------------------------
// Lokalisierung
// --------------------------------------------------------------------------------------

#[test]
fn picks_the_localized_value_when_present() {
    let firefox = parse("native/firefox.desktop");
    let de = Locale::parse("de_CH.UTF-8").unwrap();

    assert_eq!(
        firefox.entry.localized("Comment", Some(&de)).as_deref(),
        Some("Im Internet surfen"),
        "Comment[de] existiert und de_CH muss darauf zurückfallen"
    );
}

#[test]
fn falls_back_to_unlocalized_value() {
    let firefox = parse("native/firefox.desktop");
    let de = Locale::parse("de").unwrap();

    // firefox.desktop hat kein Name[de] in [Desktop Entry], nur in den Actions.
    assert_eq!(firefox.entry.localized("Name", Some(&de)).as_deref(), Some("Firefox Web Browser"));
}

#[test]
fn localized_keys_do_not_leak_across_groups() {
    let firefox = parse("native/firefox.desktop");
    let de = Locale::parse("de").unwrap();

    // Name[de] steht ausschliesslich in den [Desktop Action …]-Gruppen. Läge die Trennung
    // falsch, käme hier der Action-Name heraus.
    let entry_name = firefox.entry.localized("Name", Some(&de));
    assert_eq!(entry_name.as_deref(), Some("Firefox Web Browser"));

    let action = &firefox.actions["new-window"];
    assert_eq!(action.localized("Name", Some(&de)).as_deref(), Some("Ein neues Fenster öffnen"));
}

#[test]
fn unknown_locale_falls_back_and_no_locale_uses_default() {
    let firefox = parse("native/firefox.desktop");
    let klingon = Locale::parse("tlh_KE").unwrap();

    assert_eq!(
        firefox.entry.localized("Comment", Some(&klingon)).as_deref(),
        Some("Browse the World Wide Web")
    );
    assert_eq!(
        firefox.entry.localized("Comment", None).as_deref(),
        Some("Browse the World Wide Web")
    );
}

// --------------------------------------------------------------------------------------
// Desktop Actions
// --------------------------------------------------------------------------------------

#[test]
fn parses_desktop_actions_in_declared_order() {
    let firefox = parse("native/firefox.desktop");
    let names: Vec<_> = firefox.declared_actions().iter().map(|(name, _)| *name).collect();

    assert_eq!(names, ["new-window", "new-private-window"]);
}

#[test]
fn action_exec_lines_are_kept_verbatim() {
    let brave = parse("native/brave-origin.desktop");
    let private = &brave.actions["new-private-window"];

    // Kein Feldcode in dieser Zeile, die URL muss später angehängt werden.
    assert_eq!(private.raw("Exec"), Some("/usr/bin/brave-origin-stable --incognito"));
    assert_eq!(private.string("Name").as_deref(), Some("New Incognito Window"));
}

#[test]
fn actions_declared_but_undefined_are_skipped() {
    let text = "[Desktop Entry]\nType=Application\nName=X\nExec=/bin/x %u\n\
                Actions=defined;never-defined;\n\n\
                [Desktop Action defined]\nName=Defined\nExec=/bin/x --defined\n";
    let file =
        DesktopFile::parse_str(text, "x.desktop".into(), PathBuf::from("x.desktop")).unwrap();

    let names: Vec<_> = file.declared_actions().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["defined"]);
}

// --------------------------------------------------------------------------------------
// Reale Eigenheiten der Paketformate
// --------------------------------------------------------------------------------------

#[test]
fn comments_inside_a_group_are_ignored() {
    // brave-origin.desktop trägt '#'-Kommentare zwischen den Schlüsseln.
    let brave = parse("native/brave-origin.desktop");

    assert_eq!(brave.entry.string("GenericName").as_deref(), Some("Web Browser"));
    assert_eq!(brave.entry.string("Comment").as_deref(), Some("Access the Internet"));
}

#[test]
fn recognizes_the_nodisplay_duplicate_of_brave() {
    let visible = parse("native/brave-origin.desktop");
    let hidden = parse("native/com.brave.Origin.desktop");

    assert_eq!(visible.entry.bool("NoDisplay"), None);
    assert_eq!(hidden.entry.bool("NoDisplay"), Some(true));
    // Derselbe Browser, zwei Desktop-IDs, identische Exec-Zeile. Grundlage für ADR-3.
    assert_eq!(visible.entry.raw("Exec"), hidden.entry.raw("Exec"));
    assert_ne!(visible.id, hidden.id);
}

#[test]
fn keeps_flatpak_file_forwarding_markers_verbatim() {
    let firefox = parse("flatpak/org.mozilla.firefox.desktop");

    assert_eq!(firefox.entry.string("X-Flatpak").as_deref(), Some("org.mozilla.firefox"));
    assert_eq!(
        firefox.entry.raw("Exec"),
        Some(
            "/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=firefox \
             --file-forwarding org.mozilla.firefox @@u %u @@"
        ),
        "die @@-Marker gehören zur Exec-Syntax und werden erst beim Auflösen entfernt"
    );
}

#[test]
fn reads_snap_env_prefix_and_absolute_icon() {
    let firefox = parse("snap/firefox_firefox.desktop");

    assert_eq!(firefox.entry.string("X-SnapInstanceName").as_deref(), Some("firefox"));
    assert!(
        firefox.entry.raw("Exec").unwrap().starts_with("env BAMF_DESKTOP_FILE_HINT="),
        "das Programm ist bei Snap nicht das erste Token"
    );
    assert_eq!(
        firefox.entry.string("Icon").as_deref(),
        Some("/snap/firefox/current/default256.png"),
        "Snap nutzt absolute Icon-Pfade statt Theme-Namen"
    );
}

// --------------------------------------------------------------------------------------
// Kaputte Dateien
// --------------------------------------------------------------------------------------

#[test]
fn rejects_file_without_desktop_entry_group() {
    let path = fixtures().join("malformed/no-group-header.desktop");
    assert!(matches!(DesktopFile::parse_file(&path), Err(ParseError::NoEntryGroup)));
}

#[test]
fn rejects_empty_file() {
    let path = fixtures().join("malformed/empty.desktop");
    assert!(matches!(DesktopFile::parse_file(&path), Err(ParseError::NoEntryGroup)));
}

#[test]
fn rejects_invalid_utf8() {
    let path = fixtures().join("malformed/truncated-binary.desktop");
    assert!(matches!(DesktopFile::parse_file(&path), Err(ParseError::NotUtf8)));
}

#[test]
fn rejects_missing_file() {
    let path = fixtures().join("malformed/does-not-exist.desktop");
    assert!(matches!(DesktopFile::parse_file(&path), Err(ParseError::Io(_))));
}

#[test]
fn first_occurrence_of_a_duplicated_key_wins() {
    let dupe = parse("malformed/duplicate-keys.desktop");

    assert_eq!(dupe.entry.string("Name").as_deref(), Some("First Name"));
    assert_eq!(dupe.entry.raw("Exec"), Some("/usr/bin/dupe %u"));
}

#[test]
fn parses_entries_that_are_valid_but_unusable() {
    // Fehlt Exec, ist die Datei trotzdem wohlgeformt. Aussortiert wird später
    // in der Discovery, nicht im Parser.
    let no_exec = parse("malformed/missing-exec.desktop");
    assert_eq!(no_exec.entry.raw("Exec"), None);

    let link = parse("malformed/wrong-type.desktop");
    assert_eq!(link.entry.string("Type").as_deref(), Some("Link"));
}

// --------------------------------------------------------------------------------------
// Escaping
// --------------------------------------------------------------------------------------

#[test]
fn unescapes_string_values_but_not_exec() {
    let text = "[Desktop Entry]\nType=Application\n\
                Name=Tab\\there\\nand a backslash \\\\\n\
                Exec=/bin/x --path=C:\\\\temp %u\n";
    let file =
        DesktopFile::parse_str(text, "x.desktop".into(), PathBuf::from("x.desktop")).unwrap();

    assert_eq!(file.entry.string("Name").as_deref(), Some("Tab\there\nand a backslash \\"));
    // Exec behält die Rohform, dort gilt eigenes Quoting.
    assert_eq!(file.entry.raw("Exec"), Some("/bin/x --path=C:\\\\temp %u"));
}

#[test]
fn list_values_honour_escaped_semicolons() {
    let text = "[Desktop Entry]\nType=Application\nKeywords=a\\;b;c;\n";
    let file =
        DesktopFile::parse_str(text, "x.desktop".into(), PathBuf::from("x.desktop")).unwrap();

    assert_eq!(file.entry.list("Keywords"), ["a;b", "c"]);
}
