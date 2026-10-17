import QtQuick
import QtQuick.Controls
import Orpheus

// Edits a list of your dance styles (or tags, `tags: true`): chips to remove
// and an add menu. With `choices`, only those can be toggled on and off.
Flow {
    id: root
    property var value: []
    property var choices: null
    property bool tags: false
    signal edited(var keys)

    spacing: 4

    readonly property var all: tags ? Session.myTags : Session.myStyles
    readonly property var addable: all.filter(s => !value.includes(s.key))
    function info(k) { return all.find(s => s.key === k) || { key: k, name: k, color: Theme.neutral }; }

    Repeater {
        model: root.choices ? root.choices : root.value
        delegate: Rectangle {
            id: chip
            required property var modelData
            readonly property var s: root.info(modelData)
            readonly property bool on: root.value.includes(modelData)
            implicitWidth: row.implicitWidth + 12
            implicitHeight: 22
            radius: 11
            color: Qt.alpha(s.color || Theme.neutral, on ? 0.3 : 0.08)
            border.color: Qt.alpha(s.color || Theme.neutral, on ? 0.8 : 0.3)
            Row {
                id: row
                anchors.centerIn: parent
                spacing: 2
                Label { text: chip.s.name; font.pointSize: Theme.smallPointSize; opacity: chip.on ? 1 : 0.5; anchors.verticalCenter: parent.verticalCenter }
                ToolButton {
                    visible: !root.choices
                    icon.source: Theme.icon("x")
                    icon.width: 10; icon.height: 10
                    implicitWidth: 16; implicitHeight: 16
                    padding: 0
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Remove")
                    onClicked: root.edited(root.value.filter(x => x !== chip.modelData))
                }
            }
            MouseArea {
                anchors.fill: parent
                enabled: !!root.choices
                onClicked: root.edited(chip.on ? root.value.filter(x => x !== chip.modelData) : root.choices.filter(x => x === chip.modelData || root.value.includes(x)))
            }
        }
    }
    Button {
        visible: !root.choices && root.addable.length > 0
        flat: true
        text: root.value.length ? "+" : root.tags ? qsTr("Add tag…") : qsTr("Add style…")
        implicitHeight: 22
        onClicked: addMenu.popup()
        Menu {
            id: addMenu
            Instantiator {
                model: root.addable
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.name
                    onTriggered: root.edited(root.value.concat([modelData.key]))
                }
                onObjectAdded: (i, o) => addMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => addMenu.removeItem(o)
            }
        }
    }
    Button {
        visible: !root.choices && !root.all.length
        flat: true
        text: root.tags ? qsTr("Create tags…") : qsTr("Create dance styles…")
        implicitHeight: 22
        onClicked: Session.settingsPage = "styles"
    }
}
