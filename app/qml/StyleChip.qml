import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A style filter chip: a dot in the style's colour, its name and how many
// songs have it. Songs dropped on it are given the style (`dropped(ids)`).
Control {
    id: root
    property string text: ""
    property bool checked: false
    property int count: -1
    property color color: "transparent"
    property string tip: ""

    signal dropped(var versionIds)
    /// `add`: Ctrl or Shift was held (add to the filter instead of replacing it).
    signal chosen(bool add)

    hoverEnabled: true
    padding: 4
    leftPadding: 8
    rightPadding: 8

    contentItem: RowLayout {
        spacing: 4
        Rectangle { visible: root.color.a > 0; width: 7; height: 7; radius: 4; color: root.color }
        Label { text: root.text; font.pixelSize: 12 }
        Label { visible: root.count >= 0; text: root.count; font.pixelSize: 11; opacity: 0.6 }
    }
    background: Rectangle {
        radius: height / 2
        color: root.checked ? Qt.alpha(palette.highlight, 0.35) : drop.containsDrag ? Qt.alpha(palette.highlight, 0.2) : palette.button
        border.color: root.checked ? palette.highlight : palette.mid
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: mouse => root.chosen((mouse.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.MetaModifier)) !== 0)
    }

    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["application/x-orpheus-versions"]
        onDropped: d => root.dropped(JSON.parse(d.getDataAsString("application/x-orpheus-versions") || "[]"))
    }

    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
    ToolTip.delay: 600
}
