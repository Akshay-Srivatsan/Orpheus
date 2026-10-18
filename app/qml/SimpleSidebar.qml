import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The simple interface's sidebar: your songs, your collections (crates), and
// your sets. Choosing a set opens it (to plan it and to play it).
ViewFrame {
    id: root
    signal newSet()

    function showSongs(v) { Session.show(v); Session.simplePage = "songs"; }
    function openSet(id) { Session.run("perform_open", { setlistId: id }); Session.simplePage = "set"; }
    readonly property bool onSongs: Session.simplePage === "songs"
    function isView(v) { return onSongs && Session.view.kind === v.kind && (Session.view.id || "") === (v.id || ""); }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        ColumnLayout {
            width: parent.width
            spacing: 0

            SectionTitle { text: qsTr("Library") }
            NavItem {
                text: qsTr("Songs"); icon.source: Theme.icon("music")
                count: Session.summary ? Session.summary.songs : 0
                selected: root.isView({ kind: "all" })
                onClicked: root.showSongs({ kind: "all" })
            }
            NavItem {
                visible: !!Session.summary && Session.summary.missing > 0
                text: qsTr("Missing Files"); icon.source: Theme.icon("warning")
                warning: true
                count: Session.summary ? Session.summary.missing : 0
                selected: root.isView({ kind: "missing" })
                onClicked: root.showSongs({ kind: "missing" })
            }

            SectionTitle { text: qsTr("Collections"); visible: Session.crates.length > 0 }
            Repeater {
                model: Session.crates
                delegate: NavItem {
                    required property var modelData
                    text: modelData.name
                    icon.source: Theme.icon(modelData.smart ? "auto" : "crate")
                    count: Session.summary && Session.summary.crates ? Session.summary.crates[modelData.id] || 0 : -1
                    selected: root.isView({ kind: "crate", id: modelData.id })
                    onClicked: root.showSongs({ kind: "crate", id: modelData.id })
                }
            }

            SectionTitle {
                text: qsTr("Sets")
                actions: [Action { text: qsTr("New Set…"); icon.source: Theme.icon("plus"); onTriggered: root.newSet() }]
            }
            Repeater {
                model: Session.setlists
                delegate: NavItem {
                    required property var modelData
                    text: modelData.name
                    icon.source: Theme.icon("list")
                    count: modelData.entryCount
                    selected: !root.onSongs && !!Session.perform && Session.perform.setlistId === modelData.id
                    onClicked: root.openSet(modelData.id)
                }
            }
            Label {
                visible: !Session.setlists.length
                Layout.fillWidth: true
                Layout.margins: Theme.spacing
                wrapMode: Text.WordWrap
                opacity: Theme.dim
                text: qsTr("No sets yet: make one with + to plan what you'll play.")
            }
            Item { Layout.fillHeight: true }
        }
    }
}
