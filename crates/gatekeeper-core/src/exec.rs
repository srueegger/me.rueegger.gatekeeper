//! Auflösen von `Exec`-Zeilen zu einem `argv`-Array.
//!
//! Das Ergebnis ist immer eine Argumentliste, nie eine Kommandozeile. Es gibt in diesem
//! Crate keinen Weg, eine Zeichenkette an eine Shell zu geben. URLs sind Fremdeingabe
//! (Invariante 3).
//!
//! # Zwei Ebenen von Escaping
//!
//! Eine `Exec`-Zeile durchläuft nach Spec zwei Stufen: erst die allgemeine
//! String-Entschärfung, die für jeden Wert gilt (`\\`, `\n`, `\t`, `\r`, `\s`), danach die
//! Argumentzerlegung mit eigenem Quoting. Genau so verfährt auch GLib. Deshalb wird hier
//! der Rohwert übergeben und beides nacheinander angewandt.
//!
//! # Flatpaks `@@u … @@`
//!
//! Exportierte Flatpak-Einträge enthalten Marker der Form `--file-forwarding … @@u %u @@`.
//! Die sind nicht für den Browser bestimmt, sondern für `flatpak run`, das sie selbst
//! auswertet. Sie werden deshalb unverändert durchgereicht. Nur das `%u` dazwischen wird
//! ersetzt.

use std::fmt;

use crate::desktop::unescape;

/// Warum aus einer `Exec`-Zeile kein `argv` wurde.
#[derive(Debug, PartialEq, Eq)]
pub enum ExecError {
    /// Leere oder nur aus Feldcodes bestehende Zeile, kein Programm übrig.
    NoProgram,
    /// Ein Anführungszeichen wurde nicht geschlossen.
    UnterminatedQuote,
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProgram => write!(f, "Exec-Zeile enthält kein Programm"),
            Self::UnterminatedQuote => write!(f, "nicht geschlossenes Anführungszeichen"),
        }
    }
}

impl std::error::Error for ExecError {}

/// Werte, die für die Feldcodes eingesetzt werden.
#[derive(Debug, Default, Clone, Copy)]
pub struct FieldContext<'a> {
    /// URIs für `%u` und `%U`. Müssen vorher validiert sein, siehe [`crate::uri`].
    pub uris: &'a [String],
    /// Wert von `Icon`, eingesetzt für `%i` als `--icon <wert>`.
    pub icon: Option<&'a str>,
    /// Übersetzter Wert von `Name`, eingesetzt für `%c`.
    pub name: Option<&'a str>,
    /// Pfad der Desktop-Datei, eingesetzt für `%k`.
    pub desktop_path: Option<&'a str>,
}

/// Löst eine `Exec`-Zeile zu einem vollständigen `argv` auf.
///
/// `argv[0]` ist das Programm. Enthält die Zeile keinen URI-Feldcode, aber es liegen URIs
/// vor, werden sie angehängt. Desktop Actions wie Braves `new-private-window` schreiben
/// ihre Exec-Zeile ohne Feldcode.
pub fn build_argv(exec: &str, ctx: &FieldContext<'_>) -> Result<Vec<String>, ExecError> {
    let tokens = tokenize(&unescape(exec))?;

    let mut argv: Vec<String> = Vec::with_capacity(tokens.len() + ctx.uris.len());
    let mut consumed_uris = false;

    for token in &tokens {
        match field_code(token) {
            Some(FieldCode::SingleUri) => {
                consumed_uris = true;
                if let Some(first) = ctx.uris.first() {
                    argv.push(first.clone());
                }
            }
            Some(FieldCode::AllUris) => {
                consumed_uris = true;
                argv.extend(ctx.uris.iter().cloned());
            }
            Some(FieldCode::Icon) => {
                if let Some(icon) = ctx.icon {
                    argv.push("--icon".to_string());
                    argv.push(icon.to_string());
                }
            }
            Some(FieldCode::Name) => {
                if let Some(name) = ctx.name {
                    argv.push(name.to_string());
                }
            }
            Some(FieldCode::DesktopPath) => {
                if let Some(path) = ctx.desktop_path {
                    argv.push(path.to_string());
                }
            }
            // Veraltete Codes werden nach Spec ersatzlos entfernt.
            Some(FieldCode::Deprecated) => {}
            None => argv.push(token.replace("%%", "%")),
        }
    }

    if argv.is_empty() {
        return Err(ExecError::NoProgram);
    }

    // Ohne URI-Feldcode gibt es keinen vorgesehenen Platz für die URL. Anhängen ist das,
    // was Browser in diesem Fall erwarten.
    if !consumed_uris {
        argv.extend(ctx.uris.iter().cloned());
    }

    Ok(argv)
}

/// Entfernt alle Feldcodes und liefert die Zeile als Argumentliste.
///
/// Grundlage für die Deduplizierung: zwei Einträge, die hierin übereinstimmen, starten
/// dasselbe Programm (ADR-3).
pub fn tokenize_without_field_codes(exec: &str) -> Result<Vec<String>, ExecError> {
    let tokens = tokenize(&unescape(exec))?;
    let stripped: Vec<String> = tokens
        .iter()
        .filter(|token| field_code(token).is_none())
        .map(|token| token.replace("%%", "%"))
        .collect();

    if stripped.is_empty() {
        return Err(ExecError::NoProgram);
    }
    Ok(stripped)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum FieldCode {
    SingleUri,
    AllUris,
    Icon,
    Name,
    DesktopPath,
    Deprecated,
}

/// Erkennt einen Feldcode, der ein vollständiges Argument bildet.
///
/// Nach Spec dürfen Feldcodes nicht in ein Argument eingebettet werden. Eingebettete
/// Vorkommen bleiben deshalb unangetastet, denn sie sind kein Feldcode, sondern Text.
fn field_code(token: &str) -> Option<FieldCode> {
    match token {
        // `%f`/`%F` erwarten lokale Pfade. Browser bekommen von uns URIs; für `file:`-URLs
        // ist der URI die brauchbarere Angabe, alles andere lässt sich nicht als Pfad
        // ausdrücken. Deshalb dieselbe Behandlung wie `%u`/`%U`.
        "%u" | "%f" => Some(FieldCode::SingleUri),
        "%U" | "%F" => Some(FieldCode::AllUris),
        "%i" => Some(FieldCode::Icon),
        "%c" => Some(FieldCode::Name),
        "%k" => Some(FieldCode::DesktopPath),
        "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => Some(FieldCode::Deprecated),
        _ => None,
    }
}

/// Zerlegt eine Kommandozeile nach den Quoting-Regeln der Desktop Entry Spec.
///
/// In Anführungszeichen maskiert `\` die Zeichen `"`, `` ` ``, `$` und `\`. Ausserhalb
/// maskiert `\` das jeweils folgende Zeichen.
fn tokenize(line: &str) -> Result<Vec<String>, ExecError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            '"' => {
                has_token = true;
                let mut closed = false;
                while let Some(inner) = chars.next() {
                    match inner {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => match chars.peek() {
                            Some('"' | '`' | '$' | '\\') => {
                                current.push(chars.next().expect("peek war Some"));
                            }
                            // Undefinierte Sequenz. Backslash bleibt stehen, statt
                            // stillschweigend Information zu verlieren.
                            _ => current.push('\\'),
                        },
                        other => current.push(other),
                    }
                }
                if !closed {
                    return Err(ExecError::UnterminatedQuote);
                }
            }
            '\\' => {
                has_token = true;
                match chars.next() {
                    Some(next) => current.push(next),
                    None => current.push('\\'),
                }
            }
            other => {
                has_token = true;
                current.push(other);
            }
        }
    }

    if has_token {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return Err(ExecError::NoProgram);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(uris: &'a [String]) -> FieldContext<'a> {
        FieldContext { uris, ..FieldContext::default() }
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(tokenize("/usr/bin/chromium %U").unwrap(), ["/usr/bin/chromium", "%U"]);
        assert_eq!(tokenize("  firefox   %u  ").unwrap(), ["firefox", "%u"]);
    }

    #[test]
    fn keeps_quoted_whitespace_together() {
        let tokens = tokenize(r#""/opt/My Browser/bin/run" --flag %u"#).unwrap();
        assert_eq!(tokens, ["/opt/My Browser/bin/run", "--flag", "%u"]);
    }

    #[test]
    fn unescapes_inside_quotes() {
        let tokens = tokenize(r#""a\"b" "c\\d" "e\$f""#).unwrap();
        assert_eq!(tokens, [r#"a"b"#, r"c\d", "e$f"]);
    }

    #[test]
    fn rejects_unterminated_quote() {
        assert_eq!(tokenize(r#"/bin/x "unclosed"#), Err(ExecError::UnterminatedQuote));
    }

    #[test]
    fn empty_line_has_no_program() {
        assert_eq!(tokenize("   "), Err(ExecError::NoProgram));
        assert_eq!(build_argv("", &ctx(&[])), Err(ExecError::NoProgram));
    }

    #[test]
    fn substitutes_single_and_multiple_uris() {
        let uris = vec!["https://a.example".to_string(), "https://b.example".to_string()];

        assert_eq!(
            build_argv("firefox %u", &ctx(&uris)).unwrap(),
            ["firefox", "https://a.example"]
        );
        assert_eq!(
            build_argv("chromium %U", &ctx(&uris)).unwrap(),
            ["chromium", "https://a.example", "https://b.example"]
        );
    }

    #[test]
    fn drops_uri_field_code_when_no_uri_given() {
        assert_eq!(build_argv("firefox %u", &ctx(&[])).unwrap(), ["firefox"]);
    }

    #[test]
    fn appends_uri_when_line_has_no_field_code() {
        // Genau der Fall der Brave-Action `new-private-window`.
        let uris = vec!["https://a.example".to_string()];
        assert_eq!(
            build_argv("/usr/bin/brave-origin-stable --incognito", &ctx(&uris)).unwrap(),
            ["/usr/bin/brave-origin-stable", "--incognito", "https://a.example"]
        );
    }

    #[test]
    fn drops_deprecated_field_codes() {
        let uris = vec!["https://a.example".to_string()];
        assert_eq!(
            build_argv("browser %d %D %n %N %v %m %u", &ctx(&uris)).unwrap(),
            ["browser", "https://a.example"]
        );
    }

    #[test]
    fn substitutes_icon_name_and_path() {
        let uris = vec!["https://a.example".to_string()];
        let context = FieldContext {
            uris: &uris,
            icon: Some("firefox"),
            name: Some("Firefox Web Browser"),
            desktop_path: Some("/usr/share/applications/firefox.desktop"),
        };

        assert_eq!(
            build_argv("browser %i %c %k %u", &context).unwrap(),
            [
                "browser",
                "--icon",
                "firefox",
                "Firefox Web Browser",
                "/usr/share/applications/firefox.desktop",
                "https://a.example",
            ]
        );
    }

    #[test]
    fn omits_icon_and_name_when_unset() {
        assert_eq!(build_argv("browser %i %c %k", &ctx(&[])).unwrap(), ["browser"]);
    }

    #[test]
    fn double_percent_becomes_a_literal_percent() {
        assert_eq!(build_argv("browser 100%%", &ctx(&[])).unwrap(), ["browser", "100%"]);
    }

    #[test]
    fn embedded_field_codes_are_not_substituted() {
        // Nach Spec muss ein Feldcode ein ganzes Argument bilden. `--url=%u` ist keiner.
        let uris = vec!["https://a.example".to_string()];
        let argv = build_argv("browser --url=%u", &ctx(&uris)).unwrap();

        assert_eq!(argv[1], "--url=%u");
        // Da kein gültiger Feldcode vorkam, wird die URL zusätzlich angehängt.
        assert_eq!(argv[2], "https://a.example");
    }

    #[test]
    fn passes_flatpak_file_forwarding_markers_through() {
        let uris = vec!["https://a.example".to_string()];
        let exec = "/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=firefox \
                    --file-forwarding org.mozilla.firefox @@u %u @@";

        assert_eq!(
            build_argv(exec, &ctx(&uris)).unwrap(),
            [
                "/usr/bin/flatpak",
                "run",
                "--branch=stable",
                "--arch=x86_64",
                "--command=firefox",
                "--file-forwarding",
                "org.mozilla.firefox",
                "@@u",
                "https://a.example",
                "@@",
            ],
            "die @@-Marker wertet flatpak run selbst aus und müssen erhalten bleiben"
        );
    }

    #[test]
    fn handles_snap_env_prefix() {
        let uris = vec!["https://a.example".to_string()];
        let exec = "env BAMF_DESKTOP_FILE_HINT=/var/lib/snapd/desktop/applications/\
                    firefox_firefox.desktop /snap/bin/firefox %u";
        let argv = build_argv(exec, &ctx(&uris)).unwrap();

        assert_eq!(argv[0], "env", "bei Snap ist das Programm nicht das erste Token");
        assert_eq!(argv[2], "/snap/bin/firefox");
        assert_eq!(argv[3], "https://a.example");
    }

    #[test]
    fn strips_field_codes_for_deduplication() {
        assert_eq!(
            tokenize_without_field_codes("/usr/bin/brave-origin-stable %U").unwrap(),
            tokenize_without_field_codes("/usr/bin/brave-origin-stable %u").unwrap(),
            "identisches Programm, nur anderer Feldcode, also dasselbe Ziel"
        );
        assert_eq!(tokenize_without_field_codes("firefox %u").unwrap(), ["firefox"]);
    }

    #[test]
    fn a_uri_can_never_become_a_flag() {
        // Sähe die Zerlegung ein '-' am Anfang als Fortsetzung des vorigen Arguments,
        // würde daraus ein Schalter. Es bleibt ein eigenes Argument.
        let uris = vec!["--gpu-launcher=/bin/sh".to_string()];
        let argv = build_argv("browser %u", &ctx(&uris)).unwrap();

        assert_eq!(argv, ["browser", "--gpu-launcher=/bin/sh"]);
        assert_eq!(argv.len(), 2, "kein Aufsplitten, kein Zusammenziehen");
    }
}
