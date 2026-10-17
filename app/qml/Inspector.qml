import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The Info tab: the selected song, its versions and the audio they play,
// and the selected set entry's settings (`song_view`).
Pane {
    id: root
    padding: Theme.spacing

    property var view: null
    property bool editLinkShown: false

    function reload() {
        if (!Session.inspectTrack && !Session.inspectEntry) { view = null; return; }
        Backend.call("song_view", { trackId: Session.inspectTrack || null, entryId: Session.inspectEntry || null })
            .then(v => root.view = v, e => Session.toast(String(e)));
    }
    Timer { id: later; interval: 30; onTriggered: root.reload() }
    Connections {
        target: Session
        function onInspectTrackChanged() { later.restart(); }
        function onInspectEntryChanged() { later.restart(); }
        function onLibraryChanged() { later.restart(); }
        function onRecordingsChanged() { later.restart(); }
        function onSetlistChanged() { later.restart(); }
        function onStemsChanged() { later.restart(); }
    }

    readonly property var t: view && view.track.id ? view.track : null

    function patchTrack(p) { Session.run("track_update", { id: t.id, patch: p }); }

    function newVersion() {
        const style = Session.styleFilter.length === 1 && Session.styleFilter[0] !== "__none" ? Session.styleFilter[0] : null;
        Session.run("version_create", { trackId: t.id, version: { name: style ? Session.styleName(style) : qsTr("New version"), styles: style ? [style] : [] } })
            .then(v => { if (v) Session.selected = [v.id]; });
    }

    function addSpotifyEdit(link) {
        Session.run("spotify_resolve", { input: link }).then(hits => {
            if (!hits) return;
            if (!hits.length) { Session.toast(qsTr("That isn't a Spotify track link")); return; }
            Session.addEdit({ spotify: hits[0] }, t.id);
            root.editLinkShown = false;
        });
    }

    Label {
        visible: !root.view
        anchors.centerIn: parent
        opacity: 0.6
        text: qsTr("Select a song to see and edit its details.")
    }

    ScrollView {
        visible: !!root.view
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: Theme.spacing

            EntryBox {
                visible: !!root.view && !!root.view.entry
                Layout.fillWidth: true
                versionTargetBpm: {
                    if (!root.view || !root.view.entry) return null;
                    const v = root.view.versions.find(x => x.version.id === root.view.entry.entry.versionId);
                    return v ? v.version.targetBpm || null : null;
                }
                view: root.view && root.view.entry ? root.view.entry : { entry: {}, showAs: [], versionChoices: [], tags: [], isPause: false, shownNames: "", playback: null, defaultPauseBefore: 0, defaultPauseAfter: 0, speaks: false }
            }

            Frame {
                visible: !!root.t && root.t.inLibrary === false
                Layout.fillWidth: true
                RowLayout {
                    anchors.fill: parent
                    Label { Layout.fillWidth: true; wrapMode: Text.WordWrap; text: qsTr("Only in setlists — not in your library.") }
                    Button { text: qsTr("Add to Library"); onClicked: Session.run("track_set_in_library", { id: root.t.id, value: true }) }
                }
            }

            // The song.
            GroupBox {
                visible: !!root.t
                Layout.fillWidth: true
                title: qsTr("Song")
                ColumnLayout {
                    anchors.fill: parent
                    TextField {
                        Layout.fillWidth: true
                        font.pointSize: Theme.titlePointSize
                        font.bold: true
                        text: root.t ? root.t.title : ""
                        onEditingFinished: if (text !== root.t.title) root.patchTrack({ title: text })
                    }
                    TextField {
                        Layout.fillWidth: true
                        placeholderText: qsTr("Artist")
                        text: root.t ? root.t.artist : ""
                        onEditingFinished: if (text !== root.t.artist) root.patchTrack({ artist: text })
                    }
                    TextField {
                        Layout.fillWidth: true
                        placeholderText: qsTr("Album")
                        text: root.t ? root.t.album : ""
                        onEditingFinished: if (text !== root.t.album) root.patchTrack({ album: text })
                    }
                    TextArea {
                        Layout.fillWidth: true
                        placeholderText: qsTr("Notes")
                        wrapMode: TextEdit.Wrap
                        text: root.t ? root.t.notes || "" : ""
                        onEditingFinished: if (text !== (root.t.notes || "")) root.patchTrack({ notes: text })
                    }
                    Label {
                        opacity: Theme.dim
                        text: root.t ? qsTr("%1 version(s) · %2 audio · played %3×").arg(root.t.versions.length).arg(root.t.recordings.length).arg(root.t.playCount) : ""
                    }
                }
            }

            // Its versions.
            RowLayout {
                visible: !!root.t
                Layout.fillWidth: true
                Heading { Layout.fillWidth: true; text: qsTr("Versions") }
                Button {
                    flat: true
                    icon.source: Theme.icon("plus"); text: qsTr("New Version")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("A version playing the same audio (other styles, cues, tempo…)")
                    onClicked: root.newVersion()
                }
                Button {
                    flat: true
                    text: qsTr("Add an Edit…")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("A version playing other audio: another edit of this song")
                    onClicked: editMenu.popup()
                    Menu {
                        id: editMenu
                        MenuItem {
                            text: qsTr("From a File…")
                            onTriggered: Session.chooseFile({ title: qsTr("Audio for the new version") }, path => Session.addEdit({ path: path }, root.t.id))
                        }
                        MenuItem { text: qsTr("From Spotify…"); enabled: !!Session.spotify && Session.spotify.connected; onTriggered: root.editLinkShown = true }
                        MenuItem { text: qsTr("From the Library…"); onTriggered: { Session.pickingEditTitle = root.t.title; Session.pickingEdit = root.t.id; } }
                    }
                }
            }
            RowLayout {
                visible: root.editLinkShown
                Layout.fillWidth: true
                TextField {
                    id: editLink
                    Layout.fillWidth: true
                    placeholderText: qsTr("Spotify link of the edit")
                    onVisibleChanged: if (visible) { text = ""; forceActiveFocus(); }
                    onAccepted: root.addSpotifyEdit(text)
                    Keys.onEscapePressed: root.editLinkShown = false
                }
                Button { text: qsTr("Add"); enabled: editLink.text.trim().length > 0; onClicked: root.addSpotifyEdit(editLink.text) }
                Button { flat: true; text: qsTr("Cancel"); onClicked: root.editLinkShown = false }
            }
            Repeater {
                model: root.t ? root.view.versions : []
                delegate: VersionCard {
                    required property var modelData
                    Layout.fillWidth: true
                    view: modelData
                    song: root.view
                    highlighted: Session.selected.includes(modelData.version.id) || (!!root.view.entry && root.view.entry.entry.versionId === modelData.version.id)
                }
            }
        }
    }
}
