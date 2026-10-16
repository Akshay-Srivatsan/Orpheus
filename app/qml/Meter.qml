import QtQuick
import Orpheus

// A level meter: `value` from 0 to 1 (already scaled for display); red near the top.
Rectangle {
    id: root
    property real value: 0
    /// Above this the level shows as clipping.
    readonly property real hot: 0.985

    implicitWidth: 50
    implicitHeight: 6
    radius: 3
    color: Qt.alpha(palette.text, 0.12)

    Rectangle {
        width: parent.width * Math.max(0, Math.min(1, root.value))
        height: parent.height
        radius: parent.radius
        color: root.value > root.hot ? Theme.error : Theme.ok
    }
}
