import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A length in seconds, or empty for the default (`placeholder`: what that is).
// `edited(value)` gives the new one (null: the default).
RowLayout {
    id: root
    property var value: null
    property real placeholder: 0
    signal edited(var value)

    NumberField {
        implicitWidth: 70
        minimum: 0
        value: root.value
        placeholderText: String(root.placeholder)
        onCommitted: v => root.edited(v)
    }
    Label { text: qsTr("s") }
}
