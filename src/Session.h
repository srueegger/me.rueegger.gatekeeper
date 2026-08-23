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
    /// Hinweis, falls Gatekeeper nicht der Standardbrowser ist. Sonst leer.
    Q_PROPERTY(QString defaultBrowserHint READ defaultBrowserHint NOTIFY
                       defaultBrowserHintChanged)

public:
    explicit Session(QObject *parent = nullptr) : QObject(parent) { }

    QVariantList browsers() const { return m_browsers; }
    QString targetUri() const { return m_targetUri; }
    QString targetHost() const { return m_targetHost; }
    QString launchError() const { return m_launchError; }
    QString defaultBrowserHint() const { return m_defaultBrowserHint; }

    /// Fragt den Kern, wer aktuell Links öffnet. Billig genug für jeden Start.
    void refreshDefaultBrowserHint();

    void setBrowsers(QVariantList browsers) { m_browsers = std::move(browsers); }
    void setTarget(QString uri, QString host)
    {
        m_targetUri = std::move(uri);
        m_targetHost = std::move(host);
    }

    /// Trägt Gatekeeper als Standardbrowser ein und aktualisiert den Hinweis.
    Q_INVOKABLE void makeDefaultBrowser();

    /// Ob die Auswahl als Regel für die aktuelle Domain gemerkt werden soll.
    ///
    /// Wird von der Oberfläche gesetzt und beim nächsten `choose` ausgewertet.
    Q_INVOKABLE void setRememberChoice(bool remember) { m_rememberChoice = remember; }

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
    void defaultBrowserHintChanged();

private:
    QVariantList m_browsers;
    QString m_targetUri;
    QString m_targetHost;
    QString m_launchError;
    QString m_defaultBrowserHint;
    bool m_rememberChoice = false;
};
