# Gatekeeper

Ein Auswahldialog für Browser unter Linux. Statt dass jeder Link stur im selben Browser landet,
fragt Gatekeeper kurz nach und zeigt dabei, wohin die URL eigentlich führt.

Gatekeeper registriert sich als Standardbrowser. Klickst du irgendwo auf einen Link, im
Mailprogramm, im Chat, im Terminal oder in einem PDF, erscheint ein kleiner Dialog mit allen
installierten Browsern. Einer davon bekommt die URL.

```
Link öffnen mit
github.com
https://www.github.com/anthropics/claude-code

 1  Brave Origin              System
 2  Chromium Webbrowser       System
 3  Firefox Web Browser       System

 ☐ Für github.com merken
```

## Status

Benutzbar, aber noch nicht veröffentlicht. Es gibt kein Paketrepositorium, aus dem sich Gatekeeper
installieren liesse; wer es ausprobieren will, baut es selbst. Die Anleitung steht weiter unten.

Was funktioniert: Browser finden und starten, quer über native Pakete, Flatpaks und Snaps.
Registrierung als Standardbrowser samt Selbstprüfung bei jedem Start. Regeln, die eine Domain
ohne Rückfrage einem Browser zuordnen. Auswahl per Maus, Pfeiltasten oder Zifferntaste. Das Ganze
ist als Flatpak verpackt und baut ohne Netzzugriff.

Was fehlt: Desktop-Actions wie „privates Fenster" werden zwar aus den Desktop-Dateien gelesen und
lassen sich in Regeln benennen, im Dialog aber noch nicht auswählen. Ebenso fehlen ein Editor für
Regeln, die Wahl von Firefox-Profilen und das Auflösen von Weiterleitungs-URLs.

Die technische Analyse liegt in [docs/ANALYSE.md](docs/ANALYSE.md), die getroffenen
Architekturentscheidungen mit ihrer Begründung in [docs/DECISIONS.md](docs/DECISIONS.md).

## Wozu das gut ist

Mehrere Browser nebeneinander zu betreiben ist normal geworden: einer für die Arbeit, einer für
Privates, einer zum Testen. Der Desktop kennt aber nur einen Standard, und jeder Link landet dort.

Gatekeeper schiebt sich dazwischen und lässt dich bei jedem Link entscheiden. Wo du nicht jedes Mal
entscheiden willst, hinterlegst du eine Regel. Greift eine Regel, erscheint gar kein Dialog, und
der Browser startet nach rund 60 Millisekunden, weil in diesem Fall nicht einmal die Oberfläche
aufgebaut wird.

Nebenbei bekommst du zu sehen, wohin ein Link tatsächlich zeigt, bevor er geöffnet wird. Die
Zieldomain steht hervorgehoben im Dialog. Gegen Links, die anders aussehen als sie sind, hilft das
mehr als jede Warnmeldung nach dem Klick.

## Findet Browser aus allen Quellen

Nativ installierte Browser, Flatpaks und Snaps stehen in derselben Liste, jeder mit einem Vermerk,
woher er stammt. Dieselbe Anwendung mehrfach zu installieren ist damit kein Problem: Der native
Firefox und der Flatpak-Firefox sind verschiedene Einträge, weil sie verschiedene Profile haben.

Doppelte Einträge derselben Installation werden dagegen zusammengefasst. Manche Distributionen
registrieren einen Browser unter zwei Namen, etwa `brave-origin.desktop` und
`com.brave.Origin.desktop`. In der Liste steht er trotzdem nur einmal.

## Regeln

Regeln stehen in `~/.config/gatekeeper/rules.toml`, als Flatpak unter
`~/.var/app/me.rueegger.Gatekeeper/config/gatekeeper/rules.toml`. Die erste passende Regel gewinnt,
die Reihenfolge in der Datei ist also die Rangfolge.

```toml
[[rule]]
host = "github.com"           # gilt auch für www.github.com und gist.github.com
browser = "firefox.desktop"

[[rule]]
host = "*.intranet.example"   # nur Subdomains, nicht intranet.example selbst
browser = "chromium.desktop"
action = "new-private-window"

[[rule]]
url = "^https://docs\\."      # regulärer Ausdruck auf die vollständige URL
browser = "firefox.desktop"

[[rule]]
scheme = "file"               # lokale Dateien immer hierhin
browser = "firefox.desktop"
```

Der Name unter `browser` ist die Desktop-ID. Welche zur Auswahl stehen, zeigt
`gatekeeper --list`. Sind an einer Regel mehrere Muster gesetzt, müssen alle zutreffen.

Die Checkbox im Dialog hängt eine Regel für die aktuelle Domain an. Angehängt, nicht
vorangestellt: Was du von Hand geschrieben hast, wird nicht von einem Klick überstimmt.

Ob eine Regeldatei tut, was sie soll, lässt sich ohne Klicken nachsehen:

```
cargo run --example check-rules -- rules.toml https://gist.github.com/x https://sonstwo.example
```

## Von Hand aufrufen

```
gatekeeper <url>                        # fragen, oder Regel anwenden
gatekeeper --ask <url>                  # Regeln überspringen und in jedem Fall fragen
gatekeeper --list [url]                 # zeigen, was gefunden wird, ohne Dialog
gatekeeper --launch <desktop-id> <url>  # ohne Dialog starten
```

`--list` nennt auch, wer gerade Standardbrowser ist. Als Flatpak entsprechend mit
`flatpak run me.rueegger.Gatekeeper --list`.

## Selbst bauen

Der Kern lässt sich ohne Qt bauen und testen, dafür genügt Rust:

```
cargo test
cargo run --example scan -- https://github.com/user/repo
```

Für die vollständige Anwendung kommen Qt 6 und CMake dazu:

```
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build
./build/gatekeeper https://example.com
```

Als Flatpak, mit `org.kde.Sdk` in Version 6.10 und der Rust-Erweiterung des Freedesktop-SDK:

```
flatpak install flathub org.kde.Sdk//6.10 org.kde.Platform//6.10 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08

cd build-aux
flatpak-builder --force-clean --user \
    --state-dir=/pfad/state --repo=/pfad/repo /pfad/builddir \
    me.rueegger.Gatekeeper.yaml
```

Zustandsverzeichnis, Baubaum und Repositorium müssen auf demselben Dateisystem liegen, sonst
scheitert der Export daran, dass ostree Hardlinks braucht.

Der Paketbau läuft ohne Netzzugriff; die Rust-Abhängigkeiten liegen als
`build-aux/generated-sources.json` vor. Wer Abhängigkeiten ändert, erzeugt die Datei neu:

```
python3 build-aux/flatpak-cargo-generator.py Cargo.lock -o build-aux/generated-sources.json
```

## Aufbau

Die gesamte Logik liegt in Rust und kommt ohne Oberfläche aus: Verzeichnisse scannen,
Desktop-Dateien lesen, Duplikate zusammenfassen, Regeln prüfen, Startbefehle auflösen, Prozesse
starten. Darüber liegt eine dünne Schicht aus C++ und QML, die nur den Dialog zeigt. Trifft eine
Regel, wird sie nie angefasst.

Das ist keine Stilfrage: Die Prüfung liegt im Klickpfad jedes Links, und was dort passiert, soll
so wenig kosten wie möglich.

## Lizenz

GNU General Public License, Version 2. Ausschliesslich diese Version, ohne die übliche Klausel
„oder eine spätere Version". Der vollständige Text steht in [LICENSE](LICENSE).
