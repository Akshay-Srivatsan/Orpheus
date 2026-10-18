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
    /// Its audio's details are shown (folded at first).
    property bool audioOpen: false
    /// Just its name (the Info panel keeps which are folded).
    property bool folded: false
    /// Its place among the song's versions (to move it up or down).
    property int position: 0
    signal toggleFold()
    signal moveBy(int by)
    readonly property var v: view.version
    readonly property var a: view.audio
    /// The deck holding this version, if any (its position sets start/end/fade).
    readonly property bool onDeck: !!Session.deckVersions[v.id]
    /// Where that deck is now (read when asked: it moves many times a second).
    function deckMs() { return Math.round(Session.decks[Session.deckVersions[v.id].deck].positionMs); }

    /// Something is said before it (in a set that doesn't say otherwise).
    readonly property bool speaks: v.announcement === undefined || v.announcement === null ? view.inherited.speaks : !!v.announcement.trim()

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
                icon.color: palette.buttonText
                icon.source: Theme.icon(root.folded ? "chevron-right" : "chevron-down")
                icon.width: 12; icon.height: 12
                ToolTip.visible: hovered
                ToolTip.text: root.folded ? qsTr("Show this version's settings") : qsTr("Fold this version")
                onClicked: root.toggleFold()
            }
            ToolButton {
                icon.source: Theme.icon(root.view.isPrimary ? "star-filled" : "star")
                icon.color: root.view.isPrimary ? Theme.warn : Theme.system.buttonText
                enabled: (!root.view.isPrimary) && (root.song.versions.length > 1)
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
                icon.color: palette.buttonText
                icon.source: Theme.icon("up")
                enabled: (root.position > 0) && (root.song.versions.length > 1)
                ToolTip.visible: hovered; ToolTip.text: qsTr("Move up")
                onClicked: root.moveBy(-1)
            }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("down")
                enabled: (root.position < root.song.versions.length - 1) && (root.song.versions.length > 1)
                ToolTip.visible: hovered; ToolTip.text: qsTr("Move down")
                onClicked: root.moveBy(1)
            }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("headphones")
                ToolTip.visible: hovered; ToolTip.text: qsTr("Preview this version on the free deck")
                onClicked: Session.preview(root.v.id, null)
            }
            // Hidden from the library's lists: say so even when folded.
            Icon {
                visible: !!root.v.unlisted
                glyph: "hidden"
                opacity: Theme.dim
                ToolTip.visible: hiddenHover.hovered
                ToolTip.text: qsTr("Hidden from the library's lists (View → Show Hidden Versions lists it)")
                HoverHandler { id: hiddenHover }
            }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("dots")
                ToolTip.visible: hovered; ToolTip.text: qsTr("More for this version")
                onClicked: moreMenu.popup()
                FitMenu {
                    id: moreMenu
                    MenuItem {
                        text: qsTr("Duplicate")
                        onTriggered: {
                            const copy = Object.assign({}, root.v);
                            delete copy.id;
                            copy.name = qsTr("%1 (copy)").arg(root.v.name);
                            Session.run("version_create", { trackId: root.song.track.id, version: copy });
                        }
                    }
                    MenuItem {
                        text: qsTr("Hidden from the Library")
                        checkable: true
                        checked: !!root.v.unlisted
                        onTriggered: root.patch({ unlisted: checked })
                    }
                    MenuSeparator {}
                    MenuItem {
                        enabled: !root.v.hidden
                        text: qsTr("Remove from the Library…")
                        onTriggered: {
                            const last = root.song.versions.filter(x => !x.version.hidden).length < 2;
                            Session.confirm(last ? qsTr("Remove “%1” from the library? Sets that play it keep it.").arg(root.song.track.title)
                                : qsTr("Remove version “%1” from the library? Sets that play it keep it.").arg(root.v.name),
                                qsTr("Remove"), ok => { if (ok) Session.run("version_delete", { id: root.v.id }); });
                        }
                    }
                }
            }
        }

        // Folded: what it holds, in one line (its styles, tempo, length).
        Label {
            visible: root.folded
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing * 3
            elide: Text.ElideRight
            opacity: Theme.dim
            font.pointSize: Theme.smallPointSize
            text: [root.view.shownNames || qsTr("no style"),
                root.view.playback.target || root.view.playback.base ? Session.fmtBpm(root.view.playback.target || root.view.playback.base) + " BPM" : "",
                root.a ? Session.fmtTime(root.a.recording.durationMs) : "",
                root.v.unlisted ? qsTr("hidden from the library") : ""].filter(x => x).join(" · ")
        }

        // Everything below its name (folded away: the line above says what).
        ColumnLayout {
            visible: !root.folded
            Layout.fillWidth: true
            spacing: 4
            RowLayout {
                visible: !!root.v.hidden
                Label { text: qsTr("Not in the library: only the sets that play it keep it."); color: Theme.warn; wrapMode: Text.WordWrap; Layout.fillWidth: true }
                Button { text: qsTr("Show in Library"); onClicked: root.patch({ hidden: false }) }
            }

            // The audio it plays.
            RowLayout {
                visible: !!root.a
                Layout.fillWidth: true
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon(root.audioOpen ? "chevron-down" : "chevron-right")
                    icon.width: 12; icon.height: 12
                    text: qsTr("Audio details")
                    ToolTip.visible: hovered
                    ToolTip.text: root.audioOpen ? qsTr("Fold the audio's details") : qsTr("Show the audio's details: BPM, meter, gain, files, sources…")
                    onClicked: root.audioOpen = !root.audioOpen
                }
                ComboBox {
                    visible: root.song.recordings.length > 1
                    Layout.fillWidth: true
                    model: root.song.recordings.map(r => r.label)
                    id: audioBox
                    // The list as wide as its longest label (the box itself fits the card).
                    Layout.preferredWidth: 100
                    FontMetrics { id: audioFont; font: audioBox.font }
                    popup.width: Math.max(width, Math.max(...root.song.recordings.map(r => audioFont.advanceWidth(r.label))) + 4 * Theme.spacing)
                    // Wider than the box: it grows to the left (the card is at the window's right).
                    popup.x: width - popup.width
                    currentIndex: root.song.recordings.findIndex(r => r.id === root.v.recording)
                    onActivated: index => root.patch({ recording: root.song.recordings[index].id })
                }
                Label { visible: root.song.recordings.length <= 1; Layout.fillWidth: true; elide: Text.ElideRight; text: root.a ? root.a.recording.title : "" }
                // (The list's labels already say how long, and how fast, each is.)
                Label { visible: root.song.recordings.length <= 1; opacity: Theme.dim; text: root.a ? Session.fmtTime(root.a.recording.durationMs) + (root.a.recording.bpm ? " · " + Session.fmtBpm(root.a.recording.bpm) + " BPM" : "") : "" }
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
            // The audio's details (what "Audio details" folds).
            Frame {
                visible: root.audioOpen && !!root.a
                Layout.fillWidth: true
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        visible: root.view.sharedWith.length > 0 || root.view.otherSongs.length > 0
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
                        Layout.fillWidth: true
                        audio: root.a ? root.a : { recording: { sources: [], files: [], filePaths: {} }, chips: [], bpmFrom: "", clubSuggests: [], stemsReady: false }
                        versions: root.song.versions.filter(x => x.version.recording === root.v.recording).map(x => x.version)
                    }
                }
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Theme.spacing
                rowSpacing: 4

                FormLabel { text: qsTr("Styles") }
                StylePicker { Layout.fillWidth: true; value: root.v.styles || []; onEdited: keys => root.patch({ styles: keys }) }
                FormLabel { text: qsTr("Shown as"); ToolTip.visible: shownHover.hovered; ToolTip.text: qsTr("Shown to dancers instead of the style names, e.g. the name of a line dance"); HoverHandler { id: shownHover } }
                TextField {
                    Layout.fillWidth: true
                    text: root.v.displayName || ""
                    placeholderText: root.view.shownNames || qsTr("style names")
                    onEditingFinished: if (text.trim() !== (root.v.displayName || "")) root.patch({ displayName: text.trim() || null })
                }
                FormLabel { text: qsTr("Tags") }
                StylePicker { Layout.fillWidth: true; tags: true; value: root.v.tags || []; onEdited: keys => root.patch({ tags: keys }) }
                FormLabel { text: qsTr("Tempo") }
                RowLayout {
                    NumberField {
                        implicitWidth: 80
                        minimum: 0
                        value: root.v.targetBpm ? root.v.targetBpm : null
                        placeholderText: root.view.playback.base ? Session.fmtBpm(root.view.playback.base) : qsTr("orig.")
                        onCommitted: v => root.patch({ targetBpm: v })
                    }
                    Label { text: "BPM" }
                    Label {
                        visible: !!root.v.targetBpm && !!root.view.playback.base
                        opacity: 0.6
                        text: ((root.view.playback.rate - 1) * 100).toFixed(1) + "%"
                    }
                    Button {
                        implicitWidth: implicitContentWidth + leftPadding + rightPadding
                        enabled: root.view.canFit
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
                    Button { implicitWidth: implicitContentWidth + leftPadding + rightPadding; icon.color: palette.buttonText; enabled: root.onDeck; icon.source: Theme.icon("to-start"); ToolTip.visible: hovered; ToolTip.text: qsTr("Set start to the deck position"); onClicked: root.patch({ cueInMs: root.deckMs() }) }
                    Button { implicitWidth: implicitContentWidth + leftPadding + rightPadding; icon.color: palette.buttonText; enabled: root.onDeck; icon.source: Theme.icon("to-end"); ToolTip.visible: hovered; ToolTip.text: qsTr("Set end to the deck position"); onClicked: root.patch({ cueOutMs: root.deckMs() }) }
                }
                FormLabel { text: qsTr("Fade at") }
                RowLayout {
                    TimeField { value: root.v.fadeOutMs; placeholderText: qsTr("none"); onEdited: ms => root.patch({ fadeOutMs: ms }) }
                    Button { implicitWidth: implicitContentWidth + leftPadding + rightPadding; enabled: root.onDeck; text: qsTr("here"); onClicked: root.patch({ fadeOutMs: root.deckMs() }) }
                }
                FormLabel { text: qsTr("Gain") }
                RowLayout {
                    NumberField {
                        implicitWidth: 80
                        value: root.v.gainDb != null ? root.v.gainDb : null
                        placeholderText: qsTr("recording")
                        onCommitted: v => root.patch({ gainDb: v })
                    }
                    Label { text: "dB" }
                }
                FormLabel { text: qsTr("Announce") }
                AnnouncementField {
                    Layout.fillWidth: true
                    value: root.v.announcement === undefined ? null : root.v.announcement
                    defaultLabel: Session.announcementDefault(root.view.inherited.announcement)
                    onEdited: value => root.patch({ announcement: value })
                }
                FormLabel { visible: root.speaks; text: qsTr("Chime") }
                ChimeField {
                    visible: root.speaks
                    Layout.fillWidth: true
                    value: root.v.chime || ({})
                    inheritedOn: root.view.inherited.chime
                    inheritedSound: root.view.inherited.chimeSound || null
                    versions: root.song.versions.filter(x => x.version.id !== root.v.id).map(x => ({ id: x.version.id, label: x.version.name }))
                    title: qsTr("the version “%1”").arg(root.v.name)
                    onEdited: value => root.patch({ chime: value })
                }
                FormLabel { visible: root.speaks; text: qsTr("Pauses") }
                RowLayout {
                    visible: root.speaks
                    SecondsField {
                        value: root.v.announcePauseBefore === undefined ? null : root.v.announcePauseBefore
                        placeholder: root.view.inherited.pauseBefore
                        onEdited: v => root.patch({ announcePauseBefore: v })
                    }
                    Label { text: qsTr("announcement,") }
                    SecondsField {
                        value: root.v.announcePauseAfter === undefined ? null : root.v.announcePauseAfter
                        placeholder: root.view.inherited.pauseAfter
                        onEdited: v => root.patch({ announcePauseAfter: v })
                    }
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
                    text: qsTr("Club-file entry")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("To suggest this version in a club's file: the song's title with this recording's fingerprint, length and sources")
                    onClicked: Session.run("club_song_text", { versionId: root.v.id }).then(t => Session.copy(t, qsTr("Song entry")))
                }
                Button {
                    enabled: root.song.versions.length > 1
                    text: qsTr("Separate into Its Own Song")
                    onClicked: Session.undoable(qsTr("Separated into its own song"), () => Session.run("versions_separate", { versionIds: [root.v.id] }))
                }
            }
        }
    }
}
