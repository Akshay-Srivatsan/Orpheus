import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Play Next, the pause between songs, what's coming up, the crossfader, Skip
// and auto mode. Songs never overlap. Between the decks it's a column;
// `compact` (below the decks, when the window is narrow) it's two rows.
Frame {
    id: root

    property bool compact: false

    readonly property var p: Session.perform
    /// Something is audible in the room.
    readonly property bool playing: Session.decks.some(d => d && d.audible)
    readonly property var next: Session.set && p ? Session.set.rows.find(r => r.entry.id === p.nextEntryId) || null : null
    /// The next entry's custom label (not just its styles' short names).
    readonly property string nextLabel: next && !next.isPause && next.label && !next.styles.some(s => s.short === next.label) ? next.label : ""
    readonly property var nextDeck: p ? Session.decks.find(d => d && d.entryId && d.entryId === p.nextEntryId) || null : null
    readonly property real crossfader: p ? p.crossfader : 0.5
    readonly property bool inGap: !!p && !!p.gapStartedAt && !playing
    property real now: Date.now()
    readonly property real gapMs: inGap ? now - p.gapStartedAt : 0
    /// Expected pause: the general pause, or with announcements both pauses and the speech.
    readonly property real gapTarget: p && p.autoAdvanceAt && p.gapStartedAt ? Math.max(1000, p.autoAdvanceAt - p.gapStartedAt) : (p ? p.gapSeconds : 8) * 1000
    readonly property var autoIn: p && p.autoAdvanceAt && inGap ? Math.max(0, p.autoAdvanceAt - now) : null

    Timer { interval: 200; running: root.inGap; repeat: true; onTriggered: root.now = Date.now() }

    function playNext() {
        Session.run("perform_play_next").then(r => {
            if (r === "armed") Session.toast(qsTr("A song is playing — press Play Next again within 3 s to fade it out and continue."), "info");
        });
    }
    function setCrossfader(x) { Session.run("levels_set", { master: null, cue: null, crossfader: x }); }

    GridLayout {
        anchors.fill: parent
        columns: root.compact ? 3 : 1
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        Button {
            id: playNext
            Layout.row: 0; Layout.column: 0
            Layout.fillWidth: true
            Layout.minimumWidth: implicitWidth
            Layout.preferredWidth: root.compact ? 200 : -1
            readonly property bool armed: !!root.p && root.p.armed
            readonly property bool pending: !!root.p && root.p.pendingNext
            // The default action when nothing plays; orange and blinking while armed
            // (press again to fade the song out); plain while a song plays or it waits.
            highlighted: !root.playing && !pending && !armed
            palette.button: armed ? Theme.deckColors[1] : undefined
            palette.buttonText: armed ? "black" : undefined
            opacity: pending ? Theme.dim : 1
            SequentialAnimation on opacity {
                running: playNext.armed
                loops: Animation.Infinite
                NumberAnimation { to: 0.6; duration: 300 }
                NumberAnimation { to: 1; duration: 300 }
                onStopped: playNext.opacity = Qt.binding(() => playNext.pending ? Theme.dim : 1)
            }
            icon.source: Theme.icon("next")
            font.bold: true
            enabled: !!root.next && (!!root.next.trackId || root.next.isPause || root.playing)
            text: !root.p ? qsTr("Play Next") : root.p.pendingNext ? qsTr("Starting after the fade…")
                : root.playing ? (root.p.armed ? qsTr("Press Again") : qsTr("Fade && Play Next")) : qsTr("Play Next")
            ToolTip.visible: hovered
            ToolTip.text: root.p && root.p.armed ? qsTr("Press again within 3 s to fade out the song and play the next")
                : qsTr("Play the next song on the free deck (Ctrl+Enter). A playing song is faded out first.")
            onClicked: root.playNext()
        }

        // Cancelling a pending start, or centering the crossfader for auto mode.
        RowLayout {
            Layout.row: 1; Layout.column: 0
            visible: (!!root.p && root.p.pendingNext) || (root.crossfader !== 0.5 && !!root.p && root.p.autoAdvance)
            Button {
                visible: !!root.p && root.p.pendingNext
                text: qsTr("Cancel")
                onClicked: Session.run("perform_cancel_pending")
            }
            Button {
                visible: root.crossfader !== 0.5 && !!root.p && root.p.autoAdvance
                text: qsTr("Center the Crossfader")
                icon.source: Theme.icon("warning")
                icon.color: Theme.warn
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Auto mode needs the crossfader centered")
                onClicked: root.setCrossfader(0.5)
            }
        }

        // The pause, and what's next.
        ColumnLayout {
            Layout.row: root.compact ? 0 : 2
            Layout.column: root.compact ? 1 : 0
            Layout.rowSpan: root.compact ? 2 : 1
            Layout.fillWidth: true
            Layout.fillHeight: !root.compact
            Layout.alignment: Qt.AlignTop
            spacing: 2

            RowLayout {
                visible: root.inGap
                Label { text: Session.fmtTime(root.gapMs); font.bold: true; color: root.gapMs > root.gapTarget ? Theme.warn : Theme.system.text; font.features: { "tnum": 1 } }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: root.p && root.p.autoPhase === "held" ? Theme.warn : root.autoIn !== null || (root.p && root.p.autoPhase === "announcing") ? Theme.ok : Theme.system.text
                    text: !root.p ? "" : root.p.autoPhase === "announcing" ? qsTr("announcing…")
                        : root.p.autoPhase === "held" ? qsTr("waits for Play Next")
                        : root.autoIn !== null ? qsTr("next song in %1 s").arg(Math.ceil(root.autoIn / 1000))
                        : qsTr("pause · target %1 s").arg(Math.round(root.gapTarget / 1000))
                }
            }
            ProgressBar { visible: root.inGap; Layout.fillWidth: true; from: 0; to: 1; value: Math.min(1, root.gapMs / root.gapTarget) }
            Label {
                visible: root.playing && !root.inGap && !root.compact
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                opacity: Theme.dim
                text: !root.p ? "" : root.p.autoAdvance ? (root.p.autoPhase === "held" ? qsTr("Auto mode stops before the next song.") : qsTr("Auto mode starts the next song after the pause."))
                    : qsTr("The next song starts when you press Play Next.")
            }

            Label { text: qsTr("Up next"); opacity: Theme.dim }
            RowLayout {
                Layout.fillWidth: true
                visible: !!root.next
                // Its dance: the custom label (e.g. "Mixer"), else its styles.
                Label {
                    visible: !!root.next && !root.next.isPause
                    textFormat: Text.StyledText
                    font.bold: true
                    text: !root.next || root.next.isPause ? "" : root.nextLabel
                        ? "<font color='" + (root.next.styles.length ? Theme.readable(root.next.styles[0].color) : Theme.system.text) + "'>" + root.nextLabel + "</font>"
                        : root.next.styles.slice(0, 2).map(s => "<font color='" + Theme.readable(s.color) + "'>" + s.short + "</font>").join(" ")
                }
                Icon { visible: !!root.next && root.next.isPause; glyph: "pause"; sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: !root.next ? "" : root.next.trackId || root.next.isPause ? root.next.title : qsTr("Pick a song for this slot")
                    color: root.next && !root.next.trackId && !root.next.isPause ? Theme.warn : Theme.system.text
                    ToolTip.visible: nextHover.hovered && truncated
                    ToolTip.text: text
                    HoverHandler { id: nextHover }
                }
            }
            Label {
                visible: !!root.next
                Layout.fillWidth: true
                elide: Text.ElideRight
                opacity: Theme.dim
                text: !root.next ? "" : root.next.isPause
                    ? qsTr("%1 s").arg(Math.round(root.next.durationMs / 1000)) + (root.next.entry.announcement ? " · “" + root.next.entry.announcement + "”" : "")
                        + (root.next.entry.hold ? " · " + qsTr("stops here") : "")
                    : [root.next.artist, root.next.bpm ? Session.fmtBpm(root.next.bpm) + " BPM" : "", root.nextDeck ? qsTr("deck %1").arg(root.nextDeck.deck === 0 ? "A" : "B") : ""].filter(x => x).join(" · ")
            }
            Label {
                visible: !root.next
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                opacity: Theme.dim
                text: Session.set ? qsTr("End of setlist") : qsTr("No setlist open — add songs to start a live set")
            }
        }

        // Crossfader: which deck the room hears. It snaps to the center; it
        // flashes when an action is refused because of where it is.
        RowLayout {
            Layout.row: root.compact ? 1 : 3
            Layout.column: root.compact ? 2 : 0
            Layout.fillWidth: true
            ToolButton { text: "A"; font.bold: root.crossfader < 0.5; onClicked: root.setCrossfader(0) }
            Slider {
                id: fader
                Layout.fillWidth: true
                Layout.minimumWidth: 80
                from: 0; to: 1; stepSize: 0.01
                value: root.crossfader
                onMoved: root.setCrossfader(value)
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Crossfader: which deck the room hears (double-click: center)")
                TapHandler { onDoubleTapped: root.setCrossfader(0.5) }
                palette.highlight: flashing.running ? Theme.error : undefined
                Timer { id: flashing; interval: 900 }
                Connections { target: Session; function onCrossfaderBlocked() { flashing.restart(); } }
            }
            ToolButton { text: "B"; font.bold: root.crossfader > 0.5; onClicked: root.setCrossfader(1) }
        }

        RowLayout {
            Layout.row: root.compact ? 0 : 4
            Layout.column: root.compact ? 2 : 0
            Layout.fillWidth: true
            Button {
                text: qsTr("Skip")
                icon.source: Theme.icon("skip")
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
                icon.source: Theme.icon("auto")
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
