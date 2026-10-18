import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The simple interface: a music player's layout. The sidebar (songs,
// collections, sets) beside the page it shows: the open set (to plan and
// play it) or the library. The player is along the window's bottom.
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
        SplitView.minimumWidth: 160
        onNewSet: root.newSet()
    }
    Pane {
        SplitView.fillWidth: true
        SplitView.minimumWidth: 360
        padding: Theme.spacing
        StackLayout {
            anchors.fill: parent
            currentIndex: Session.simplePage === "songs" ? 1 : 0
            ViewFrame {
                padding: 0
                SetPanel {
                    id: setPanel
                    anchors.fill: parent
                    simple: true
                    onNewSetlist: root.newSet()
                }
            }
            LibraryPane {
                id: library
                simple: true
                onCombine: (songs, recordings) => root.combine(songs, recordings)
                onEditSmartCrate: id => root.editSmartCrate(id)
            }
        }
    }
}
