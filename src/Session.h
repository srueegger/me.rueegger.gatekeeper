#pragma once

#include <QObject>
#include <QString>
#include <QVariantList>
#include <QtQml/qqmlregistration.h>

class QJSEngine;
class QQmlEngine;

/// Alles, was die Oberfläche über den aktuellen Aufruf wissen muss.
///
/// Bewusst als registrierter Singleton statt als Context-Property: Context-Properties sind
/// für qmllint und den QML-Compiler unsichtbar, jeder Zugriff darauf bleibt ungeprüft.
class Session : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

    /// Die Browser zur Auswahl. Jeder Eintrag trägt id, name, icon, origin und argv.
    Q_PROPERTY(QVariantList browsers READ browsers CONSTANT)
    /// Die vollständige Ziel-URL.
    Q_PROPERTY(QString targetUri READ targetUri CONSTANT)
    /// Die Domain, die hervorgehoben wird. Leer bei Schemata ohne Host.
    Q_PROPERTY(QString targetHost READ targetHost CONSTANT)
    /// Grund des letzten fehlgeschlagenen Starts. Leer, solange nichts schiefging.
    Q_PROPERTY(QString launchError READ launchError NOTIFY launchErrorChanged)

public:
    explicit Session(QObject *parent = nullptr) : QObject(parent) { }

    QVariantList browsers() const { return m_browsers; }
    QString targetUri() const { return m_targetUri; }
    QString targetHost() const { return m_targetHost; }
    QString launchError() const { return m_launchError; }

    void setBrowsers(QVariantList browsers) { m_browsers = std::move(browsers); }
    void setTarget(QString uri, QString host)
    {
        m_targetUri = std::move(uri);
        m_targetHost = std::move(host);
    }

    /// Der Index des Browsers mit dieser Desktop-ID, oder -1.
    int indexOfDesktopId(const QString &desktopId) const;

    /// Startet den Browser an `index` und beendet die Anwendung.
    ///
    /// Schlägt der Start fehl, bleibt das Fenster stehen und `launchError` trägt den Grund.
    /// Wer gerade auf einen Link geklickt hat, soll nicht vor einem verschwundenen Fenster
    /// ohne Erklärung sitzen.
    Q_INVOKABLE void choose(int index);

    /// Die Instanz, die QML als Singleton bekommt. Wird vor dem Laden der QML-Wurzel
    /// gesetzt; QML greift erst danach zu.
    static Session *instance;
    static Session *create(QQmlEngine *, QJSEngine *) { return instance; }

Q_SIGNALS:
    void launchErrorChanged();

private:
    QVariantList m_browsers;
    QString m_targetUri;
    QString m_targetHost;
    QString m_launchError;
};
