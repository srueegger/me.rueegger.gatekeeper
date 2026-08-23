//! Brücke zwischen dem Rust-Kern und der Qt-Oberfläche.
//!
//! Die Fläche bleibt bewusst klein und stabil. Alles, was denken muss, steht in
//! `gatekeeper-core`; hier wird nur übersetzt.

use gatekeeper_core::default_browser::{self, ConfigEnvironment, DefaultBrowser};
use gatekeeper_core::discovery::{self, DiscoveryOptions};
use gatekeeper_core::exec::{self, FieldContext};
use gatekeeper_core::launcher::{LaunchRequest, default_launcher};
use gatekeeper_core::rules::{RuleSet, rules_path};
use gatekeeper_core::uri::TargetUri;

#[cxx::bridge(namespace = "gatekeeper")]
mod ffi {
    /// Ein Browser, so wie ihn die Oberfläche braucht.
    struct Browser {
        /// Desktop-ID, dient als stabiler Schlüssel für Regeln.
        id: String,
        name: String,
        /// Theme-Name oder absoluter Pfad. Leer, wenn der Eintrag keines nennt.
        icon: String,
        /// Etikett der Herkunft: System, Benutzer, Flatpak oder Snap.
        origin: String,
        /// Fertiges Startkommando. Erstes Element ist das Programm.
        argv: Vec<String>,
    }

    /// Was die Regeln mit dieser URL gemacht haben.
    struct RuleOutcome {
        /// Eine Regel hat gegriffen.
        matched: bool,
        /// Der Browser wurde gestartet. Der Aufruf ist damit erledigt.
        launched: bool,
        /// Desktop-ID des Browsers, den die Regel benannt hat.
        browser: String,
        /// Warum es trotz Treffer nicht klappte.
        error: String,
    }

    /// Wer aktuell Links öffnet.
    struct DefaultBrowserStatus {
        /// Gatekeeper ist für http und https zuständig.
        ours: bool,
        /// Kurzer Satz für die Oberfläche. Leer, wenn alles in Ordnung ist.
        message: String,
    }

    /// Ergebnis eines Startversuchs.
    struct LaunchOutcome {
        started: bool,
        /// Fehlertext, wenn `started` falsch ist.
        error: String,
    }

    /// Ergebnis der Zielprüfung.
    struct Target {
        valid: bool,
        /// Fehlertext, wenn `valid` falsch ist.
        error: String,
        uri: String,
        /// Die Domain, die in der Oberfläche hervorgehoben wird.
        display_host: String,
    }

    extern "Rust" {
        fn init_logging();
        fn check_target(raw: &str) -> Target;
        fn list_browsers(uri: &str) -> Vec<Browser>;
        fn launch(argv: &[String]) -> LaunchOutcome;
        fn apply_rules(uri: &str) -> RuleOutcome;
        fn remember_rule(host: &str, browser: &str) -> LaunchOutcome;
        fn default_browser_status() -> DefaultBrowserStatus;
        fn make_default_browser() -> LaunchOutcome;
    }
}

/// Richtet die Protokollierung ein. Steuerbar über `RUST_LOG`.
pub fn init_logging() {
    let env = env_logger::Env::default().default_filter_or("warn");
    // Mehrfachaufruf ist kein Fehler, nur ein No-Op.
    let _ = env_logger::Builder::from_env(env).try_init();
}

/// Prüft die übergebene URL, bevor sie irgendwo als Argument landet.
pub fn check_target(raw: &str) -> ffi::Target {
    match TargetUri::parse(raw) {
        Ok(uri) => ffi::Target {
            valid: true,
            error: String::new(),
            uri: uri.as_str().to_string(),
            display_host: uri.display_host().unwrap_or_default().to_string(),
        },
        Err(err) => ffi::Target {
            valid: false,
            error: err.to_string(),
            uri: String::new(),
            display_host: String::new(),
        },
    }
}

/// Sucht alle startbaren Browser und löst gleich das Startkommando für `uri` auf.
///
/// Einträge, deren `Exec`-Zeile sich nicht auflösen lässt, fallen hier weg statt später
/// beim Klick zu scheitern.
pub fn list_browsers(uri: &str) -> Vec<ffi::Browser> {
    let options = DiscoveryOptions::from_env(gatekeeper_core::SELF_DESKTOP_ID);
    let uris: Vec<String> = if uri.is_empty() { Vec::new() } else { vec![uri.to_string()] };

    discovery::discover(&options)
        .into_iter()
        .filter_map(|browser| {
            let context = FieldContext {
                uris: &uris,
                icon: browser.icon.as_deref(),
                name: Some(&browser.name),
                desktop_path: browser.path.to_str(),
            };
            let argv = exec::build_argv(&browser.exec, &context).ok()?;

            Some(ffi::Browser {
                id: browser.id,
                name: browser.name,
                icon: browser.icon.unwrap_or_default(),
                origin: browser.origin.label().to_string(),
                argv,
            })
        })
        .collect()
}

/// Startet ein zuvor aufgelöstes Kommando.
///
/// Es wird bewusst ein fertiges `argv` übergeben und keine Kommandozeile. Der Weg von einer
/// Zeichenkette zu einem Prozess existiert in diesem Crate nicht (Invariante 3).
pub fn launch(argv: &[String]) -> ffi::LaunchOutcome {
    let request = LaunchRequest::new(argv.to_vec());
    match default_launcher().launch(&request) {
        Ok(()) => ffi::LaunchOutcome { started: true, error: String::new() },
        Err(err) => ffi::LaunchOutcome { started: false, error: err.to_string() },
    }
}

/// Prüft, ob Gatekeeper der Standardbrowser ist.
///
/// Läuft bei jedem Start und liest dafür nur Dateien, ohne einen Prozess zu starten.
pub fn default_browser_status() -> ffi::DefaultBrowserStatus {
    let env = ConfigEnvironment::from_env();
    match default_browser::current(&env) {
        DefaultBrowser::Ours => ffi::DefaultBrowserStatus { ours: true, message: String::new() },
        DefaultBrowser::Unset => ffi::DefaultBrowserStatus {
            ours: false,
            message: "Es ist kein Standardbrowser eingetragen.".to_string(),
        },
        DefaultBrowser::Other { desktop_id } => ffi::DefaultBrowserStatus {
            ours: false,
            message: format!("Links öffnet zurzeit {desktop_id}, nicht Gatekeeper."),
        },
        DefaultBrowser::Mixed { .. } => ffi::DefaultBrowserStatus {
            ours: false,
            message: "http und https sind auf verschiedene Anwendungen eingetragen.".to_string(),
        },
    }
}

/// Trägt Gatekeeper als Standardbrowser ein und prüft anschliessend nach.
pub fn make_default_browser() -> ffi::LaunchOutcome {
    let env = ConfigEnvironment::from_env();
    match default_browser::make_default(&env) {
        Ok(()) => ffi::LaunchOutcome { started: true, error: String::new() },
        Err(err) => ffi::LaunchOutcome { started: false, error: err.to_string() },
    }
}

/// Prüft die gespeicherten Regeln und startet bei einem Treffer sofort.
///
/// Wird aufgerufen, bevor irgendetwas von Qt existiert. Greift eine Regel, bleibt es
/// dabei: kein Fenster, keine Szenengrafik, keine Wartezeit.
///
/// Scheitert der Start trotz Treffer, etwa weil der eingetragene Browser nicht mehr
/// installiert ist, wird das gemeldet und der Dialog übernimmt. Wortlos zu scheitern wäre
/// die schlechtere Antwort.
pub fn apply_rules(uri: &str) -> ffi::RuleOutcome {
    let miss = || ffi::RuleOutcome {
        matched: false,
        launched: false,
        browser: String::new(),
        error: String::new(),
    };

    let Ok(target) = TargetUri::parse(uri) else {
        return miss();
    };
    let Some(path) = rules_path() else {
        return miss();
    };

    let rules = RuleSet::load(&path);
    let Some(rule) = rules.first_match(&target) else {
        return miss();
    };

    let options = DiscoveryOptions::from_env(gatekeeper_core::SELF_DESKTOP_ID);
    let Some(browser) =
        discovery::discover(&options).into_iter().find(|found| found.id == rule.browser)
    else {
        return ffi::RuleOutcome {
            matched: true,
            launched: false,
            browser: rule.browser.clone(),
            error: format!("{} ist nicht installiert", rule.browser),
        };
    };

    // Nennt die Regel eine Aktion, gilt deren eigene Exec-Zeile.
    let exec = match &rule.action {
        Some(wanted) => match browser.actions.iter().find(|action| &action.id == wanted) {
            Some(action) => action.exec.clone(),
            None => {
                return ffi::RuleOutcome {
                    matched: true,
                    launched: false,
                    browser: rule.browser.clone(),
                    error: format!("{} kennt keine Aktion '{wanted}'", rule.browser),
                };
            }
        },
        None => browser.exec.clone(),
    };

    let uris = vec![target.as_str().to_string()];
    let context = FieldContext {
        uris: &uris,
        icon: browser.icon.as_deref(),
        name: Some(&browser.name),
        desktop_path: browser.path.to_str(),
    };

    let argv = match exec::build_argv(&exec, &context) {
        Ok(argv) => argv,
        Err(err) => {
            return ffi::RuleOutcome {
                matched: true,
                launched: false,
                browser: rule.browser.clone(),
                error: err.to_string(),
            };
        }
    };

    match default_launcher().launch(&LaunchRequest::new(argv)) {
        Ok(()) => ffi::RuleOutcome {
            matched: true,
            launched: true,
            browser: rule.browser.clone(),
            error: String::new(),
        },
        Err(err) => ffi::RuleOutcome {
            matched: true,
            launched: false,
            browser: rule.browser.clone(),
            error: err.to_string(),
        },
    }
}

/// Hängt eine Regel an, die diesen Host künftig diesem Browser zuordnet.
///
/// Angehängt, nicht eingefügt: Die erste passende Regel gewinnt, und was der Nutzer von
/// Hand geschrieben hat, soll nicht von einem Klick überstimmt werden.
pub fn remember_rule(host: &str, browser: &str) -> ffi::LaunchOutcome {
    let fail = |message: String| ffi::LaunchOutcome { started: false, error: message };

    if host.is_empty() {
        return fail("ohne Host lässt sich keine Regel merken".to_string());
    }
    let Some(path) = rules_path() else {
        return fail("kein Konfigurationsverzeichnis gefunden".to_string());
    };

    let mut rules = RuleSet::load(&path);
    rules.remember_host(host, browser);

    match rules.save(&path) {
        Ok(()) => ffi::LaunchOutcome { started: true, error: String::new() },
        Err(err) => fail(err.to_string()),
    }
}
