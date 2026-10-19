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
    /// The two decks as double buffering: the front one plays the song (or
    /// holds it paused partway through), the other has the next one ready.
    /// Worked out from the decks themselves (the engine's `liveDeck` moves
    /// only once the room hears the new song). -1: none (the next is shown).
    readonly property int front: {
        const ds = Session.decks;
        const playing = [0, 1].filter(i => ds[i] && ds[i].status === "playing");
        // Both, while one fades out: the other.
        if (playing.length === 2) {
            const steady = playing.filter(i => !ds[i].fading);
            return steady.length === 1 ? steady[0] : p ? p.liveDeck : playing[0];
        }
        if (playing.length === 1) return playing[0];
        // Paused partway through the song the room heard last.
        const paused = [0, 1].filter(i => ds[i] && ds[i].status === "paused" && ds[i].elapsedMs > 0 && ds[i].remainingMs > 0
            && !!p && !!ds[i].entryId && ds[i].entryId === p.currentEntryId);
        return paused.length ? paused[0] : -1;
    }
    readonly property var d: front >= 0 ? Session.decks[front] : null
    readonly property var song: d && d.loaded ? d.song : null
    readonly property bool audible: !!d && d.status === "playing"
    readonly property bool pausedMidSong: !!d && d.status === "paused"
    readonly property real lengthMs: song ? d.elapsedMs + d.remainingMs : 0
    /// The song shown: the front deck's; once it has faded or ended, the next
    /// one (from its deck if it's loaded, else from the set).
    readonly property bool showingLive: !!song
    readonly property var nextRow: plan.next && !plan.next.isPause && plan.next.trackId ? plan.next : null
    readonly property var shown: {
        // A song just started (still loading, or not yet past its start) counts as the one shown.
        const starting = !p ? null : Session.decks.find(x => x && x.song && x.entryId && x.entryId === p.currentEntryId && (x.status === "loading" || (x.status === "paused" && x.elapsedMs === 0))) || null;
        const k = showingLive ? d : starting || (plan.nextDeck && plan.nextDeck.loaded && plan.nextDeck.song ? plan.nextDeck : null);
        if (k) return { title: k.song.title, artist: k.song.artist, styles: k.song.styles, color: k.song.color, meter: k.song.meter, bpm: k.bpm,
            recordingId: k.recordingId || "", trackId: k.trackId, entryId: k.entryId, elapsedMs: k.elapsedMs, remainingMs: k.remainingMs };
        const r = nextRow;
        if (r) return { title: r.title, artist: r.artist, styles: r.styles, color: r.styles.length ? r.styles[0].color : Theme.neutral, meter: r.meter, bpm: r.bpm,
            recordingId: r.recordingId || "", trackId: r.trackId, entryId: r.entry.id, elapsedMs: 0, remainingMs: r.durationMs };
        return null;
    }
    /// Text goes white over the darkened artwork (else the window's own colours).
    readonly property bool overArt: art.status === Image.Ready
    readonly property color fg: overArt ? "white" : Theme.system.text

    function playPause() {
        if (front >= 0) Session.run("deck_action", { deck: front, action: "toggle" });
        else plan.playNext();
    }
    /// Dragging the position is deliberate, so it seeks even the song on air
    /// (the protection is against a stray jog wheel or cue button).
    function seek(fraction) {
        if (!showingLive) return;
        Session.run("deck_seek", { deck: root.front, ms: Math.round(fraction * lengthMs), force: true });
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
                color: root.shown ? Qt.tint(Theme.system.window, Qt.alpha(root.shown.color, 0.18)) : Theme.system.window
            }
            Image {
                id: art
                anchors.fill: parent
                visible: false
                fillMode: Image.PreserveAspectCrop
                property string forRecording: ""
                readonly property string want: root.shown ? root.shown.recordingId : ""
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
                    color: root.shown ? Qt.alpha(root.shown.color, 0.3) : Theme.system.alternateBase
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
                    // Whether it's playing, paused, or the one to come.
                    Label {
                        visible: !!root.shown
                        text: !root.showingLive ? qsTr("Up next") : root.audible ? qsTr("Now playing") : qsTr("Paused")
                        font.pointSize: Theme.smallPointSize
                        font.capitalization: Font.AllUppercase
                        color: root.fg
                        opacity: Theme.dim
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.nowPlayingTitlePointSize
                        font.bold: true
                        color: root.fg
                        text: root.shown ? root.shown.title : qsTr("Nothing playing")
                        opacity: root.shown ? 1 : Theme.dim
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.nowPlayingArtistPointSize
                        visible: !!text
                        color: root.fg
                        opacity: Theme.dim
                        text: root.shown ? root.shown.artist : Session.set ? qsTr("Press ▶ to start “%1”").arg(Session.set.setlist.name) : qsTr("Open or make a set to play it")
                    }
                    // Its dance, tempo and meter.
                    Label {
                        Layout.fillWidth: true
                        visible: !!root.shown
                        elide: Text.ElideRight
                        textFormat: Text.StyledText
                        color: root.fg
                        text: !root.shown ? "" : [root.shown.styles.map(s => "<font color='" + Theme.readable(s.color, root.overArt || undefined) + "'><b>" + s.name + "</b></font>").join(" / "),
                            root.shown.bpm ? Session.fmtBpm(root.shown.bpm) + " BPM" : "", Session.meterName(root.shown.meter)].filter(x => x).join("  ·  ")
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
                        visible: plan.steps.length > 0 || root.showingLive
                        text: plan.steps.length ? qsTr("Next") : qsTr("Up next")
                        font.pointSize: Theme.smallPointSize
                        font.capitalization: Font.AllUppercase
                        color: root.fg
                        opacity: Theme.dim
                    }
                    Label {
                        Layout.fillWidth: true
                        // The song shown is the next one: not again here.
                        visible: plan.steps.length > 0 || root.showingLive
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
                    onClicked: Session.run("deck_action", { deck: root.front, action: "fadeOut" })
                }
                Label { text: Session.fmtTime(root.shown ? root.shown.elapsedMs : 0); opacity: Theme.dim; font.features: { "tnum": 1 } }
                Slider {
                    id: position
                    Layout.fillWidth: true
                    enabled: root.showingLive
                    from: 0; to: 1
                    // Follows the song, except while you drag it.
                    Binding on value { when: !position.pressed; value: root.showingLive && root.lengthMs > 0 ? root.d.elapsedMs / root.lengthMs : 0; restoreMode: Binding.RestoreNone }
                    onPressedChanged: if (!pressed) root.seek(value)
                }
                Label { text: root.shown ? "−" + Session.fmtTime(root.shown.remainingMs) : Session.fmtTime(0); opacity: Theme.dim; font.features: { "tnum": 1 } }
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
                    enabled: !!root.shown && !!root.shown.trackId
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Song Info (the song shown)")
                    onClicked: Session.showInfo(root.shown.trackId, root.shown.entryId)
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
