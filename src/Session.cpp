#include "Session.h"

#include <QCoreApplication>
#include <QStringList>
#include <QVariantMap>

#include <vector>

#include "gatekeeper-ffi/src/lib.rs.h"

Session *Session::instance = nullptr;

int Session::indexOfDesktopId(const QString &desktopId) const
{
    for (int index = 0; index < m_browsers.size(); ++index) {
        if (m_browsers.at(index).toMap().value(QStringLiteral("id")).toString() == desktopId)
            return index;
    }
    return -1;
}

void Session::choose(int index)
{
    if (index < 0 || index >= m_browsers.size()) {
        qWarning("choose(%d) ausserhalb des gültigen Bereichs", index);
        return;
    }

    const QVariantMap browser = m_browsers.at(index).toMap();
    const QStringList argv = browser.value(QStringLiteral("argv")).toStringList();

    std::vector<rust::String> command;
    command.reserve(static_cast<size_t>(argv.size()));
    for (const QString &argument : argv) {
        const QByteArray utf8 = argument.toUtf8();
        command.emplace_back(utf8.constData(), utf8.size());
    }

    const auto outcome =
        gatekeeper::launch(rust::Slice<const rust::String>(command.data(), command.size()));
    if (!outcome.started) {
        m_launchError = QString::fromUtf8(outcome.error.data(),
                                          static_cast<qsizetype>(outcome.error.size()));
        qWarning("Start fehlgeschlagen: %s", qUtf8Printable(m_launchError));
        Q_EMIT launchErrorChanged();
        return;
    }

    // Der Browser läuft, unsere Arbeit ist getan. Gatekeeper bleibt nicht im Weg stehen.
    // Ohne laufende Ereignisschleife, etwa beim Start über --launch, ist das ein No-Op.
    QCoreApplication::quit();
}
