//! Locale-Auswahl für lokalisierte Desktop-Entry-Schlüssel.
//!
//! Desktop-Dateien tragen Übersetzungen als Schlüsselsuffix: `Name[de_CH]`, `Name[de]`,
//! `Name[sr@latin]`. Welche Variante gewinnt, regelt die Desktop Entry Spec, Abschnitt
//! „Localized values for keys".

/// Eine Locale in ihre Bestandteile zerlegt.
///
/// Die Kodierung (`.UTF-8`) wird nach Spec beim Vergleich ignoriert und deshalb gar nicht
/// erst gespeichert.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Locale {
    lang: String,
    country: Option<String>,
    modifier: Option<String>,
}

impl Locale {
    /// Zerlegt eine Locale der Form `lang_COUNTRY.ENCODING@MODIFIER`.
    ///
    /// Alle Teile ausser `lang` sind optional. `ENCODING` wird verworfen.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() || raw == "C" || raw == "POSIX" {
            return None;
        }

        let (head, modifier) = match raw.split_once('@') {
            Some((h, m)) => (h, Some(m.to_string())),
            None => (raw, None),
        };
        // Kodierung interessiert beim Abgleich nicht.
        let head = head.split_once('.').map_or(head, |(h, _)| h);
        let (lang, country) = match head.split_once('_') {
            Some((l, c)) => (l, Some(c.to_string())),
            None => (head, None),
        };

        if lang.is_empty() {
            return None;
        }
        Some(Self { lang: lang.to_string(), country, modifier })
    }

    /// Liest die Locale aus der Umgebung: `LC_ALL`, dann `LC_MESSAGES`, dann `LANG`.
    pub fn from_env() -> Option<Self> {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|var| std::env::var(var).ok())
            .find_map(|value| Self::parse(&value))
    }

    /// Die Suffixe, unter denen ein Wert gesucht wird — in absteigender Genauigkeit.
    ///
    /// Nach Spec: `lang_COUNTRY@MODIFIER`, `lang_COUNTRY`, `lang@MODIFIER`, `lang`.
    /// Der unlokalisierte Wert ist nicht Teil der Liste; er ist der Fallback danach.
    pub fn candidates(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(4);
        if let (Some(country), Some(modifier)) = (&self.country, &self.modifier) {
            out.push(format!("{}_{}@{}", self.lang, country, modifier));
        }
        if let Some(country) = &self.country {
            out.push(format!("{}_{}", self.lang, country));
        }
        if let Some(modifier) = &self.modifier {
            out.push(format!("{}@{}", self.lang, modifier));
        }
        out.push(self.lang.clone());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_locale() {
        let locale = Locale::parse("de_CH.UTF-8@euro").unwrap();
        assert_eq!(locale.lang, "de");
        assert_eq!(locale.country.as_deref(), Some("CH"));
        assert_eq!(locale.modifier.as_deref(), Some("euro"));
    }

    #[test]
    fn drops_encoding() {
        assert_eq!(Locale::parse("de_CH.UTF-8"), Locale::parse("de_CH"));
    }

    #[test]
    fn c_and_posix_are_not_locales() {
        assert!(Locale::parse("C").is_none());
        assert!(Locale::parse("POSIX").is_none());
        assert!(Locale::parse("").is_none());
    }

    #[test]
    fn candidates_are_ordered_most_specific_first() {
        let locale = Locale::parse("de_CH@euro").unwrap();
        assert_eq!(locale.candidates(), ["de_CH@euro", "de_CH", "de@euro", "de"]);
    }

    #[test]
    fn candidates_without_country_or_modifier() {
        assert_eq!(Locale::parse("de").unwrap().candidates(), ["de"]);
        assert_eq!(Locale::parse("de_CH").unwrap().candidates(), ["de_CH", "de"]);
    }
}
