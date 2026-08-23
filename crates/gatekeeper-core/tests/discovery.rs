//! Discovery-Tests gegen die Fixture-Verzeichnisse.

use std::path::{Path, PathBuf};

use gatekeeper_core::discovery::{
    Browser, DiscoveryOptions, Origin, SearchPath, SourceKind, discover,
};

const SELF_ID: &str = "me.rueegger.Gatekeeper.desktop";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Einstellungen ohne jeden Bezug zum echten System, damit die Tests überall gleich laufen.
fn options(paths: Vec<SearchPath>) -> DiscoveryOptions {
    DiscoveryOptions {
        search_paths: paths,
        self_desktop_id: SELF_ID.to_string(),
        locale: None,
        current_desktops: vec!["KDE".to_string()],
        program_dirs: Vec::new(),
    }
}

fn all_sources() -> Vec<SearchPath> {
    vec![
        SearchPath::new(fixtures().join("native"), SourceKind::System),
        SearchPath::new(fixtures().join("flatpak"), SourceKind::Flatpak),
        SearchPath::new(fixtures().join("snap"), SourceKind::Snap),
        SearchPath::new(fixtures().join("malformed"), SourceKind::System),
        SearchPath::new(fixtures().join("excluded"), SourceKind::System),
    ]
}

fn names(browsers: &[Browser]) -> Vec<&str> {
    browsers.iter().map(|b| b.name.as_str()).collect()
}

// --------------------------------------------------------------------------------------
// Invariante 1: niemals wir selbst
// --------------------------------------------------------------------------------------

#[test]
fn never_offers_itself_from_any_source() {
    // Ein Fehler hier erzeugt eine Endlosschleife aus Dialogen. Deshalb je Quellenart.
    for kind in [SourceKind::System, SourceKind::User, SourceKind::Flatpak, SourceKind::Snap] {
        let paths = vec![SearchPath::new(fixtures().join("excluded"), kind)];
        let found = discover(&options(paths));

        assert!(
            !found.iter().any(|b| b.id == SELF_ID),
            "Gatekeeper hat sich aus Quelle {kind:?} selbst angeboten"
        );
    }
}

#[test]
fn never_offers_itself_in_a_full_scan() {
    let found = discover(&options(all_sources()));
    assert!(!found.iter().any(|b| b.id == SELF_ID));
    assert!(!found.iter().any(|b| b.name == "Gatekeeper"));
}

// --------------------------------------------------------------------------------------
// Was gefunden wird
// --------------------------------------------------------------------------------------

#[test]
fn finds_browsers_across_all_packaging_formats() {
    let found = discover(&options(all_sources()));

    assert!(found.iter().any(|b| matches!(b.origin, Origin::System)));
    assert!(found.iter().any(|b| matches!(b.origin, Origin::Flatpak { .. })));
    assert!(found.iter().any(|b| matches!(b.origin, Origin::Snap { .. })));
}

#[test]
fn results_are_sorted_by_display_name() {
    let found = discover(&options(all_sources()));
    let mut sorted = names(&found);
    sorted.sort_by_key(|name| name.to_lowercase());

    assert_eq!(names(&found), sorted);
}

#[test]
fn reads_origin_details_from_packaging_keys() {
    let found = discover(&options(all_sources()));

    let flatpak = found.iter().find(|b| b.id == "org.mozilla.firefox.desktop").unwrap();
    assert_eq!(flatpak.origin, Origin::Flatpak { app_id: Some("org.mozilla.firefox".into()) });

    let snap = found.iter().find(|b| b.id == "firefox_firefox.desktop").unwrap();
    assert_eq!(snap.origin, Origin::Snap { instance: Some("firefox".into()) });
}

#[test]
fn carries_desktop_actions_through() {
    let found = discover(&options(all_sources()));
    let brave = found.iter().find(|b| b.id == "brave-origin.desktop").unwrap();

    let private = brave.actions.iter().find(|a| a.id == "new-private-window").unwrap();
    assert_eq!(private.name, "New Incognito Window");
    assert_eq!(private.exec, "/usr/bin/brave-origin-stable --incognito");
}

// --------------------------------------------------------------------------------------
// Deduplizierung (ADR-3)
// --------------------------------------------------------------------------------------

#[test]
fn merges_the_brave_duplicate_and_keeps_the_visible_one() {
    let found = discover(&options(all_sources()));
    let braves: Vec<_> = found.iter().filter(|b| b.name == "Brave Origin").collect();

    assert_eq!(braves.len(), 1, "Brave doppelt gelistet: {:?}", names(&found));
    assert_eq!(
        braves[0].id, "brave-origin.desktop",
        "der sichtbare Eintrag muss den mit NoDisplay=true schlagen"
    );
    assert!(!braves[0].no_display);
}

#[test]
fn precedence_order_decides_which_duplicate_wins() {
    // Dieselbe Datei zweimal, einmal als User- und einmal als System-Quelle. Die zuerst
    // genannte hat die höhere Präzedenz.
    let native = fixtures().join("native");
    let user_first = discover(&options(vec![
        SearchPath::new(&native, SourceKind::User),
        SearchPath::new(&native, SourceKind::System),
    ]));

    let firefox = user_first.iter().find(|b| b.id == "firefox.desktop").unwrap();
    assert_eq!(firefox.origin, Origin::User);
}

#[test]
fn different_browsers_are_not_merged() {
    let found =
        discover(&options(vec![SearchPath::new(fixtures().join("native"), SourceKind::System)]));

    // firefox, chromium, brave, wobei das Brave-Paar zu einem zusammengefasst wird.
    assert_eq!(found.len(), 3, "unerwartete Liste: {:?}", names(&found));
}

#[test]
fn same_browser_from_different_packaging_stays_separate() {
    // Native und Flatpak-Firefox sind verschiedene Installationen mit verschiedenen
    // Profilen. Sie dürfen nicht zusammengefasst werden.
    let found = discover(&options(vec![
        SearchPath::new(fixtures().join("native"), SourceKind::System),
        SearchPath::new(fixtures().join("flatpak"), SourceKind::Flatpak),
        SearchPath::new(fixtures().join("snap"), SourceKind::Snap),
    ]));

    let firefoxes: Vec<_> = found
        .iter()
        .filter(|b| b.name.to_lowercase().contains("firefox"))
        .map(|b| &b.origin)
        .collect();

    assert_eq!(firefoxes.len(), 3, "je einmal nativ, Flatpak und Snap: {firefoxes:?}");
}

// --------------------------------------------------------------------------------------
// Was ausgefiltert wird
// --------------------------------------------------------------------------------------

#[test]
fn excludes_entries_that_are_not_launchable_browsers() {
    let found =
        discover(&options(vec![SearchPath::new(fixtures().join("excluded"), SourceKind::System)]));

    assert!(found.is_empty(), "nichts davon ist ein Kandidat: {:?}", names(&found));
}

#[test]
fn honours_onlyshowin() {
    let path = vec![SearchPath::new(fixtures().join("excluded"), SourceKind::System)];

    let mut on_gnome = options(path.clone());
    on_gnome.current_desktops = vec!["GNOME".to_string()];
    let found = discover(&on_gnome);

    assert!(
        found.iter().any(|b| b.id == "onlyshowin-gnome.desktop"),
        "unter GNOME ist der Eintrag sichtbar"
    );

    let mut on_kde = options(path);
    on_kde.current_desktops = vec!["KDE".to_string()];
    assert!(!discover(&on_kde).iter().any(|b| b.id == "onlyshowin-gnome.desktop"));
}

#[test]
fn skips_tryexec_that_does_not_resolve() {
    let mut opts = options(vec![SearchPath::new(fixtures().join("excluded"), SourceKind::System)]);
    opts.program_dirs = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];

    assert!(!discover(&opts).iter().any(|b| b.id == "tryexec-missing.desktop"));
}

#[test]
fn a_broken_file_does_not_abort_the_scan() {
    // malformed/ und native/ zusammen: der Müll darf die echten Einträge nicht mitnehmen.
    let found = discover(&options(vec![
        SearchPath::new(fixtures().join("malformed"), SourceKind::System),
        SearchPath::new(fixtures().join("native"), SourceKind::System),
    ]));

    for expected in ["Firefox Web Browser", "Chromium Web Browser", "Brave Origin"] {
        assert!(names(&found).contains(&expected), "{expected} fehlt: {:?}", names(&found));
    }
}

#[test]
fn a_recoverable_file_is_kept_rather_than_discarded() {
    // Doppelte Schlüssel machen einen Eintrag nicht unbrauchbar, der erste Wert gilt.
    // Wegwerfen wäre hier der schlechtere Umgang als reparieren.
    let found =
        discover(&options(vec![SearchPath::new(fixtures().join("malformed"), SourceKind::System)]));

    assert_eq!(names(&found), ["First Name"]);
    assert_eq!(found[0].exec, "/usr/bin/dupe %u");
}

#[test]
fn missing_directories_are_simply_skipped() {
    let found = discover(&options(vec![
        SearchPath::new(fixtures().join("does-not-exist"), SourceKind::System),
        SearchPath::new(fixtures().join("native"), SourceKind::System),
    ]));

    assert_eq!(found.len(), 3);
}
