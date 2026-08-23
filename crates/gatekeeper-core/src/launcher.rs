//! Zielbrowser starten.
//!
//! Der Launcher nimmt ausschliesslich `argv`-Arrays entgegen. Es gibt in diesem Modul keine
//! Funktion, die eine Kommandozeile als Zeichenkette annimmt, und damit keinen Weg, aus einer
//! URL versehentlich Shell-Syntax werden zu lassen (Invariante 3).
//!
//! Ebenso wenig gibt es hier `xdg-open`, ein Portal oder sonst einen generischen Öffner. Alle
//! davon schlagen den Default-Handler nach, und der ist Gatekeeper selbst (ADR-1).
//!
//! # Zwei Umgebungen
//!
//! Ausserhalb einer Sandbox wird das Programm direkt gestartet. Innerhalb einer
//! Flatpak-Sandbox läuft der Zielbrowser auf dem Host und wird über `flatpak-spawn --host`
//! erreicht (ADR-2). Welcher Weg gilt, entscheidet [`default_launcher`]; getestet wird
//! gegen [`RecordingLauncher`], der nichts startet, sondern aufzeichnet.

use std::fmt;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use log::{debug, info};

use crate::discovery::in_flatpak_sandbox;

/// Umgebungsvariablen, die an den Zielbrowser durchgereicht werden, sofern gesetzt.
///
/// Ohne ein gültiges Aktivierungs-Token erscheint das Fenster des Browsers unter Wayland
/// ohne Fokus und teils hinter anderen Fenstern. `DESKTOP_STARTUP_ID` ist das Gegenstück
/// unter X11.
pub const FORWARDED_ENV: &[&str] = &["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"];

/// Warum ein Start nicht zustande kam.
#[derive(Debug)]
pub enum LaunchError {
    /// Leeres `argv`. Es gäbe kein Programm zu starten.
    EmptyCommand,
    /// Ein Argument enthält ein Nullbyte und lässt sich nicht übergeben.
    NulInArgument(usize),
    Spawn(std::io::Error),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommand => write!(f, "kein Programm zum Starten"),
            Self::NulInArgument(index) => {
                write!(f, "Argument {index} enthält ein Nullbyte")
            }
            Self::Spawn(err) => write!(f, "Start fehlgeschlagen: {err}"),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(err) => Some(err),
            _ => None,
        }
    }
}

/// Was gestartet werden soll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// Vollständiges Kommando. `argv[0]` ist das Programm.
    pub argv: Vec<String>,
    /// Zusätzliche Umgebungsvariablen für den Zielprozess.
    pub env: Vec<(String, String)>,
}

impl LaunchRequest {
    /// Baut eine Anfrage aus einem `argv` und übernimmt die weiterzureichenden Variablen
    /// aus der eigenen Umgebung.
    pub fn new(argv: Vec<String>) -> Self {
        let env = FORWARDED_ENV
            .iter()
            .filter_map(|name| std::env::var(name).ok().map(|value| ((*name).to_string(), value)))
            .collect();
        Self { argv, env }
    }

    fn validate(&self) -> Result<(), LaunchError> {
        if self.argv.is_empty() {
            return Err(LaunchError::EmptyCommand);
        }
        if let Some(index) = self.argv.iter().position(|arg| arg.contains('\0')) {
            return Err(LaunchError::NulInArgument(index));
        }
        Ok(())
    }
}

/// Startet ein Programm.
///
/// Bewusst nur diese eine Methode und bewusst nur mit `argv`. Wer eine Zeichenkette
/// ausführen will, findet hier keinen Weg dazu.
pub trait Launcher: Send + Sync {
    fn launch(&self, request: &LaunchRequest) -> Result<(), LaunchError>;
}

/// Der Launcher, der zur laufenden Umgebung passt.
pub fn default_launcher() -> Box<dyn Launcher> {
    if in_flatpak_sandbox() {
        debug!("Sandbox erkannt, starte über flatpak-spawn --host");
        Box::new(HostSpawnLauncher)
    } else {
        Box::new(DirectLauncher)
    }
}

/// Startet das Programm unmittelbar. Der Weg ausserhalb jeder Sandbox.
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectLauncher;

impl Launcher for DirectLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<(), LaunchError> {
        request.validate()?;
        info!("starte {:?}", request.argv);

        let mut command = Command::new(&request.argv[0]);
        command.args(&request.argv[1..]);
        for (name, value) in &request.env {
            command.env(name, value);
        }
        // Der Browser soll nichts von unserer Standardeingabe lesen. Ausgabe und Fehler
        // bleiben verbunden, damit sich Startprobleme überhaupt beobachten lassen.
        command.stdin(Stdio::null());

        // Es wird bewusst nicht gewartet: Gatekeeper beendet sich sofort und überlässt den
        // Browser dem init-Prozess.
        command.spawn().map(|_| ()).map_err(LaunchError::Spawn)
    }
}

/// Startet das Programm über das Host-Command-Portal von Flatpak.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostSpawnLauncher;

impl HostSpawnLauncher {
    /// Baut die Kommandozeile für `flatpak-spawn`.
    ///
    /// Als eigene Funktion, damit sich prüfen lässt, was übergeben wird, ohne etwas zu
    /// starten.
    pub fn build_argv(request: &LaunchRequest) -> Vec<String> {
        let mut argv = Vec::with_capacity(request.argv.len() + request.env.len() + 3);
        argv.push("flatpak-spawn".to_string());
        argv.push("--host".to_string());
        for (name, value) in &request.env {
            argv.push(format!("--env={name}={value}"));
        }
        // Ohne diesen Trenner läse flatpak-spawn ein Argument des Browsers, das mit '-'
        // beginnt, als eigenen Schalter.
        argv.push("--".to_string());
        argv.extend(request.argv.iter().cloned());
        argv
    }
}

impl Launcher for HostSpawnLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<(), LaunchError> {
        request.validate()?;
        let argv = Self::build_argv(request);
        info!("starte über den Host: {:?}", request.argv);

        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]);
        command.stdin(Stdio::null());

        // `flatpak-spawn` ohne `--watch-bus` beendet den Zielprozess nicht, wenn es selbst
        // verschwindet. Der Browser überlebt also unser Beenden.
        command.spawn().map(|_| ()).map_err(LaunchError::Spawn)
    }
}

/// Zeichnet auf, statt zu starten. Nur für Tests.
///
/// Damit lässt sich überprüfen, was tatsächlich übergeben würde. Ohne diesen Typ wäre
/// „nie über eine Shell" ein Vorsatz statt einer geprüften Eigenschaft.
#[derive(Debug, Default)]
pub struct RecordingLauncher {
    calls: Mutex<Vec<LaunchRequest>>,
}

impl RecordingLauncher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Alle bisher aufgezeichneten Anfragen.
    pub fn calls(&self) -> Vec<LaunchRequest> {
        self.calls.lock().expect("Mutex vergiftet").clone()
    }
}

impl Launcher for RecordingLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<(), LaunchError> {
        request.validate()?;
        self.calls.lock().expect("Mutex vergiftet").push(request.clone());
        Ok(())
    }
}

/// Programme, deren Aufruf bedeuten würde, dass eine Zeichenkette interpretiert wird.
///
/// Wird nur im Test verwendet, steht aber hier, damit die Liste neben dem Launcher lebt.
#[cfg(test)]
const SHELLS: &[&str] = &["sh", "bash", "dash", "zsh", "fish", "ksh", "csh", "tcsh", "busybox"];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn request(argv: &[&str]) -> LaunchRequest {
        LaunchRequest { argv: argv.iter().map(|s| s.to_string()).collect(), env: Vec::new() }
    }

    // ----------------------------------------------------------------------------------
    // Invariante 3
    // ----------------------------------------------------------------------------------

    #[test]
    fn never_routes_a_command_through_a_shell() {
        let launcher = RecordingLauncher::new();
        let hostile = "https://example.com/a;rm -rf ~;#$(whoami)`id`";

        launcher.launch(&request(&["/usr/bin/chromium", hostile])).unwrap();

        let calls = launcher.calls();
        assert_eq!(calls.len(), 1);
        let argv = &calls[0].argv;

        assert!(
            !SHELLS.contains(&Path::new(&argv[0]).file_name().unwrap().to_str().unwrap()),
            "das Programm darf nie eine Shell sein: {argv:?}"
        );
        assert!(!argv.contains(&"-c".to_string()), "kein -c: {argv:?}");
        // Die URL bleibt genau ein Argument, unverändert und ungedeutet.
        assert_eq!(argv.len(), 2);
        assert_eq!(argv[1], hostile);
    }

    #[test]
    fn host_spawn_never_routes_a_command_through_a_shell() {
        let hostile = "https://example.com/$(id)";
        let argv = HostSpawnLauncher::build_argv(&request(&["/usr/bin/chromium", hostile]));

        assert!(
            !argv.iter().any(|arg| {
                SHELLS.contains(&Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or(""))
            }),
            "keine Shell im Aufruf: {argv:?}"
        );
        assert_eq!(argv.last().unwrap(), hostile);
    }

    #[test]
    fn arguments_are_never_joined_into_one_string() {
        let launcher = RecordingLauncher::new();
        launcher
            .launch(&request(&["/usr/bin/firefox", "--new-window", "https://a b.example"]))
            .unwrap();

        let argv = &launcher.calls()[0].argv;
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[2], "https://a b.example", "Leerzeichen trennen kein Argument");
    }

    // ----------------------------------------------------------------------------------
    // flatpak-spawn
    // ----------------------------------------------------------------------------------

    #[test]
    fn host_spawn_builds_the_expected_command() {
        let argv =
            HostSpawnLauncher::build_argv(&request(&["/usr/bin/firefox", "https://example.com"]));

        assert_eq!(
            argv,
            ["flatpak-spawn", "--host", "--", "/usr/bin/firefox", "https://example.com"]
        );
    }

    #[test]
    fn host_spawn_separates_its_own_flags_from_the_browsers() {
        // Ohne '--' läse flatpak-spawn '--incognito' als eigenen Schalter.
        let argv = HostSpawnLauncher::build_argv(&request(&[
            "/usr/bin/brave",
            "--incognito",
            "https://example.com",
        ]));

        let separator = argv.iter().position(|arg| arg == "--").expect("Trenner fehlt");
        let browser_flag = argv.iter().position(|arg| arg == "--incognito").unwrap();
        assert!(separator < browser_flag, "{argv:?}");
    }

    #[test]
    fn host_spawn_forwards_the_activation_token() {
        let mut req = request(&["/usr/bin/firefox", "https://example.com"]);
        req.env = vec![("XDG_ACTIVATION_TOKEN".to_string(), "token-123".to_string())];
        let argv = HostSpawnLauncher::build_argv(&req);

        let token = argv.iter().position(|a| a == "--env=XDG_ACTIVATION_TOKEN=token-123");
        let separator = argv.iter().position(|arg| arg == "--").unwrap();
        assert!(token.is_some(), "Token fehlt: {argv:?}");
        assert!(token.unwrap() < separator, "Token gehört vor den Trenner: {argv:?}");
    }

    // ----------------------------------------------------------------------------------
    // Prüfungen
    // ----------------------------------------------------------------------------------

    #[test]
    fn refuses_an_empty_command() {
        let launcher = RecordingLauncher::new();
        assert!(matches!(launcher.launch(&request(&[])), Err(LaunchError::EmptyCommand)));
        assert!(launcher.calls().is_empty());
    }

    #[test]
    fn refuses_an_argument_containing_a_nul_byte() {
        let launcher = RecordingLauncher::new();
        let result = launcher.launch(&request(&["/usr/bin/firefox", "https://a\0b"]));

        assert!(matches!(result, Err(LaunchError::NulInArgument(1))));
        assert!(launcher.calls().is_empty(), "nichts darf aufgezeichnet werden");
    }

    #[test]
    fn forwarded_env_is_read_from_the_process_environment() {
        // Es werden nur die genannten Variablen übernommen, nicht die ganze Umgebung.
        let req = LaunchRequest::new(vec!["/usr/bin/firefox".to_string()]);
        for (name, _) in &req.env {
            assert!(FORWARDED_ENV.contains(&name.as_str()), "unerwartete Variable {name}");
        }
    }
}
