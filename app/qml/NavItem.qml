import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A sidebar row: an icon, a name and a count; songs can be dropped on it
// (`dropKeys`, `dropped(versionIds)`).
ItemDelegate {
    id: root
    property int count: 0
    property bool selected: false
    /// A dot after the name (e.g. the set open in the set panel).
    property bool marked: false
    property string tip: ""
    property var dropKeys: []

    signal dropped(var versionIds)
    signal contextMenu()

    Layout.fillWidth: true
    highlighted: selected || drop.containsDrag
    icon.width: 16
    icon.height: 16
    rightPadding: badges.width + Theme.spacing * 2

    RowLayout {
        id: badges
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        Label { visible: root.marked; text: "●"; color: Theme.ok; font.pixelSize: 9 }
        Label { visible: root.count > 0; text: root.count; opacity: 0.55; font.pixelSize: 11 }
    }

    TapHandler { acceptedButtons: Qt.RightButton; onTapped: root.contextMenu() }

    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
    ToolTip.delay: 600

    DropArea {
        id: drop
        anchors.fill: parent
        keys: root.dropKeys
        enabled: root.dropKeys.length > 0
        onDropped: d => root.dropped(JSON.parse(d.getDataAsString("application/x-orpheus-versions") || "[]"))
    }
}
