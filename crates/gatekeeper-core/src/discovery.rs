//! Installierte Browser finden: nativ, als Flatpak und als Snap.
//!
//! Der Ablauf: Verzeichnisse in Präzedenzreihenfolge scannen, Kandidaten filtern,
//! Duplikate zusammenfassen. Kaputte Dateien werden protokolliert und übersprungen; ein
//! einzelner beschädigter Eintrag darf nie dazu führen, dass gar kein Browser gefunden wird.
//!
//! # Invariante 1
//!
//! Die eigene Desktop-ID wird in jeder Quelle gefiltert. Bliebe sie stehen, könnte
//! Gatekeeper sich selbst starten, und das erzeugt eine Endlosschleife aus Dialogen, die
//! die Sitzung unbenutzbar macht. Dafür gibt es einen eigenen Test je Quelle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use log::{debug, warn};

use crate::desktop::{DesktopFile, Group};
use crate::exec;
use crate::locale::Locale;

/// Woher ein Browser stammt. Steuert das Etikett in der Oberfläche.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// Systemweit installiert.
    System,
    /// Im Home-Verzeichnis des Nutzers installiert.
    User,
    Flatpak {
        app_id: Option<String>,
    },
    Snap {
        instance: Option<String>,
    },
}

impl Origin {
    /// Kurzes Etikett für die Oberfläche.
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "System",
            Self::User => "Benutzer",
            Self::Flatpak { .. } => "Flatpak",
            Self::Snap { .. } => "Snap",
        }
    }
}

/// Art eines Suchverzeichnisses. Bestimmt die Herkunft der darin gefundenen Einträge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    System,
    User,
    Flatpak,
    Snap,
}

/// Ein Suchverzeichnis mit seiner Art.
#[derive(Debug, Clone)]
pub struct SearchPath {
    pub dir: PathBuf,
    pub kind: SourceKind,
}

impl SearchPath {
    pub fn new(dir: impl Into<PathBuf>, kind: SourceKind) -> Self {
        Self { dir: dir.into(), kind }
    }
}

/// Eine Zusatzaktion eines Browsers, etwa „privates Fenster".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAction {
    pub id: String,
    pub name: String,
    /// Rohe `Exec`-Zeile der Aktion.
    pub exec: String,
}

/// Ein startbarer Browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    /// Desktop-ID, etwa `firefox.desktop`.
    pub id: String,
    /// Angezeigter Name in der Sprache des Nutzers.
    pub name: String,
    /// Icon, entweder Theme-Name oder absoluter Pfad.
    pub icon: Option<String>,
    pub origin: Origin,
    /// Rohe `Exec`-Zeile. Wird erst beim Start aufgelöst.
    pub exec: String,
    pub path: PathBuf,
    /// `NoDisplay=true`: gültiger Handler, aber kein Menüeintrag.
    pub no_display: bool,
    pub actions: Vec<BrowserAction>,
}

/// Einstellungen für den Scan. Alles injizierbar, damit sich der Scan ohne echtes System
/// testen lässt.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Suchverzeichnisse, höchste Präzedenz zuerst.
    pub search_paths: Vec<SearchPath>,
    /// Eigene Desktop-ID. Wird immer ausgefiltert (Invariante 1).
    pub self_desktop_id: String,
    pub locale: Option<Locale>,
    /// Inhalt von `XDG_CURRENT_DESKTOP`, für `OnlyShowIn`/`NotShowIn`.
    pub current_desktops: Vec<String>,
    /// Verzeichnisse, in denen `TryExec` und nicht-absolute Programme gesucht werden.
    pub program_dirs: Vec<PathBuf>,
}

impl DiscoveryOptions {
    /// Einstellungen aus der Umgebung des laufenden Prozesses.
    pub fn from_env(self_desktop_id: impl Into<String>) -> Self {
        Self {
            search_paths: default_search_paths(),
            self_desktop_id: self_desktop_id.into(),
            locale: Locale::from_env(),
            current_desktops: std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            program_dirs: std::env::split_paths(&std::env::var("PATH").unwrap_or_default())
                .collect(),
        }
    }
}

/// Die Verzeichnisse, in denen Desktop-Einträge liegen, höchste Präzedenz zuerst.
///
/// Die Flatpak- und Snap-Exportpfade werden ausdrücklich aufgeführt und nicht `XDG_DATA_DIRS`
/// überlassen: In der Sandbox enthält diese Variable die Pfade der Runtime, nicht die des
/// Hosts.
pub fn default_search_paths() -> Vec<SearchPath> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));

    let mut paths = Vec::new();
    let mut push = |dir: PathBuf, kind: SourceKind| paths.push(SearchPath::new(dir, kind));

    if let Some(data_home) = &data_home {
        push(data_home.join("applications"), SourceKind::User);
        push(data_home.join("flatpak/exports/share/applications"), SourceKind::Flatpak);
    }

    // Flatpak hängt seine Exportpfade auf dem Host selbst an XDG_DATA_DIRS an. Würden sie
    // hier pauschal als System durchgehen, trüge ein systemweit installierter Flatpak-Browser
    // das falsche Etikett.
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in std::env::split_paths(&data_dirs) {
        let applications = dir.join("applications");
        let kind = classify_dir(&applications);
        push(applications, kind);
    }

    push("/usr/local/share/applications".into(), SourceKind::System);
    push("/usr/share/applications".into(), SourceKind::System);
    push("/var/lib/flatpak/exports/share/applications".into(), SourceKind::Flatpak);
    push("/var/lib/snapd/desktop/applications".into(), SourceKind::Snap);

    // Doppelte Verzeichnisse würden Einträge doppelt einlesen. Das erste Vorkommen zählt,
    // weil es die höhere Präzedenz hat.
    let mut seen = std::collections::BTreeSet::new();
    paths.retain(|path| seen.insert(path.dir.clone()));
    paths
}

/// Sucht alle startbaren Browser.
///
/// Das Ergebnis ist dedupliziert und nach Anzeigename sortiert.
pub fn discover(options: &DiscoveryOptions) -> Vec<Browser> {
    let mut by_desktop_id: BTreeMap<String, (usize, Browser)> = BTreeMap::new();

    for (rank, source) in options.search_paths.iter().enumerate() {
        for file in read_dir(&source.dir) {
            let Some(browser) = candidate(&file, source.kind, options) else {
                continue;
            };
            // Gleiche Desktop-ID in mehreren Verzeichnissen: das höherpriore gewinnt.
            by_desktop_id.entry(browser.id.clone()).or_insert((rank, browser));
        }
    }

    let mut browsers = deduplicate(by_desktop_id.into_values().collect(), options);
    browsers.sort_by_key(|browser| browser.name.to_lowercase());
    browsers
}

fn read_dir(dir: &Path) -> Vec<DesktopFile> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            debug!("Verzeichnis {} übersprungen: {err}", dir.display());
            return Vec::new();
        }
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "desktop") {
            continue;
        }
        match DesktopFile::parse_file(&path) {
            Ok(file) => files.push(file),
            // Auf echten Systemen liegt immer irgendwo Müll. Weitermachen.
            Err(err) => warn!("{} übersprungen: {err}", path.display()),
        }
    }
    files
}

/// Prüft, ob ein Eintrag als Browser in Frage kommt, und baut ihn.
fn candidate(file: &DesktopFile, kind: SourceKind, options: &DiscoveryOptions) -> Option<Browser> {
    // Invariante 1: niemals wir selbst.
    if file.id == options.self_desktop_id {
        debug!("eigener Eintrag {} ausgefiltert", file.id);
        return None;
    }

    let entry = &file.entry;
    if entry.string("Type").as_deref() != Some("Application") {
        return None;
    }
    // `Hidden=true` heisst nach Spec „gelöscht", im Gegensatz zu `NoDisplay`.
    if entry.bool("Hidden") == Some(true) {
        return None;
    }
    if !handles_web_links(entry) {
        return None;
    }
    if !visible_in_current_desktop(entry, &options.current_desktops) {
        return None;
    }
    if let Some(try_exec) = entry.raw("TryExec")
        && resolve_program(try_exec, &options.program_dirs).is_none()
    {
        debug!("{}: TryExec '{try_exec}' nicht auflösbar", file.id);
        return None;
    }

    let exec = entry.raw("Exec")?.to_string();
    // Eine Exec-Zeile, die sich nicht zerlegen lässt, ist unbrauchbar.
    if exec::tokenize_without_field_codes(&exec).is_err() {
        warn!("{}: Exec-Zeile nicht zerlegbar: {exec}", file.id);
        return None;
    }

    let name = entry
        .localized("Name", options.locale.as_ref())
        .unwrap_or_else(|| file.id.trim_end_matches(".desktop").to_string());

    let actions = file
        .declared_actions()
        .into_iter()
        .filter_map(|(id, group)| {
            Some(BrowserAction {
                id: id.to_string(),
                name: group.localized("Name", options.locale.as_ref())?,
                exec: group.raw("Exec")?.to_string(),
            })
        })
        .collect();

    Some(Browser {
        id: file.id.clone(),
        name,
        icon: entry.string("Icon"),
        origin: determine_origin(kind, entry),
        exec,
        path: file.path.clone(),
        no_display: entry.bool("NoDisplay") == Some(true),
        actions,
    })
}

fn handles_web_links(entry: &Group) -> bool {
    entry
        .list("MimeType")
        .iter()
        .any(|mime| mime == "x-scheme-handler/http" || mime == "x-scheme-handler/https")
}

fn visible_in_current_desktop(entry: &Group, current: &[String]) -> bool {
    let matches = |list: Vec<String>| {
        list.iter().any(|wanted| current.iter().any(|have| have.eq_ignore_ascii_case(wanted)))
    };

    let only_show_in = entry.list("OnlyShowIn");
    if !only_show_in.is_empty() && !matches(only_show_in) {
        return false;
    }
    let not_show_in = entry.list("NotShowIn");
    if !not_show_in.is_empty() && matches(not_show_in) {
        return false;
    }
    true
}

fn determine_origin(kind: SourceKind, entry: &Group) -> Origin {
    // Die Schlüssel gewinnen gegen die Verzeichnisart: ein Flatpak-Export kann auch
    // ausserhalb der bekannten Pfade liegen.
    if let Some(app_id) = entry.string("X-Flatpak") {
        return Origin::Flatpak { app_id: Some(app_id) };
    }
    if let Some(instance) =
        entry.string("X-SnapInstanceName").or_else(|| entry.string("X-Snap-Instance-Name"))
    {
        return Origin::Snap { instance: Some(instance) };
    }

    match kind {
        SourceKind::System => Origin::System,
        SourceKind::User => Origin::User,
        SourceKind::Flatpak => Origin::Flatpak { app_id: None },
        SourceKind::Snap => Origin::Snap { instance: None },
    }
}

// --------------------------------------------------------------------------------------
// Deduplizierung (ADR-3)
// --------------------------------------------------------------------------------------

/// Fasst Einträge zusammen, die dasselbe Programm starten.
///
/// Der Schlüssel ist die von Feldcodes befreite, normalisierte `Exec`-Zeile. Über die
/// Desktop-ID allein liesse sich das Brave-Paar `brave-origin.desktop` /
/// `com.brave.Origin.desktop` nicht zusammenführen.
fn deduplicate(candidates: Vec<(usize, Browser)>, options: &DiscoveryOptions) -> Vec<Browser> {
    let mut groups: BTreeMap<Vec<String>, (usize, Browser)> = BTreeMap::new();

    for (rank, browser) in candidates {
        let key = dedup_key(&browser.exec, &options.program_dirs);
        match groups.get(&key) {
            Some((existing_rank, existing))
                if !prefer(&browser, rank, existing, *existing_rank) =>
            {
                debug!("{} als Duplikat von {} verworfen", browser.id, existing.id);
            }
            _ => {
                groups.insert(key, (rank, browser));
            }
        }
    }

    groups.into_values().map(|(_, browser)| browser).collect()
}

/// Entscheidet, welcher von zwei Einträgen desselben Programms angezeigt wird.
fn prefer(new: &Browser, new_rank: usize, old: &Browser, old_rank: usize) -> bool {
    // Ein sichtbarer Eintrag schlägt einen mit NoDisplay. Sonst zeigte die Liste den
    // Namen, den der Nutzer nirgends sonst sieht.
    match (new.no_display, old.no_display) {
        (false, true) => return true,
        (true, false) => return false,
        _ => {}
    }
    // Danach das höherpriore Verzeichnis, dann stabil nach ID.
    match new_rank.cmp(&old_rank) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => new.id < old.id,
    }
}

/// Bildet den Vergleichsschlüssel einer `Exec`-Zeile.
///
/// Bei Flatpak sind das Wort `flatpak`, das Unterkommando und die App-ID stabil, während
/// `--branch` und `--arch` je nach Installation abweichen. Deshalb wird für Flatpak nur die
/// App-ID verglichen.
fn dedup_key(exec: &str, program_dirs: &[PathBuf]) -> Vec<String> {
    let Ok(tokens) = exec::tokenize_without_field_codes(exec) else {
        return vec![exec.to_string()];
    };
    let tokens = strip_env_prefix(&tokens);
    let Some((program, args)) = tokens.split_first() else {
        return vec![exec.to_string()];
    };

    let resolved = resolve_program(program, program_dirs)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string());

    if Path::new(&resolved).file_name().is_some_and(|name| name == "flatpak")
        && let Some(app_id) = flatpak_app_id(args)
    {
        return vec!["flatpak".to_string(), app_id];
    }

    let mut key = Vec::with_capacity(args.len() + 1);
    key.push(resolved);
    key.extend(args.iter().cloned());
    key
}

/// Entfernt ein `env VAR=WERT …`-Präfix, wie es Snap-Exporte verwenden.
fn strip_env_prefix(tokens: &[String]) -> Vec<String> {
    let Some((first, rest)) = tokens.split_first() else {
        return Vec::new();
    };
    if Path::new(first).file_name().is_none_or(|name| name != "env") {
        return tokens.to_vec();
    }
    // Nach `env` folgen Zuweisungen, danach das eigentliche Programm.
    let start = rest.iter().position(|token| !token.contains('=')).unwrap_or(rest.len());
    rest[start..].to_vec()
}

/// Zieht die App-ID aus den Argumenten eines `flatpak run …`-Aufrufs.
fn flatpak_app_id(args: &[String]) -> Option<String> {
    let mut args = args.iter();
    if args.next()? != "run" {
        return None;
    }
    // Erstes Argument, das kein Schalter ist, ist die App-ID.
    args.find(|arg| !arg.starts_with('-')).cloned()
}

/// Leitet die Quellenart aus dem Pfad ab.
///
/// Nötig, weil `XDG_DATA_DIRS` auf dem Host die Flatpak-Exportpfade mitbringt und diese
/// sonst als gewöhnliche Systempfade gälten.
fn classify_dir(dir: &Path) -> SourceKind {
    let path = dir.to_string_lossy();
    if path.contains("/flatpak/exports/") {
        SourceKind::Flatpak
    } else if path.contains("/snapd/desktop") {
        SourceKind::Snap
    } else if dir.starts_with("/usr") || dir.starts_with("/opt") || dir.starts_with("/var") {
        SourceKind::System
    } else if std::env::var_os("HOME").is_some_and(|home| dir.starts_with(PathBuf::from(home))) {
        SourceKind::User
    } else {
        SourceKind::System
    }
}

/// Sucht ein Programm im Dateisystem und löst Symlinks auf.
///
/// Absolute Pfade werden direkt geprüft, alles andere in `program_dirs` gesucht.
fn resolve_program(program: &str, program_dirs: &[PathBuf]) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return candidate
            .exists()
            .then(|| candidate.canonicalize().unwrap_or_else(|_| candidate.to_path_buf()));
    }
    program_dirs
        .iter()
        .map(|dir| dir.join(program))
        .find_map(|path| path.exists().then(|| path.canonicalize().unwrap_or(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_flatpak_exports_even_when_they_come_from_data_dirs() {
        // Flatpak hängt diese Pfade auf dem Host selbst an XDG_DATA_DIRS an.
        assert_eq!(
            classify_dir(Path::new("/var/lib/flatpak/exports/share/applications")),
            SourceKind::Flatpak
        );
        assert_eq!(
            classify_dir(Path::new("/home/x/.local/share/flatpak/exports/share/applications")),
            SourceKind::Flatpak
        );
        assert_eq!(
            classify_dir(Path::new("/var/lib/snapd/desktop/applications")),
            SourceKind::Snap
        );
        assert_eq!(classify_dir(Path::new("/usr/share/applications")), SourceKind::System);
    }

    #[test]
    fn strips_the_env_prefix_snap_exports_use() {
        let tokens: Vec<String> = ["env", "BAMF_DESKTOP_FILE_HINT=/x.desktop", "/snap/bin/firefox"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(strip_env_prefix(&tokens), ["/snap/bin/firefox"]);
    }

    #[test]
    fn leaves_a_normal_command_untouched() {
        let tokens: Vec<String> =
            ["/usr/bin/chromium", "--flag"].iter().map(|s| s.to_string()).collect();

        assert_eq!(strip_env_prefix(&tokens), tokens);
    }

    #[test]
    fn extracts_the_flatpak_app_id_past_varying_flags() {
        let args: Vec<String> = [
            "run",
            "--branch=stable",
            "--arch=x86_64",
            "--command=firefox",
            "--file-forwarding",
            "org.mozilla.firefox",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(flatpak_app_id(&args).as_deref(), Some("org.mozilla.firefox"));
    }

    #[test]
    fn dedup_key_ignores_branch_and_arch_for_flatpak() {
        let stable =
            "/usr/bin/flatpak run --branch=stable --arch=x86_64 org.mozilla.firefox @@u %u @@";
        let beta = "/usr/bin/flatpak run --arch=aarch64 org.mozilla.firefox %u";

        assert_eq!(dedup_key(stable, &[]), dedup_key(beta, &[]));
        assert_eq!(dedup_key(stable, &[]), ["flatpak", "org.mozilla.firefox"]);
    }

    #[test]
    fn dedup_key_separates_different_flatpak_apps() {
        assert_ne!(
            dedup_key("/usr/bin/flatpak run org.mozilla.firefox %u", &[]),
            dedup_key("/usr/bin/flatpak run com.brave.Browser %u", &[])
        );
    }

    #[test]
    fn dedup_key_matches_the_real_brave_pair() {
        // Beide Dateien auf dem Entwicklungssystem, siehe ADR-3.
        assert_eq!(
            dedup_key("/usr/bin/brave-origin-stable %U", &[]),
            dedup_key("/usr/bin/brave-origin-stable %U", &[])
        );
    }
}
