import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A sidebar section's title, with small buttons (`actions`) at its end.
RowLayout {
    id: root
    property alias text: label.text
    property list<Action> actions

    Layout.fillWidth: true
    Layout.topMargin: Theme.spacing
    Layout.leftMargin: Theme.spacing
    Layout.rightMargin: 4

    Label {
        id: label
        Layout.fillWidth: true
        font.pixelSize: 11
        font.bold: true
        font.capitalization: Font.AllUppercase
        opacity: 0.6
    }
    Repeater {
        model: root.actions
        delegate: ToolButton {
            required property var modelData
            action: modelData
            implicitWidth: 24
            implicitHeight: 22
        }
    }
}
