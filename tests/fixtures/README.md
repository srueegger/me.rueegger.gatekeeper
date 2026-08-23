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

- Flatpaks `--file-forwarding`-Form `@@u %u @@`. Die `@@`-Marker sind Argumente für `flatpak run`,
  nicht für den Browser: `flatpak run` wertet sie selbst aus und entfernt sie. Sie müssen deshalb
  unverändert durchgereicht werden — nur das `%u` dazwischen wird ersetzt.
- `Exec` beginnt mit `/usr/bin/flatpak run …`; die App-ID steht mitten in der Zeile, hinter
  variablen `--branch`/`--arch`/`--command`-Argumenten. Genau daran hängt die Dedup-Normalisierung.
- Snaps `env BAMF_DESKTOP_FILE_HINT=… /snap/bin/…`-Präfix. Das Programm ist nicht das erste Token.
- Snap-Icons sind absolute Pfade (`/snap/firefox/current/default256.png`), keine Theme-Namen.

## `malformed/` — darf den Scan nicht abbrechen

Auf echten Systemen liegt immer irgendwo Müll. Ein einzelner kaputter Eintrag darf nie dazu führen,
dass gar keine Browser gefunden werden. Nicht jeder Eintrag hier wird verworfen — manche sind
reparierbar, und dann ist Reparieren der bessere Umgang als Wegwerfen:

| Datei | Ausgang |
|---|---|
| `no-group-header.desktop` | verworfen — keine `[Desktop Entry]`-Gruppe |
| `truncated-binary.desktop` | verworfen — kein gültiges UTF-8 |
| `empty.desktop` | verworfen — leer |
| `missing-exec.desktop` | geparst, aber kein Kandidat: nichts zu starten |
| `wrong-type.desktop` | geparst, aber kein Kandidat: `Type=Link` |
| `duplicate-keys.desktop` | **behalten** — nach Spec gewinnt der erste Wert, der Eintrag bleibt brauchbar |

## `excluded/` — gültig, aber kein Kandidat

| Datei | Grund |
|---|---|
| `hidden-browser.desktop` | `Hidden=true` heisst nach Spec „gelöscht". |
| `tryexec-missing.desktop` | `TryExec` zeigt auf ein nicht installiertes Programm. |
| `not-a-browser.desktop` | Nur `x-scheme-handler/mailto`. |
| `onlyshowin-gnome.desktop` | `OnlyShowIn=GNOME;` ausserhalb von GNOME. |
| `me.rueegger.Gatekeeper.desktop` | **Wir selbst.** Invariante 1. Dieser Fall bekommt einen eigenen Test je Discovery-Quelle, weil ein Fehler hier eine Endlosschleife erzeugt. |
