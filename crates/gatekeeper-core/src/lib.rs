//! Kern von Gatekeeper: Browser finden, Desktop-Entries lesen, Zielbrowser starten.
//!
//! Bewusst ohne GUI-Abhängigkeit, damit sich alles headless testen lässt.
//!
//! # Invarianten
//!
//! Zwei Regeln gelten im gesamten Crate und sind der Grund, warum es ihn gibt:
//!
//! 1. Gatekeeper bietet sich nie selbst als Browser an. Die eigene Desktop-ID wird in jeder
//!    Discovery-Quelle gefiltert. Ein Fehler hier erzeugt eine Endlosschleife aus Dialogen.
//! 2. Zielbrowser werden nie über `xdg-open`, ein Portal oder `QDesktopServices` gestartet.
//!    Alle drei schlagen den Default-Handler nach — und der ist Gatekeeper selbst.

pub mod desktop;
pub mod discovery;
pub mod exec;
pub mod locale;
pub mod uri;

pub use desktop::{DesktopFile, Group, ParseError};
pub use exec::{ExecError, FieldContext, build_argv};
pub use discovery::{Browser, BrowserAction, DiscoveryOptions, Origin, SearchPath, SourceKind, discover};
pub use uri::{TargetUri, UriError};
pub use locale::Locale;
