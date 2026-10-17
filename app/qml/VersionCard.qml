import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// One version of a song in the Info panel (`view` from `song_view`): its name,
// the audio it plays, its styles, and how it plays (tempo, start/end, fade).
Frame {
    id: root

    property var view
    property var song
    property bool highlighted: false
    property bool audioOpen: (song ? song.versions.length : 0) === 1
    readonly property var v: view.version
    readonly property var a: view.audio
    /// The deck holding this version, if any (its position sets start/end/fade).
    readonly property var deck: Session.decks.find(d => d && d.versionId === v.id && d.status !== "empty") || null

    function patch(p) { return Session.run("version_update", { id: v.id, patch: p }); }

    // The selected one (or the set entry's) stands out.
    background: Rectangle {
        radius: 3
        color: root.highlighted ? Qt.alpha(Theme.system.highlight, 0.08) : "transparent"
        border.color: root.highlighted ? Theme.system.highlight : Theme.system.mid
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 4

        RowLayout {
            Layout.fillWidth: true
            ToolButton {
                visible: root.song.versions.length > 1
                icon.source: Theme.icon(root.view.isPrimary ? "star-filled" : "star")
                icon.color: root.view.isPrimary ? Theme.warn : Theme.system.buttonText
                enabled: !root.view.isPrimary
                ToolTip.visible: hovered
                ToolTip.text: root.view.isPrimary ? qsTr("The primary version: it stands for the song in the library") : qsTr("Make this the primary version")
                onClicked: Session.run("track_update", { id: root.song.track.id, patch: { primary: root.v.id } })
            }
            TextField {
                Layout.fillWidth: true
                text: root.v.name
                onEditingFinished: if (text !== root.v.name) root.patch({ name: text })
            }
            ToolButton {
                icon.source: Theme.icon("headphones")
                ToolTip.visible: hovered; ToolTip.text: qsTr("Preview this version on the free deck")
                onClicked: Session.preview(root.v.id, null)
            }
            ToolButton {
                icon.source: Theme.icon("copy")
                ToolTip.visible: hovered; ToolTip.text: qsTr("Duplicate")
                onClicked: {
                    const copy = Object.assign({}, root.v);
                    delete copy.id;
                    copy.name = qsTr("%1 (copy)").arg(root.v.name);
                    Session.run("version_create", { trackId: root.song.track.id, version: copy });
                }
            }
            ToolButton {
                visible: !root.v.hidden
                icon.source: Theme.icon("trash")
                ToolTip.visible: hovered; ToolTip.text: qsTr("Remove this version from the library (sets that play it keep it)")
                onClicked: {
                    const last = root.song.versions.filter(x => !x.version.hidden).length < 2;
                    Session.confirm(last ? qsTr("Remove “%1” from the library? Sets that play it keep it.").arg(root.song.track.title)
                        : qsTr("Remove version “%1” from the library? Sets that play it keep it.").arg(root.v.name),
                        qsTr("Remove"), ok => { if (ok) Session.run("version_delete", { id: root.v.id }); });
                }
            }
        }
        RowLayout {
            visible: !!root.v.hidden
            Label { text: qsTr("Not in the library: only the sets that play it keep it."); color: Theme.warn; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Button { flat: true; text: qsTr("Show in Library"); onClicked: root.patch({ hidden: false }) }
        }

        // The audio it plays.
        RowLayout {
            visible: !!root.a
            Layout.fillWidth: true
            ToolButton {
                icon.source: Theme.icon(root.audioOpen ? "chevron-down" : "chevron-right")
                icon.width: 12; icon.height: 12
                text: qsTr("Audio")
                ToolTip.visible: hovered
                ToolTip.text: root.audioOpen ? qsTr("Hide the audio's details") : qsTr("Show the audio's details: BPM, meter, files…")
                onClicked: root.audioOpen = !root.audioOpen
            }
            ComboBox {
                visible: root.song.recordings.length > 1
                Layout.fillWidth: true
                model: root.song.recordings.map(r => r.label)
                currentIndex: root.song.recordings.findIndex(r => r.id === root.v.recording)
                onActivated: index => root.patch({ recording: root.song.recordings[index].id })
            }
            Label { visible: root.song.recordings.length <= 1; Layout.fillWidth: true; elide: Text.ElideRight; text: root.a ? root.a.recording.title : "" }
            Label { opacity: 0.6; text: root.a ? Session.fmtTime(root.a.recording.durationMs) + (root.a.recording.bpm ? " · " + Session.fmtBpm(root.a.recording.bpm) + " BPM" : "") : "" }
            Repeater {
                model: root.a ? root.a.chips : []
                delegate: Label {
                    required property var modelData
                    text: modelData.id
                    font.pointSize: Theme.smallPointSize
                    padding: 2
                    background: Rectangle { radius: 3; border.color: palette.mid; color: "transparent" }
                    ToolTip.visible: chipHover.hovered
                    ToolTip.text: modelData.label
                    HoverHandler { id: chipHover }
                }
            }
        }
        Label {
            visible: !root.a || !!root.a.recording.removed
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.warn
            text: !root.a ? qsTr("Its audio was removed from the library: add it again in Library → Files (or find it on Spotify again).")
                : qsTr("Removed from the library: it can't play until you add it again in Library → Files (or a file that sounds the same).")
        }
        Label {
            visible: root.audioOpen && (root.view.sharedWith.length > 0 || root.view.otherSongs.length > 0)
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.warn
            font.pointSize: Theme.smallPointSize
            // The other songs are links: they open in Info.
            textFormat: Text.StyledText
            linkColor: palette.link
            text: qsTr("Shared: changes here also apply to %1.").arg(root.view.sharedWith.map(n => "“" + n + "”")
                .concat(root.view.otherSongs.map(s => "<a href=\"" + s.id + "\">" + s.label.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</a>")).join(", "))
            onLinkActivated: id => Session.inspectTrack = id
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
        }
        AudioDetails {
            visible: root.audioOpen && !!root.a
            Layout.fillWidth: true
            audio: root.a ? root.a : { recording: { sources: [], files: [], filePaths: {} }, chips: [], bpmFrom: "", clubSuggests: [], stemsReady: false }
            versions: root.song.versions.filter(x => x.version.recording === root.v.recording).map(x => x.version)
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Theme.spacing
            rowSpacing: 4

            FormLabel { text: qsTr("Styles") }
            ChipEditor { Layout.fillWidth: true; value: root.v.styles || []; onEdited: keys => root.patch({ styles: keys }) }
            FormLabel { text: qsTr("Shown as"); ToolTip.visible: shownHover.hovered; ToolTip.text: qsTr("Shown to dancers instead of the style names, e.g. the name of a line dance"); HoverHandler { id: shownHover } }
            TextField {
                Layout.fillWidth: true
                text: root.v.displayName || ""
                placeholderText: root.view.shownNames || qsTr("style names")
                onEditingFinished: if (text.trim() !== (root.v.displayName || "")) root.patch({ displayName: text.trim() || null })
            }
            FormLabel { text: qsTr("Tags") }
            ChipEditor { Layout.fillWidth: true; tags: true; value: root.v.tags || []; onEdited: keys => root.patch({ tags: keys }) }
            FormLabel { text: qsTr("Tempo") }
            RowLayout {
                TextField {
                    implicitWidth: 80
                    text: root.v.targetBpm ? String(root.v.targetBpm) : ""
                    placeholderText: root.view.playback.base ? Session.fmtBpm(root.view.playback.base) : qsTr("orig.")
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: root.patch({ targetBpm: text ? Number(text) : null })
                }
                Label { text: "BPM" }
                Label {
                    visible: !!root.v.targetBpm && !!root.view.playback.base
                    opacity: 0.6
                    text: ((root.view.playback.rate - 1) * 100).toFixed(1) + "%"
                }
                Button {
                    visible: root.view.canFit
                    flat: true
                    text: qsTr("Fit")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Set the tempo to the nearest edge of the style's sweet range")
                    onClicked: Session.run("version_fit_tempo", { id: root.v.id })
                }
            }
            FormLabel { text: qsTr("Start / end") }
            RowLayout {
                TimeField { value: root.v.cueInMs; placeholderText: qsTr("auto"); onEdited: ms => root.patch({ cueInMs: ms }) }
                TimeField { value: root.v.cueOutMs; placeholderText: qsTr("auto"); onEdited: ms => root.patch({ cueOutMs: ms }) }
                ToolButton { visible: !!root.deck; icon.source: Theme.icon("to-start"); ToolTip.visible: hovered; ToolTip.text: qsTr("Set start to the deck position"); onClicked: root.patch({ cueInMs: Math.round(root.deck.positionMs) }) }
                ToolButton { visible: !!root.deck; icon.source: Theme.icon("to-end"); ToolTip.visible: hovered; ToolTip.text: qsTr("Set end to the deck position"); onClicked: root.patch({ cueOutMs: Math.round(root.deck.positionMs) }) }
            }
            FormLabel { text: qsTr("Fade at") }
            RowLayout {
                TimeField { value: root.v.fadeOutMs; placeholderText: qsTr("none"); onEdited: ms => root.patch({ fadeOutMs: ms }) }
                Button { visible: !!root.deck; flat: true; text: qsTr("here"); onClicked: root.patch({ fadeOutMs: Math.round(root.deck.positionMs) }) }
            }
            FormLabel { text: qsTr("Gain") }
            RowLayout {
                TextField {
                    implicitWidth: 80
                    text: root.v.gainDb != null ? String(root.v.gainDb) : ""
                    placeholderText: qsTr("recording")
                    validator: DoubleValidator {}
                    onEditingFinished: root.patch({ gainDb: text ? Number(text) : null })
                }
                Label { text: "dB" }
            }
        }

        // Stems it starts with (when the audio has them).
        RowLayout {
            visible: !!root.a && root.a.stemsReady
            Label { text: qsTr("Stems") }
            Repeater {
                model: ["vocals", "drums", "bass", "other"]
                delegate: CheckBox {
                    required property string modelData
                    text: modelData
                    checked: !root.v.stems || root.v.stems[modelData] !== false
                    onToggled: {
                        const next = Object.assign({}, root.v.stems || {});
                        if (checked) delete next[modelData]; else next[modelData] = false;
                        root.patch({ stems: Object.keys(next).length ? next : null });
                    }
                }
            }
        }

        TextField {
            Layout.fillWidth: true
            placeholderText: qsTr("Comment (e.g. slowed for teaching)")
            text: root.v.comment || ""
            onEditingFinished: if (text !== (root.v.comment || "")) root.patch({ comment: text })
        }
        RowLayout {
            Button {
                flat: true
                text: qsTr("Club-file entry")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("To suggest this version in a club's file: the song's title with this recording's fingerprint, length and sources")
                onClicked: Session.run("club_song_text", { versionId: root.v.id }).then(t => Session.copy(t, qsTr("Song entry")))
            }
            Button {
                visible: root.song.versions.length > 1
                flat: true
                text: qsTr("Separate into Its Own Song")
                onClicked: Session.undoable(qsTr("Separated into its own song"), () => Session.run("versions_separate", { versionIds: [root.v.id] }))
            }
        }
    }
}
