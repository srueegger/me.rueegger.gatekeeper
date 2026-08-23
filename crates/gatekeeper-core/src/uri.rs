//! Prüfung der übergebenen URL, bevor sie irgendwo als Argument landet.
//!
//! Die URL kommt von aussen: aus einer E-Mail, einem Chat, einem PDF. Sie wird geprüft,
//! bevor sie in ein `argv` gelangt, nicht danach.
//!
//! Der wichtigste Fall: eine Zeichenkette, die mit `-` beginnt, würde vom Zielbrowser als
//! Schalter gelesen. Bei Chromium-Abkömmlingen ist `--gpu-launcher=…` gleichbedeutend mit
//! Codeausführung. Ein gültiges URI kann nie so beginnen, weil ein Schema mit einem
//! Buchstaben anfangen muss. Die Schemaprüfung deckt das also mit ab. Ein zusätzlicher
//! expliziter Test hält das bewusst fest, statt sich auf diesen Nebeneffekt zu verlassen.

use std::fmt;

use url::Url;

/// Schemata, die Gatekeeper an einen Browser weitergibt.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "ftp", "file"];

/// Warum eine Eingabe nicht als Ziel akzeptiert wurde.
#[derive(Debug, PartialEq, Eq)]
pub enum UriError {
    Empty,
    /// Nicht als URI lesbar.
    Malformed,
    /// Syntaktisch gültig, aber kein Schema, das an einen Browser gehört.
    UnsupportedScheme(String),
    /// Beginnt mit `-` und würde als Schalter gelesen.
    LooksLikeFlag,
}

impl fmt::Display for UriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "leere Eingabe"),
            Self::Malformed => write!(f, "kein gültiges URI"),
            Self::UnsupportedScheme(scheme) => write!(f, "Schema '{scheme}' wird nicht geöffnet"),
            Self::LooksLikeFlag => write!(f, "beginnt mit '-' und wäre ein Schalter"),
        }
    }
}

impl std::error::Error for UriError {}

/// Ein geprüftes Ziel, das an einen Browser übergeben werden darf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetUri {
    parsed: Url,
}

impl TargetUri {
    /// Prüft eine Eingabe und nimmt sie an, wenn sie gefahrlos weitergereicht werden kann.
    pub fn parse(input: &str) -> Result<Self, UriError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(UriError::Empty);
        }
        if trimmed.starts_with('-') {
            return Err(UriError::LooksLikeFlag);
        }

        // Ein absoluter Pfad ist kein URI, kommt aber vor: Dateimanager und
        // Kommandozeile übergeben lokale Dateien oft so. Ein Pfad ist eindeutig als
        // solcher erkennbar und kann nie ein Schalter sein, weil er mit '/' beginnt.
        if trimmed.starts_with('/') {
            return Url::from_file_path(trimmed)
                .map(|parsed| Self { parsed })
                .map_err(|()| UriError::Malformed);
        }

        let parsed = Url::parse(trimmed).map_err(|_| UriError::Malformed)?;
        let scheme = parsed.scheme().to_ascii_lowercase();
        if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
            return Err(UriError::UnsupportedScheme(scheme));
        }

        Ok(Self { parsed })
    }

    /// Die URL in der Form, die an den Browser geht.
    pub fn as_str(&self) -> &str {
        self.parsed.as_str()
    }

    pub fn scheme(&self) -> &str {
        self.parsed.scheme()
    }

    /// Der Host, sofern das Schema einen hat. `file:`-URLs haben keinen.
    pub fn host(&self) -> Option<&str> {
        self.parsed.host_str()
    }

    /// Der Teil, der in der Oberfläche hervorgehoben wird.
    ///
    /// Ein führendes `www.` wird entfernt, weil es keine Information trägt und den
    /// eigentlich interessanten Teil nach rechts schiebt.
    pub fn display_host(&self) -> Option<&str> {
        self.host().map(|host| host.strip_prefix("www.").unwrap_or(host))
    }
}

impl fmt::Display for TargetUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_web_urls() {
        for input in ["https://example.com", "http://example.com/a?b=c#d", "ftp://example.com"] {
            assert!(TargetUri::parse(input).is_ok(), "{input} sollte akzeptiert werden");
        }
    }

    #[test]
    fn rejects_anything_that_would_read_as_a_flag() {
        for input in ["--gpu-launcher=/bin/sh", "-remote", "--", "-"] {
            assert_eq!(
                TargetUri::parse(input),
                Err(UriError::LooksLikeFlag),
                "{input} darf nie an einen Browser gehen"
            );
        }
    }

    #[test]
    fn rejects_schemes_that_do_not_belong_in_a_browser() {
        assert_eq!(
            TargetUri::parse("javascript:alert(1)"),
            Err(UriError::UnsupportedScheme("javascript".into()))
        );
        assert_eq!(
            TargetUri::parse("data:text/html,<script>x</script>"),
            Err(UriError::UnsupportedScheme("data".into()))
        );
    }

    #[test]
    fn accepts_an_absolute_path_as_a_local_file() {
        // Dateimanager und Kommandozeile übergeben lokale Dateien oft als blossen Pfad.
        let uri = TargetUri::parse("/home/user/seite.html").unwrap();

        assert_eq!(uri.scheme(), "file");
        assert_eq!(uri.as_str(), "file:///home/user/seite.html");
        assert_eq!(uri.host(), None);
    }

    #[test]
    fn a_path_with_spaces_is_encoded_not_split() {
        let uri = TargetUri::parse("/home/user/mein ordner/a b.html").unwrap();
        assert_eq!(uri.as_str(), "file:///home/user/mein%20ordner/a%20b.html");
    }

    #[test]
    fn a_relative_path_is_not_guessed_at() {
        // Ohne bekanntes Arbeitsverzeichnis wäre jede Auflösung geraten.
        assert_eq!(TargetUri::parse("seite.html"), Err(UriError::Malformed));
        assert_eq!(TargetUri::parse("./seite.html"), Err(UriError::Malformed));
    }

    #[test]
    fn rejects_empty_and_malformed_input() {
        assert_eq!(TargetUri::parse(""), Err(UriError::Empty));
        assert_eq!(TargetUri::parse("   "), Err(UriError::Empty));
        assert_eq!(TargetUri::parse("example.com"), Err(UriError::Malformed));
    }

    #[test]
    fn scheme_comparison_ignores_case() {
        assert!(TargetUri::parse("HTTPS://example.com").is_ok());
    }

    #[test]
    fn exposes_host_for_rules_and_display() {
        let uri = TargetUri::parse("https://www.github.com/user/repo").unwrap();
        assert_eq!(uri.host(), Some("www.github.com"));
        assert_eq!(uri.display_host(), Some("github.com"));

        let file = TargetUri::parse("file:///home/user/page.html").unwrap();
        assert_eq!(file.host(), None);
        assert_eq!(file.display_host(), None);
    }

    #[test]
    fn keeps_query_and_fragment_intact() {
        let raw = "https://example.com/search?q=a%20b&x=1#frag";
        assert_eq!(TargetUri::parse(raw).unwrap().as_str(), raw);
    }

    #[test]
    fn a_url_can_never_start_with_a_dash_anyway() {
        // Hält den Nebeneffekt fest, auf den sich die Flag-Prüfung stützt: ein Schema
        // muss mit einem Buchstaben beginnen.
        assert!(Url::parse("-foo://bar").is_err());
    }
}
