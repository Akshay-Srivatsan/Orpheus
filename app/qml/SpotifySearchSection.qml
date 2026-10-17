import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Spotify results under the library while searching (once typing pauses).
ColumnLayout {
    id: root
    property string query: ""
    property var results: []
    property bool loading: false
    property string error: ""
    property bool open: true
    /// Shortest search sent to Spotify, and how long typing must pause first (ms).
    readonly property int minLength: 3
    readonly property int pauseMs: 800
    /// Results shown here (the rest are in Spotify search).
    readonly property int shownMax: 20

    visible: query.trim().length >= minLength
    spacing: 0

    onQueryChanged: { results = []; if (query.trim().length >= minLength) later.restart(); }
    Timer {
        id: later
        interval: root.pauseMs
        onTriggered: {
            const q = root.query.trim();
            root.loading = true;
            root.error = "";
            Backend.call("spotify_search", { query: q }).then(r => { if (root.query.trim() === q) root.results = r; root.loading = false; },
                e => { root.error = String(e); root.loading = false; });
        }
    }

    ItemDelegate {
        Layout.fillWidth: true
        onClicked: root.open = !root.open
        contentItem: RowLayout {
            Label { text: qsTr("Spotify"); font.bold: true }
            Label { text: root.loading ? qsTr("searching…") : root.error ? "" : qsTr("%1 results").arg(root.results.length); opacity: 0.6 }
            Item { Layout.fillWidth: true }
            Label { text: root.open ? qsTr("Hide") : qsTr("Show"); opacity: 0.6 }
        }
    }
    Label { visible: !!root.error; text: root.error; color: Theme.error; Layout.leftMargin: Theme.spacing }
    ViewFrame {
        visible: root.open
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(260, list.implicitHeight + 2)
        ScrollView {
            anchors.fill: parent
            contentWidth: availableWidth
            SpotifyTrackList { id: list; width: parent.width; tracks: root.results.slice(0, root.shownMax); compact: true }
        }
    }
    Button {
        visible: root.open && root.results.length > root.shownMax
        flat: true
        text: qsTr("Show All %1 in Spotify Search…").arg(root.results.length)
        onClicked: Session.show({ kind: "spotify-search" })
    }
}
