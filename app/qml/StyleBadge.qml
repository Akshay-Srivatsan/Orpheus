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
    /// Shown instead of the style's name (e.g. a set entry's custom label).
    property string label: ""

    implicitWidth: badgeText.implicitWidth + 10
    implicitHeight: badgeText.implicitHeight + 4
    radius: height / 2
    color: Qt.alpha(style ? style.color : Theme.neutral, 0.25)
    border.color: Qt.alpha(style ? style.color : Theme.neutral, style && style.problems.length ? 1 : 0.6)
    border.width: style && style.problems.length ? 2 : 1

    Label {
        id: badgeText
        anchors.centerIn: parent
        text: root.label || (root.style ? (root.short ? root.style.short : root.style.name) : "")
        font.pointSize: Theme.smallPointSize
        font.bold: true
    }

    HoverHandler { id: hover }
    ToolTip.visible: hover.hovered && (!!root.style || !!root.label)
    ToolTip.text: root.style ? [root.style.name].concat(root.style.problems).join("\n") : root.label
}
