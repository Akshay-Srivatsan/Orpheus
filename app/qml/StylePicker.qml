import QtQuick
import QtQuick.Controls
import Orpheus

// Chooses some of your dance styles (or tags, `tags: true`): a combo box
// showing those chosen, whose list ticks them on and off. With `choices`, only
// those are offered. `edited(keys)` gives the new choice.
ComboBox {
    id: root
    property var value: []
    property var choices: null
    property bool tags: false
    signal edited(var keys)

    readonly property var all: tags ? Session.myTags : Session.myStyles
    readonly property var offered: choices ? all.filter(s => choices.includes(s.key)) : all
    function nameOf(k) { const s = all.find(x => x.key === k); return s ? s.name : k; }
    function toggle(key) {
        edited(value.includes(key) ? value.filter(k => k !== key) : offered.map(s => s.key).filter(k => k === key || value.includes(k)));
    }

    // The styles, then a way to edit them.
    model: offered.map(s => ({ key: s.key, name: s.name })).concat([{ key: "", name: tags ? qsTr("Edit Tags…") : qsTr("Edit Dance Styles…") }])
    textRole: "name"
    currentIndex: -1
    displayText: value.length ? value.map(k => nameOf(k)).join(", ") : tags ? qsTr("No tags") : qsTr("No style")
    ToolTip.visible: hovered && value.length > 2
    ToolTip.text: displayText

    delegate: CheckDelegate {
        required property var modelData
        required property int index
        width: ListView.view.width
        text: modelData.name
        checkable: !!modelData.key
        checked: !!modelData.key && root.value.includes(modelData.key)
        indicator.visible: !!modelData.key
        onClicked: modelData.key ? root.toggle(modelData.key) : Session.settingsPage = "styles"
    }
}
