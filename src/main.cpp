// Einstiegspunkt von Gatekeeper.
//
// Die Reihenfolge hier ist Absicht: Zuerst wird die URL geprüft und der Kern befragt,
// erst danach entsteht eine QGuiApplication. Sobald Regeln umgesetzt sind, kehrt ein
// Regeltreffer an dieser Stelle zurück, ohne dass jemals Qt initialisiert wird.

#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
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
// liegen die Themes der Browser aber auf dem Host, eingehängt über die Berechtigungen
// aus dem Manifest. Ohne diese Ergänzung bleibt die Liste ohne Symbole.
void addHostIconPaths()
{
    QStringList paths = QIcon::themeSearchPaths();
    for (const QString &host : {QStringLiteral("/run/host/usr/share/icons"),
                                QStringLiteral("/usr/share/icons"),
                                QStringLiteral("/var/lib/flatpak/exports/share/icons"),
                                QStringLiteral("/usr/share/pixmaps")}) {
        if (!paths.contains(host))
            paths.append(host);
    }
    QIcon::setThemeSearchPaths(paths);
}

} // namespace

int main(int argc, char *argv[])
{
    gatekeeper::init_logging();

    const QString rawTarget = argc > 1 ? QString::fromLocal8Bit(argv[1]) : QString();
    const auto target = gatekeeper::check_target(rust::Str(rawTarget.toUtf8().constData()));

    if (!rawTarget.isEmpty() && !target.valid) {
        qWarning("Ziel abgelehnt: %s", toQString(target.error).toUtf8().constData());
        return 2;
    }

    const auto browsers = gatekeeper::list_browsers(rust::Str(toQString(target.uri).toUtf8().constData()));
    if (browsers.empty()) {
        qWarning("Kein Browser gefunden.");
        return 1;
    }

    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("Gatekeeper"));
    app.setDesktopFileName(QStringLiteral("me.rueegger.Gatekeeper"));
    addHostIconPaths();

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

    Session session;
    session.setBrowsers(std::move(model));
    session.setTarget(toQString(target.uri), toQString(target.display_host));
    Session::instance = &session;

    QQmlApplicationEngine engine;
    engine.loadFromModule("GatekeeperUi", "Main");

    if (engine.rootObjects().isEmpty())
        return 3;

    return app.exec();
}
