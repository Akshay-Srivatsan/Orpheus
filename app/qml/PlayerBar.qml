import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The simple interface's player, along the bottom: the song the room hears,
// play/pause, Play Next and fade, how far it is, what auto mode does next,
// and the room volume. (The engine still readies the next song on its other
// deck; this shows one player.)
ToolBar {
    id: root
    position: ToolBar.Footer

    AutoPlan { id: plan }
    readonly property var p: plan.p
    readonly property var d: plan.liveDeck
    readonly property var song: d && d.loaded ? d.song : null
    readonly property bool audible: !!d && d.audible
    /// Paused partway through (the room heard it, and the pause hasn't started).
    readonly property bool pausedMidSong: !!d && d.loaded && d.status === "paused" && !!p && !p.gapStartedAt && d.elapsedMs > 0

    function playPause() {
        if (audible || pausedMidSong) Session.run("deck_action", { deck: p.liveDeck, action: "toggle" });
        else plan.playNext();
    }

    leftPadding: Theme.spacing
    rightPadding: Theme.spacing
    topPadding: Theme.spacing / 2
    bottomPadding: Theme.spacing / 2

    contentItem: RowLayout {
        spacing: Theme.spacing * 2

        // The song: artwork (else its dance), title, artist and dance.
        Rectangle {
            Layout.preferredWidth: 48
            Layout.preferredHeight: 48
            radius: 4
            color: root.song ? Qt.alpha(root.song.color, 0.3) : Theme.system.alternateBase
            border.color: Theme.system.mid
            Image {
                id: art
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                visible: status === Image.Ready
                property string forRecording: ""
                readonly property string want: root.d && root.d.loaded ? root.d.recordingId || "" : ""
                onWantChanged: {
                    if (want === forRecording) return;
                    forRecording = want;
                    source = "";
                    const id = want;
                    if (id) Backend.call("recording_artwork", { id: id }).then(a => { if (art.forRecording === id && a) art.source = a; });
                }
            }
            Icon {
                anchors.centerIn: parent
                visible: !art.visible
                glyph: "music"
                sourceSize.width: 22; sourceSize.height: 22
                opacity: Theme.faint
            }
        }
        ColumnLayout {
            Layout.preferredWidth: 260
            Layout.minimumWidth: 120
            spacing: 0
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.bold: true
                text: root.song ? root.song.title : qsTr("Nothing playing")
                opacity: root.song ? 1 : Theme.dim
            }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                textFormat: Text.StyledText
                opacity: root.song ? 1 : Theme.dim
                text: !root.song ? (Session.set ? qsTr("Press ▶ to start the set") : qsTr("Open or make a set to play it")) :
                    [root.song.artist, root.song.styles.map(s => "<font color='" + Theme.readable(s.color) + "'>" + s.name + "</font>").join(" / "),
                        root.d.bpm ? Session.fmtBpm(root.d.bpm) + " BPM" : "", Session.meterName(root.song.meter)].filter(x => x).join(" · ")
            }
        }

        // Play / pause, Play Next, fade out; and how far the song is.
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Button {
                    implicitWidth: implicitContentWidth + leftPadding + rightPadding
                    icon.source: Theme.icon(root.audible ? "pause" : "play")
                    // The main control: a larger icon than its neighbours.
                    icon.width: Theme.playIconSize
                    icon.height: Theme.playIconSize
                    enabled: root.audible || root.pausedMidSong || (!!plan.next && (!!plan.next.trackId || plan.next.isPause))
                    ToolTip.visible: hovered
                    ToolTip.text: root.audible ? qsTr("Pause the song") : root.pausedMidSong ? qsTr("Go on with the song") : qsTr("Start the next song in the set (Ctrl+Enter)")
                    onClicked: root.playPause()
                }
                Button {
                    implicitWidth: implicitContentWidth + leftPadding + rightPadding
                    icon.source: Theme.icon("next")
                    text: root.audible ? (!!root.p && root.p.armed ? qsTr("Press Again") : qsTr("Next")) : ""
                    enabled: !!plan.next && (!!plan.next.trackId || plan.next.isPause)
                    ToolTip.visible: hovered
                    ToolTip.text: root.audible ? qsTr("Fade this song out and start the next (press twice)") : qsTr("Start the next song in the set")
                    onClicked: plan.playNext()
                }
                Button {
                    implicitWidth: implicitContentWidth + leftPadding + rightPadding
                    icon.source: Theme.icon("fade")
                    enabled: root.audible && !(root.d && root.d.fading)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Fade the song out")
                    onClicked: Session.run("deck_action", { deck: root.p.liveDeck, action: "fadeOut" })
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: root.song ? Session.fmtTime(root.d.elapsedMs) : "0:00"; opacity: Theme.dim; font.features: { "tnum": 1 } }
                ProgressBar {
                    Layout.fillWidth: true
                    from: 0; to: 1
                    value: root.song && root.d.elapsedMs + root.d.remainingMs > 0 ? root.d.elapsedMs / (root.d.elapsedMs + root.d.remainingMs) : 0
                }
                Label { text: root.song ? "−" + Session.fmtTime(root.d.remainingMs) : "0:00"; opacity: Theme.dim; font.features: { "tnum": 1 } }
            }
        }

        // What happens next: auto mode's steps (else what's up next), and warnings.
        ColumnLayout {
            Layout.preferredWidth: 340
            Layout.minimumWidth: 160
            spacing: 0
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                textFormat: Text.StyledText
                text: plan.steps.length ? plan.stepsText
                    : plan.next ? qsTr("Up next: %1").arg(plan.next.isPause ? qsTr("a pause") : plan.next.title || plan.next.label) : Session.set ? qsTr("End of the set") : ""
                ToolTip.visible: nextHover.hovered && truncated
                ToolTip.text: text
                HoverHandler { id: nextHover }
            }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.pointSize: Theme.smallPointSize
                visible: plan.problems.length > 0
                color: plan.stops ? Theme.warn : Theme.system.text
                text: plan.problems.length ? "⚠ " + plan.problems[0].text + (plan.problems.length > 1 ? " " + qsTr("(%1 more)").arg(plan.problems.length - 1) : "") : ""
                ToolTip.visible: problemHover.hovered
                ToolTip.text: plan.problems.map(x => x.text).join("\n")
                HoverHandler { id: problemHover }
            }
        }
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
        // The room volume (slider positions are square roots of the gain).
        Icon { glyph: "speaker" }
        Slider {
            Layout.preferredWidth: 100
            from: 0; to: 1; stepSize: 0.01
            value: Session.levels ? Math.sqrt(Session.levels.masterVolume) : 0
            onMoved: Session.run("levels_set", { master: value * value, cue: null, crossfader: null })
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Volume")
        }
    }
}
