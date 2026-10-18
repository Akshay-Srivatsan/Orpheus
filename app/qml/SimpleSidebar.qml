import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The simple interface's sidebar: your songs, your collections (crates), your
// sets, and Spotify. Choosing a set opens it beside the library (to plan it
// and to play it); songs dropped on a set or a collection join it.
ViewFrame {
    id: root
    signal newSet()

    function showSongs(v) { Session.show(v); }
    function openSet(id) { Session.run("perform_open", { setlistId: id }); }
    function isView(v) { return Session.view.kind === v.kind && (Session.view.id || "") === (v.id || "") && (Session.view.uri || "") === (v.uri || ""); }
    readonly property bool spotifyOn: !!Session.spotify && Session.spotify.connected

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
                    // Songs dropped on a collection join it (a smart one picks its own).
                    dropKeys: modelData.smart ? [] : ["application/x-orpheus-versions"]
                    onDropped: ids => Session.run("crate_add", { id: modelData.id, versionIds: ids })
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
                    // The open set (beside the library): a dot, as the library view is the selection.
                    marked: !!Session.perform && Session.perform.setlistId === modelData.id
                    tip: marked ? qsTr("Open beside the library") : qsTr("Open this set")
                    // Songs dropped on a set go at its end.
                    dropKeys: ["application/x-orpheus-versions"]
                    onDropped: ids => Session.addToSetlist(modelData, ids)
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

            SectionTitle { text: qsTr("Spotify") }
            NavItem {
                visible: root.spotifyOn
                text: qsTr("Search Spotify"); icon.source: Theme.icon("search")
                selected: root.isView({ kind: "spotify-search" })
                onClicked: root.showSongs({ kind: "spotify-search" })
            }
            Repeater {
                model: root.spotifyOn && Session.spotifyPlaylists ? Session.spotifyPlaylists : []
                delegate: NavItem {
                    required property var modelData
                    text: modelData.name
                    icon.source: Theme.icon("spotify")
                    selected: root.isView({ kind: "spotify-playlist", uri: modelData.uri })
                    onClicked: root.showSongs({ kind: "spotify-playlist", uri: modelData.uri, name: modelData.name })
                }
            }
            Label {
                visible: Session.loadingPlaylists
                Layout.leftMargin: Theme.spacing * 2
                text: qsTr("Loading playlists…")
                opacity: Theme.dim
            }
            NavItem {
                visible: !root.spotifyOn
                text: Session.spotify && Session.spotify.connecting ? qsTr("Connecting…") : qsTr("Connect Spotify…")
                icon.source: Theme.icon("spotify")
                onClicked: Session.settingsPage = "spotify"
            }
            Item { Layout.fillHeight: true }
        }
    }
}
