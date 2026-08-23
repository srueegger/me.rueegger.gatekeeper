//! Zeigt und setzt den Standardbrowser. Nur zum Prüfen von Hand.
//!
//! ```text
//! cargo run --example default-browser          # nur anzeigen
//! cargo run --example default-browser -- set   # eintragen
//! ```

use gatekeeper_core::default_browser::{ConfigEnvironment, current, make_default};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    let env = ConfigEnvironment::from_env();

    println!("Konfigurationsdateien in Reihenfolge:");
    for file in env.mimeapps_files() {
        println!("  {} {}", if file.is_file() { "*" } else { " " }, file.display());
    }
    println!("\nZustand: {:?}", current(&env));

    if std::env::args().nth(1).as_deref() == Some("set") {
        match make_default(&env) {
            Ok(()) => println!("eingetragen, Zustand jetzt: {:?}", current(&env)),
            Err(err) => println!("fehlgeschlagen: {err}"),
        }
    }
}
