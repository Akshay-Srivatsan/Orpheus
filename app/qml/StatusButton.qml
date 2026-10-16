import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A connection: a light (status "ok", "warn", "bad", or off) and its name.
ToolButton {
    id: root
    property string status: ""
    property string tip: ""

    contentItem: RowLayout {
        spacing: 4
        Rectangle {
            width: 8; height: 8; radius: 4
            color: root.status === "ok" ? Theme.ok : root.status === "warn" ? Theme.warn : root.status === "bad" ? Theme.error : Theme.neutral
        }
        Label { text: root.text }
    }
    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
}
