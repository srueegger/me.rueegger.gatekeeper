#include "Session.h"

#include <QLoggingCategory>
#include <QStringList>
#include <QVariantMap>

Session *Session::instance = nullptr;

void Session::choose(int index) const
{
    if (index < 0 || index >= m_browsers.size()) {
        qWarning("choose(%d) ausserhalb des gültigen Bereichs", index);
        return;
    }

    const QVariantMap browser = m_browsers.at(index).toMap();
    qInfo("Gewählt: %s", qUtf8Printable(browser.value(QStringLiteral("name")).toString()));
    qInfo("argv: %s",
          qUtf8Printable(browser.value(QStringLiteral("argv")).toStringList().join(QLatin1Char(' '))));
}
