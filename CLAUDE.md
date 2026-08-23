# Gatekeeper: Projektinstruktionen für Claude

## Was ist das

**Gatekeeper** (App-ID `me.rueegger.Gatekeeper`) ist eine Linux-Desktop-App, die sich als
Standardbrowser registriert. Klickt der Nutzer irgendwo auf einen Link (E-Mail-Client, Chat,
Terminal, PDF-Viewer), bekommt nicht ein fest verdrahteter Browser die URL, sondern Gatekeeper.
Gatekeeper zeigt einen schnellen Auswahldialog mit allen installierten Browsern, ob nativ, Flatpak
oder Snap, und startet den gewählten mit der URL.

Vertrieb: **Flatpak**. UI: **Qt 6 / QML**. Logik: **Rust**.
Begründung und Details in `docs/ANALYSE.md`, Entscheidungen in `docs/DECISIONS.md`.

## Nicht-verhandelbare Invarianten

Der Kern des Projekts. Änderungen daran nie ohne Rückfrage:

1. **Kein Selbstaufruf.** Gatekeeper darf sich niemals selbst als Browser anbieten oder starten.
   Jede Discovery-Quelle filtert die eigene Desktop-ID heraus, mit eigenem Test je Quelle. Ein
   Fehler hier erzeugt eine Endlosschleife, die die Sitzung lahmlegt.
2. **Kein `xdg-open`, kein Portal, kein `QDesktopServices`.** Alle drei schlagen den
   Default-Handler nach, und der sind wir (ADR-1). Zielbrowser werden immer direkt über ihre
   `Exec`-Zeile gestartet.
3. **Nie über eine Shell starten.** URLs sind Fremdeingabe. Immer `argv`-Arrays, nie
   String-Interpolation in `sh -c`. URLs mit führendem `-` werden nie als Argument durchgereicht.
4. **Gatekeeper muss ohne Flatpak lauffähig bleiben.** Der Launcher liegt hinter dem Trait
   `Launcher`: `DirectLauncher` nativ, `HostSpawnLauncher` in der Sandbox. Sonst ist lokales
   Entwickeln unzumutbar. `RecordingLauncher` startet nichts, sondern zeichnet auf, und macht
   Invariante 3 überprüfbar statt nur beabsichtigt.

## Architektur

`gatekeeper-core` (Rust) macht die gesamte Arbeit ohne GUI: XDG-Verzeichnisse scannen,
Desktop-Entries parsen, Browser deduplizieren, Herkunft bestimmen, Regeln matchen, `Exec`-Feldcodes
auflösen, Prozess starten. Headless testbar, keine Qt-Abhängigkeit.

Darüber eine dünne Qt-6-Schicht in C++ mit QML, angebunden über `cxx` (ADR-4). CMake ruft Cargo
direkt auf, ohne Corrosion (ADR-7). `main()` liegt in C++ und befragt zuerst den Rust-Kern. Wird
das Ziel abgelehnt oder greift später eine gespeicherte Regel, endet der Aufruf, bevor
`QGuiApplication` überhaupt konstruiert wird.

Daten erreichen QML ausschliesslich über den registrierten Singleton `Session`, nie über
Context-Properties (ADR-8). Context-Properties sind für `qmllint` und `qmlcachegen` unsichtbar.

## Verzeichnisse

```
crates/gatekeeper-core/   Discovery, Parsing, Regeln, Launcher. Keine GUI, voll unit-testbar
                          rules.toml liegt in $XDG_CONFIG_HOME/gatekeeper/
crates/gatekeeper-ffi/    cxx-Bridge, schmale und stabile Fläche zum C++-Teil
src/                      C++/QML-Frontend, main() und Session
src/qml/                  QML-Modul GatekeeperUi
data/                     .desktop, AppStream-Metainfo, Icons
build-aux/                Flatpak-Manifest, flatpak-cargo-generator
docs/ANALYSE.md           Technische Analyse, Stack-Entscheidung, Risiken
docs/DECISIONS.md         Architecture Decision Records, append-only
tests/fixtures/           Echte .desktop-Dateien (nativ/Flatpak/Snap) als Testdaten
```

## Bauen

```
cargo test                                        # nur der Kern, ohne Qt
cargo run --example scan -- https://example.com   # Kern gegen das laufende System

cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build
cmake --build build --target all_qmllint
```

Als Flatpak. Zustandsverzeichnis, Baubaum und Repo müssen auf demselben Dateisystem liegen,
sonst scheitert der Export an fehlenden Hardlinks:

```
cd build-aux
flatpak-builder --force-clean --user \
    --state-dir=/pfad/state --repo=/pfad/repo /pfad/builddir \
    me.rueegger.Gatekeeper.yaml
```

Zwei Schalter zum Nachsehen, beide ohne Dialog:

```
gatekeeper --list [url]                       # was gefunden wird, mit aufgelöstem argv
gatekeeper --launch <desktop-id> <url>        # ohne Dialog starten
gatekeeper --ask <url>                        # Regeln überspringen und fragen
```

`--list` ist vor allem in der Sandbox nützlich, wo sich von aussen nicht nachvollziehen
lässt, welche Verzeichnisse ankommen. `--launch` nimmt denselben Weg, den später ein
Regeltreffer nimmt: kein Fenster, keine `QGuiApplication`.

Die Konfiguration ist warnungsfrei und soll es bleiben. Eine neue Warnung ist ein Befund, kein
Rauschen.

Die Oberfläche lässt sich ohne Bildschirm prüfen:

```
QT_QPA_PLATFORM=offscreen GATEKEEPER_GRAB=/pfad/fenster.png ./build/gatekeeper https://example.com
```

Das zeichnet das Fenster in eine Datei und beendet sich. Bei QML-Änderungen bitte benutzen: Ein
leeres Fenster und fehlende Symbole erzeugen keine einzige Warnung.

## Arbeitsweise

- **Regelmässig committen.** Jede abgeschlossene, für sich sinnvolle Einheit bekommt einen Commit.
  Nicht am Ende einer langen Session alles in einen Klumpen werfen.
- **Commit-Messages**: [Conventional Commits](https://www.conventionalcommits.org), Englisch,
  Imperativ. Scopes: `core`, `ffi`, `ui`, `flatpak`, `docs`, `ci`, `data`.
  Beispiel: `feat(core): dedupe desktop entries by normalized exec line`
- **Kein Hinweis auf KI/Claude/Anthropic** in Commits, Merge-Commits, PR-Titeln oder
  PR-Beschreibungen. Keine Co-Authored-By-Trailer.
- **CLAUDE.md pflegen.** Ändert sich eine Invariante, ein Verzeichnis oder eine Konvention, wird
  diese Datei im selben Commit mitgezogen.
- **Architekturentscheidungen** kommen als ADR nach `docs/DECISIONS.md`: Kontext, Entscheidung,
  Konsequenz. Append-only; überholte Einträge werden als „superseded by ADR-N" markiert, nicht
  umgeschrieben.
- Vor jedem Commit: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
  Bei QML-Änderungen zusätzlich `cmake --build build --target all_qmllint`.
- `unsafe` ist im Workspace verboten. Einzige Ausnahme ist `gatekeeper-ffi`, weil der von cxx
  erzeugte Code daraus besteht. Die Ausnahme bleibt auf diese Kiste beschränkt.

## Testen

Der Kern wird gegen echte `.desktop`-Fixtures getestet, nicht gegen erfundene. Neue Browser oder
neue Paketformate kommen als Fixture nach `tests/fixtures/` plus Testfall. Beim Parser gilt: lieber
eine kaputte Desktop-Datei überspringen und loggen als den ganzen Scan abbrechen. Auf echten
Systemen liegt immer irgendwo Müll.

Der Launcher wird gegen einen Fake getestet, der das `argv`-Array aufzeichnet statt zu starten.
Damit ist Invariante 3 überprüfbar und nicht bloss Vorsatz.

## Bekannte Stolpersteine

- **Dedup-Duplikate**: Auf einem realen Debian-System existieren `brave-origin.desktop` und
  `com.brave.Origin.desktop` mit identischer Exec-Zeile, zweitere mit `NoDisplay=true`. Dedup über
  normalisierte Exec-Zeile, nicht über die Desktop-ID (ADR-3).
- **`NoDisplay=true` ist kein Ausschlusskriterium.** Nach Spec heisst es „nicht im Menü zeigen",
  nicht „kein gültiger Handler". Nur `Hidden=true` bedeutet gelöscht.
- **Icons**: Qt findet in der Sandbox die Icon-Themes des Hosts nicht. `QIcon::setThemeSearchPaths()`
  und `setFallbackSearchPaths()` müssen um die Host-Pfade erweitert werden.
- **Wayland-Fokus**: `XDG_ACTIVATION_TOKEN` muss an den Zielbrowser durchgereicht werden, sonst
  erscheint dessen Fenster ohne Fokus.
- **QML-Singletons dürfen nicht standardkonstruierbar sein.** Qt entscheidet in
  `singletonConstructionMode()` zuerst über `std::is_default_constructible`, erst danach über
  `create()`. Ein `QObject *parent = nullptr` im Konstruktor schaltet die Fabrik lautlos ab, QML
  bekommt eine zweite leere Instanz und die App startet mit leerem Fenster, ohne Fehlermeldung.
  In `Session.h` hält ein `static_assert` das fest.
- **`image://theme/...` gibt es in Qt Quick nicht.** Ein `Image` mit dieser Quelle bleibt leer.
  Symbole laufen über `IconProvider`, der `QIcon` befragt und Theme-Namen wie absolute Pfade
  kennt. Dazu muss ein Theme-Name gesetzt sein: In der Sandbox gibt es kein Plattform-Theme,
  weshalb `main()` bei Bedarf `hicolor` einsetzt.
- **Default-Klau**: Chrome und Firefox setzen sich beim Start gern selbst als Default. Gatekeeper
  prüft das bei jedem Start und bietet Reparatur an.
- **`text/html` gehört beim Eintragen dazu, beim Prüfen nicht.** KDE fällt auf `text/html`
  zurück, wenn kein Schema-Handler gesetzt ist, deshalb schreibt der Rückfallpfad es mit, genau
  wie `xdg-settings`. Für die Frage „sind wir der Standardbrowser" zählen aber nur
  `x-scheme-handler/http` und `x-scheme-handler/https`.
- **Die Reihenfolge in `main()` trägt Bedeutung.** Regelprüfung und Zielprüfung laufen vor
  jeder Berührung von Qt (ADR-11). Wer sie dahinter schiebt, verliert den schnellen Pfad, ohne
  dass ein Test es merkt.
- **Flatpak baut offline.** Der Bau ruft `cargo --offline --locked` auf. Neue oder geänderte
  Cargo-Abhängigkeiten brauchen deshalb ein neu erzeugtes `generated-sources.json` im selben
  Commit:

  ```
  python3 build-aux/flatpak-cargo-generator.py Cargo.lock -o build-aux/generated-sources.json
  ```

  Wird das vergessen, scheitert erst der Paketbau, nicht der lokale. Braucht `python3-tomlkit`.
