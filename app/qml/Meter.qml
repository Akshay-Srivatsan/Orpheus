import QtQuick
import Orpheus

// A vertical level meter: `value` from 0 to 1 (already scaled for display),
// green, then amber near the top and red when clipping.
Rectangle {
    id: root
    property real value: 0
    /// Above these the level shows as loud, then clipping.
    readonly property real loud: 0.85
    readonly property real hot: 0.985
    readonly property real shown: Math.max(0, Math.min(1, value))

    implicitWidth: 6
    implicitHeight: 24
    radius: 1
    color: Qt.alpha(Theme.system.text, 0.1)
    border.color: Qt.alpha(Theme.system.text, 0.15)

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: parent.height * root.shown
        radius: parent.radius
        color: root.shown > root.hot ? Theme.error : root.shown > root.loud ? Theme.warn : Theme.ok
    }
}
