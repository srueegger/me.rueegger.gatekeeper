// Einstiegspunkt von Gatekeeper.
//
// Die Reihenfolge hier ist Absicht: Zuerst wird die URL geprüft und der Kern befragt,
// erst danach entsteht eine QGuiApplication. Wird das Ziel abgelehnt oder greift später
// eine gespeicherte Regel, endet der Aufruf, bevor Qt überhaupt hochgefahren ist.

#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QTextStream>
#include <QVariantList>
#include <QVariantMap>

#include "Session.h"
#include "gatekeeper-ffi/src/lib.rs.h"

namespace {

QString toQString(const rust::String &value)
{
    return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
}

QStringList toQStringList(const rust::Vec<rust::String> &values)
{
    QStringList out;
    out.reserve(static_cast<qsizetype>(values.size()));
    for (const auto &value : values)
        out.append(toQString(value));
    return out;
}

// Qt sucht Icon-Themes nur in den Pfaden der eigenen Runtime. In der Flatpak-Sandbox
// liegen die Themes der Browser aber auf dem Host und erscheinen unter /run/host.
// Ohne diese Ergänzung bleibt die Liste ohne Symbole. Ausserhalb der Sandbox schaden
// die zusätzlichen Pfade nicht, sie existieren dort schlicht nicht.
void addHostIconPaths()
{
    QStringList paths = QIcon::themeSearchPaths();
    for (const QString &host : {QStringLiteral("/run/host/usr/share/icons"),
                                QStringLiteral("/run/host/usr/share/pixmaps"),
                                QStringLiteral("/run/host/share/icons"),
                                QStringLiteral("/run/host/user-share/icons"),
                                QStringLiteral("/var/lib/flatpak/exports/share/icons"),
                                QStringLiteral("/usr/share/icons"),
                                QStringLiteral("/usr/share/pixmaps")}) {
        if (!paths.contains(host))
            paths.append(host);
    }
    QIcon::setThemeSearchPaths(paths);
}

/// Wie der Aufruf zu verstehen ist.
struct Invocation
{
    enum class Mode {
        /// Dialog zeigen.
        Ask,
        /// Nur auflisten, was gefunden wird, und beenden.
        List,
        /// Ohne Dialog starten, Browser über seine Desktop-ID gewählt.
        Launch,
    };

    Mode mode = Mode::Ask;
    /// Regeln überspringen und in jedem Fall fragen.
    bool skipRules = false;
    QString browserId;
    QString target;
};

Invocation parseArguments(int argc, char *argv[])
{
    Invocation invocation;
    if (argc > 2 && qstrcmp(argv[1], "--ask") == 0) {
        invocation.skipRules = true;
        invocation.target = QString::fromLocal8Bit(argv[2]);
        return invocation;
    }
    if (argc > 1 && qstrcmp(argv[1], "--list") == 0) {
        invocation.mode = Invocation::Mode::List;
        if (argc > 2)
            invocation.target = QString::fromLocal8Bit(argv[2]);
        return invocation;
    }
    if (argc > 3 && qstrcmp(argv[1], "--launch") == 0) {
        invocation.mode = Invocation::Mode::Launch;
        invocation.browserId = QString::fromLocal8Bit(argv[2]);
        invocation.target = QString::fromLocal8Bit(argv[3]);
        return invocation;
    }
    if (argc > 1)
        invocation.target = QString::fromLocal8Bit(argv[1]);
    return invocation;
}

} // namespace

int main(int argc, char *argv[])
{
    gatekeeper::init_logging();

    const Invocation invocation = parseArguments(argc, argv);

    const QByteArray rawTarget = invocation.target.toUtf8();
    const auto target = gatekeeper::check_target(rust::Str(rawTarget.constData(), rawTarget.size()));
    if (!invocation.target.isEmpty() && !target.valid) {
        qWarning("Ziel abgelehnt: %s", qUtf8Printable(toQString(target.error)));
        return 2;
    }

    const QByteArray uri = toQString(target.uri).toUtf8();

    // Der schnelle Pfad. Greift eine Regel, endet der Aufruf hier, und Qt wurde nie
    // angefasst: keine QGuiApplication, keine Szenengrafik, kein Fenster.
    if (invocation.mode == Invocation::Mode::Ask && !invocation.skipRules && target.valid) {
        const auto outcome = gatekeeper::apply_rules(rust::Str(uri.constData(), uri.size()));
        if (outcome.launched)
            return 0;
        if (outcome.matched) {
            // Die Regel benennt einen Browser, der nicht startet. Statt wortlos zu
            // scheitern übernimmt der Dialog.
            qWarning("Regel auf %s greift nicht: %s",
                     qUtf8Printable(toQString(outcome.browser)),
                     qUtf8Printable(toQString(outcome.error)));
        }
    }

    const auto browsers = gatekeeper::list_browsers(rust::Str(uri.constData(), uri.size()));
    if (browsers.empty()) {
        qWarning("Kein Browser gefunden.");
        return 1;
    }

    QVariantList model;
    for (const auto &browser : browsers) {
        QVariantMap entry;
        entry[QStringLiteral("id")] = toQString(browser.id);
        entry[QStringLiteral("name")] = toQString(browser.name);
        entry[QStringLiteral("icon")] = toQString(browser.icon);
        entry[QStringLiteral("origin")] = toQString(browser.origin);
        entry[QStringLiteral("argv")] = toQStringList(browser.argv);
        model.append(entry);
    }

    if (invocation.mode == Invocation::Mode::List) {
        QTextStream out(stdout);

        const auto status = gatekeeper::default_browser_status();
        out << (status.ours ? QStringLiteral("Standardbrowser: Gatekeeper")
                            : QStringLiteral("Standardbrowser: ") + toQString(status.message))
            << "\n\n";

        for (const QVariant &item : std::as_const(model)) {
            const QVariantMap browser = item.toMap();
            out << browser[QStringLiteral("name")].toString() << "  ["
                << browser[QStringLiteral("origin")].toString() << "]  "
                << browser[QStringLiteral("id")].toString() << "\n    "
                << browser[QStringLiteral("argv")].toStringList().join(QLatin1Char(' ')) << "\n";
        }
        return 0;
    }

    Session session;
    session.setBrowsers(std::move(model));
    session.setTarget(toQString(target.uri), toQString(target.display_host));
    session.refreshDefaultBrowserHint();

    // Denselben Weg nimmt später ein Regeltreffer: kein Fenster, keine QGuiApplication.
    if (invocation.mode == Invocation::Mode::Launch) {
        const int index = session.indexOfDesktopId(invocation.browserId);
        if (index < 0) {
            qWarning("Kein Browser mit der Desktop-ID '%s'",
                     qUtf8Printable(invocation.browserId));
            return 4;
        }
        session.choose(index);
        return session.launchError().isEmpty() ? 0 : 5;
    }

    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("Gatekeeper"));
    app.setDesktopFileName(QStringLiteral("me.rueegger.Gatekeeper"));
    addHostIconPaths();

    Session::instance = &session;

    QQmlApplicationEngine engine;
    engine.loadFromModule("GatekeeperUi", "Main");

    if (engine.rootObjects().isEmpty())
        return 3;

    return app.exec();
}
