//! Regeln, die eine URL ohne Rückfrage einem Browser zuordnen.
//!
//! Trifft eine Regel, wird der Browser sofort gestartet und die Oberfläche gar nicht erst
//! aufgebaut. Das ist nicht nur bequem, sondern der schnelle Pfad: ohne Qt liegt der Start
//! im zweistelligen Millisekundenbereich.
//!
//! Die erste passende Regel gewinnt. Reihenfolge ist damit Bedeutung, und die Datei bleibt
//! von oben nach unten lesbar.
//!
//! # Wo die Datei liegt
//!
//! In `$XDG_CONFIG_HOME/gatekeeper/rules.toml`. Anders als bei `mimeapps.list` ist das
//! auch in der Sandbox richtig: Dies ist unsere eigene Konfiguration, und die gehört in
//! das app-eigene Verzeichnis, wo sie Updates übersteht.

use std::fmt;
use std::path::PathBuf;

use log::{debug, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::uri::TargetUri;

/// Eine einzelne Regel.
///
/// Genau eines der Felder `host`, `url` oder `scheme` muss gesetzt sein. Sie ausdrücklich
/// zu benennen ist klarer, als die Art des Musters aus dessen Schreibweise zu erraten.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Hostname. `example.com` trifft die Domain und alle Subdomains,
    /// `*.example.com` nur die Subdomains.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Regulärer Ausdruck auf die vollständige URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Schema, etwa `file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,

    /// Desktop-ID des Browsers, der starten soll.
    pub browser: String,

    /// Optionale Desktop-Action, etwa `new-private-window`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Der Inhalt von `rules.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// Warum eine Regeldatei nicht gelesen werden konnte.
#[derive(Debug)]
pub enum RulesError {
    Io(std::io::Error),
    Syntax(toml::de::Error),
    Serialize(toml::ser::Error),
    NoConfigDirectory,
}

impl fmt::Display for RulesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "nicht lesbar: {err}"),
            Self::Syntax(err) => write!(f, "fehlerhafte Regeldatei: {err}"),
            Self::Serialize(err) => write!(f, "Regeln nicht schreibbar: {err}"),
            Self::NoConfigDirectory => write!(f, "kein Konfigurationsverzeichnis gefunden"),
        }
    }
}

impl std::error::Error for RulesError {}

impl RuleSet {
    /// Liest die Regeln aus einer Zeichenkette.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        toml::from_str(text).map_err(RulesError::Syntax)
    }

    /// Schreibt die Regeln als TOML.
    pub fn to_toml(&self) -> Result<String, RulesError> {
        toml::to_string_pretty(self).map_err(RulesError::Serialize)
    }

    /// Liest die Regeln aus der Datei des Nutzers.
    ///
    /// Fehlt die Datei, gibt es schlicht keine Regeln. Ist sie fehlerhaft, wird das
    /// protokolliert und ebenfalls mit leerer Menge weitergemacht: Eine kaputte
    /// Regeldatei darf nicht dazu führen, dass gar kein Link mehr aufgeht.
    pub fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::parse(&text) {
                Ok(set) => {
                    debug!("{} Regeln aus {}", set.rules.len(), path.display());
                    set
                }
                Err(err) => {
                    warn!("{} wird ignoriert: {err}", path.display());
                    Self::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                warn!("{} nicht lesbar: {err}", path.display());
                Self::default()
            }
        }
    }

    /// Sucht die erste Regel, die auf `uri` passt.
    pub fn first_match(&self, uri: &TargetUri) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.matches(uri))
    }

    /// Hängt eine Regel an, die diesen Host künftig diesem Browser zuordnet.
    ///
    /// Angehängt, nicht vorangestellt: Die erste passende Regel gewinnt, und was jemand
    /// von Hand geschrieben hat, soll nicht von einem Klick überstimmt werden.
    ///
    /// Gibt es bereits eine Regel für genau diesen Host, wird deren Browser geändert,
    /// statt eine zweite anzuhängen, die nie zum Zug käme.
    pub fn remember_host(&mut self, host: &str, browser: &str) {
        let host = host.trim();
        if let Some(existing) =
            self.rules.iter_mut().find(|rule| rule.host.as_deref() == Some(host))
        {
            existing.browser = browser.to_string();
            existing.action = None;
            return;
        }
        self.rules.push(Rule {
            host: Some(host.to_string()),
            browser: browser.to_string(),
            ..Rule::default()
        });
    }

    /// Speichert die Regeln, das Verzeichnis wird bei Bedarf angelegt.
    pub fn save(&self, path: &std::path::Path) -> Result<(), RulesError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(RulesError::Io)?;
        }
        std::fs::write(path, self.to_toml()?).map_err(RulesError::Io)
    }
}

impl Rule {
    /// Passt diese Regel auf die URL?
    pub fn matches(&self, uri: &TargetUri) -> bool {
        // Sind mehrere Muster gesetzt, müssen alle passen. Das ist die konservative
        // Auslegung: Eine Regel, die enger gemeint war, greift nicht zu breit.
        let mut checked_anything = false;

        if let Some(pattern) = &self.host {
            checked_anything = true;
            if !matches_host(pattern, uri.host()) {
                return false;
            }
        }
        if let Some(pattern) = &self.scheme {
            checked_anything = true;
            if !pattern.eq_ignore_ascii_case(uri.scheme()) {
                return false;
            }
        }
        if let Some(pattern) = &self.url {
            checked_anything = true;
            match Regex::new(pattern) {
                Ok(regex) => {
                    if !regex.is_match(uri.as_str()) {
                        return false;
                    }
                }
                Err(err) => {
                    // Ein fehlerhafter Ausdruck trifft nichts, statt alles zu treffen.
                    warn!("Regel mit ungültigem Ausdruck '{pattern}' wird übersprungen: {err}");
                    return false;
                }
            }
        }

        // Eine Regel ohne jedes Muster würde jede URL abfangen. Das ist mit Sicherheit
        // nicht gemeint, sondern ein Tippfehler.
        checked_anything
    }
}

/// Vergleicht einen Hostnamen mit einem Muster.
///
/// `example.com` trifft die Domain selbst und jede Subdomain. `*.example.com` trifft nur
/// die Subdomains. Der Vergleich ist unabhängig von Gross- und Kleinschreibung, weil
/// Hostnamen das auch sind.
fn matches_host(pattern: &str, host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let pattern = pattern.trim().to_ascii_lowercase();

    if let Some(domain) = pattern.strip_prefix("*.") {
        return host.ends_with(&format!(".{domain}"));
    }
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

/// Der Pfad der Regeldatei.
///
/// `$XDG_CONFIG_HOME/gatekeeper/rules.toml`, mit dem üblichen Rückfall auf
/// `$HOME/.config`. In der Sandbox ist die Variable hier genau richtig, anders als bei
/// `mimeapps.list`: Dies ist unsere eigene Konfiguration.
pub fn rules_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("gatekeeper").join("rules.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(raw: &str) -> TargetUri {
        TargetUri::parse(raw).unwrap()
    }

    fn host_rule(host: &str, browser: &str) -> Rule {
        Rule { host: Some(host.to_string()), browser: browser.to_string(), ..Rule::default() }
    }

    // ----------------------------------------------------------------------------------
    // Hostnamen
    // ----------------------------------------------------------------------------------

    #[test]
    fn a_plain_domain_covers_its_subdomains() {
        // Wer "github.com" schreibt, meint auch www.github.com.
        let rule = host_rule("github.com", "firefox.desktop");

        assert!(rule.matches(&uri("https://github.com/a")));
        assert!(rule.matches(&uri("https://www.github.com/a")));
        assert!(rule.matches(&uri("https://gist.github.com/a")));
    }

    #[test]
    fn a_domain_does_not_match_a_similar_one() {
        let rule = host_rule("github.com", "firefox.desktop");

        assert!(!rule.matches(&uri("https://notgithub.com/a")));
        assert!(!rule.matches(&uri("https://github.com.evil.example/a")));
    }

    #[test]
    fn a_wildcard_covers_only_subdomains() {
        let rule = host_rule("*.example.com", "firefox.desktop");

        assert!(rule.matches(&uri("https://intern.example.com/a")));
        assert!(!rule.matches(&uri("https://example.com/a")));
    }

    #[test]
    fn host_comparison_ignores_case() {
        let rule = host_rule("GitHub.com", "firefox.desktop");
        assert!(rule.matches(&uri("https://GITHUB.COM/a")));
    }

    #[test]
    fn a_host_rule_never_matches_a_url_without_host() {
        let rule = host_rule("example.com", "firefox.desktop");
        assert!(!rule.matches(&uri("file:///home/user/seite.html")));
    }

    // ----------------------------------------------------------------------------------
    // Schema und Ausdruck
    // ----------------------------------------------------------------------------------

    #[test]
    fn a_scheme_rule_catches_local_files() {
        let rule = Rule {
            scheme: Some("file".to_string()),
            browser: "firefox.desktop".to_string(),
            ..Rule::default()
        };

        assert!(rule.matches(&uri("file:///home/user/seite.html")));
        assert!(!rule.matches(&uri("https://example.com")));
    }

    #[test]
    fn a_regex_rule_matches_the_whole_url() {
        let rule = Rule {
            url: Some(r"^https://docs\.".to_string()),
            browser: "firefox.desktop".to_string(),
            ..Rule::default()
        };

        assert!(rule.matches(&uri("https://docs.example.com/a")));
        assert!(!rule.matches(&uri("https://www.example.com/docs")));
    }

    #[test]
    fn an_invalid_regex_matches_nothing_rather_than_everything() {
        let rule = Rule {
            url: Some("(".to_string()),
            browser: "firefox.desktop".to_string(),
            ..Rule::default()
        };

        assert!(!rule.matches(&uri("https://example.com")));
    }

    #[test]
    fn several_patterns_must_all_hold() {
        let rule = Rule {
            host: Some("example.com".to_string()),
            scheme: Some("https".to_string()),
            browser: "firefox.desktop".to_string(),
            ..Rule::default()
        };

        assert!(rule.matches(&uri("https://example.com/a")));
        assert!(!rule.matches(&uri("http://example.com/a")));
    }

    #[test]
    fn a_rule_without_any_pattern_catches_nothing() {
        // Sonst wäre ein Tippfehler eine Regel, die jede URL abfängt.
        let rule = Rule { browser: "firefox.desktop".to_string(), ..Rule::default() };

        assert!(!rule.matches(&uri("https://example.com")));
        assert!(!rule.matches(&uri("file:///tmp/a.html")));
    }

    // ----------------------------------------------------------------------------------
    // Reihenfolge und Datei
    // ----------------------------------------------------------------------------------

    #[test]
    fn the_first_matching_rule_wins() {
        let set = RuleSet {
            rules: vec![
                host_rule("example.com", "erster.desktop"),
                host_rule("example.com", "zweiter.desktop"),
            ],
        };

        assert_eq!(set.first_match(&uri("https://example.com")).unwrap().browser, "erster.desktop");
    }

    #[test]
    fn no_match_means_ask() {
        let set = RuleSet { rules: vec![host_rule("example.com", "firefox.desktop")] };
        assert!(set.first_match(&uri("https://andere.example")).is_none());
    }

    #[test]
    fn reads_the_documented_file_format() {
        let text = r#"
            [[rule]]
            host = "github.com"
            browser = "firefox.desktop"

            [[rule]]
            host = "*.intranet.example"
            browser = "chromium.desktop"
            action = "new-private-window"

            [[rule]]
            url = "^https://docs\\."
            browser = "firefox.desktop"

            [[rule]]
            scheme = "file"
            browser = "firefox.desktop"
        "#;
        let set = RuleSet::parse(text).unwrap();

        assert_eq!(set.rules.len(), 4);
        assert_eq!(set.rules[1].action.as_deref(), Some("new-private-window"));
        assert_eq!(
            set.first_match(&uri("https://gist.github.com/a")).unwrap().browser,
            "firefox.desktop"
        );
        assert_eq!(
            set.first_match(&uri("https://intern.intranet.example/a")).unwrap().browser,
            "chromium.desktop"
        );
    }

    #[test]
    fn an_unknown_key_is_an_error_rather_than_silently_ignored() {
        // Ein Tippfehler im Schlüssel soll auffallen, nicht wirkungslos verpuffen.
        let text = "[[rule]]\nhostname = \"example.com\"\nbrowser = \"firefox.desktop\"\n";
        assert!(RuleSet::parse(text).is_err());
    }

    #[test]
    fn a_broken_file_yields_no_rules_instead_of_no_browser() {
        let dir = std::env::temp_dir().join("gatekeeper-rules-broken");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rules.toml");
        std::fs::write(&path, "das ist [[kein gültiges TOML").unwrap();

        assert!(RuleSet::load(&path).rules.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let path = std::env::temp_dir().join("gatekeeper-rules-gibtsnicht/rules.toml");
        assert!(RuleSet::load(&path).rules.is_empty());
    }

    // ----------------------------------------------------------------------------------
    // Merken
    // ----------------------------------------------------------------------------------

    #[test]
    fn a_remembered_rule_goes_to_the_end() {
        // Eine von Hand geschriebene Regel darf nicht von einem Klick überstimmt werden.
        let mut set =
            RuleSet { rules: vec![host_rule("*.example.com", "handgeschrieben.desktop")] };
        set.remember_host("example.com", "geklickt.desktop");

        assert_eq!(set.rules.len(), 2);
        assert_eq!(set.rules[0].browser, "handgeschrieben.desktop");
        assert_eq!(set.rules[1].browser, "geklickt.desktop");
        // Die vorhandene, engere Regel greift weiterhin zuerst.
        assert_eq!(
            set.first_match(&uri("https://intern.example.com/a")).unwrap().browser,
            "handgeschrieben.desktop"
        );
    }

    #[test]
    fn remembering_the_same_host_twice_updates_instead_of_piling_up() {
        let mut set = RuleSet::default();
        set.remember_host("example.com", "erster.desktop");
        set.remember_host("example.com", "zweiter.desktop");

        assert_eq!(set.rules.len(), 1, "eine zweite Regel käme nie zum Zug");
        assert_eq!(set.rules[0].browser, "zweiter.desktop");
    }

    #[test]
    fn a_remembered_rule_survives_a_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join("gatekeeper-rules-remember");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("gatekeeper").join("rules.toml");

        let mut set = RuleSet::load(&path);
        set.remember_host("github.com", "firefox.desktop");
        set.save(&path).unwrap();

        let reloaded = RuleSet::load(&path);
        assert_eq!(
            reloaded.first_match(&uri("https://gist.github.com/x")).unwrap().browser,
            "firefox.desktop"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writing_and_reading_agree() {
        let set = RuleSet {
            rules: vec![
                host_rule("github.com", "firefox.desktop"),
                Rule {
                    host: Some("*.intranet.example".to_string()),
                    browser: "chromium.desktop".to_string(),
                    action: Some("new-private-window".to_string()),
                    ..Rule::default()
                },
            ],
        };

        let text = set.to_toml().unwrap();
        assert_eq!(RuleSet::parse(&text).unwrap(), set);
        // Ungesetzte Felder tauchen nicht als leere Einträge auf.
        assert!(!text.contains("url ="), "{text}");
        assert!(!text.contains("scheme ="), "{text}");
    }
}
