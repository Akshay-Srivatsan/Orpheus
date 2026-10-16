import QtQuick
import QtQuick.Controls
import Orpheus

// A dance style as a coloured chip (its short name; the full name and any
// warnings on hover).
Rectangle {
    id: root

    /// A `Shown` style from the engine: name, short, color, problems.
    property var style
    property bool short: true

    implicitWidth: label.implicitWidth + 10
    implicitHeight: label.implicitHeight + 4
    radius: height / 2
    color: Qt.alpha(style ? style.color : Theme.neutral, 0.25)
    border.color: Qt.alpha(style ? style.color : Theme.neutral, style && style.problems.length ? 1 : 0.6)
    border.width: style && style.problems.length ? 2 : 1

    Label {
        id: label
        anchors.centerIn: parent
        text: root.style ? (root.short ? root.style.short : root.style.name) : ""
        font.pixelSize: 11
        font.bold: true
    }

    HoverHandler { id: hover }
    ToolTip.visible: hover.hovered && !!root.style
    ToolTip.text: root.style ? [root.style.name].concat(root.style.problems).join("\n") : ""
}
