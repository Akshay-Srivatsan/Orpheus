import QtQuick
import QtQuick.Controls
import Orpheus

// A table column's header: click to sort by it.
ItemDelegate {
    id: root
    property string tip: ""
    signal contextMenu()

    implicitHeight: Theme.rowHeight
    padding: 2
    font.pixelSize: 11
    font.bold: true
    font.capitalization: Font.AllUppercase
    opacity: 0.75

    TapHandler { acceptedButtons: Qt.RightButton; onTapped: root.contextMenu() }
    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
    ToolTip.delay: 600
}
