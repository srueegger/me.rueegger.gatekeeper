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

public:
    explicit Session(QObject *parent = nullptr) : QObject(parent) { }

    QVariantList browsers() const { return m_browsers; }
    QString targetUri() const { return m_targetUri; }
    QString targetHost() const { return m_targetHost; }

    void setBrowsers(QVariantList browsers) { m_browsers = std::move(browsers); }
    void setTarget(QString uri, QString host)
    {
        m_targetUri = std::move(uri);
        m_targetHost = std::move(host);
    }

    /// Startet den Browser an `index`.
    ///
    /// Noch nicht umgesetzt: der Launcher folgt in M1. Bis dahin wird das aufgelöste
    /// Kommando nur ausgegeben, damit sichtbar ist, was gestartet würde.
    Q_INVOKABLE void choose(int index) const;

    /// Die Instanz, die QML als Singleton bekommt. Wird vor dem Laden der QML-Wurzel
    /// gesetzt; QML greift erst danach zu.
    static Session *instance;
    static Session *create(QQmlEngine *, QJSEngine *) { return instance; }

private:
    QVariantList m_browsers;
    QString m_targetUri;
    QString m_targetHost;
};
