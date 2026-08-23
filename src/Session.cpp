#include "Session.h"

#include <QCoreApplication>
#include <QStringList>
#include <QVariantMap>

#include <vector>

#include "gatekeeper-ffi/src/lib.rs.h"

Session *Session::instance = nullptr;

namespace {

QString toQString(const rust::String &value)
{
    return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
}

} // namespace

void Session::refreshDefaultBrowserHint()
{
    const auto status = gatekeeper::default_browser_status();
    const QString hint = status.ours ? QString() : toQString(status.message);
    if (hint == m_defaultBrowserHint)
        return;

    m_defaultBrowserHint = hint;
    Q_EMIT defaultBrowserHintChanged();
}

void Session::makeDefaultBrowser()
{
    const auto outcome = gatekeeper::make_default_browser();
    if (!outcome.started) {
        m_launchError = toQString(outcome.error);
        qWarning("Konnte nicht Standardbrowser werden: %s", qUtf8Printable(m_launchError));
        Q_EMIT launchErrorChanged();
        return;
    }
    refreshDefaultBrowserHint();
}

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
        m_launchError = toQString(outcome.error);
        qWarning("Start fehlgeschlagen: %s", qUtf8Printable(m_launchError));
        Q_EMIT launchErrorChanged();
        return;
    }

    // Der Browser läuft, unsere Arbeit ist getan. Gatekeeper bleibt nicht im Weg stehen.
    // Ohne laufende Ereignisschleife, etwa beim Start über --launch, ist das ein No-Op.
    QCoreApplication::quit();
}
