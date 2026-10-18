import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The simple interface, laid out like a music player (Elisa): the sidebar
// (songs, collections, sets, Spotify), the library it shows, and the open
// set beside it, as a player's playlist. The player is along the top.
SplitView {
    id: root
    orientation: Qt.Horizontal

    property alias setPanel: setPanel
    property alias library: library
    signal newSet()
    signal combine(var songs, var recordings)
    signal editSmartCrate(string id)

    SimpleSidebar {
        SplitView.preferredWidth: Theme.sidebarWidth
        SplitView.minimumWidth: 140
        onNewSet: root.newSet()
    }
    Pane {
        SplitView.fillWidth: true
        SplitView.minimumWidth: 280
        padding: Theme.spacing
        LibraryPane {
            id: library
            anchors.fill: parent
            simple: true
            onCombine: (songs, recordings) => root.combine(songs, recordings)
            onEditSmartCrate: id => root.editSmartCrate(id)
        }
    }
    // The open set: what plays, in order (double-click a song to add it).
    Pane {
        SplitView.preferredWidth: Theme.rightPanelWidth
        SplitView.minimumWidth: 220
        padding: Theme.spacing
        leftPadding: 0
        ViewFrame {
            anchors.fill: parent
            padding: 0
            SetPanel {
                id: setPanel
                anchors.fill: parent
                simple: true
                onNewSetlist: root.newSet()
            }
        }
    }
}
