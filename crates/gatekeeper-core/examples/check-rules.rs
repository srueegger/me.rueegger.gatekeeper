//! Prüft eine Regeldatei, ohne etwas zu starten.
//!
//! ```text
//! cargo run --example check-rules -- rules.toml [url ...]
//! ```

use gatekeeper_core::rules::RuleSet;
use gatekeeper_core::uri::TargetUri;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("Pfad zur Regeldatei fehlt");
    let text = std::fs::read_to_string(&path).expect("Regeldatei nicht lesbar");

    let set = match RuleSet::parse(&text) {
        Ok(set) => set,
        Err(err) => {
            eprintln!("{path}: {err}");
            std::process::exit(1);
        }
    };
    println!("{} Regeln gelesen", set.rules.len());

    for raw in args {
        match TargetUri::parse(&raw) {
            Ok(uri) => match set.first_match(&uri) {
                Some(rule) => println!("  {raw} -> {}", rule.browser),
                None => println!("  {raw} -> fragen"),
            },
            Err(err) => println!("  {raw} -> abgelehnt: {err}"),
        }
    }
}
