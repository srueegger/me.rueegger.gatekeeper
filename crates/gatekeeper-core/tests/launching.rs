//! Startet wirklich einen Prozess und liest zurück, was dort ankam.
//!
//! Der Aufzeichner in den Unit-Tests prüft, was Gatekeeper übergeben *will*. Hier wird
//! geprüft, was tatsächlich ankommt. Nur so ist belegt, dass zwischen Gatekeeper und dem
//! Ziel niemand die Argumente noch einmal anfasst.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gatekeeper_core::launcher::{DirectLauncher, LaunchRequest, Launcher};

/// Legt ein Programm an, das seine Argumente zeilenweise in eine Datei schreibt.
///
/// Bewusst kein Shell-Skript mit `$@`: Eine Shell würde die Argumente selbst noch einmal
/// deuten und der Test würde genau das messen, was er ausschliessen soll. Perl liest
/// `@ARGV` unverändert.
fn recorder(dir: &Path) -> PathBuf {
    let program = dir.join("recorder");
    let output = dir.join("argv.txt");

    let mut file = std::fs::File::create(&program).unwrap();
    writeln!(
        file,
        "#!/usr/bin/perl\n\
         open(my $fh, '>', '{}') or die $!;\n\
         print $fh \"$_\\n\" for @ARGV;\n\
         close($fh);",
        output.display()
    )
    .unwrap();
    drop(file);

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

/// Wartet, bis der gestartete Prozess geschrieben hat. Er läuft nebenläufig, deshalb wird
/// gepollt statt gewartet.
fn read_when_written(path: &Path) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(path)
            && !content.is_empty()
        {
            return content.lines().map(str::to_string).collect();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("der gestartete Prozess hat nichts geschrieben");
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gatekeeper-launch-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn arguments_arrive_exactly_as_given() {
    let dir = workspace("plain");
    let program = recorder(&dir);

    let argv = vec![
        program.to_string_lossy().into_owned(),
        "--new-window".to_string(),
        "https://example.com/path?a=1&b=2#frag".to_string(),
    ];
    DirectLauncher.launch(&LaunchRequest { argv: argv.clone(), env: Vec::new() }).unwrap();

    assert_eq!(read_when_written(&dir.join("argv.txt")), argv[1..]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn shell_metacharacters_reach_the_target_uninterpreted() {
    let dir = workspace("hostile");
    let program = recorder(&dir);

    // Käme irgendwo auf dem Weg eine Shell ins Spiel, würde hier nicht diese Zeichenkette
    // ankommen, sondern deren Ergebnis, und im Zweifel wäre nebenbei etwas ausgeführt
    // worden.
    let hostile = "https://example.com/;touch /tmp/gatekeeper-should-never-exist;$(id)`id`&|<>";
    let marker = Path::new("/tmp/gatekeeper-should-never-exist");
    let _ = std::fs::remove_file(marker);

    DirectLauncher
        .launch(&LaunchRequest {
            argv: vec![program.to_string_lossy().into_owned(), hostile.to_string()],
            env: Vec::new(),
        })
        .unwrap();

    let received = read_when_written(&dir.join("argv.txt"));
    assert_eq!(received, [hostile], "die URL kam verändert an");
    assert!(!marker.exists(), "es wurde etwas ausgeführt, das nur Text sein durfte");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn spaces_and_quotes_do_not_split_an_argument() {
    let dir = workspace("quoting");
    let program = recorder(&dir);

    let awkward = r#"https://example.com/a b "c" 'd' \e"#;
    DirectLauncher
        .launch(&LaunchRequest {
            argv: vec![program.to_string_lossy().into_owned(), awkward.to_string()],
            env: Vec::new(),
        })
        .unwrap();

    assert_eq!(read_when_written(&dir.join("argv.txt")), [awkward]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reports_a_missing_program_instead_of_failing_silently() {
    let result = DirectLauncher.launch(&LaunchRequest {
        argv: vec!["/nirgendwo/gibt/es/diesen/browser".to_string()],
        env: Vec::new(),
    });

    assert!(result.is_err(), "ein fehlendes Programm muss auffallen");
}

// --------------------------------------------------------------------------------------
// Die ganze Kette
// --------------------------------------------------------------------------------------

/// Von der Desktop-Datei bis zum empfangenen Argument, ohne Abkürzung.
///
/// Discovery liest den Eintrag, die Exec-Auflösung setzt den Feldcode ein, der Launcher
/// startet. Jeder einzelne Schritt ist anderswo geprüft; hier geht es darum, dass sie
/// zusammen das Richtige tun.
#[test]
fn a_desktop_entry_leads_to_the_right_command_line() {
    use gatekeeper_core::discovery::{DiscoveryOptions, SearchPath, SourceKind, discover};
    use gatekeeper_core::exec::{FieldContext, build_argv};
    use gatekeeper_core::uri::TargetUri;

    let dir = workspace("full-chain");
    let apps = dir.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    let program = recorder(&dir);

    std::fs::write(
        apps.join("fake-browser.desktop"),
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Fake Browser\n\
             Exec={} --new-window %u\n\
             MimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
            program.display()
        ),
    )
    .unwrap();

    let options = DiscoveryOptions {
        search_paths: vec![SearchPath::new(&apps, SourceKind::System)],
        self_desktop_id: "me.rueegger.Gatekeeper.desktop".to_string(),
        locale: None,
        current_desktops: vec!["KDE".to_string()],
        program_dirs: Vec::new(),
    };

    let browsers = discover(&options);
    assert_eq!(browsers.len(), 1, "der Eintrag muss gefunden werden");

    let target = TargetUri::parse("https://example.com/a?b=c#d").unwrap();
    let uris = vec![target.as_str().to_string()];
    let argv =
        build_argv(&browsers[0].exec, &FieldContext { uris: &uris, ..FieldContext::default() })
            .unwrap();

    DirectLauncher.launch(&LaunchRequest { argv, env: Vec::new() }).unwrap();

    assert_eq!(
        read_when_written(&dir.join("argv.txt")),
        ["--new-window", "https://example.com/a?b=c#d"],
        "der Feldcode wurde durch die geprüfte URL ersetzt, sonst nichts"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
