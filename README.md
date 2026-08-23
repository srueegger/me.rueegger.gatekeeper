# Gatekeeper

Ein Auswahldialog für Browser unter Linux. Statt dass jeder Link stur im selben Browser landet,
fragt Gatekeeper kurz nach und zeigt dabei, wohin die URL eigentlich führt.

Gatekeeper registriert sich als Standardbrowser. Klickst du irgendwo auf einen Link, im
Mailprogramm, im Chat, im Terminal oder in einem PDF, erscheint ein kleiner Dialog mit allen
installierten Browsern. Einer davon bekommt die URL.

## Status

Frühe Entwicklung. Es gibt noch nichts zu installieren.

Fertig ist der Kern, der die eigentliche Arbeit macht: Browser finden, die Einträge des Desktops lesen,
Duplikate zusammenfassen, Startbefehle auflösen. Was noch fehlt, ist die Oberfläche und die
Verpackung als Flatpak.

Wer sehen will, was der Kern auf dem eigenen System findet:

```
cargo run --example scan
cargo run --example scan -- https://github.com/user/repo
```

Die technische Analyse liegt in [docs/ANALYSE.md](docs/ANALYSE.md), die getroffenen
Architekturentscheidungen in [docs/DECISIONS.md](docs/DECISIONS.md).

## Wozu das gut ist

Mehrere Browser nebeneinander zu betreiben ist normal geworden: einer für die Arbeit, einer für
Privates, einer zum Testen. Der Desktop kennt aber nur einen Standard, und jeder Link landet dort.

Gatekeeper schiebt sich dazwischen und lässt dich bei jedem Link entscheiden. Wo du nicht jedes Mal
entscheiden willst, hinterlegst du eine Regel: alles von `github.com` nach Firefox, alles aus der
Arbeitsdomain nach Chromium, der Rest fragt nach. Greift eine Regel, erscheint gar kein Dialog.

Nebenbei bekommst du zu sehen, wohin ein Link tatsächlich zeigt, bevor er geöffnet wird. Die
Zieldomain steht hervorgehoben im Dialog. Gegen Links, die anders aussehen als sie sind, hilft das
mehr als jede Warnmeldung nach dem Klick.

## Findet Browser aus allen Quellen

Nativ installierte Browser, Flatpaks und Snaps. Duplikate werden zusammengefasst: Manche
Distributionen registrieren denselben Browser unter zwei Namen, etwa `brave-origin.desktop` und
`com.brave.Origin.desktop`. In der Liste steht er trotzdem nur einmal.

## Bauen

Der Kern lässt sich schon jetzt bauen und testen, dafür genügt eine Installation von Rust:

```
cargo test
```

Für die vollständige Anwendung kommen Qt 6 und CMake dazu, für das Paket zusätzlich
`flatpak-builder` mit `org.kde.Sdk` in Version 6.10.

## Lizenz

GNU General Public License, Version 2. Ausschliesslich diese Version, ohne die übliche Klausel
„oder eine spätere Version". Der vollständige Text steht in [LICENSE](LICENSE).
