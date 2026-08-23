//! Brücke zwischen dem Rust-Kern und der Qt-Oberfläche.
//!
//! Die Fläche bleibt bewusst klein und stabil. Alles, was denken muss, steht in
//! `gatekeeper-core`; hier wird nur übersetzt.

use gatekeeper_core::discovery::{self, DiscoveryOptions};
use gatekeeper_core::exec::{self, FieldContext};
use gatekeeper_core::uri::TargetUri;

/// Eigene Desktop-ID. Wird in der Discovery immer ausgefiltert (Invariante 1).
pub const SELF_DESKTOP_ID: &str = "me.rueegger.Gatekeeper.desktop";

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
    let options = DiscoveryOptions::from_env(SELF_DESKTOP_ID);
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
