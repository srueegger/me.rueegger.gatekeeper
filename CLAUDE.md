# Gatekeeper — Projektinstruktionen für Claude

## Was ist das

**Gatekeeper** (App-ID `me.rueegger.gatekeeper`) ist eine Linux-Desktop-App, die sich als
Standardbrowser registriert. Klickt der Nutzer irgendwo auf einen Link (E-Mail-Client, Chat,
Terminal, PDF-Viewer), bekommt nicht ein fest verdrahteter Browser die URL, sondern Gatekeeper.
Gatekeeper zeigt einen schnellen Auswahldialog mit allen installierten Browsern — nativ, Flatpak
und Snap — und startet den gewählten mit der URL.

Vertrieb: **Flatpak**. UI: **Qt 6 / QML**. Logik: **Rust**.
Begründung und Details in `docs/ANALYSE.md`, Entscheidungen in `docs/DECISIONS.md`.

## Nicht-verhandelbare Invarianten

Der Kern des Projekts. Änderungen daran nie ohne Rückfrage:

1. **Kein Selbstaufruf.** Gatekeeper darf sich niemals selbst als Browser anbieten oder starten.
   Jede Discovery-Quelle filtert die eigene Desktop-ID heraus, mit eigenem Test je Quelle. Ein
   Fehler hier erzeugt eine Endlosschleife, die die Sitzung lahmlegt.
2. **Kein `xdg-open`, kein Portal, kein `QDesktopServices`.** Alle drei schlagen den
   Default-Handler nach — und der sind wir (ADR-1). Zielbrowser werden immer direkt über ihre
   `Exec`-Zeile gestartet.
3. **Nie über eine Shell starten.** URLs sind Fremdeingabe. Immer `argv`-Arrays, nie
   String-Interpolation in `sh -c`. URLs mit führendem `-` werden nie als Argument durchgereicht.
4. **Gatekeeper muss ohne Flatpak lauffähig bleiben.** Der Launcher ist hinter einem Trait
   abstrahiert: Direktstart nativ, `flatpak-spawn --host` in der Sandbox. Sonst ist lokales
   Entwickeln unzumutbar.

## Architektur

`gatekeeper-core` (Rust) macht die gesamte Arbeit ohne GUI: XDG-Verzeichnisse scannen,
Desktop-Entries parsen, Browser deduplizieren, Herkunft bestimmen, Regeln matchen, `Exec`-Feldcodes
auflösen, Prozess starten. Headless testbar, keine Qt-Abhängigkeit.

Darüber eine dünne Qt-6-Schicht in C++ mit QML, angebunden über `cxx`, gebaut mit CMake +
Corrosion (ADR-4). `main()` liegt in C++ und ruft zuerst `resolve(url)` im Rust-Kern auf. Greift
eine gespeicherte Regel, wird `QGuiApplication` nie konstruiert — der Browser startet ohne jede
GUI-Initialisierung.

## Verzeichnisse

```
crates/gatekeeper-core/   Discovery, Parsing, Regeln, Launcher — keine GUI, voll unit-testbar
crates/gatekeeper-ffi/    cxx-Bridge, schmale und stabile Fläche zum C++-Teil
src/                      C++/QML-Frontend + main()
data/                     .desktop, AppStream-Metainfo, Icons
build-aux/                Flatpak-Manifest, flatpak-cargo-generator
docs/ANALYSE.md           Technische Analyse, Stack-Entscheidung, Risiken
docs/DECISIONS.md         Architecture Decision Records, append-only
tests/fixtures/           Echte .desktop-Dateien (nativ/Flatpak/Snap) als Testdaten
```

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
- **Architekturentscheidungen** kommen als ADR nach `docs/DECISIONS.md` — Kontext, Entscheidung,
  Konsequenz. Append-only; überholte Einträge werden als „superseded by ADR-N" markiert, nicht
  umgeschrieben.
- Vor jedem Commit: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
  Bei QML-Änderungen zusätzlich `qmllint`.

## Testen

Der Kern wird gegen echte `.desktop`-Fixtures getestet, nicht gegen erfundene. Neue Browser oder
neue Paketformate kommen als Fixture nach `tests/fixtures/` plus Testfall. Beim Parser gilt: lieber
eine kaputte Desktop-Datei überspringen und loggen als den ganzen Scan abbrechen — auf echten
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
- **Default-Klau**: Chrome und Firefox setzen sich beim Start gern selbst als Default. Gatekeeper
  prüft das bei jedem Start und bietet Reparatur an.
- **Flatpak baut offline.** Neue Cargo-Abhängigkeiten erfordern ein neu generiertes
  `generated-sources.json` im selben Commit.
