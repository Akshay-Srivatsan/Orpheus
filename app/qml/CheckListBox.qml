import QtQuick
import QtQuick.Controls
import Orpheus

// Some of a list, chosen: a combo box showing those on, whose list ticks
// them on and off (`items`: { key, name, on, note? }; `toggled(key)`). An
// optional last item (`actionText`) does something else (`action()`), e.g.
// going back to the automatic choice.
ComboBox {
    id: root
    property var items: []
    property string emptyText: qsTr("None")
    property string actionText: ""
    property bool actionEnabled: true
    signal toggled(string key)
    signal action()

    model: items.map(i => ({ key: i.key, name: i.name, note: i.note || "" })).concat(actionText ? [{ key: "", name: actionText, note: "" }] : [])
    textRole: "name"
    currentIndex: -1
    displayText: items.some(i => i.on) ? items.filter(i => i.on).map(i => i.name).join(", ") : emptyText
    ToolTip.visible: hovered && displayText.length > 30
    ToolTip.text: displayText

    delegate: CheckDelegate {
        required property var modelData
        required property int index
        width: ListView.view.width
        text: modelData.name
        enabled: !!modelData.key || root.actionEnabled
        checkable: !!modelData.key
        checked: !!modelData.key && root.items.some(i => i.key === modelData.key && i.on)
        indicator.visible: !!modelData.key
        ToolTip.visible: hovered && !!modelData.note
        ToolTip.text: modelData.note
        onClicked: modelData.key ? root.toggled(modelData.key) : root.action()
    }
}
