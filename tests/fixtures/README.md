# Test-Fixtures

Echte `.desktop`-Dateien als Testdaten. Wo eine Datei nicht wörtlich von einem System kopiert
werden konnte, steht das hier ausdrücklich dabei — erfundene Fixtures, die als echt ausgegeben
werden, sind schlimmer als gar keine.

## `native/` — wörtlich kopiert

Von einem TUXEDO OS (Debian-Basis), `/usr/share/applications`, 2026-08-23:

| Datei | Wofür sie im Test steht |
|---|---|
| `firefox.desktop` | ~40 lokalisierte `Name[..]`-Zeilen. Der Parser muss Lokalisierungs-Suffixe erkennen und überspringen, ohne dabei langsam zu werden. `Exec=firefox %u` ist nicht absolut — muss über `PATH` aufgelöst werden. |
| `chromium.desktop` | Schlichter Normalfall mit absolutem Pfad und `%U`. |
| `brave-origin.desktop` | Duplikat-Partner. Enthält `#`-Kommentarzeilen mitten in der Gruppe und `Actions=`. Die Action-`Exec`-Zeilen haben **keinen** Feldcode — die URL muss angehängt werden. |
| `com.brave.Origin.desktop` | Derselbe Browser mit anderer Desktop-ID und `NoDisplay=true`. Der Kommentar in der Datei erklärt den Grund selbst. Belegt ADR-3: Dedup muss über die Exec-Zeile laufen. |

## `flatpak/`, `snap/` — nachgebaut

Auf dem Entwicklungssystem ist kein Browser als Flatpak oder Snap installiert. Diese Dateien sind
nach der Form nachgebaut, die `flatpak` bzw. `snapd` beim Export tatsächlich erzeugen. Sobald ein
echter Export vorliegt, werden sie dadurch ersetzt.

Was sie abdecken:

- Flatpaks `--file-forwarding`-Form `@@u %u @@` — die `@@`-Marker sind keine Argumente und dürfen
  nicht beim Browser landen.
- `Exec` beginnt mit `/usr/bin/flatpak run …`; die App-ID steht mitten in der Zeile, hinter
  variablen `--branch`/`--arch`/`--command`-Argumenten. Genau daran hängt die Dedup-Normalisierung.
- Snaps `env BAMF_DESKTOP_FILE_HINT=… /snap/bin/…`-Präfix. Das Programm ist nicht das erste Token.
- Snap-Icons sind absolute Pfade (`/snap/firefox/current/default256.png`), keine Theme-Namen.

## `malformed/` — muss übersprungen werden, ohne den Scan abzubrechen

Fehlender Gruppenkopf, abgeschnittene Datei mit Binärmüll und ungültigem UTF-8, leere Datei,
fehlendes `Exec`, `Type=Link`, doppelte Schlüssel (nach Spec gewinnt der erste).

Auf echten Systemen liegt immer irgendwo Müll. Ein einzelner kaputter Eintrag darf nie dazu führen,
dass gar keine Browser gefunden werden.

## `excluded/` — gültig, aber kein Kandidat

| Datei | Grund |
|---|---|
| `hidden-browser.desktop` | `Hidden=true` heisst nach Spec „gelöscht". |
| `tryexec-missing.desktop` | `TryExec` zeigt auf ein nicht installiertes Programm. |
| `not-a-browser.desktop` | Nur `x-scheme-handler/mailto`. |
| `onlyshowin-gnome.desktop` | `OnlyShowIn=GNOME;` ausserhalb von GNOME. |
| `me.rueegger.Gatekeeper.desktop` | **Wir selbst.** Invariante 1. Dieser Fall bekommt einen eigenen Test je Discovery-Quelle, weil ein Fehler hier eine Endlosschleife erzeugt. |
