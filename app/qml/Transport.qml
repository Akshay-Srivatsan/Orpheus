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

    /// The deck the room hears (its song's time left starts the steps while it plays).
    readonly property var liveDeck: p ? Session.decks[p.liveDeck] : null
    /// Auto mode's way to the next song, in order: { kind, from, to, estimated }
    /// (epoch ms), the song playing first (see the engine's `AutoStep`).
    readonly property var steps: {
        if (!p || !p.autoSteps || !p.autoSteps.length) return [];
        if (!p.stepsAfterSong) return p.autoSteps;
        const end = now + (liveDeck && liveDeck.audible ? liveDeck.remainingMs : 0);
        return [{ kind: "song", from: now, to: end, estimated: false }]
            .concat(p.autoSteps.map(s => Object.assign({}, s, { from: s.from + end, to: s.to + end })));
    }
    readonly property real nextAt: steps.length ? steps[steps.length - 1].to : 0
    readonly property bool estimated: steps.some(s => s.estimated)
    /// What will go wrong at the next entry: { text, stops } (stops: auto mode can't go on there).
    readonly property var problems: p && p.nextProblems ? p.nextProblems : []
    readonly property bool stops: problems.some(x => x.stops)
    function stepName(kind) {
        return { song: qsTr("This song"), pause: qsTr("Pause"), chime: qsTr("Chime"), announcement: qsTr("Announcement"), pauseAfter: qsTr("Pause") }[kind] || kind;
    }
    function secs(ms) { return ms < 60000 ? qsTr("%1 s").arg(Math.max(0, Math.ceil(ms / 1000))) : Session.fmtTime(ms); }
    /// A step's time: done, left (the current one), or how long (estimated: about).
    function stepTime(st) {
        if (st.to <= now) return "✓";
        if (st.from <= now) return qsTr("%1 left").arg(secs(st.to - now));
        return (st.estimated ? qsTr("about %1") : "%1").arg(secs(st.to - st.from));
    }

    Timer { interval: 200; running: root.inGap || root.steps.length > 0; repeat: true; onTriggered: root.now = Date.now() }

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
            Layout.minimumWidth: 0
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

            // The pause, without auto mode's steps (Play Next starts the next song,
            // or auto mode waits for it).
            RowLayout {
                visible: root.inGap && !root.steps.length
                Label { text: Session.fmtTime(root.gapMs); font.bold: true; color: root.gapMs > root.gapTarget ? Theme.warn : Theme.system.text; font.features: { "tnum": 1 } }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: root.p && root.p.autoPhase === "held" ? Theme.warn : Theme.system.text
                    text: root.p && root.p.autoPhase === "held" ? qsTr("waits for Play Next") : qsTr("pause · target %1 s").arg(Math.round(root.gapTarget / 1000))
                }
            }
            ProgressBar {
                visible: root.inGap
                Layout.fillWidth: true
                from: 0; to: 1
                value: root.steps.length ? Math.min(1, root.gapMs / Math.max(1, root.nextAt - root.p.gapStartedAt)) : Math.min(1, root.gapMs / root.gapTarget)
            }
            Label {
                visible: root.playing && !root.inGap && !root.compact && !root.steps.length
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                opacity: Theme.dim
                text: !root.p ? "" : root.p.autoAdvance ? (root.p.autoPhase === "held" ? qsTr("Auto mode stops before the next song.") : qsTr("Auto mode starts the next song after the pause."))
                    : qsTr("The next song starts when you press Play Next.")
            }

            // Auto mode's steps to the next song: each with its time (done, left,
            // or how long), then when the next song starts.
            ColumnLayout {
                visible: root.steps.length > 0 && !root.compact
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: root.steps
                    delegate: RowLayout {
                        id: step
                        required property var modelData
                        readonly property bool done: modelData.to <= root.now
                        readonly property bool current: !done && modelData.from <= root.now
                        Layout.fillWidth: true
                        opacity: done ? Theme.dim : 1
                        Label { Layout.fillWidth: true; elide: Text.ElideRight; text: root.stepName(step.modelData.kind); font.bold: step.current }
                        Label { text: root.stepTime(step.modelData); font.bold: step.current; font.features: { "tnum": 1 } }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    readonly property color tint: root.stops ? Theme.warn : Theme.ok
                    Label { Layout.fillWidth: true; elide: Text.ElideRight; text: root.stops ? qsTr("Auto mode stops") : qsTr("Next song"); color: parent.tint; font.bold: true }
                    Label {
                        color: parent.tint
                        font.bold: true
                        font.features: { "tnum": 1 }
                        text: (root.estimated ? qsTr("in about %1") : qsTr("in %1")).arg(root.secs(root.nextAt - root.now))
                    }
                }
            }
            // The same on one line (below the decks).
            Label {
                visible: root.steps.length > 0 && root.compact
                Layout.fillWidth: true
                elide: Text.ElideRight
                textFormat: Text.StyledText
                text: root.steps.filter(st => st.to > root.now).map(st => {
                    const t = root.stepName(st.kind) + " " + root.stepTime(st);
                    return st.from <= root.now ? "<b>" + t + "</b>" : t;
                }).concat([root.stops ? "<font color='" + Theme.warn + "'>" + qsTr("auto mode stops") + "</font>"
                    : "<font color='" + Theme.ok + "'>" + qsTr("next song") + "</font>"]).join(" → ")
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
            // What will go wrong there, known now.
            Repeater {
                model: root.problems
                delegate: RowLayout {
                    id: problem
                    required property var modelData
                    Layout.fillWidth: true
                    Icon {
                        Layout.alignment: Qt.AlignTop
                        glyph: "warning"
                        color: problem.modelData.stops ? Theme.warn : Theme.system.text
                        sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: root.compact ? Text.NoWrap : Text.WordWrap
                        elide: Text.ElideRight
                        maximumLineCount: 2
                        font.pointSize: Theme.smallPointSize
                        color: problem.modelData.stops ? Theme.warn : Theme.system.text
                        text: problem.modelData.text
                        ToolTip.visible: problemHover.hovered && truncated
                        ToolTip.text: text
                        HoverHandler { id: problemHover }
                    }
                }
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
            Button {
                visible: !!root.p && !!root.p.nextChime
                icon.source: Theme.icon("bell")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Play the next song's chime now")
                onClicked: Session.run("chime_next")
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
