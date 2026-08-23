pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import GatekeeperUi

// Auswahldialog. Beim Start liegt der Fokus auf der Liste, damit sofort mit Pfeiltasten
// und Eingabetaste bedient werden kann, ohne vorher irgendwo hinklicken zu müssen.
ApplicationWindow {
    id: root

    width: 460
    height: Math.min(560, header.implicitHeight + list.contentHeight + 80)
    visible: true
    title: qsTr("Gatekeeper")

    // Zifferntasten wählen direkt aus. Bei mehr als neun Browsern greift das nicht mehr,
    // dann bleiben Pfeiltasten und Maus.
    readonly property int directPickLimit: 9

    function pick(index) {
        if (index < 0 || index >= Session.browsers.length)
            return
        Session.choose(index)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        ColumnLayout {
            id: header

            Layout.fillWidth: true
            spacing: 2

            Label {
                text: qsTr("Link öffnen mit")
                font.pointSize: root.font.pointSize + 1
                font.bold: true
            }

            // Die Domain steht bewusst getrennt und hervorgehoben. Sie ist der Teil, an
            // dem sich erkennen lässt, wohin ein Link wirklich führt.
            Label {
                text: Session.targetHost
                visible: Session.targetHost.length > 0
                font.bold: true
                elide: Text.ElideRight
                Layout.fillWidth: true
            }

            Label {
                text: Session.targetUri
                visible: Session.targetUri.length > 0
                opacity: 0.6
                elide: Text.ElideMiddle
                Layout.fillWidth: true
            }

            // Schlägt der Start fehl, bleibt das Fenster stehen und sagt warum. Wortlos
            // zu verschwinden wäre die schlechtere Antwort auf einen Klick.
            Label {
                text: qsTr("Start fehlgeschlagen: %1").arg(Session.launchError)
                visible: Session.launchError.length > 0
                color: "#c0392b"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
                Layout.topMargin: 4
            }
        }

        // Chrome und Firefox tragen sich beim Start gern selbst wieder als Standard ein.
        // Der Hinweis erscheint nur, wenn das tatsächlich passiert ist, und lässt sich an
        // Ort und Stelle beheben.
        Pane {
            visible: Session.defaultBrowserHint.length > 0
            padding: 10
            Layout.fillWidth: true

            RowLayout {
                anchors.fill: parent
                spacing: 10

                Label {
                    text: Session.defaultBrowserHint
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Button {
                    text: qsTr("Übernehmen")
                    onClicked: Session.makeDefaultBrowser()
                }
            }
        }

        CheckBox {
            id: remember

            text: qsTr("Für %1 merken").arg(Session.targetHost)
            visible: Session.targetHost.length > 0
            Layout.fillWidth: true
            onToggled: Session.setRememberChoice(checked)
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
            model: Session.browsers
            focus: true
            clip: true
            spacing: 2
            keyNavigationWraps: true

            delegate: ItemDelegate {
                id: entry

                required property int index
                required property var modelData

                width: ListView.view.width
                highlighted: ListView.isCurrentItem
                onClicked: {
                    ListView.view.currentIndex = entry.index
                    root.pick(entry.index)
                }

                contentItem: RowLayout {
                    spacing: 10

                    Label {
                        text: entry.index < root.directPickLimit ? (entry.index + 1) : ""
                        opacity: 0.45
                        Layout.preferredWidth: 12
                    }

                    Image {
                        source: entry.modelData.icon.startsWith("/")
                                ? "file://" + entry.modelData.icon
                                : "image://theme/" + entry.modelData.icon
                        sourceSize: Qt.size(28, 28)
                        fillMode: Image.PreserveAspectFit
                        Layout.preferredWidth: 28
                        Layout.preferredHeight: 28
                    }

                    Label {
                        text: entry.modelData.name
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    // Woher der Browser stammt, ist bei mehreren Installationen desselben
                    // Browsers die einzige Unterscheidung.
                    Label {
                        text: entry.modelData.origin
                        opacity: 0.5
                        font.pointSize: root.font.pointSize - 1
                    }
                }
            }

            Keys.onReturnPressed: root.pick(currentIndex)
            Keys.onEnterPressed: root.pick(currentIndex)
            Keys.onEscapePressed: Qt.quit()
            Keys.onPressed: function (event) {
                if (event.key >= Qt.Key_1 && event.key <= Qt.Key_9) {
                    const index = event.key - Qt.Key_1
                    if (index < Session.browsers.length) {
                        currentIndex = index
                        root.pick(index)
                        event.accepted = true
                    }
                }
            }
        }
    }
}
