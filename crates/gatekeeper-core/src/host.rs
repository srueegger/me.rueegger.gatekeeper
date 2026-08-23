//! Kommandos auf dem Host ausführen.
//!
//! Innerhalb einer Flatpak-Sandbox laufen Zielbrowser und Systemwerkzeuge nicht in unserer
//! Sandbox, sondern auf dem Host. Erreichbar sind sie nur über `flatpak-spawn --host`
//! (ADR-2). Wie dieser Aufruf aussieht, steht ausschliesslich hier, damit der
//! abgekoppelte Start und die synchrone Ausführung nicht auseinanderlaufen.
//!
//! Auch hier gilt: nur `argv`, nie eine Kommandozeile (Invariante 3).

use std::process::{Command, Output, Stdio};

use crate::discovery::in_flatpak_sandbox;

/// Umhüllt ein Kommando so, dass es auf dem Host ausgeführt wird.
///
/// Ausserhalb einer Sandbox bleibt es unverändert.
pub fn wrap(argv: &[String], env: &[(String, String)], sandboxed: bool) -> Vec<String> {
    if !sandboxed {
        // Umgebungsvariablen setzt der Aufrufer dann direkt am Prozess.
        return argv.to_vec();
    }

    let mut wrapped = Vec::with_capacity(argv.len() + env.len() + 3);
    wrapped.push("flatpak-spawn".to_string());
    wrapped.push("--host".to_string());
    for (name, value) in env {
        wrapped.push(format!("--env={name}={value}"));
    }
    // Ohne diesen Trenner läse flatpak-spawn ein Argument des Zielprogramms, das mit '-'
    // beginnt, als eigenen Schalter.
    wrapped.push("--".to_string());
    wrapped.extend(argv.iter().cloned());
    wrapped
}

/// Führt ein Kommando auf dem Host aus und wartet auf sein Ende.
///
/// Für kurze Abfragen wie `xdg-settings get`. Zum Starten eines Browsers ist der
/// [`crate::launcher`] zuständig, der bewusst nicht wartet.
pub fn run(argv: &[String]) -> std::io::Result<Output> {
    let wrapped = wrap(argv, &[], in_flatpak_sandbox());
    let Some((program, args)) = wrapped.split_first() else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "leeres Kommando"));
    };

    Command::new(program).args(args).stdin(Stdio::null()).output()
}

/// Die Ausgabe eines erfolgreichen Laufs, sonst `None`.
///
/// Ob ein Werkzeug fehlt oder mit Fehler endet, macht für die Aufrufer hier keinen
/// Unterschied: In beiden Fällen gibt es kein verwertbares Ergebnis und es wird auf einen
/// anderen Weg ausgewichen.
pub fn stdout_if_successful(argv: &[String]) -> Option<String> {
    let output = run(argv).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn outside_a_sandbox_the_command_is_untouched() {
        let command = argv(&["xdg-settings", "get", "default-web-browser"]);
        assert_eq!(wrap(&command, &[], false), command);
    }

    #[test]
    fn inside_a_sandbox_the_command_goes_through_flatpak_spawn() {
        let wrapped = wrap(&argv(&["xdg-settings", "get", "default-web-browser"]), &[], true);

        assert_eq!(
            wrapped,
            ["flatpak-spawn", "--host", "--", "xdg-settings", "get", "default-web-browser"]
        );
    }

    #[test]
    fn the_separator_comes_before_any_argument_of_the_target() {
        // Ohne '--' läse flatpak-spawn '--incognito' als eigenen Schalter.
        let wrapped = wrap(&argv(&["/usr/bin/brave", "--incognito"]), &[], true);

        let separator = wrapped.iter().position(|a| a == "--").expect("Trenner fehlt");
        let flag = wrapped.iter().position(|a| a == "--incognito").unwrap();
        assert!(separator < flag, "{wrapped:?}");
    }

    #[test]
    fn environment_is_passed_as_flags_before_the_separator() {
        let env = vec![("XDG_ACTIVATION_TOKEN".to_string(), "token-123".to_string())];
        let wrapped = wrap(&argv(&["/usr/bin/firefox"]), &env, true);

        let token = wrapped
            .iter()
            .position(|a| a == "--env=XDG_ACTIVATION_TOKEN=token-123")
            .expect("Token fehlt");
        let separator = wrapped.iter().position(|a| a == "--").unwrap();
        assert!(token < separator, "{wrapped:?}");
    }

    #[test]
    fn environment_is_ignored_outside_a_sandbox() {
        // Dort setzt der Aufrufer die Variablen direkt am Prozess, statt sie als
        // Argumente durchzureichen.
        let env = vec![("XDG_ACTIVATION_TOKEN".to_string(), "token-123".to_string())];
        assert_eq!(wrap(&argv(&["/usr/bin/firefox"]), &env, false), ["/usr/bin/firefox"]);
    }
}
