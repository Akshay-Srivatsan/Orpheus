import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A table column's header section: click to sort by it (the sorted one shows
// which way); drag its right edge to resize the column (`resizable`).
AbstractButton {
    id: root
    property string tip: ""
    /// The column is sorted by: "" (it isn't), "up" or "down".
    property string sorted: ""
    property bool resizable: false
    property int alignment: Qt.AlignLeft
    /// Space before the text (to line it up with the cells' text).
    property real indent: 0
    signal contextMenu()
    /// The column's edge was dragged by `dx` pixels.
    signal resized(real dx)
    /// The drag that resized it ended.
    signal resizeFinished()

    implicitHeight: Theme.rowHeight
    hoverEnabled: true
    leftPadding: 6 + indent
    rightPadding: 6

    contentItem: RowLayout {
        spacing: 2
        Item { visible: root.alignment === Qt.AlignRight; Layout.fillWidth: true }
        Label {
            Layout.fillWidth: root.alignment !== Qt.AlignRight
            text: root.text
            elide: Text.ElideRight
            color: Theme.system.buttonText
        }
        Icon {
            visible: !!root.sorted
            glyph: root.sorted === "down" ? "sort-down" : "sort-up"
            sourceSize.width: 12
            sourceSize.height: 12
            color: Theme.system.buttonText
            opacity: Theme.dim
        }
    }
    background: Rectangle {
        color: root.pressed ? Theme.system.mid : root.hovered ? Qt.lighter(Theme.system.button, Theme.dark ? 1.3 : 1.04) : Theme.system.button
        Rectangle { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; width: 1; height: parent.height - 8; color: Theme.system.mid }
    }

    TapHandler { acceptedButtons: Qt.RightButton; onTapped: root.contextMenu() }
    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
    ToolTip.delay: 600

    // The resize handle on the right edge.
    MouseArea {
        visible: root.resizable
        anchors.right: parent.right
        anchors.rightMargin: -3
        width: 7
        height: parent.height
        cursorShape: Qt.SplitHCursor
        property real startX: 0
        onPressed: m => startX = m.x
        onPositionChanged: m => { if (pressed) root.resized(m.x - startX); }
        onReleased: root.resizeFinished()
    }
}
