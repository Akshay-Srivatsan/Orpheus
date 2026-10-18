import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Searching Spotify (or opening a link), and your playlists.
Pane {
    id: root
    padding: 0

    readonly property var playlist: Session.view.kind === "spotify-playlist" ? Session.view : null
    property var results: []
    property bool loading: false
    property string error: ""
    /// The query searched last (a search isn't repeated).
    property string searched: ""

    function search(q) {
        q = q.trim();
        if (!q || q === searched) return;
        searched = q;
        loading = true;
        error = "";
        const link = /^(spotify:|https?:\/\/open\.spotify\.com\/|open\.spotify\.com\/)/.test(q);
        Backend.call(link ? "spotify_resolve" : "spotify_search", link ? { input: q } : { query: q }).then(r => { results = r; loading = false; },
            e => { error = String(e); loading = false; searched = ""; });
    }
    function loadPlaylist() {
        if (!playlist) return;
        const uri = playlist.uri;
        searched = "";
        loading = true;
        error = "";
        results = [];
        Backend.call("spotify_playlist_tracks", { uri: uri }).then(r => { if (root.playlist && root.playlist.uri === uri) results = r; loading = false; },
            e => { error = String(e); loading = false; });
    }
    onPlaylistChanged: loadPlaylist()
    Component.onCompleted: { if (playlist) loadPlaylist(); else if (Session.search.trim()) { query.text = Session.search; search(Session.search); } }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacing

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            Label { visible: !!root.playlist; text: root.playlist ? root.playlist.name : ""; font.bold: true; font.pointSize: Theme.titlePointSize; Layout.fillWidth: true; elide: Text.ElideRight }
            Button {
                text: qsTr("Add All to Library")
                enabled: (root.results.length > 0) && (!!root.playlist)
                onClicked: Session.run("spotify_versions", { tracks: root.results, inLibrary: true }).then(v => { if (v) Session.toast(qsTr("Added %1 songs to your library").arg(v.length), "info"); })
            }
            Button {
                highlighted: true
                text: qsTr("New Setlist from Playlist")
                enabled: (root.results.length > 0) && (!!root.playlist)
                onClicked: Session.run("setlist_from_spotify", { name: root.playlist.name, tracks: root.results }).then(n => {
                    if (n !== undefined) Session.toast(qsTr("Created setlist “%1” with %2 songs. Label their dance styles in the set panel.").arg(root.playlist.name).arg(n), "info");
                })
            }
            TextField {
                id: query
                visible: !root.playlist
                Layout.fillWidth: true
                placeholderText: qsTr("Search Spotify, or paste a track / album / playlist link")
                onAccepted: root.search(text)
            }
            Button { enabled: !root.playlist; text: qsTr("Search"); onClicked: root.search(query.text) }
        }
        Label { visible: !!root.error; Layout.leftMargin: Theme.spacing; text: root.error; color: Theme.error; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        ViewFrame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.bottomMargin: Theme.spacing
            ScrollView {
                anchors.fill: parent
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width
                    Label { visible: root.loading; text: qsTr("Loading…"); opacity: 0.6; Layout.margins: Theme.spacing }
                    SpotifyTrackList { Layout.fillWidth: true; tracks: root.results }
                    Label {
                        visible: !root.loading && !root.results.length && !root.playlist
                        Layout.margins: Theme.spacing * 2
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        text: qsTr("Search Spotify by song or artist, or paste an open.spotify.com link.\nPreviewing or adding a song to your set doesn't add it to your library.")
                    }
                }
            }
        }
    }
}
