import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// What is said before a song: the default (`defaultLabel`: what's chosen
// above), this text, or nothing. `value` is null (the default), "" (nothing)
// or the text; `edited(value)` gives the new one. A pause has no default.
ColumnLayout {
    id: root
    property var value: null
    property string defaultLabel: ""
    property bool pause: false
    signal edited(var value)

    readonly property bool custom: !!value
    readonly property var kinds: pause ? ["custom", "none"] : ["default", "custom", "none"]

    ComboBox {
        Layout.fillWidth: true
        model: root.pause ? [qsTr("This text:"), qsTr("Nothing")] : [root.defaultLabel, qsTr("This text:"), qsTr("Nothing")]
        currentIndex: root.kinds.indexOf(root.value === undefined || root.value === null ? (root.pause ? "none" : "default") : root.value === "" ? "none" : "custom")
        onActivated: index => {
            const k = root.kinds[index];
            root.edited(k === "default" ? null : k === "none" ? "" : (root.value || (root.pause ? qsTr("We'll take a short break.") : qsTr("The next dance is a {dance}."))));
        }
    }
    TextField {
        visible: root.custom
        Layout.fillWidth: true
        text: root.value || ""
        onEditingFinished: if (text !== root.value) root.edited(text)
    }
    Label { visible: root.custom; text: qsTr("{dance} is replaced by the dance's name."); font.pointSize: Theme.smallPointSize; opacity: 0.6 }
}
