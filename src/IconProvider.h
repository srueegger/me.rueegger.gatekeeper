#pragma once

#include <QQuickImageProvider>

/// Liefert Browser-Symbole an QML.
///
/// Qt Quick hat von Haus aus keinen Provider für Icon-Themes; `image://theme/...` gibt es
/// nicht. Ein `Image` mit dieser Quelle bleibt still leer, ohne Fehler im Log ausser einer
/// einzeiligen Warnung. Deshalb hier ein eigener Provider, der `QIcon` befragt.
///
/// Er deckt beide Formen ab, die in Desktop-Dateien vorkommen: den Namen aus einem
/// Icon-Theme und den absoluten Pfad, wie ihn Snap-Einträge verwenden.
class IconProvider : public QQuickImageProvider
{
public:
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) { }

    QPixmap requestPixmap(const QString &id, QSize *size, const QSize &requestedSize) override;
};
