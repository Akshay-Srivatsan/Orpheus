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

    // What happens next (shared with the simple interface's player bar).
    AutoPlan { id: plan }
    readonly property var p: plan.p
    readonly property bool playing: plan.playing
    readonly property var next: plan.next
    readonly property string nextLabel: plan.nextLabel
    readonly property var nextDeck: plan.nextDeck
    readonly property real crossfader: p ? p.crossfader : 0.5
    readonly property bool inGap: plan.inGap
    readonly property real now: plan.now
    readonly property real gapMs: plan.gapMs
    readonly property real gapTarget: plan.gapTarget
    readonly property var liveDeck: plan.liveDeck
    readonly property var steps: plan.steps
    readonly property real nextAt: plan.nextAt
    readonly property bool estimated: plan.estimated
    readonly property var problems: plan.problems
    readonly property bool stops: plan.stops
    readonly property string stepsText: plan.stepsText
    function stepName(kind) { return plan.stepName(kind); }
    function secs(ms) { return plan.secs(ms); }
    function stepTime(st) { return plan.stepTime(st); }
    function playNext() { plan.playNext(); }
    /// Lines of text the middle keeps room for beside the decks: the steps
    /// (wrapped: three), what's up next (two), two warnings (a line each); and
    /// the progress bar.
    readonly property int reservedLines: 7
    FontMetrics { id: lines; font: root.font }
    function setCrossfader(x) { Session.run("levels_set", { master: null, cue: null, crossfader: x }); }

    GridLayout {
        anchors.fill: parent
        columns: root.compact ? 3 : 1
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        // Play Next, and (while a start waits for a fade) Cancel beside it.
        RowLayout {
            Layout.row: 0; Layout.column: 0
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Button {
                id: playNext
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
            Button {
                enabled: !!root.p && root.p.pendingNext
                Layout.minimumWidth: 0
                text: qsTr("Cancel")
                onClicked: Session.run("perform_cancel_pending")
            }
        }

        // The pause, and what's next. Beside the decks its height is fixed (the
        // most it usually shows: the steps, what's next, two warnings), so the
        // panel doesn't change height as things happen; it scrolls if it ever
        // needs more.
        ScrollView {
            id: middle
            Layout.row: root.compact ? 0 : 2
            Layout.column: root.compact ? 1 : 0
            Layout.rowSpan: root.compact ? 2 : 1
            Layout.fillWidth: true
            Layout.fillHeight: true
            // Its contents never widen the panel (long titles elide); below the
            // decks it takes most of the width beside the buttons.
            Layout.minimumWidth: 0
            Layout.preferredWidth: 0
            Layout.horizontalStretchFactor: root.compact ? 3 : 1
            clip: true
            Layout.preferredHeight: root.compact ? implicitHeight : lines.height * root.reservedLines + progress.implicitHeight
            Layout.alignment: Qt.AlignTop
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            ColumnLayout {
                width: middle.availableWidth
                spacing: 2

                // Without auto mode's steps (Play Next starts the next song, or
                // auto mode waits for it): one line, in the pause or not.
                RowLayout {
                    visible: !root.steps.length
                    Layout.fillWidth: true
                    Label { visible: root.inGap; text: Session.fmtTime(root.gapMs); font.bold: true; color: root.gapMs > root.gapTarget ? Theme.warn : Theme.system.text; font.features: { "tnum": 1 } }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        readonly property bool held: !!root.p && root.p.autoPhase === "held"
                        color: held ? Theme.warn : Theme.system.text
                        opacity: root.inGap || held ? 1 : Theme.dim
                        text: !root.p ? "" : root.inGap ? (held ? qsTr("waits for Play Next") : qsTr("pause · target %1 s").arg(Math.round(root.gapTarget / 1000)))
                            : held ? qsTr("Auto mode stops before the next song.")
                            // Nothing playing, no pause (the set's start, or auto mode stopped).
                            : !root.playing ? qsTr("Press Play Next to start the next song.")
                            : root.p.autoAdvance ? qsTr("Auto mode starts the next song after the pause.")
                            : qsTr("The next song starts when you press Play Next.")
                        ToolTip.visible: statusHover.hovered && truncated
                        ToolTip.text: text
                        HoverHandler { id: statusHover }
                    }
                }
                // The pause so far (its room kept while a song plays).
                ProgressBar {
                    id: progress
                    visible: !root.compact
                    Layout.fillWidth: true
                    opacity: root.inGap ? 1 : 0
                    from: 0; to: 1
                    value: !root.inGap ? 0 : root.steps.length ? Math.min(1, root.gapMs / Math.max(1, root.nextAt - root.p.gapStartedAt)) : Math.min(1, root.gapMs / root.gapTarget)
                }

                // Auto mode's steps to the next song, as one line (wrapping beside
                // the decks): each with its time (done: struck out; the current
                // one bold), then when the next song starts.
                Label {
                    visible: root.steps.length > 0
                    Layout.fillWidth: true
                    wrapMode: root.compact ? Text.NoWrap : Text.WordWrap
                    elide: root.compact ? Text.ElideRight : Text.ElideNone
                    textFormat: Text.StyledText
                    text: root.stepsText
                }

                // (Below the decks there are three lines: the status or steps, what's
                // next, a warning: the progress bar and the artist line are left out.)
                RowLayout {
                    Layout.fillWidth: true
                    visible: !!root.next
                    Label { text: qsTr("Up next:"); opacity: Theme.dim }
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
                    visible: !!root.next && !root.compact
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    opacity: Theme.dim
                    text: !root.next ? "" : root.next.isPause
                        ? qsTr("%1 s").arg(Math.round(root.next.durationMs / 1000)) + (root.next.entry.announcement ? " · “" + root.next.entry.announcement + "”" : "")
                            + (root.next.entry.hold ? " · " + qsTr("stops here") : "")
                        : [root.next.artist, root.next.bpm ? Session.fmtBpm(root.next.bpm) + " BPM" : "", root.nextDeck ? qsTr("deck %1").arg(root.nextDeck.deck === 0 ? "A" : "B") : ""].filter(x => x).join(" · ")
                }
                // What will go wrong there, known now.
                // (Below the decks: the first, on one line; all in its tooltip.)
                Repeater {
                    model: root.compact ? root.problems.slice(0, 1) : root.problems
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
                            // One line each (the panel's room is fixed); all of it in the tooltip.
                            elide: Text.ElideRight
                            font.pointSize: Theme.smallPointSize
                            color: problem.modelData.stops ? Theme.warn : Theme.system.text
                            text: (root.compact && root.problems.length > 1 ? qsTr("(%1 more) ").arg(root.problems.length - 1) : "") + problem.modelData.text
                            ToolTip.visible: problemHover.hovered && (truncated || (root.compact && root.problems.length > 1))
                            ToolTip.text: root.compact ? root.problems.map(x => x.text).join("\n") : text
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
            // Auto mode needs it centered.
            Button {
                // An icon's width (Fusion's buttons are otherwise at least 80 px).
                implicitWidth: implicitContentWidth + leftPadding + rightPadding
                enabled: root.crossfader !== 0.5 && !!root.p && root.p.autoAdvance
                icon.source: Theme.icon("warning")
                icon.color: enabled ? Theme.warn : palette.buttonText
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Auto mode needs the crossfader centered: click to center it")
                onClicked: root.setCrossfader(0.5)
            }
        }

        RowLayout {
            Layout.row: root.compact ? 0 : 4
            Layout.column: root.compact ? 2 : 0
            Layout.fillWidth: true
            // They share the width: none keeps more than its content needs
            // (Fusion's buttons are otherwise at least 80 px wide), and all
            // shrink to fit (text elides).
            Button {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: implicitContentWidth + leftPadding + rightPadding
                text: qsTr("Skip")
                icon.source: Theme.icon("skip")
                enabled: !!root.next
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Skip the up-next song (it stays in the set, greyed out)")
                onClicked: Session.run("perform_skip_next")
            }
            Button {
                Layout.minimumWidth: 0
                Layout.preferredWidth: implicitContentWidth + leftPadding + rightPadding
                icon.source: Theme.icon("mic")
                enabled: (!!root.next) && (!!Session.tts && Session.tts.state === "ready")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Announce the next dance now")
                onClicked: Session.run("announce_next")
            }
            Button {
                Layout.minimumWidth: 0
                Layout.preferredWidth: implicitContentWidth + leftPadding + rightPadding
                enabled: !!root.p && !!root.p.nextChime
                icon.source: Theme.icon("bell")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Play the next song's chime now")
                onClicked: Session.run("chime_next")
            }
            Button {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: implicitContentWidth + leftPadding + rightPadding
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
