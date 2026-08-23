//! Prüfen und Setzen, ob Gatekeeper der Standardbrowser ist.
//!
//! Die Prüfung läuft bei jedem Start und liegt damit im Klickpfad. Sie liest deshalb die
//! `mimeapps.list`-Dateien selbst, statt ein Werkzeug aufzurufen: Dateien lesen kostet
//! Mikrosekunden, ein Prozessstart Millisekunden.
//!
//! Gesetzt wird dagegen über `xdg-settings`, weil dabei auch
//! desktopspezifische Dateien und Zwischenspeicher berührt werden, die wir nicht alle
//! kennen. Erst wenn das Werkzeug fehlt oder scheitert, wird `mimeapps.list` selbst
//! geschrieben.
//!
//! # Sandbox
//!
//! In der Sandbox zeigt `XDG_CONFIG_HOME` auf das app-eigene Konfigurationsverzeichnis,
//! nicht auf das des Nutzers. Gelesen wird dort deshalb `$HOME/.config`, dieselbe
//! Unterscheidung wie bei den Suchpfaden (ADR-9).

use std::fmt;
use std::path::PathBuf;

use log::{debug, warn};

use crate::desktop::parse_ini;
use crate::discovery::in_flatpak_sandbox;
use crate::host;

/// Die Typen, die darüber entscheiden, ob Gatekeeper der Standardbrowser ist.
///
/// Bewusst nur die beiden Schema-Handler: Sie sind die eindeutige Aussage. Wer `text/html`
/// besitzt, ist damit noch nicht der Browser, der Links öffnet.
pub const WEB_HANDLER_TYPES: &[&str] = &["x-scheme-handler/http", "x-scheme-handler/https"];

/// Die Typen, die beim Eintragen geschrieben werden.
///
/// Zusätzlich `text/html`, weil KDE darauf zurückfällt, wenn kein Schema-Handler
/// eingetragen ist. `xdg-settings` verfährt genauso; ohne diesen Zusatz verhielte sich der
/// Rückfallpfad anders als der Normalfall.
pub const MANAGED_TYPES: &[&str] =
    &["text/html", "x-scheme-handler/http", "x-scheme-handler/https"];

/// Wer aktuell Links öffnet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultBrowser {
    /// Gatekeeper selbst. So soll es sein.
    Ours,
    /// Ein anderer Browser.
    Other { desktop_id: String },
    /// Es ist nichts eingetragen.
    Unset,
    /// Uneinheitlich: `http` und `https` zeigen auf verschiedene Anwendungen.
    Mixed { http: String, https: String },
}

impl DefaultBrowser {
    /// Ist Gatekeeper für alle Web-Typen zuständig?
    pub fn is_ours(&self) -> bool {
        matches!(self, Self::Ours)
    }
}

/// Die Teile der Umgebung, aus denen sich die Konfigurationspfade ergeben.
#[derive(Debug, Clone, Default)]
pub struct ConfigEnvironment {
    pub sandboxed: bool,
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_config_dirs: Option<String>,
    /// Inhalt von `XDG_CURRENT_DESKTOP`, für die desktopspezifischen Dateien.
    pub current_desktops: Vec<String>,
}

impl ConfigEnvironment {
    pub fn from_env() -> Self {
        Self {
            sandboxed: in_flatpak_sandbox(),
            home: std::env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            xdg_config_dirs: std::env::var("XDG_CONFIG_DIRS").ok(),
            current_desktops: std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// Das Verzeichnis, in das ein eigener Eintrag geschrieben wird.
    pub fn user_config_dir(&self) -> Option<PathBuf> {
        if self.sandboxed {
            // In der Sandbox zeigt XDG_CONFIG_HOME auf ~/.var/app/<id>/config. Der
            // Standardbrowser steht aber in der Konfiguration des Nutzers.
            return self.home.as_ref().map(|home| home.join(".config"));
        }
        self.xdg_config_home.clone().or_else(|| self.home.as_ref().map(|home| home.join(".config")))
    }

    /// Alle `mimeapps.list`-Dateien in der Reihenfolge, in der die Spec sie befragt.
    ///
    /// Desktopspezifische Dateien schlagen die allgemeinen, Nutzer schlägt System.
    pub fn mimeapps_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let add_dir = |dir: &PathBuf, files: &mut Vec<PathBuf>| {
            for desktop in &self.current_desktops {
                files.push(dir.join(format!("{}-mimeapps.list", desktop.to_lowercase())));
            }
            files.push(dir.join("mimeapps.list"));
        };

        if let Some(dir) = self.user_config_dir() {
            add_dir(&dir, &mut files);
        }

        let config_dirs = self.xdg_config_dirs.clone().unwrap_or_else(|| "/etc/xdg".to_string());
        for dir in std::env::split_paths(&config_dirs) {
            // In der Sandbox gehören diese Verzeichnisse der Runtime. Was dort steht, sagt
            // nichts über den Host aus.
            if self.sandboxed && !dir.starts_with("/run/host") {
                continue;
            }
            add_dir(&dir, &mut files);
        }

        files
    }
}

/// Ermittelt, wer aktuell Links öffnet.
pub fn current(env: &ConfigEnvironment) -> DefaultBrowser {
    let http = lookup(env, "x-scheme-handler/http");
    let https = lookup(env, "x-scheme-handler/https");

    match (http, https) {
        (None, None) => DefaultBrowser::Unset,
        (Some(a), Some(b)) if a == b => classify(a),
        // Nur einer gesetzt heisst: der andere fällt auf irgendetwas zurück. Das ist
        // nicht „unser Zustand", also wird es wie ein fremder Eintrag behandelt.
        (Some(a), None) => DefaultBrowser::Mixed { http: a, https: String::new() },
        (None, Some(b)) => DefaultBrowser::Mixed { http: String::new(), https: b },
        (Some(a), Some(b)) => DefaultBrowser::Mixed { http: a, https: b },
    }
}

fn classify(desktop_id: String) -> DefaultBrowser {
    if desktop_id == crate::SELF_DESKTOP_ID {
        DefaultBrowser::Ours
    } else {
        DefaultBrowser::Other { desktop_id }
    }
}

/// Sucht den Eintrag für einen MIME-Typ in der Reihenfolge der Spec.
///
/// Die erste Datei, die den Typ unter `[Default Applications]` nennt, entscheidet. Steht
/// dort eine Liste, gilt der erste Eintrag.
fn lookup(env: &ConfigEnvironment, mime_type: &str) -> Option<String> {
    for file in env.mimeapps_files() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let groups = parse_ini(&text);
        let Some(defaults) = groups.get("Default Applications") else {
            continue;
        };
        if let Some(first) = defaults.list(mime_type).into_iter().find(|id| !id.is_empty()) {
            debug!("{mime_type} kommt aus {}: {first}", file.display());
            return Some(first);
        }
    }
    None
}

/// Warum Gatekeeper nicht zum Standardbrowser gemacht werden konnte.
#[derive(Debug)]
pub enum SetDefaultError {
    /// Kein Konfigurationsverzeichnis ermittelbar.
    NoConfigDirectory,
    Io(std::io::Error),
    /// Der Eintrag wurde geschrieben, aber die anschliessende Prüfung fand ihn nicht.
    NotApplied,
}

impl fmt::Display for SetDefaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDirectory => write!(f, "kein Konfigurationsverzeichnis gefunden"),
            Self::Io(err) => write!(f, "Schreiben fehlgeschlagen: {err}"),
            Self::NotApplied => write!(f, "der Eintrag wurde nicht übernommen"),
        }
    }
}

impl std::error::Error for SetDefaultError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

/// Trägt Gatekeeper als Standardbrowser ein.
///
/// Zuerst über `xdg-settings`, weil das die desktopspezifischen Dateien mitpflegt. Fehlt
/// das Werkzeug oder scheitert es, wird `mimeapps.list` direkt geschrieben. Am Ende wird
/// nachgesehen, ob es tatsächlich gilt: Ein „hat geklappt", das nicht stimmt, ist
/// schlimmer als ein ehrliches Scheitern.
pub fn make_default(env: &ConfigEnvironment) -> Result<(), SetDefaultError> {
    let via_tool = host::run(&[
        "xdg-settings".to_string(),
        "set".to_string(),
        "default-web-browser".to_string(),
        crate::SELF_DESKTOP_ID.to_string(),
    ])
    .map(|output| output.status.success())
    .unwrap_or(false);

    if !via_tool {
        debug!("xdg-settings nicht verfügbar oder gescheitert, schreibe mimeapps.list selbst");
        write_mimeapps(env)?;
    }

    if current(env).is_ours() {
        return Ok(());
    }

    // xdg-settings meldete Erfolg, es gilt aber nicht. Dann eben selbst schreiben.
    if via_tool {
        warn!("xdg-settings meldete Erfolg, der Eintrag gilt aber nicht");
        write_mimeapps(env)?;
        if current(env).is_ours() {
            return Ok(());
        }
    }
    Err(SetDefaultError::NotApplied)
}

/// Schreibt die Einträge selbst in die `mimeapps.list` des Nutzers.
///
/// Bestehende Zeilen bleiben erhalten; nur die Web-Typen unter `[Default Applications]`
/// werden ersetzt. Die Datei gehört nicht uns, dort wird nicht aufgeräumt.
fn write_mimeapps(env: &ConfigEnvironment) -> Result<(), SetDefaultError> {
    let dir = env.user_config_dir().ok_or(SetDefaultError::NoConfigDirectory)?;
    std::fs::create_dir_all(&dir).map_err(SetDefaultError::Io)?;
    let path = dir.join("mimeapps.list");

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = replace_web_defaults(&existing, crate::SELF_DESKTOP_ID);

    std::fs::write(&path, updated).map_err(SetDefaultError::Io)
}

/// Ersetzt die Web-Handler unter `[Default Applications]`, ohne den Rest anzutasten.
///
/// Als reine Funktion, damit sich das Ergebnis prüfen lässt, ohne eine echte
/// Konfigurationsdatei zu überschreiben.
pub fn replace_web_defaults(existing: &str, desktop_id: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_defaults = false;
    let mut wrote_defaults = false;

    let is_managed_key = |line: &str| {
        line.split_once('=').is_some_and(|(key, _)| MANAGED_TYPES.contains(&key.trim()))
    };

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_defaults {
                // Gruppe endet: unsere Einträge kommen ans Ende der Gruppe.
                push_web_defaults(&mut out, desktop_id);
                wrote_defaults = true;
            }
            in_defaults = trimmed == "[Default Applications]";
            out.push(line.to_string());
            continue;
        }
        // Alte Web-Einträge in dieser Gruppe fallen weg, alles andere bleibt.
        if in_defaults && is_managed_key(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }

    if in_defaults {
        push_web_defaults(&mut out, desktop_id);
        wrote_defaults = true;
    }
    if !wrote_defaults {
        if !out.is_empty() && !out.last().is_some_and(|line| line.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[Default Applications]".to_string());
        push_web_defaults(&mut out, desktop_id);
    }

    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn push_web_defaults(out: &mut Vec<String>, desktop_id: &str) {
    for mime_type in MANAGED_TYPES {
        out.push(format!("{mime_type}={desktop_id}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(dir: &std::path::Path, sandboxed: bool) -> ConfigEnvironment {
        ConfigEnvironment {
            sandboxed,
            home: Some(dir.to_path_buf()),
            xdg_config_home: Some(dir.join(".config")),
            xdg_config_dirs: Some("/nirgendwo".to_string()),
            current_desktops: vec!["KDE".to_string()],
        }
    }

    fn workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gatekeeper-default-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".config")).unwrap();
        dir
    }

    fn write_mimeapps_file(dir: &std::path::Path, name: &str, content: &str) {
        std::fs::write(dir.join(".config").join(name), content).unwrap();
    }

    // ----------------------------------------------------------------------------------
    // Zustand erkennen
    // ----------------------------------------------------------------------------------

    #[test]
    fn recognizes_another_browser() {
        let dir = workspace("other");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            "[Default Applications]\n\
             x-scheme-handler/http=brave-origin.desktop\n\
             x-scheme-handler/https=brave-origin.desktop\n",
        );

        assert_eq!(
            current(&env_with(&dir, false)),
            DefaultBrowser::Other { desktop_id: "brave-origin.desktop".into() }
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recognizes_itself() {
        let dir = workspace("ours");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            &format!(
                "[Default Applications]\n\
                 x-scheme-handler/http={id}\n\
                 x-scheme-handler/https={id}\n",
                id = crate::SELF_DESKTOP_ID
            ),
        );

        assert!(current(&env_with(&dir, false)).is_ours());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_nothing_configured() {
        let dir = workspace("unset");
        assert_eq!(current(&env_with(&dir, false)), DefaultBrowser::Unset);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn notices_when_http_and_https_disagree() {
        // Kommt vor, wenn ein Browser sich nur für eines von beiden einträgt. Als „unser
        // Zustand" darf das nicht durchgehen.
        let dir = workspace("mixed");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            &format!(
                "[Default Applications]\n\
                 x-scheme-handler/http={id}\n\
                 x-scheme-handler/https=firefox.desktop\n",
                id = crate::SELF_DESKTOP_ID
            ),
        );

        let status = current(&env_with(&dir, false));
        assert!(matches!(status, DefaultBrowser::Mixed { .. }), "{status:?}");
        assert!(!status.is_ours());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_desktop_specific_file_wins_over_the_general_one() {
        let dir = workspace("desktop-specific");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            "[Default Applications]\nx-scheme-handler/http=allgemein.desktop\n\
             x-scheme-handler/https=allgemein.desktop\n",
        );
        write_mimeapps_file(
            &dir,
            "kde-mimeapps.list",
            "[Default Applications]\nx-scheme-handler/http=speziell.desktop\n\
             x-scheme-handler/https=speziell.desktop\n",
        );

        assert_eq!(
            current(&env_with(&dir, false)),
            DefaultBrowser::Other { desktop_id: "speziell.desktop".into() }
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_list_of_handlers_resolves_to_the_first() {
        let dir = workspace("list");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            "[Default Applications]\n\
             x-scheme-handler/http=erster.desktop;zweiter.desktop;\n\
             x-scheme-handler/https=erster.desktop;zweiter.desktop;\n",
        );

        assert_eq!(
            current(&env_with(&dir, false)),
            DefaultBrowser::Other { desktop_id: "erster.desktop".into() }
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn other_groups_are_not_mistaken_for_defaults() {
        // [Added Associations] listet dieselben Typen, bestimmt aber nicht den Standard.
        let dir = workspace("added");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            "[Added Associations]\nx-scheme-handler/http=irgendwas.desktop;\n",
        );

        assert_eq!(current(&env_with(&dir, false)), DefaultBrowser::Unset);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ----------------------------------------------------------------------------------
    // Sandbox
    // ----------------------------------------------------------------------------------

    #[test]
    fn in_the_sandbox_the_users_config_is_read_not_the_apps_own() {
        let dir = workspace("sandbox");
        let env = ConfigEnvironment {
            sandboxed: true,
            home: Some(dir.clone()),
            // So sieht es in der Sandbox aus: die Variable zeigt woandershin.
            xdg_config_home: Some(dir.join(".var/app/me.rueegger.Gatekeeper/config")),
            xdg_config_dirs: Some("/etc/xdg".to_string()),
            current_desktops: vec!["KDE".to_string()],
        };

        let files = env.mimeapps_files();
        assert!(files.iter().any(|f| f == &dir.join(".config/mimeapps.list")), "{files:?}");
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("/.var/app/")), "{files:?}");
        // /etc/xdg gehört in der Sandbox der Runtime und sagt nichts über den Host.
        assert!(!files.iter().any(|f| f.starts_with("/etc/xdg")), "{files:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ----------------------------------------------------------------------------------
    // Schreiben
    // ----------------------------------------------------------------------------------

    #[test]
    fn writing_into_an_empty_file_creates_the_group() {
        let result = replace_web_defaults("", "me.rueegger.Gatekeeper.desktop");

        assert!(result.contains("[Default Applications]"));
        assert!(result.contains("x-scheme-handler/http=me.rueegger.Gatekeeper.desktop"));
        assert!(result.contains("x-scheme-handler/https=me.rueegger.Gatekeeper.desktop"));
    }

    #[test]
    fn writing_also_claims_text_html_like_xdg_settings_does() {
        // KDE fällt auf text/html zurück, wenn kein Schema-Handler eingetragen ist.
        // Liesse der Rückfallpfad das aus, verhielte er sich anders als der Normalfall.
        let result = replace_web_defaults("", "me.rueegger.Gatekeeper.desktop");
        assert!(result.contains("text/html=me.rueegger.Gatekeeper.desktop"), "{result}");
    }

    #[test]
    fn text_html_alone_does_not_make_us_the_default() {
        let dir = workspace("html-only");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            &format!("[Default Applications]\ntext/html={}\n", crate::SELF_DESKTOP_ID),
        );

        assert_eq!(current(&env_with(&dir, false)), DefaultBrowser::Unset);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writing_replaces_only_the_web_handlers() {
        let existing = "[Added Associations]\n\
                        x-scheme-handler/http=brave-origin.desktop;\n\
                        \n\
                        [Default Applications]\n\
                        text/html=brave-origin.desktop\n\
                        x-scheme-handler/http=brave-origin.desktop\n\
                        x-scheme-handler/https=brave-origin.desktop\n\
                        x-scheme-handler/jetbrains=jetbrainsd.desktop\n\
                        application/pdf=okular.desktop\n";
        let result = replace_web_defaults(existing, "me.rueegger.Gatekeeper.desktop");

        // Fremde Einträge bleiben unangetastet, auch in anderen Gruppen.
        assert!(result.contains("x-scheme-handler/jetbrains=jetbrainsd.desktop"));
        assert!(result.contains("application/pdf=okular.desktop"));
        assert!(result.contains("[Added Associations]"));
        // Der alte Web-Handler ist genau einmal ersetzt, nicht zusätzlich eingefügt.
        assert_eq!(result.matches("x-scheme-handler/http=").count(), 2, "{result}");
        assert!(!result.contains("x-scheme-handler/http=brave-origin.desktop\n"));
        assert!(result.contains("x-scheme-handler/http=me.rueegger.Gatekeeper.desktop"));
    }

    #[test]
    fn writing_appends_a_group_when_there_is_none() {
        let existing = "[Added Associations]\nimage/png=gwenview.desktop\n";
        let result = replace_web_defaults(existing, "me.rueegger.Gatekeeper.desktop");

        assert!(result.contains("image/png=gwenview.desktop"));
        assert!(result.contains("[Default Applications]"));
        assert_eq!(result.matches("[Default Applications]").count(), 1, "{result}");
    }

    #[test]
    fn writing_is_idempotent() {
        let once = replace_web_defaults("", "me.rueegger.Gatekeeper.desktop");
        let twice = replace_web_defaults(&once, "me.rueegger.Gatekeeper.desktop");

        assert_eq!(once, twice);
    }

    #[test]
    fn writing_then_reading_agrees() {
        let dir = workspace("roundtrip");
        write_mimeapps_file(
            &dir,
            "mimeapps.list",
            "[Default Applications]\nx-scheme-handler/http=brave-origin.desktop\n\
             x-scheme-handler/https=brave-origin.desktop\n",
        );
        let env = env_with(&dir, false);

        let path = dir.join(".config/mimeapps.list");
        let existing = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, replace_web_defaults(&existing, crate::SELF_DESKTOP_ID)).unwrap();

        assert!(current(&env).is_ours(), "geschrieben, aber nicht wiedergefunden");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
