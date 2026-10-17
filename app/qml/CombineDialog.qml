import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Combine: songs become one song. If their audio is the same (FLAC and MP3,
// a file and its Spotify track: suggested by comparing fingerprints), it
// becomes one audio too; otherwise each version keeps its own. Where they
// differ (title, BPM, …), the DJ picks which value to keep. It can be undone.
Dialog {
    id: root
    modal: true
    anchors.centerIn: Overlay.overlay
    width: 620
    title: songs.length > 1 ? qsTr("Combine %1 Songs").arg(songs.length) : qsTr("Combine")

    property var songs: []
    property var recordings: []
    property var preview: null
    property bool sameAudio: false
    /// Field → the song or audio whose value is kept.
    property var picks: ({})

    function show(songIds, recordingIds) {
        songs = songIds;
        recordings = recordingIds;
        preview = null;
        picks = {};
        open();
        Backend.call("combine_preview", { songs: songIds, recordings: recordingIds }).then(p => {
            preview = p;
            sameAudio = p.verdict === "same" || songIds.length < 2;
        }, e => { Session.toast(String(e)); close(); });
    }

    readonly property var fields: preview ? preview.songFields.concat(sameAudio && recordings.length > 1 ? preview.audioFields : []) : []
    function pick(f) { return picks[f.key] || f.values[0].id; }

    footer: DialogButtonBox {
        Button { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        Button {
            text: qsTr("Combine")
            highlighted: true
            enabled: !!root.preview && (root.songs.length > 1 || root.sameAudio)
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
    }
    onAccepted: {
        const chosen = {};
        for (const f of fields) chosen[f.key] = pick(f);
        const what = sameAudio ? qsTr("Combined as the same audio") : qsTr("Combined %1 songs").arg(songs.length);
        const kept = songs[0];
        const inspecting = songs.includes(Session.inspectTrack);
        Session.undoable(what, () => Session.run("combine", { songs: songs, recordings: recordings, sameAudio: sameAudio, picks: chosen }))
            .then(() => { if (inspecting && kept) Session.inspectTrack = kept; });
    }

    ColumnLayout {
        width: parent.width
        spacing: Theme.spacing

        Label { visible: !root.preview; text: qsTr("Comparing their audio…"); opacity: 0.7 }
        Label {
            visible: !!root.preview && root.recordings.length > 1
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: !root.preview ? "" : root.preview.verdict === "same"
                ? qsTr("Their audio is the same (%1% match): %2.").arg(Math.round(root.preview.coverage * 100)).arg(root.preview.sources.join(", "))
                : root.preview.verdict === "different"
                    ? qsTr("Their audio is different") + (root.preview.coverage >= 0.95 ? qsTr(" (it lines up all through, but %1% of the detail differs: a remaster, or a lossy copy?)").arg(Math.round(root.preview.error * 100))
                        : root.preview.coverage > 0.05 ? qsTr(" (only %1% of it lines up: an edit?)").arg(Math.round(root.preview.coverage * 100)) : "") + "."
                    : qsTr("Not all of it is analysed yet, so Orpheus can't compare it.")
            color: root.preview && root.preview.verdict === "same" ? Theme.ok : palette.text
        }
        ColumnLayout {
            visible: !!root.preview && root.recordings.length > 1
            Layout.fillWidth: true
            RadioButton {
                Layout.fillWidth: true
                enabled: root.songs.length > 1
                checked: !root.sameAudio
                onToggled: if (checked) root.sameAudio = false
                text: qsTr("Different audio — one song, each version keeping its own audio (e.g. several edits for one line dance)")
                    + (root.songs.length < 2 ? qsTr(" (they're already one song)") : "")
                contentItem: Label { text: parent.text; wrapMode: Text.WordWrap; leftPadding: parent.indicator.width + parent.spacing }
            }
            RadioButton {
                Layout.fillWidth: true
                checked: root.sameAudio
                onToggled: if (checked) root.sameAudio = true
                text: qsTr("The same audio — one song and one audio (e.g. a FLAC and an MP3, or a file and its Spotify track): analysed once, one BPM, one set of hot cues; versions that become identical merge")
                contentItem: Label { text: parent.text; wrapMode: Text.WordWrap; leftPadding: parent.indicator.width + parent.spacing }
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            font.pixelSize: 11
            opacity: 0.7
            text: (root.songs.length > 1 ? qsTr("Their versions become versions of one song, and count as the same song in a set. ") : "")
                + qsTr("Setlists and crates keep working. You can undo it right after, or later with “Separate into Its Own Song” or Split off… (in a version's Audio).")
        }
        Label { visible: root.fields.length > 0; text: qsTr("They differ in these; choose what to keep:") }
        Label { visible: !!root.preview && !root.fields.length; text: qsTr("Nothing differs between them, so nothing is lost."); opacity: 0.6 }
        Repeater {
            model: root.fields
            delegate: RowLayout {
                id: field
                required property var modelData
                Layout.fillWidth: true
                Label { Layout.preferredWidth: 90; Layout.alignment: Qt.AlignTop; text: field.modelData.label }
                ColumnLayout {
                    Layout.fillWidth: true
                    Repeater {
                        model: field.modelData.values
                        delegate: RadioButton {
                            required property var modelData
                            checked: root.pick(field.modelData) === modelData.id
                            text: (modelData.text || qsTr("(empty)")) + (modelData.of !== modelData.text ? "  — " + modelData.of : "")
                            onToggled: if (checked) { const p = Object.assign({}, root.picks); p[field.modelData.key] = modelData.id; root.picks = p; }
                        }
                    }
                }
            }
        }
    }
}
