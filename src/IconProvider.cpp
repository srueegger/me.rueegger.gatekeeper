#include "IconProvider.h"

#include <QIcon>
#include <QPixmap>

QPixmap IconProvider::requestPixmap(const QString &id, QSize *size, const QSize &requestedSize)
{
    const int edge = requestedSize.width() > 0 ? requestedSize.width() : 32;

    // Snap-Einträge nennen absolute Pfade statt Theme-Namen.
    QIcon icon = id.startsWith(QLatin1Char('/')) ? QIcon(id) : QIcon::fromTheme(id);

    // Lieber ein allgemeines Symbol als eine Lücke in der Liste. Ohne Icon ist ein Eintrag
    // schlechter zu treffen, weil das Auge sich am Bild orientiert, nicht am Text.
    if (icon.isNull())
        icon = QIcon::fromTheme(QStringLiteral("applications-internet"));
    if (icon.isNull())
        icon = QIcon::fromTheme(QStringLiteral("text-html"));

    QPixmap pixmap = icon.pixmap(edge, edge);
    if (size)
        *size = pixmap.isNull() ? QSize(edge, edge) : pixmap.size();
    return pixmap;
}
