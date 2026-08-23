# Gatekeeper

Ein Browser-Auswahldialog für Linux. Statt dass jeder Link stur im selben Browser landet, fragt
Gatekeeper kurz nach — und zeigt dabei, wohin die URL eigentlich führt.

Gatekeeper registriert sich als Standardbrowser. Klickst du irgendwo auf einen Link — im
E-Mail-Programm, im Chat, im Terminal, in einem PDF — erscheint ein kleiner Dialog mit allen
installierten Browsern. Einer davon bekommt die URL.

## Status

Frühe Planungsphase. Es gibt noch nichts zu installieren.
Die technische Analyse liegt in [docs/ANALYSE.md](docs/ANALYSE.md).

## Warum

- **Mehrere Browser nebeneinander** — Arbeit in einem, Privates in einem anderen, Testing im dritten.
- **Sichtbarkeit vor dem Klick** — die Zieldomain wird hervorgehoben angezeigt, bevor irgendetwas
  geladen wird.
- **Regeln** — `github.com` immer in Firefox, alles von der Arbeit in Chromium, der Rest fragt nach.

## Erkennt Browser aus allen Quellen

Nativ installierte, Flatpaks und Snaps — inklusive Deduplizierung, wenn dieselbe Anwendung mehrfach
registriert ist.

## Bauen

Noch nicht sinnvoll möglich. Voraussetzungen sind Qt 6, Rust, CMake und für das Paket
`flatpak-builder` mit `org.kde.Sdk//6.10`.

## Lizenz

Noch nicht festgelegt.
