# Architecture Decision Records

Append-only. Alte Einträge werden nicht umgeschrieben, sondern als „superseded by ADR-N" markiert.

---

## ADR-1: Zielbrowser werden direkt gestartet, nie über `xdg-open` oder ein Portal

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

## ADR-2: `flatpak-spawn --host` als Startmechanismus, `--talk-name=org.freedesktop.Flatpak` als Preis

**Status**: akzeptiert (2026-08-23)

**Kontext**: Aus ADR-1 folgt, dass wir ein Host-Programm direkt starten müssen. Aus der
Flatpak-Sandbox geht das nur über das Host-Command-Portal.

**Entscheidung**: Start über `flatpak-spawn --host -- <argv>`. Das Manifest deklariert
`--talk-name=org.freedesktop.Flatpak`.

**Konsequenz**: Die Sandbox ist faktisch offen, denn die Berechtigung erlaubt beliebige
Host-Befehle.
Das muss bei einer Flathub-Einreichung begründet werden; Präzedenzfall ist Junction
(`re.sonny.Junction`). Die Dateisystem-Berechtigungen bleiben trotzdem granular, damit der
tatsächliche Bedarf sichtbar bleibt und die App ohne Portal-Zugriff wenigstens noch anzeigen kann.

---

## ADR-3: Deduplizierung über die normalisierte `Exec`-Zeile, nicht über die Desktop-ID

**Status**: akzeptiert (2026-08-23)

**Kontext**: Auf dem Entwicklungssystem (TUXEDO OS, Debian-Basis) existieren
`/usr/share/applications/brave-origin.desktop` und
`/usr/share/applications/com.brave.Origin.desktop`. Beide zeigen auf
`/usr/bin/brave-origin-stable`, letztere mit `NoDisplay=true`. Dedup über die Desktop-ID würde
denselben Browser zweimal anbieten.

**Entscheidung**: Primärschlüssel für die Dedup ist die normalisierte `Exec`-Zeile: Feldcodes
entfernt, Programmname über `PATH` und Symlinks aufgelöst, bei Flatpak zusätzlich die App-ID
extrahiert. Innerhalb einer Gruppe gewinnt `NoDisplay=false`, dann das höherpriore Verzeichnis,
dann die Reverse-DNS-ID.

**Konsequenz**: `NoDisplay=true` verwirft einen Eintrag nicht hart. Er ist nach Spec ein gültiger
Handler, nur kein Menüeintrag.

---

## ADR-4: Rust-Kern, C++/QML-Schale, CMake als Dach

**Status**: akzeptiert (2026-08-23)

**Kontext**: Qt 6 ist gesetzt, Rust bevorzugt. Für Rust-Qt-Bindings kommt praktisch nur cxx-qt
(0.9.1, KDAB) in Frage. Flatpak-Builds sind offline, Cargo-Abhängigkeiten müssen vendored werden.
Ein reiner cxx-qt-Aufbau legt Rust, Qt-Buildtooling, offline-Vendoring und QML-Modulregistrierung
gleichzeitig auf den kritischen Pfad, und das für eine UI-Schicht von rund 300 Zeilen.

**Entscheidung**: Die gesamte Logik (Discovery, Parsing, Dedup, Feldcodes, Regeln, Launcher) liegt
in `gatekeeper-core` in Rust. Die Qt-Schicht ist C++ mit QML (Qt Quick Controls 2). Gebrückt wird
mit `cxx` über eine kleine, stabile Fläche; eingebunden über Corrosion in CMake. `main()` liegt in
C++ und ruft zuerst `resolve(url)` im Rust-Kern auf. Greift eine Regel, wird `QGuiApplication` nie
konstruiert.

**Konsequenz**: Zwei Build-Systeme und eine FFI-Grenze. Dafür bleibt der Umgang mit Fremdeingaben
vollständig in Rust, der Flatpak-Build folgt dem eingefahrenen KDE-Pfad, und das vorhandene
Qt-Tooling (CMake, qmllint, qmltestrunner, Qt-Doku) ist nutzbar. Wird in M0 durch einen Spike
abgesichert; scheitert der, wird diese Entscheidung neu getroffen.

---

## ADR-5: App-ID `me.rueegger.Gatekeeper`

**Status**: akzeptiert (2026-08-23)

**Kontext**: Flatpak-App-IDs sind Reverse-DNS einer Domain, über die der Herausgeber verfügt.
Flathub schreibt für das letzte Segment üblicherweise Grossschreibung vor. Der ursprüngliche
Vorschlag lautete `me.rueegger.gatekeeper`, in Frage kam ausserdem `dev.rueegger.Gatekeeper`.

**Entscheidung**: `me.rueegger.Gatekeeper`. Domain `rueegger.me`, letztes Segment gross.

**Konsequenz**: Desktop-ID ist `me.rueegger.Gatekeeper.desktop`, der Sandbox-Zustand liegt unter
`~/.var/app/me.rueegger.Gatekeeper/`. Die ID ist ab jetzt festgeschrieben, denn sie hängt in
`.desktop`, AppStream-Metainfo, Sandbox-Pfaden, dem Selbstfilter der Discovery (Invariante 1) und
der Default-Browser-Registrierung. Ein späterer Wechsel wäre teuer.

---

## ADR-6: Verteilung zunächst über das eigene Flatpak-Repo

**Status**: akzeptiert (2026-08-23)

**Kontext**: Ein `rueegger-dev`-Flatpak-Remote existiert bereits. Flathub würde für
`--talk-name=org.freedesktop.Flatpak` (ADR-2) eine Review-Begründung verlangen und die Iteration
verlangsamen, solange die App noch nicht steht.

**Entscheidung**: Ausgeliefert wird zunächst über das eigene Repo. Flathub bleibt als späteres Ziel
offen, bestimmt aber nicht die frühen Entscheidungen.

**Konsequenz**: Metainfo und Manifest werden trotzdem sauber gehalten, damit eine spätere
Einreichung kein Umbau wird. Die Berechtigungen bleiben granular begründet (ADR-2), auch wenn
niemand sie vorerst prüft.

---

## ADR-7: Cargo wird direkt aus CMake aufgerufen, ohne Corrosion

**Status**: akzeptiert (2026-08-23). Verfeinert ADR-4.

**Kontext**: ADR-4 nannte „CMake + Corrosion" als Bindeglied zwischen Rust und C++. Corrosion
ist in Debian als Paket verfügbar, seine cxx-Unterstützung (`corrosion_add_cxxbridge`) setzt
aber zusätzlich das Kommandozeilenwerkzeug `cxxbridge` voraus. Flatpak baut ohne Netz, also
müssten für den Paketbau sowohl Corrosion als auch `cxxbridge` eigens mitgeliefert und gebaut
werden. Gleichzeitig erzeugt `cxx_build` in `build.rs` Header und Glue-Code bereits innerhalb
des normalen Cargo-Laufs, ganz ohne Zusatzwerkzeug.

**Entscheidung**: Kein Corrosion. CMake ruft `cargo build` über ein `add_custom_command` auf,
setzt `CARGO_TARGET_DIR` auf ein bekanntes Verzeichnis und bindet die entstehende `staticlib`
über eine INTERFACE-Bibliothek ein, die zugleich den von cxx erzeugten Header-Pfad und die
Systembibliotheken (`pthread`, `dl`, `m`) mitbringt. Der Schalter `GK_CARGO_OFFLINE` schaltet
`--offline --locked` zu, wie es der Flatpak-Bau braucht.

**Konsequenz**: Rund 40 Zeilen CMake, die wir selbst pflegen, dafür zwei Abhängigkeiten
weniger im Paketbau. Was Corrosion sonst noch abnimmt, betrifft uns nicht: Es gibt genau ein
Ziel, genau eine Plattform und keine Cross-Kompilierung. Käme das dazu, ist die Entscheidung
neu zu treffen.

---

## ADR-8: Daten erreichen QML über einen registrierten Singleton, nicht über Context-Properties

**Status**: akzeptiert (2026-08-23)

**Kontext**: Der erste Entwurf reichte Browserliste und Ziel-URL mit
`QQmlContext::setContextProperty` an QML. Das funktioniert, ist aber für `qmllint` und den
QML-Compiler unsichtbar: Jeder Zugriff darauf gilt als unqualifiziert, wird nicht typgeprüft
und nicht zu C++ kompiliert. `qmllint` meldete das entsprechend.

**Entscheidung**: Ein `Session`-Typ mit `QML_ELEMENT` und `QML_SINGLETON` trägt die Daten als
`Q_PROPERTY`. QML importiert `GatekeeperUi` und greift über `Session.browsers`,
`Session.targetUri` und `Session.targetHost` zu. Die Auswahl geht über `Session.choose(index)`
zurück nach C++.

**Konsequenz**: `qmllint` läuft ohne Befund, die Zugriffe sind typgeprüft und `qmlcachegen`
kann sie übersetzen. Preis ist eine zusätzliche Header/Quelldatei und ein statischer Zeiger,
der vor dem Laden der QML-Wurzel gesetzt wird.

---

## ADR-9: Hostpfade in der Sandbox über `/run/host`, XDG-Variablen dort ignorieren

**Status**: akzeptiert (2026-08-23)

**Kontext**: Der erste Entwurf des Manifests forderte `--filesystem=/usr/share/applications:ro`
und ähnliche Pfade an. Flatpak lehnt das ab: „Not sharing /usr/share/applications with sandbox:
Path /usr is reserved by Flatpak". In der Sandbox gehört `/usr` der Runtime. Der Scan fand
dort 13 Anwendungen der KDE-Runtime und keinen einzigen Browser des Hosts.

Ebenso unbrauchbar sind in der Sandbox die XDG-Variablen. `XDG_DATA_HOME` zeigt auf
`~/.var/app/me.rueegger.Gatekeeper/data`, also auf unser eigenes Datenverzeichnis, nicht auf
das Home des Nutzers. `XDG_DATA_DIRS` enthält `/app/share` und `/usr/share`; unter
`/app/share/applications` steht ausgerechnet unser eigener Desktop-Eintrag.

**Entscheidung**: Das Manifest fordert `--filesystem=host-os:ro` an; das `/usr` des Hosts
erscheint dann unter `/run/host/usr`. Der Kern erkennt die Sandbox an `/.flatpak-info` und
bildet die Suchpfade dann ausdrücklich, ohne `XDG_DATA_HOME` und `XDG_DATA_DIRS`: das Home
des Nutzers über `$HOME`, die Systemverzeichnisse über `/run/host/usr`, die Export-Pfade von
Flatpak und Snap unter `/var` unverändert. Aus demselben Grund werden `TryExec` und
nicht-absolute Programme in der Sandbox gegen `/run/host/usr/bin` aufgelöst statt gegen den
`PATH` der Runtime.

**Konsequenz**: Die Pfadbildung hat zwei Zweige, die getrennt getestet werden. Ein Test hält
ausdrücklich fest, dass in der Sandbox weder `/usr/share/applications` noch `/app/...` noch
`~/.var/app/...` gescannt werden. Gäbe es diese Trennung nicht, böte Gatekeeper Anwendungen
der Runtime als Browser an und im schlimmsten Fall sich selbst.

Bestätigt am laufenden Paket: Sandbox und Host finden dieselben drei Browser.
