import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Settings, in a window of its own: a page per topic. Opens on the page
// `Session.settingsPage` names; closing it clears that.
ApplicationWindow {
    id: root
    title: qsTr("Orpheus Settings")
    width: 1000
    height: 720
    minimumWidth: 760
    minimumHeight: 520
    visible: Session.settingsPage !== ""
    onClosing: Session.settingsPage = ""

    readonly property var pages: [
        ["general", qsTr("General")], ["library", qsTr("Library")], ["audio", qsTr("Audio")], ["midi", qsTr("Controllers")],
        ["stems", qsTr("Stems")], ["spotify", qsTr("Spotify")], ["projector", qsTr("Projector")], ["auto", qsTr("Auto Mode")],
        ["styles", qsTr("Styles && Tags")], ["clubs", qsTr("Clubs")],
    ]
    readonly property int current: Math.max(0, pages.findIndex(p => p[0] === Session.settingsPage))

    Shortcut { sequences: [StandardKey.Cancel]; onActivated: root.close() }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // The pages, in a full-height list.
        Rectangle {
            Layout.preferredWidth: 180
            Layout.fillHeight: true
            color: Theme.system.base
            ListView {
                anchors.fill: parent
                anchors.topMargin: Theme.spacing / 2
                model: root.pages
                currentIndex: root.current
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    text: modelData[1]
                    highlighted: index === root.current
                    onClicked: Session.settingsPage = modelData[0]
                }
            }
        }
        Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: Theme.system.mid }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.current
            // Pages load when first shown (and stay loaded).
            Repeater {
                model: ["GeneralPage", "LibraryPage", "AudioPage", "ControllersPage", "StemsPage", "SpotifyPage", "ProjectorPage", "AutoModePage", "StylesPage", "ClubsPage"]
                delegate: Loader {
                    required property string modelData
                    required property int index
                    active: root.visible && (index === root.current || item !== null)
                    source: modelData + ".qml"
                }
            }
        }
    }
}
