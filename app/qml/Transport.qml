import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Between the decks: Play Next, the pause between songs, what's coming up,
// the crossfader, Skip and auto mode. Songs never overlap.
Pane {
    id: root

    readonly property var p: Session.perform
    /// Something is audible in the room.
    readonly property bool playing: Session.decks.some(d => d && d.audible)
    readonly property var next: Session.set && p ? Session.set.rows.find(r => r.entry.id === p.nextEntryId) || null : null
    readonly property var nextDeck: p ? Session.decks.find(d => d && d.entryId && d.entryId === p.nextEntryId) || null : null
    readonly property real crossfader: p ? p.crossfader : 0.5
    readonly property bool inGap: !!p && !!p.gapStartedAt && !playing
    property real now: Date.now()
    readonly property real gapMs: inGap ? now - p.gapStartedAt : 0
    /// Expected pause: the general pause, or with announcements both pauses and the speech.
    readonly property real gapTarget: p && p.autoAdvanceAt && p.gapStartedAt ? Math.max(1000, p.autoAdvanceAt - p.gapStartedAt) : (p ? p.gapSeconds : 8) * 1000
    readonly property var autoIn: p && p.autoAdvanceAt && inGap ? Math.max(0, p.autoAdvanceAt - now) : null

    padding: Theme.spacing

    Timer { interval: 200; running: root.inGap; repeat: true; onTriggered: root.now = Date.now() }

    function playNext() {
        Session.run("perform_play_next").then(r => {
            if (r === "armed") Session.toast(qsTr("A song is playing — press Play Next again within 3 s to fade it out and continue."), "info");
        });
    }
    function setCrossfader(x) { Session.run("levels_set", { master: null, cue: null, crossfader: x }); }

    background: Rectangle { color: Theme.system.base; radius: 6; border.color: Theme.system.mid }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        Button {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            highlighted: true
            font.bold: true
            enabled: !!root.next && (!!root.next.trackId || root.next.isPause || root.playing)
            text: !root.p ? qsTr("Play Next") : root.p.pendingNext ? qsTr("Starting after fade…")
                : root.playing ? (root.p.armed ? qsTr("Press again to fade & play") : qsTr("Fade & Play Next")) : qsTr("Play Next")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Play the next song on the free deck (Ctrl+Enter). A playing song is faded out first.")
            onClicked: root.playNext()
        }
        Button {
            visible: !!root.p && root.p.pendingNext
            flat: true
            text: qsTr("Cancel")
            onClicked: Session.run("perform_cancel_pending")
        }

        // The pause between songs.
        ColumnLayout {
            visible: root.inGap
            Layout.fillWidth: true
            spacing: 2
            RowLayout {
                Label { text: Session.fmtTime(root.gapMs); font.bold: true; font.pixelSize: 16; color: root.gapMs > root.gapTarget ? Theme.warn : Theme.system.text; font.features: { "tnum": 1 } }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    opacity: Theme.dim
                    text: !root.p ? "" : root.p.autoPhase === "announcing" ? qsTr("announcing…")
                        : root.p.autoPhase === "held" ? qsTr("waits for Play Next")
                        : root.autoIn !== null ? qsTr("next song in %1s").arg(Math.ceil(root.autoIn / 1000))
                        : qsTr("pause · target %1s").arg(Math.round(root.gapTarget / 1000))
                }
            }
            ProgressBar { Layout.fillWidth: true; from: 0; to: 1; value: Math.min(1, root.gapMs / root.gapTarget) }
        }
        Label {
            visible: root.playing && !root.inGap
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            opacity: Theme.dim
            text: !root.p ? "" : root.p.autoAdvance ? (root.p.autoPhase === "held" ? qsTr("Auto mode stops before the next song.") : qsTr("Auto mode starts the next song after the pause."))
                : qsTr("Next song starts only when you press Play Next.")
        }

        // Up next.
        Heading { text: qsTr("Up Next") }
        RowLayout {
            Layout.fillWidth: true
            visible: !!root.next
            Repeater {
                model: root.next && !root.next.isPause ? root.next.styles.slice(0, 2) : []
                delegate: StyleBadge { required property var modelData; style: modelData }
            }
            Icon { visible: !!root.next && root.next.isPause; glyph: "pause"; sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.bold: true
                text: !root.next ? "" : root.next.trackId || root.next.isPause ? root.next.title : qsTr("Pick a song for this slot")
            }
        }
        Label {
            visible: !!root.next
            Layout.fillWidth: true
            elide: Text.ElideRight
            opacity: Theme.dim
            text: !root.next ? "" : root.next.isPause
                ? qsTr("%1 s").arg(Math.round(root.next.durationMs / 1000)) + (root.next.entry.announcement ? " · “" + root.next.entry.announcement + "”" : "")
                : [root.next.artist, root.next.bpm ? Session.fmtBpm(root.next.bpm) + " BPM" : "", root.nextDeck ? qsTr("deck %1").arg(root.nextDeck.deck === 0 ? "A" : "B") : ""].filter(x => x).join(" · ")
        }
        Label {
            visible: !root.next
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            opacity: Theme.dim
            text: Session.set ? qsTr("End of setlist") : qsTr("No setlist open — add songs to start a live set")
        }

        Item { Layout.fillHeight: true }

        // Crossfader: which deck the room hears. It snaps to the center.
        RowLayout {
            Layout.fillWidth: true
            ToolButton { text: "A"; font.bold: root.crossfader < 0.5; onClicked: root.setCrossfader(0) }
            Slider {
                Layout.fillWidth: true
                from: 0; to: 1; stepSize: 0.01
                value: root.crossfader
                onMoved: root.setCrossfader(value)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Crossfader: which deck the room hears (double-click: center)")
                TapHandler { onDoubleTapped: root.setCrossfader(0.5) }
            }
            ToolButton { text: "B"; font.bold: root.crossfader > 0.5; onClicked: root.setCrossfader(1) }
        }
        Button {
            visible: root.crossfader !== 0.5 && !!root.p && root.p.autoAdvance
            flat: true
            text: qsTr("Center the Crossfader")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Auto mode needs the crossfader centered")
            onClicked: root.setCrossfader(0.5)
        }

        RowLayout {
            Layout.fillWidth: true
            Button {
                text: qsTr("Skip")
                enabled: !!root.next
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skip the up-next song (it stays in the set, greyed out)")
                onClicked: Session.run("perform_skip_next")
            }
            Button {
                visible: !!Session.tts && Session.tts.state === "ready"
                icon.source: Theme.icon("mic")
                enabled: !!root.next
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Announce the next dance now")
                onClicked: Session.run("announce_next")
            }
            Item { Layout.fillWidth: true }
            Button {
                text: qsTr("Auto")
                checkable: true
                checked: !!root.p && root.p.autoAdvance
                ToolTip.visible: hovered
                ToolTip.text: Session.settings && Session.settings.announce.enabled
                    ? qsTr("Auto mode: after a song, pause, announce the next dance, pause, and start it")
                    : qsTr("Auto mode: start the next song after the pause")
                onClicked: Session.run("perform_set_auto_advance", { value: checked })
            }
        }
    }
}
