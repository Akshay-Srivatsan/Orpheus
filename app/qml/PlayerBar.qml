import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Layouts
import Orpheus

// The simple interface's player, along the top (as in Elisa): the song the
// room hears, large, over its blurred artwork, with what happens next; then a
// toolbar: play/pause, Play Next, fade, where the song is, auto mode, its Info
// and the room volume. (The engine still readies the next song on its other
// deck; this shows one player.)
Control {
    id: root
    padding: 0

    AutoPlan { id: plan }
    readonly property var p: plan.p
    readonly property var d: plan.liveDeck
    readonly property var song: d && d.loaded ? d.song : null
    readonly property bool audible: !!d && d.audible
    /// Paused partway through (the room heard it, and the pause hasn't started).
    readonly property bool pausedMidSong: !!d && d.loaded && d.status === "paused" && !!p && !p.gapStartedAt && d.elapsedMs > 0
    readonly property real lengthMs: song ? d.elapsedMs + d.remainingMs : 0
    /// Text goes white over the darkened artwork (else the window's own colours).
    readonly property bool overArt: art.status === Image.Ready
    readonly property color fg: overArt ? "white" : Theme.system.text

    function playPause() {
        if (audible || pausedMidSong) Session.run("deck_action", { deck: p.liveDeck, action: "toggle" });
        else plan.playNext();
    }
    /// The song on air is protected from seeking (Settings → General), as on a deck.
    function seek(fraction) {
        if (!song) return;
        if (d.protected) {
            Session.toast(qsTr("The song is playing to the room: seeking is off while it plays (Settings → General → Protect the on-air deck)."), "info");
            return;
        }
        Session.run("deck_scrub", { deck: p.liveDeck, ms: Math.round(fraction * lengthMs), force: false });
    }

    contentItem: ColumnLayout {
        spacing: 0

        // ------------------------------------------------ now playing
        Item {
            Layout.fillWidth: true
            implicitHeight: nowPlaying.implicitHeight + Theme.spacing * 3
            clip: true

            Rectangle {
                anchors.fill: parent
                color: root.song ? Qt.tint(Theme.system.window, Qt.alpha(root.song.color, 0.18)) : Theme.system.window
            }
            Image {
                id: art
                anchors.fill: parent
                visible: false
                fillMode: Image.PreserveAspectCrop
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
            MultiEffect {
                anchors.fill: parent
                visible: root.overArt
                source: art
                blurEnabled: true
                blur: 1
                blurMax: Theme.nowPlayingBlur
                autoPaddingEnabled: false
            }
            Rectangle { anchors.fill: parent; visible: root.overArt; color: "black"; opacity: Theme.nowPlayingScrim }

            RowLayout {
                id: nowPlaying
                anchors.fill: parent
                anchors.margins: Theme.spacing * 1.5
                spacing: Theme.spacing * 2

                // The artwork (else a note, in the dance's colour).
                Rectangle {
                    Layout.preferredWidth: Theme.nowPlayingArtSize
                    Layout.preferredHeight: Theme.nowPlayingArtSize
                    radius: 4
                    color: root.song ? Qt.alpha(root.song.color, 0.3) : Theme.system.alternateBase
                    border.color: Theme.system.mid
                    Image {
                        anchors.fill: parent
                        anchors.margins: 1
                        source: art.source
                        fillMode: Image.PreserveAspectCrop
                        visible: root.overArt
                    }
                    Icon {
                        anchors.centerIn: parent
                        visible: !root.overArt
                        glyph: "music"
                        sourceSize.width: Theme.nowPlayingArtSize / 3
                        sourceSize.height: Theme.nowPlayingArtSize / 3
                        opacity: Theme.faint
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.nowPlayingTitlePointSize
                        font.bold: true
                        color: root.fg
                        text: root.song ? root.song.title : qsTr("Nothing playing")
                        opacity: root.song ? 1 : Theme.dim
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.nowPlayingArtistPointSize
                        visible: !!text
                        color: root.fg
                        opacity: Theme.dim
                        text: root.song ? root.song.artist : Session.set ? qsTr("Press ▶ to start “%1”").arg(Session.set.setlist.name) : qsTr("Open or make a set to play it")
                    }
                    // Its dance, tempo and meter.
                    Label {
                        Layout.fillWidth: true
                        visible: !!root.song
                        elide: Text.ElideRight
                        textFormat: Text.StyledText
                        color: root.fg
                        text: !root.song ? "" : [root.song.styles.map(s => "<font color='" + Theme.readable(s.color, root.overArt || undefined) + "'><b>" + s.name + "</b></font>").join(" / "),
                            root.d.bpm ? Session.fmtBpm(root.d.bpm) + " BPM" : "", Session.meterName(root.song.meter)].filter(x => x).join("  ·  ")
                    }
                }
                // What happens next: auto mode's steps (else what's up next), and warnings.
                ColumnLayout {
                    Layout.fillWidth: false
                    Layout.preferredWidth: Theme.upNextWidth
                    Layout.minimumWidth: 140
                    Layout.alignment: Qt.AlignTop
                    spacing: 2
                    Label {
                        text: plan.steps.length ? qsTr("Next") : qsTr("Up next")
                        font.pointSize: Theme.smallPointSize
                        font.capitalization: Font.AllUppercase
                        color: root.fg
                        opacity: Theme.dim
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        textFormat: Text.StyledText
                        color: root.fg
                        text: plan.steps.length ? plan.stepsText
                            : plan.next ? (plan.next.isPause ? qsTr("A pause") : plan.next.title || plan.next.label) : Session.set ? qsTr("The end of the set") : "—"
                        ToolTip.visible: nextHover.hovered && truncated
                        ToolTip.text: text
                        HoverHandler { id: nextHover }
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.smallPointSize
                        visible: plan.problems.length > 0
                        color: root.overArt ? Theme.readable(Theme.warn, true) : Theme.warn
                        text: plan.problems.length ? "⚠ " + plan.problems[0].text + (plan.problems.length > 1 ? " " + qsTr("(%1 more)").arg(plan.problems.length - 1) : "") : ""
                        ToolTip.visible: problemHover.hovered
                        ToolTip.text: plan.problems.map(x => x.text).join("\n")
                        HoverHandler { id: problemHover }
                    }
                }
            }
        }

        // ------------------------------------------------ controls
        ToolBar {
            Layout.fillWidth: true
            position: ToolBar.Header
            leftPadding: Theme.spacing
            rightPadding: Theme.spacing
            contentItem: RowLayout {
                spacing: Theme.spacing
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon(root.audible ? "pause" : "play")
                    icon.width: Theme.playIconSize
                    icon.height: Theme.playIconSize
                    enabled: root.audible || root.pausedMidSong || (!!plan.next && (!!plan.next.trackId || plan.next.isPause))
                    ToolTip.visible: hovered
                    ToolTip.text: root.audible ? qsTr("Pause (Space)") : root.pausedMidSong ? qsTr("Go on with the song (Space)") : qsTr("Start the next song in the set (Space)")
                    onClicked: root.playPause()
                }
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon("next")
                    // A second press is needed while a song plays (Settings → General).
                    text: !!root.p && root.p.armed ? qsTr("Press Again") : ""
                    display: text ? AbstractButton.TextBesideIcon : AbstractButton.IconOnly
                    enabled: !!plan.next && (!!plan.next.trackId || plan.next.isPause)
                    ToolTip.visible: hovered
                    ToolTip.text: root.audible ? qsTr("Fade this song out and start the next (Ctrl+Enter)") : qsTr("Start the next song in the set (Ctrl+Enter)")
                    onClicked: plan.playNext()
                }
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon("fade")
                    enabled: root.audible && !(root.d && root.d.fading)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Fade the song out")
                    onClicked: Session.run("deck_action", { deck: root.p.liveDeck, action: "fadeOut" })
                }
                Label { text: Session.fmtTime(root.song ? root.d.elapsedMs : 0); opacity: Theme.dim; font.features: { "tnum": 1 } }
                Slider {
                    id: position
                    Layout.fillWidth: true
                    enabled: !!root.song
                    from: 0; to: 1
                    // Follows the song, except while you drag it.
                    Binding on value { when: !position.pressed; value: root.lengthMs > 0 ? root.d.elapsedMs / root.lengthMs : 0; restoreMode: Binding.RestoreNone }
                    onPressedChanged: if (!pressed) root.seek(value)
                }
                Label { text: root.song ? "−" + Session.fmtTime(root.d.remainingMs) : Session.fmtTime(0); opacity: Theme.dim; font.features: { "tnum": 1 } }
                ToolSeparator {}
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon("auto")
                    text: qsTr("Auto")
                    checkable: true
                    checked: !!root.p && root.p.autoAdvance
                    ToolTip.visible: hovered
                    ToolTip.text: Session.settings && Session.settings.announce.enabled
                        ? qsTr("Auto mode: after a song, pause, announce the next dance, pause, and start it")
                        : qsTr("Auto mode: start the next song after the pause")
                    onClicked: Session.run("perform_set_auto_advance", { value: checked })
                }
                ToolButton {
                    icon.color: palette.buttonText
                    icon.source: Theme.icon("info")
                    enabled: !!root.song && !!root.d.trackId
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Song Info (the song playing)")
                    onClicked: Session.showInfo(root.d.trackId, root.d.entryId)
                }
                ToolSeparator {}
                // The room volume (slider positions are square roots of the gain).
                Icon { glyph: "speaker" }
                Slider {
                    Layout.preferredWidth: Theme.volumeSliderWidth
                    from: 0; to: 1; stepSize: 0.01
                    value: Session.levels ? Math.sqrt(Session.levels.masterVolume) : 0
                    onMoved: Session.run("levels_set", { master: value * value, cue: null, crossfader: null })
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Volume")
                }
            }
        }
    }
}
