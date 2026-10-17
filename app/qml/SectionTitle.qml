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
        font.bold: true
        opacity: Theme.dim
    }
    Repeater {
        model: root.actions
        delegate: ToolButton {
            required property var modelData
            action: modelData
            display: AbstractButton.IconOnly
            icon.width: Theme.smallIconSize
            icon.height: Theme.smallIconSize
            implicitWidth: 26
            implicitHeight: 24
            ToolTip.visible: hovered
            ToolTip.text: modelData.text
        }
    }
}
