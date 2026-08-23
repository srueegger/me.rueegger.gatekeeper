# Architecture Decision Records

Append-only. Alte Einträge werden nicht umgeschrieben, sondern als „superseded by ADR-N" markiert.

---

## ADR-1 — Zielbrowser werden direkt gestartet, nie über `xdg-open` oder ein Portal

**Status**: akzeptiert (2026-08-23)

**Kontext**: Gatekeeper registriert sich als Default-Handler für `http`/`https`. Jeder generische
Weg, eine URL zu öffnen, schlägt den Default-Handler nach: `xdg-open`, `gio open`,
`org.freedesktop.portal.OpenURI` und `QDesktopServices::openUrl()` (Qt-Doku: *„Opens the given url
in the appropriate Web browser for the user's desktop environment"*). Alle vier landen wieder bei
uns.

**Entscheidung**: Der Zielbrowser wird ausschliesslich über die `Exec`-Zeile seiner eigenen
`.desktop`-Datei gestartet, aufgelöst nach Desktop Entry Spec, als `argv`-Array, ohne Shell.
`QDesktopServices` wird nicht verlinkt; CI prüft per Grep auf Verstösse.

**Konsequenz**: Wir müssen `Exec`-Feldcodes selbst korrekt implementieren, inklusive Quoting. Dafür
gibt es keine Endlosschleife, und das Verhalten ist unit-testbar.

---

## ADR-2 — `flatpak-spawn --host` als Startmechanismus, `--talk-name=org.freedesktop.Flatpak` als Preis

**Status**: akzeptiert (2026-08-23)

**Kontext**: Aus ADR-1 folgt, dass wir ein Host-Programm direkt starten müssen. Aus der
Flatpak-Sandbox geht das nur über das Host-Command-Portal.

**Entscheidung**: Start über `flatpak-spawn --host -- <argv>`. Das Manifest deklariert
`--talk-name=org.freedesktop.Flatpak`.

**Konsequenz**: Die Sandbox ist faktisch offen — die Berechtigung erlaubt beliebige Host-Befehle.
Das muss bei einer Flathub-Einreichung begründet werden; Präzedenzfall ist Junction
(`re.sonny.Junction`). Die Dateisystem-Berechtigungen bleiben trotzdem granular, damit der
tatsächliche Bedarf sichtbar bleibt und die App ohne Portal-Zugriff wenigstens noch anzeigen kann.

---

## ADR-3 — Deduplizierung über die normalisierte `Exec`-Zeile, nicht über die Desktop-ID

**Status**: akzeptiert (2026-08-23)

**Kontext**: Auf dem Entwicklungssystem (TUXEDO OS, Debian-Basis) existieren
`/usr/share/applications/brave-origin.desktop` und
`/usr/share/applications/com.brave.Origin.desktop`. Beide zeigen auf
`/usr/bin/brave-origin-stable`, letztere mit `NoDisplay=true`. Dedup über die Desktop-ID würde
denselben Browser zweimal anbieten.

**Entscheidung**: Primärschlüssel für die Dedup ist die normalisierte `Exec`-Zeile — Feldcodes
entfernt, Programmname über `PATH` und Symlinks aufgelöst, bei Flatpak zusätzlich die App-ID
extrahiert. Innerhalb einer Gruppe gewinnt `NoDisplay=false`, dann das höherpriore Verzeichnis,
dann die Reverse-DNS-ID.

**Konsequenz**: `NoDisplay=true` verwirft einen Eintrag nicht hart — er ist nach Spec ein gültiger
Handler, nur kein Menüeintrag.

---

## ADR-4 — Rust-Kern, C++/QML-Schale, CMake als Dach

**Status**: akzeptiert (2026-08-23)

**Kontext**: Qt 6 ist gesetzt, Rust bevorzugt. Für Rust-Qt-Bindings kommt praktisch nur cxx-qt
(0.9.1, KDAB) in Frage. Flatpak-Builds sind offline, Cargo-Abhängigkeiten müssen vendored werden.
Ein reiner cxx-qt-Aufbau legt Rust, Qt-Buildtooling, offline-Vendoring und QML-Modulregistrierung
gleichzeitig auf den kritischen Pfad — für eine UI-Schicht von rund 300 Zeilen.

**Entscheidung**: Die gesamte Logik (Discovery, Parsing, Dedup, Feldcodes, Regeln, Launcher) liegt
in `gatekeeper-core` in Rust. Die Qt-Schicht ist C++ mit QML (Qt Quick Controls 2). Gebrückt wird
mit `cxx` über eine kleine, stabile Fläche; eingebunden über Corrosion in CMake. `main()` liegt in
C++ und ruft zuerst `resolve(url)` im Rust-Kern auf — greift eine Regel, wird `QGuiApplication` nie
konstruiert.

**Konsequenz**: Zwei Build-Systeme und eine FFI-Grenze. Dafür bleibt der Umgang mit Fremdeingaben
vollständig in Rust, der Flatpak-Build folgt dem eingefahrenen KDE-Pfad, und das vorhandene
Qt-Tooling (CMake, qmllint, qmltestrunner, Qt-Doku) ist nutzbar. Wird in M0 durch einen Spike
abgesichert; scheitert der, wird diese Entscheidung neu getroffen.

---

## ADR-5 — App-ID `me.rueegger.Gatekeeper`

**Status**: akzeptiert (2026-08-23)

**Kontext**: Flatpak-App-IDs sind Reverse-DNS einer Domain, über die der Herausgeber verfügt.
Flathub schreibt für das letzte Segment üblicherweise Grossschreibung vor. Der ursprüngliche
Vorschlag lautete `me.rueegger.gatekeeper`, in Frage kam ausserdem `dev.rueegger.Gatekeeper`.

**Entscheidung**: `me.rueegger.Gatekeeper`. Domain `rueegger.me`, letztes Segment gross.

**Konsequenz**: Desktop-ID ist `me.rueegger.Gatekeeper.desktop`, der Sandbox-Zustand liegt unter
`~/.var/app/me.rueegger.Gatekeeper/`. Die ID ist ab jetzt festgeschrieben — sie hängt in
`.desktop`, AppStream-Metainfo, Sandbox-Pfaden, dem Selbstfilter der Discovery (Invariante 1) und
der Default-Browser-Registrierung. Ein späterer Wechsel wäre teuer.

---

## ADR-6 — Verteilung zunächst über das eigene Flatpak-Repo

**Status**: akzeptiert (2026-08-23)

**Kontext**: Ein `rueegger-dev`-Flatpak-Remote existiert bereits. Flathub würde für
`--talk-name=org.freedesktop.Flatpak` (ADR-2) eine Review-Begründung verlangen und die Iteration
verlangsamen, solange die App noch nicht steht.

**Entscheidung**: Ausgeliefert wird zunächst über das eigene Repo. Flathub bleibt als späteres Ziel
offen, bestimmt aber nicht die frühen Entscheidungen.

**Konsequenz**: Metainfo und Manifest werden trotzdem sauber gehalten, damit eine spätere
Einreichung kein Umbau wird. Die Berechtigungen bleiben granular begründet (ADR-2), auch wenn
niemand sie vorerst prüft.
