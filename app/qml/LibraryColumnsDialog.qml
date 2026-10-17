import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The library table's columns: which show, and in what order (most important
// first: when the table is narrow, the last ones go to each song's menu).
AppDialog {
    id: root
    title: qsTr("Library Columns")
    standardButtons: Dialog.Ok | Dialog.Cancel | Dialog.RestoreDefaults

    /// The table (its columns' names, and your choice as saved).
    property var table: null
    /// The choice being edited: columns in order, and those hidden.
    property var order: []
    property var hidden: []

    onAboutToShow: {
        order = table.order.slice();
        hidden = table.saved.hidden.slice();
    }
    onAccepted: table.saveColumns({ order: order, hidden: hidden })
    onReset: { order = table.optional.slice(); hidden = []; }

    function move(i, by) {
        const next = order.slice();
        const [k] = next.splice(i, 1);
        next.splice(i + by, 0, k);
        order = next;
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacing
        Label {
            Layout.fillWidth: true
            Layout.maximumWidth: 360
            wrapMode: Text.WordWrap
            text: qsTr("Title comes first. Columns higher in the list are kept when the window is narrow; those that don't fit show in each song's menu.")
        }
        ViewFrame {
            Layout.fillWidth: true
            implicitHeight: list.contentHeight + 2
            ListView {
                id: list
                anchors.fill: parent
                interactive: false
                model: root.order
                delegate: ItemDelegate {
                    id: item
                    required property string modelData
                    required property int index
                    width: ListView.view.width
                    contentItem: RowLayout {
                        CheckBox {
                            Layout.fillWidth: true
                            text: root.table ? root.table.names[item.modelData] : item.modelData
                            checked: !root.hidden.includes(item.modelData)
                            onToggled: root.hidden = checked ? root.hidden.filter(k => k !== item.modelData) : root.hidden.concat([item.modelData])
                        }
                        ToolButton {
                            icon.source: Theme.icon("up")
                            enabled: item.index > 0
                            ToolTip.visible: hovered; ToolTip.text: qsTr("More important")
                            onClicked: root.move(item.index, -1)
                        }
                        ToolButton {
                            icon.source: Theme.icon("down")
                            enabled: item.index < root.order.length - 1
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Less important")
                            onClicked: root.move(item.index, 1)
                        }
                    }
                }
            }
        }
    }
}
