//! Zeigt, was der Kern auf dem laufenden System findet, ohne GUI.
//!
//! ```text
//! cargo run --example scan
//! cargo run --example scan -- https://github.com/user/repo
//! RUST_LOG=debug cargo run --example scan
//! ```

use gatekeeper_core::discovery::{DiscoveryOptions, discover};
use gatekeeper_core::exec::{FieldContext, build_argv};
use gatekeeper_core::uri::TargetUri;

const SELF_ID: &str = "me.rueegger.Gatekeeper.desktop";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let target = std::env::args().nth(1).map(|raw| match TargetUri::parse(&raw) {
        Ok(uri) => uri,
        Err(err) => {
            eprintln!("Ziel abgelehnt: {err}");
            std::process::exit(2);
        }
    });

    let options = DiscoveryOptions::from_env(SELF_ID);

    println!("Suchpfade (höchste Präzedenz zuerst):");
    for path in &options.search_paths {
        let marker = if path.dir.is_dir() { "*" } else { " " };
        println!("  {marker} {:<9} {}", format!("{:?}", path.kind), path.dir.display());
    }

    let started = std::time::Instant::now();
    let browsers = discover(&options);
    let elapsed = started.elapsed();

    println!("\n{} Browser in {:.1?}:\n", browsers.len(), elapsed);

    let uris: Vec<String> = target.iter().map(|uri| uri.as_str().to_string()).collect();
    for browser in &browsers {
        println!("  {} [{}]", browser.name, browser.origin.label());
        println!("      id    {}", browser.id);
        println!("      icon  {}", browser.icon.as_deref().unwrap_or("(keines)"));
        println!("      exec  {}", browser.exec);

        let context = FieldContext {
            uris: &uris,
            icon: browser.icon.as_deref(),
            name: Some(&browser.name),
            desktop_path: browser.path.to_str(),
        };
        match build_argv(&browser.exec, &context) {
            Ok(argv) => println!("      argv  {argv:?}"),
            Err(err) => println!("      argv  FEHLER: {err}"),
        }
        for action in &browser.actions {
            println!("      + {} ({})", action.name, action.id);
        }
        println!();
    }

    if let Some(uri) = &target {
        println!("Ziel: {uri}");
        println!("Domain für die Anzeige: {}", uri.display_host().unwrap_or("(keine)"));
    } else {
        println!("Tipp: URL als Argument übergeben, um das aufgelöste argv zu sehen.");
    }
}
