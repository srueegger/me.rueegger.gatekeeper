# Gatekeeper — Technische Analyse

Stand: 2026-08-23. Analysebasis: TUXEDO OS (Debian), KDE Plasma, Wayland, Flatpak 1.18.1.

---

## 1. Was die App tut

Gatekeeper registriert sich beim Desktop als Handler für `x-scheme-handler/http`,
`x-scheme-handler/https` und `text/html`. Öffnet irgendein Programm eine URL, ruft der Desktop
`gatekeeper <url>` auf statt eines fest gewählten Browsers. Gatekeeper zeigt eine Liste aller
installierten Browser und startet den gewählten mit der URL.

Klingt klein. Ist es nicht — der Aufwand steckt in drei Stellen:

| Problem | Warum es schwierig ist |
|---|---|
| **Browser finden** | Nativ, Flatpak und Snap legen `.desktop`-Dateien an verschiedenen Orten ab, mit unterschiedlichen `Exec`-Konventionen. Duplikate sind Normalfall, nicht Ausnahme. |
| **Browser starten — aus einer Sandbox heraus** | Wir laufen als Flatpak. Der Zielbrowser läuft auf dem Host. Alle bequemen Wege (`xdg-open`, Portal, `QDesktopServices`) führen zurück zum Default-Handler, also zu uns. |
| **Nicht sich selbst aufrufen** | Ein Fehler hier erzeugt eine Fork-Bombe aus Dialogen und legt die Sitzung lahm. |

---

## 2. Browser-Discovery

### 2.1 Suchpfade

Nach XDG Base Directory Spec plus die Paketformat-spezifischen Exports:

| Quelle | Pfad | Auf diesem System |
|---|---|---|
| System | `/usr/share/applications` | 296 Einträge |
| System (lokal) | `/usr/local/share/applications` | 12 Einträge |
| Nutzer | `$XDG_DATA_HOME/applications` | 15 Einträge |
| Flatpak (system) | `/var/lib/flatpak/exports/share/applications` | 8 Einträge |
| Flatpak (user) | `$XDG_DATA_HOME/flatpak/exports/share/applications` | — |
| Snap | `/var/lib/snapd/desktop/applications` | — |

`$XDG_DATA_DIRS` deckt Nix, Home-Manager und Guix implizit mit ab und wird zusätzlich ausgewertet.
Präzedenz nach Spec: gleicher Dateiname in einem höherprioren Verzeichnis überschreibt den
niedrigeren, wird also **nicht** als zweiter Browser gelistet.

### 2.2 Filterkriterien

Ein Eintrag ist ein Browser-Kandidat, wenn:

- `Type=Application`
- `MimeType` enthält `x-scheme-handler/http` oder `x-scheme-handler/https`
- `Hidden=true` fehlt (Spec: bedeutet „gelöscht", Eintrag muss ignoriert werden)
- `TryExec` — falls gesetzt — im `PATH` auflösbar ist
- `OnlyShowIn`/`NotShowIn` die aktuelle Desktop-Umgebung nicht ausschliessen
- **die Desktop-ID nicht unsere eigene ist** (Invariante 1)

`NoDisplay=true` bedeutet nach Spec „nicht im Anwendungsmenü zeigen", nicht „ist kein gültiger
Handler". Solche Einträge werden deshalb nicht hart verworfen, sondern gehen in die Deduplizierung
mit niedrigerer Priorität ein.

### 2.3 Deduplizierung — belegt am realen System

Auf diesem Rechner liegen zwei Dateien:

```
/usr/share/applications/brave-origin.desktop
    Name=Brave Origin      Exec=/usr/bin/brave-origin-stable %U
/usr/share/applications/com.brave.Origin.desktop
    Name=Brave Origin      Exec=/usr/bin/brave-origin-stable %U      NoDisplay=true
```

Verschiedene Desktop-IDs, identisches Programm. Dedup rein über die Desktop-ID greift hier nicht.
Deshalb zweistufig:

1. **Primärschlüssel**: normalisierte `Exec`-Zeile — Feldcodes (`%u %U %f %F …`) entfernt,
   Argumente getrimmt, Programmname über `PATH`/Symlinks zum realen Ziel aufgelöst.
   Bei Flatpak-Exec-Zeilen zusätzlich die App-ID extrahiert, weil `--branch`/`--arch`-Argumente
   je nach Installation abweichen.
2. **Gewinner-Auswahl** innerhalb einer Gruppe: `NoDisplay=false` schlägt `NoDisplay=true`;
   danach gewinnt das höherpriore Verzeichnis; danach die Reverse-DNS-ID (stabiler über Updates).

### 2.4 Herkunft bestimmen

Für das Badge in der UI („Flatpak", „Snap", „System") und weil der Start sich unterscheidet:

- **Flatpak**: Datei liegt unter einem `flatpak/exports`-Pfad, **oder** der Schlüssel
  `X-Flatpak=<app-id>` ist gesetzt, **oder** die `Exec`-Zeile beginnt mit `flatpak run`.
- **Snap**: Datei liegt unter `/var/lib/snapd/desktop/applications`, **oder** `X-SnapInstanceName`
  ist gesetzt.
- Sonst: System bzw. Nutzer-lokal.

### 2.5 Desktop Actions als Bonus

Firefox, Chromium und Brave definieren alle `[Desktop Action new-private-window]`. Das kostet fast
nichts mitzuparsen und ergibt eine sehr nützliche Sekundäraktion („Im privaten Fenster öffnen") —
inklusive einer eigenen `Exec`-Zeile, an die die URL angehängt wird.

---

## 3. Starten — der kritische Teil

### 3.1 Was **nicht** geht

| Weg | Warum nicht |
|---|---|
| `xdg-open <url>` | Schlägt den Default-Handler nach. Das sind wir. Endlosschleife. |
| `org.freedesktop.portal.OpenURI` | Gleiches Problem; `ask=true` würde den Portal-eigenen Chooser zeigen, also genau das, was wir ersetzen. |
| `QDesktopServices::openUrl()` | Laut Qt-Doku: *„Opens the given url in the appropriate Web browser for the user's desktop environment."* Also wieder wir. |
| `gio launch` / `gtk-launch` | Wäre korrekt, ist aber auf dem Host nicht garantiert vorhanden und in der Sandbox nutzlos. |

`QDesktopServices` wird im Projekt nicht verlinkt und per Clippy-/Grep-Check in CI verboten.

### 3.2 Was geht

Zielbrowser werden **direkt über ihre eigene `Exec`-Zeile** gestartet. Aus der Sandbox heraus
geht das über den Host-Command-Portal:

```
flatpak-spawn --host --env=XDG_ACTIVATION_TOKEN=<token> -- <argv...>
```

Das setzt `--talk-name=org.freedesktop.Flatpak` im Manifest voraus. Diese Berechtigung ist faktisch
ein Sandbox-Ausbruch und muss bei einer Flathub-Einreichung begründet werden. Es gibt dafür keinen
Ersatz — der Zweck der App ist, Programme auf dem Host zu starten. Präzedenzfall: *Junction*
(`re.sonny.Junction`) macht exakt dasselbe und ist auf Flathub akzeptiert.

### 3.3 `Exec`-Feldcodes korrekt auflösen

Die `Exec`-Zeile wird nach Desktop Entry Spec §"The Exec key" geparst, nicht per String-Ersetzung:

- Zuerst in Argumente zerlegen (Quoting mit `"`, Escaping mit `\`), **dann** Feldcodes ersetzen.
  Ein Feldcode muss ein vollständiges Argument sein — `--url=%u` ist nach Spec ungültig.
- `%u` → genau eine URL, `%U` → alle URLs, `%f`/`%F` → lokale Pfade (für `file:`-URLs).
- `%i` → `--icon <Icon>`, `%c` → `Name`, `%k` → Pfad der Desktop-Datei.
- `%%` → literales `%`. Veraltete Codes `%d %D %n %N %v %m` werden ersatzlos entfernt.
- Fehlt jeder URL-Feldcode, wird die URL als letztes Argument angehängt (pragmatisch, kommt vor).

### 3.4 Sicherheit

Die URL ist Fremdeingabe und kann alles enthalten. Deshalb:

- **Niemals eine Shell.** Kein `sh -c`, kein `system()`, keine String-Interpolation. Immer ein
  `argv`-Array. `flatpak-spawn --host -- prog arg1 arg2` nimmt argv entgegen — passt.
- Kein Argument darf sich in einen Schalter verwandeln: URLs, die mit `-` beginnen, werden
  abgelehnt oder mit `--`-Separator abgetrennt (Chromium-Flags wie `--gpu-launcher` sind
  Codeausführung).
- Nur bekannte Schemata (`http`, `https`, `file`, optional `mailto`, `ftp`) durchlassen.
- Die URL wird in der UI mit **hervorgehobener Registrable Domain** angezeigt. Das ist eine
  Anti-Phishing-Massnahme und einer der eigentlichen Mehrwerte der App.

### 3.5 Wayland-Fokus

Unter Wayland bekommt ein neu gestartetes Fenster ohne gültiges Aktivierungs-Token keinen Fokus.
Gatekeeper liest `XDG_ACTIVATION_TOKEN` aus der eigenen Umgebung bzw. fordert über
`xdg-activation-v1` ein neues an und reicht es per `--env=` an den Zielbrowser durch.
Nach dem Start beendet sich Gatekeeper sofort und bleibt nicht Parent des Browsers.

---

## 4. Registrierung als Standardbrowser

Die eigene `.desktop` deklariert:

```
MimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;application/xhtml+xml;
Exec=gatekeeper %u
```

Setzen aus der App heraus über
`flatpak-spawn --host xdg-settings set default-web-browser me.rueegger.gatekeeper.desktop`,
Verifikation über `xdg-settings check`. Fällt `xdg-settings` aus, wird `mimeapps.list` direkt
geschrieben.

**Bekanntes Ärgernis**: Chrome und Firefox setzen sich beim Start gern selbst wieder als Default.
Gatekeeper prüft den Zustand bei jedem Start (billig) und blendet bei Abweichung einen Hinweis mit
Reparatur-Knopf ein.

---

## 5. Flatpak-Berechtigungen (Entwurf)

```yaml
finish-args:
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri

  # Kern: Prozesse auf dem Host starten
  - --talk-name=org.freedesktop.Flatpak

  # .desktop-Dateien lesen
  - --filesystem=/usr/share/applications:ro
  - --filesystem=/usr/local/share/applications:ro
  - --filesystem=xdg-data/applications:ro
  - --filesystem=/var/lib/flatpak/exports/share:ro
  - --filesystem=xdg-data/flatpak/exports/share:ro
  - --filesystem=/var/lib/snapd/desktop:ro

  # Icons der Browser
  - --filesystem=/usr/share/icons:ro
  - --filesystem=/usr/share/pixmaps:ro
  - --filesystem=xdg-data/icons:ro

  # Default-Browser setzen
  - --filesystem=xdg-config/mimeapps.list:create
```

Ehrlich bleiben: `--talk-name=org.freedesktop.Flatpak` erlaubt ohnehin beliebige Host-Befehle, die
granulare Dateiliste ist damit eher Dokumentation als Härtung. Sie bleibt trotzdem granular, weil
sie den tatsächlichen Bedarf sichtbar macht und die App ohne die Portal-Berechtigung wenigstens
noch die Browser *anzeigen* kann.

**Icons in der Sandbox**: Qt sucht Icon-Themes nur in den Runtime-Pfaden und findet die Host-Themes
nicht. `QIcon::setThemeSearchPaths()` und `setFallbackSearchPaths()` müssen beim Start um die oben
gemounteten Host-Pfade erweitert werden. Snap-Einträge nutzen häufig absolute Icon-Pfade, die
direkt geladen werden.

---

## 6. Technologie-Entscheidung

### 6.1 Randbedingungen

- Qt 6 ist gesetzt (Vorgabe).
- Rust ist bevorzugt, aber nicht zwingend.
- Flatpak-Builds sind **offline** — Cargo-Abhängigkeiten müssen vorab vendored werden
  (`flatpak-cargo-generator.py` → `generated-sources.json`).
- Verfügbar geprüft: `org.kde.Sdk//6.10` und `//6.11`, `org.freedesktop.Sdk.Extension.rust-stable//25.08`
  (Rust 1.98.0). `cxx-qt` steht bei 0.9.1 und verlangt Rust ≥ 1.85 — passt.

### 6.2 Optionen

| | A: alles Rust (cxx-qt) | **B: Rust-Kern + C++/QML-Schale** | C: alles C++ |
|---|---|---|---|
| Kernlogik | Rust | Rust | C++ |
| UI | QML via cxx-qt | QML via Qt Quick Controls | QML |
| Build | Cargo | CMake + Corrosion | CMake |
| Untrusted Input in Rust | ja | ja | nein |
| Flatpak-Build-Risiko | **hoch** | mittel | niedrig |
| Passt zu installiertem Qt-Tooling | teilweise | **ja** | ja |

### 6.3 Empfehlung: Option B

Die Kernlogik — Desktop-Parsing, Dedup, `Exec`-Feldcodes, Regel-Matching, Prozessstart — bleibt
vollständig in Rust. Genau dort liegt der Umgang mit Fremdeingaben und genau dort ist Rust die
Ansage wert. Die Qt-Schicht ist demgegenüber winzig: ein Dialog, eine Liste, ein Tastaturhandler.

Option A würde das grösste Build-Risiko des Projekts (Rust + Qt + offline Cargo-Vendoring +
QML-Modulregistrierung, alles gleichzeitig) auf den kritischen Pfad legen, und das für eine
UI-Schicht von vielleicht 300 Zeilen. Das lohnt nicht.

Aufbau:

```
main.cpp                       ~40 Zeilen
  └─ gatekeeper::resolve(url)  → Rust, via cxx
       ├─ Launched            → return, Qt wird nie initialisiert
       └─ NeedsDialog{...}    → QGuiApplication + QML-Dialog
```

Die FFI-Fläche bleibt bewusst klein und stabil: `resolve`, `list_browsers`, `launch`, `save_rule`,
`default_browser_status`. Gebrückt mit `cxx` (typsicher, kein handgeschriebenes `unsafe`),
eingebunden über Corrosion in CMake.

Der Regel-Kurzschluss vor `QGuiApplication` ist nicht nur Eleganz: bei einem Regeltreffer startet
Gatekeeper ohne jede GUI-Initialisierung, also im zweistelligen Millisekundenbereich.

**UI-Stil**: Qt Quick Controls 2 mit `org.kde.desktop`-Style, wo verfügbar (in der KDE-Runtime
enthalten), sonst `Fusion`. QML wird per `qmlcachegen` vorkompiliert, damit der Kaltstart im
Klickpfad nicht auffällt.

---

## 7. Funktionsumfang

**MVP** — URL entgegennehmen, Browser aus allen Quellen auflisten, Auswahl per Maus oder Zifferntaste,
Zielbrowser starten, Escape bricht ab.

**v1** — Regeln („für `github.com` immer Firefox"), Merken-Checkbox im Dialog, private Fenster über
Desktop Actions, URL kopieren, Reihenfolge nach letzter Nutzung, Selbstprüfung als Default-Browser.

**v2** — Regel-Editor, Firefox-Profile (`-P`), Auflösen von Redirect-Wrappern, optionales Entfernen
von Tracking-Parametern, Timeout mit Fallback-Browser.

### Konfiguration

In `$XDG_CONFIG_HOME/gatekeeper/` (in der Sandbox `~/.var/app/me.rueegger.gatekeeper/config/`):

- `config.toml` — Reihenfolge, Fallback, UI-Optionen
- `rules.toml` — Muster (Domain, Wildcard, Regex) → Desktop-ID plus optionale Action

Erste Übereinstimmung gewinnt. Wird beim Aufruf eine Modifiertaste gehalten, werden Regeln
übersprungen und der Dialog erscheint trotzdem.

---

## 8. Testen

Der Parser wird gegen **echte** `.desktop`-Dateien getestet, die als Fixtures im Repo liegen —
nativ (Firefox, Chromium, Brave), Flatpak-Export, Snap-Export, plus das oben belegte
Brave-Duplikat und bewusst kaputte Dateien.

Testfälle mit Substanz:

- Dedup: `brave-origin.desktop` + `com.brave.Origin.desktop` ergeben **einen** Eintrag
- `Exec`-Feldcodes inklusive Quoting, `%%`, veralteter Codes und Flatpaks `@@u … @@`-Form
- Eigene Desktop-ID wird aus jeder Quelle gefiltert (Invariante 1 — eigener Test pro Quelle)
- Verzeichnispräzedenz: `~/.local/share/applications/firefox.desktop` verdrängt `/usr/share`
- URLs mit führendem `-`, eingebetteten Anführungszeichen, Leerzeichen, Unicode
- Kaputte Datei mitten im Verzeichnis bricht den Scan nicht ab

Der Launcher wird über das Trait gegen einen Fake getestet, der das `argv`-Array aufzeichnet statt
zu starten. Damit ist „nie über eine Shell" testbar und nicht nur Vorsatz.

---

## 9. Risiken

| Risiko | Auswirkung | Umgang |
|---|---|---|
| Endlosschleife durch Selbstaufruf | Sitzung unbenutzbar | Filter in jeder Quelle, eigener Test je Quelle, zusätzlich Laufzeit-Guard über Zähler-Env-Var |
| `--talk-name=org.freedesktop.Flatpak` | Flathub-Review-Diskussion | Begründung in der Einreichung, Präzedenzfall Junction |
| Browser klaut Default-Handler zurück | App wirkt kaputt | Selbstprüfung beim Start plus Reparatur-Knopf |
| Kaltstart zu langsam | Fühlt sich träge an | Regel-Kurzschluss vor Qt-Init, Scan-Cache, `qmlcachegen` |
| Icons fehlen in der Sandbox | Hässlich | Host-Icon-Pfade explizit setzen, Fallback auf generisches Icon |
| Snap/Flatpak-Browser nicht startbar | Teilausfall | Start über die exportierte `Exec`-Zeile, nicht über geratene Kommandos |

---

## 10. Meilensteine

- **M0 — Spike.** `flatpak-builder` bringt ein CMake+Corrosion+Qt6-Hello-World mit vendored Cargo
  durch. Erst wenn das steht, wird weitergebaut. Scheitert es, fällt die Entscheidung aus §6 neu.
- **M1 — Kern.** Discovery, Parser, Dedup, Feldcodes, Launcher-Trait. Headless CLI zum Prüfen,
  volle Testabdeckung. Keine GUI.
- **M2 — GUI.** QML-Dialog, Tastaturbedienung, Icons.
- **M3 — Flatpak.** Manifest, `.desktop`, AppStream-Metainfo, Registrierung als Default-Browser.
- **M4 — Regeln.** Persistenz, Merken-Checkbox, Kurzschluss vor Qt-Init.
- **M5 — Feinschliff.** Private Fenster, Selbstprüfung, Release.

---

## 11. Offene Punkte

1. **App-ID.** `me.rueegger.gatekeeper` setzt Kontrolle über `rueegger.me` voraus; die hinterlegte
   Adresse deutet auf `rueegger.dev`. Flathub verlangt eine Domain, die dir gehört, und schreibt
   für das letzte Segment üblicherweise Grossschreibung — also eher
   `dev.rueegger.Gatekeeper`. Vor dem ersten Commit von `.desktop` und Metainfo klären, ein
   späterer Wechsel ist teuer.
2. **Stack** — Bestätigung für Option B (§6.3).
3. **Flathub-Einreichung** von Anfang an anpeilen oder erst eigenes Repo (`rueegger-dev`-Remote
   existiert bereits)? Beeinflusst, wie streng die Berechtigungen begründet werden müssen.
