//! Parser für `.desktop`-Dateien nach der freedesktop.org Desktop Entry Specification.
//!
//! Bewusst tolerant: Auf echten Systemen liegt in den Application-Verzeichnissen immer
//! irgendwo Müll. Eine kaputte Datei wird übersprungen und protokolliert, sie darf nie den
//! ganzen Scan abbrechen.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::locale::Locale;

/// Warum eine Datei nicht als Desktop-Entry gelesen werden konnte.
#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    /// Desktop-Dateien sind nach Spec UTF-8. Alles andere ist beschädigt.
    NotUtf8,
    /// Keine `[Desktop Entry]`-Gruppe gefunden.
    NoEntryGroup,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "nicht lesbar: {err}"),
            Self::NotUtf8 => write!(f, "kein gültiges UTF-8"),
            Self::NoEntryGroup => write!(f, "keine [Desktop Entry]-Gruppe"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Eine Gruppe einer Desktop-Datei — `[Desktop Entry]` oder `[Desktop Action …]`.
///
/// Werte liegen unverändert so vor, wie sie in der Datei stehen. Entschärft wird erst beim
/// Auslesen, weil `Exec` eigene Quoting-Regeln hat und die allgemeine String-Entschärfung
/// dort schaden würde.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    values: BTreeMap<String, String>,
    localized: BTreeMap<(String, String), String>,
}

impl Group {
    /// Rohwert ohne jede Entschärfung. Für `Exec` und `TryExec` zu verwenden.
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Wert als Zeichenkette, mit aufgelösten `\s \n \t \r \\`-Sequenzen.
    pub fn string(&self, key: &str) -> Option<String> {
        self.values.get(key).map(|value| unescape(value))
    }

    /// Wert in der passendsten verfügbaren Übersetzung, sonst unlokalisiert.
    pub fn localized(&self, key: &str, locale: Option<&Locale>) -> Option<String> {
        if let Some(locale) = locale {
            for suffix in locale.candidates() {
                if let Some(value) = self.localized.get(&(key.to_string(), suffix)) {
                    return Some(unescape(value));
                }
            }
        }
        self.string(key)
    }

    /// Wahrheitswert. Nach Spec sind nur exakt `true` und `false` gültig.
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.values.get(key)?.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Semikolon-getrennte Liste. `\;` maskiert ein Semikolon im Wert.
    ///
    /// Das abschliessende Semikolon ist nach Spec Teil der Syntax und erzeugt kein
    /// leeres letztes Element.
    pub fn list(&self, key: &str) -> Vec<String> {
        let Some(raw) = self.values.get(key) else {
            return Vec::new();
        };

        let mut items = Vec::new();
        let mut current = String::new();
        let mut chars = raw.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some(';') => current.push(';'),
                    Some('\\') => current.push('\\'),
                    Some('s') => current.push(' '),
                    Some('n') => current.push('\n'),
                    Some('t') => current.push('\t'),
                    Some('r') => current.push('\r'),
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                },
                ';' => items.push(std::mem::take(&mut current)),
                other => current.push(other),
            }
        }
        if !current.is_empty() {
            items.push(current);
        }
        items
    }

    fn insert(&mut self, key: &str, locale: Option<&str>, value: &str) {
        match locale {
            // Nach Spec darf ein Schlüssel je Gruppe nur einmal vorkommen. Kommt er doch
            // mehrfach, gewinnt der erste — spätere werden verworfen, nicht überschrieben.
            None => {
                self.values.entry(key.to_string()).or_insert_with(|| value.to_string());
            }
            Some(locale) => {
                self.localized
                    .entry((key.to_string(), locale.to_string()))
                    .or_insert_with(|| value.to_string());
            }
        }
    }
}

/// Eine geparste `.desktop`-Datei.
#[derive(Debug, Clone)]
pub struct DesktopFile {
    /// Desktop-ID nach Spec: Pfad relativ zum `applications`-Verzeichnis, `/` durch `-`
    /// ersetzt. Für flach abgelegte Dateien also schlicht der Dateiname.
    pub id: String,
    pub path: PathBuf,
    pub entry: Group,
    /// Zusatzaktionen aus `[Desktop Action …]`, in der Reihenfolge des `Actions`-Schlüssels
    /// abrufbar.
    pub actions: BTreeMap<String, Group>,
}

impl DesktopFile {
    /// Liest und parst eine Datei. Die Desktop-ID wird aus dem Dateinamen gebildet.
    pub fn parse_file(path: &Path) -> Result<Self, ParseError> {
        let bytes = std::fs::read(path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| ParseError::NotUtf8)?;
        let id = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::parse_str(text, id, path.to_path_buf())
    }

    /// Parst den Inhalt einer Desktop-Datei mit vorgegebener ID und Herkunft.
    pub fn parse_str(text: &str, id: String, path: PathBuf) -> Result<Self, ParseError> {
        let mut entry: Option<Group> = None;
        let mut actions: BTreeMap<String, Group> = BTreeMap::new();
        let mut current: Option<(String, Group)> = None;

        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}');
            let trimmed = line.trim();

            // Leerzeilen und Kommentare. Kommentare stehen auch mitten in Gruppen —
            // brave-origin.desktop tut das.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if let Some((name, group)) = current.take() {
                    store_group(&mut entry, &mut actions, name, group);
                }
                current = Some((name.to_string(), Group::default()));
                continue;
            }

            // Werte ausserhalb jeder Gruppe sind ungültig und werden verworfen.
            let Some((_, group)) = current.as_mut() else {
                continue;
            };
            let Some((key_part, value)) = trimmed.split_once('=') else {
                continue;
            };

            let key_part = key_part.trim_end();
            let value = value.trim_start();
            match split_locale(key_part) {
                Some((key, locale)) => group.insert(key, Some(locale), value),
                None => group.insert(key_part, None, value),
            }
        }

        if let Some((name, group)) = current.take() {
            store_group(&mut entry, &mut actions, name, group);
        }

        Ok(Self { id, path, entry: entry.ok_or(ParseError::NoEntryGroup)?, actions })
    }

    /// Die in `Actions=` gelisteten Aktionen, in deklarierter Reihenfolge.
    ///
    /// Genannte, aber nicht definierte Aktionen werden übersprungen.
    pub fn declared_actions(&self) -> Vec<(&str, &Group)> {
        self.entry
            .list("Actions")
            .into_iter()
            .filter_map(|name| {
                let (key, group) = self.actions.get_key_value(&name)?;
                Some((key.as_str(), group))
            })
            .collect()
    }
}

fn store_group(
    entry: &mut Option<Group>,
    actions: &mut BTreeMap<String, Group>,
    name: String,
    group: Group,
) {
    if name == "Desktop Entry" {
        // Bei einer zweiten [Desktop Entry]-Gruppe gewinnt die erste.
        entry.get_or_insert(group);
    } else if let Some(action) = name.strip_prefix("Desktop Action ") {
        actions.entry(action.trim().to_string()).or_insert(group);
    }
}

/// Trennt `Name[de_CH]` in `("Name", "de_CH")`.
fn split_locale(key_part: &str) -> Option<(&str, &str)> {
    let rest = key_part.strip_suffix(']')?;
    let (key, locale) = rest.split_once('[')?;
    if key.is_empty() || locale.is_empty() {
        return None;
    }
    Some((key, locale))
}

/// Löst die Escape-Sequenzen für Werte vom Typ „string" auf.
///
/// Nicht auf `Exec` anwenden — dort gelten die eigenen Quoting-Regeln aus [`crate::exec`].
fn unescape(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // Unbekannte Sequenz unverändert stehen lassen statt Information zu verlieren.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
