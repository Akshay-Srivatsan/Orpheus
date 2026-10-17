import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// One of the two identical decks. The crossfader decides which deck the room
// hears ("on air"); the other is the free deck, cued in headphones.
Frame {
    id: root
    // Sized by its contents (it has overlays besides them).
    implicitWidth: content.implicitWidth + leftPadding + rightPadding
    implicitHeight: content.implicitHeight + topPadding + bottomPadding

    property int deck: 0
    readonly property var d: Session.decks[deck]
    readonly property var song: d ? d.song : null
    readonly property string letter: deck === 0 ? "A" : "B"
    readonly property color accent: Theme.deckColors[deck]
    readonly property bool loaded: !!d && d.loaded
    readonly property bool playing: !!d && d.status === "playing"
    /// A custom label it's shown with (not just its styles' short names).
    readonly property string customLabel: song && song.label && !song.styles.some(s => s.short === song.label) ? song.label : ""
    readonly property real range: (Session.settings ? Session.settings.tempoRangePct : 10) / 100
    readonly property var stemJob: d && d.recordingId && Session.stems ? Session.stems.jobs[d.recordingId] : undefined
    readonly property var stemNames: [qsTr("Vocals"), qsTr("Drums"), qsTr("Bass"), qsTr("Other (melody, instruments)")]
    readonly property var stemShort: [qsTr("Vox"), qsTr("Drm"), qsTr("Bas"), qsTr("Oth")]

    function call(name, args) {
        const a = Object.assign({ deck: deck }, args || {});
        return Session.run(name, a);
    }

    /// On the protected on-air deck, seeking needs ⌥ / Alt.
    function seek(ms, alt) {
        if (d.protected && !alt) {
            Session.toast(qsTr("Deck %1 is on air. Hold Alt to seek it anyway, or turn off protection in Settings → General.").arg(letter), "info");
            return;
        }
        if (d.protected) call("deck_seek", { ms: Math.round(ms), force: true });
        else call("deck_scrub", { ms: Math.round(ms), force: false });
    }

    // While a song is dragged over it.
    Rectangle {
        visible: drop.containsDrag
        anchors.fill: parent
        anchors.margins: -root.padding + 1
        color: Qt.alpha(Theme.system.highlight, 0.15)
        border.color: Theme.system.highlight
    }
    DropArea {
        id: drop
        anchors.fill: parent
        keys: Session.songKeys
        enabled: !(root.d && root.d.audible)
        onDropped: drop => {
            // A set entry dragged here plays as that entry.
            const entries = drop.formats.includes("application/x-orpheus-entries") ? JSON.parse(drop.getDataAsString("application/x-orpheus-entries")) : [];
            Session.droppedVersions(drop, false).then(ids => { if (ids.length) Session.loadDeck(root.deck, ids[0], entries.length === 1 ? entries[0] : null); });
        }
    }

    ColumnLayout {
        id: content
        anchors.fill: parent
        spacing: 6

        // Header: deck, who hears it, its volume.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing
            Label { text: root.letter; font.bold: true; font.pointSize: Theme.titlePointSize; color: root.accent }
            // Who hears it.
            RowLayout {
                visible: !!root.d && (root.d.audible || (root.d.onAir && root.loaded) || root.d.pfl)
                spacing: 4
                Rectangle { visible: !!root.d && root.d.audible; implicitWidth: 8; implicitHeight: 8; radius: 4; color: Theme.onAir }
                Label {
                    text: !root.d ? "" : root.d.audible ? qsTr("On Air") : root.d.onAir ? qsTr("In the room") : qsTr("In headphones")
                    color: root.d && root.d.audible ? Theme.onAir : Theme.system.text
                    opacity: root.d && root.d.audible ? 1 : Theme.dim
                    font.bold: !!root.d && root.d.audible
                }
            }
            Label {
                visible: !!root.d && root.d.upNext
                text: qsTr("Up next")
                color: Theme.ok
            }
            Label {
                color: root.d && root.d.status === "error" ? Theme.error : Theme.system.text
                opacity: root.d && root.d.status === "error" ? 1 : Theme.dim
                text: !root.d ? "" : root.d.status === "loading" ? qsTr("Loading…") : root.d.fading ? qsTr("Fading out…")
                    : root.d.status === "ended" ? qsTr("Ended") : root.d.status === "error" ? qsTr("Error")
                    : root.d.status === "paused" && root.loaded ? (Math.abs(root.d.positionMs - root.d.cueMs) < 60 ? qsTr("Cued") : qsTr("Paused")) : ""
            }
            Item { Layout.fillWidth: true }
            Slider {
                id: volume
                Layout.fillWidth: true; Layout.minimumWidth: 50; Layout.maximumWidth: 110
                from: -24; to: 12; stepSize: 0.5
                value: root.d ? root.d.trimDb : 0
                onMoved: root.call("deck_set", { trimDb: value })
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Deck volume (double-click: 0 dB)")
                TapHandler { acceptedButtons: Qt.LeftButton; onDoubleTapped: root.call("deck_set", { trimDb: 0 }) }
            }
            Label { text: Session.fmtDb(volume.value); opacity: Theme.dim; font.features: { "tnum": 1 }; Layout.preferredWidth: 52; visible: root.width > 420 }
            // The deck's level, before the crossfader.
            ProgressBar {
                Layout.preferredWidth: 36
                Layout.preferredHeight: 8
                from: 0; to: 1
                value: Session.levels ? Math.min(1, Math.sqrt(Session.levels.decks[root.deck])) : 0
                palette.highlight: value > 0.985 ? Theme.error : Theme.ok
                ToolTip.visible: levelHover.hovered
                ToolTip.text: qsTr("Deck %1's level").arg(root.letter)
                HoverHandler { id: levelHover }
            }
            ToolButton {
                checkable: true
                checked: !!root.d && root.d.pfl
                icon.source: Theme.icon("headphones")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Hear this deck in headphones")
                onClicked: root.call("deck_set", { pfl: checked })
            }
            ToolButton {
                icon.source: Theme.icon("dots")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("More for deck %1").arg(root.letter)
                onClicked: deckMenu.popup()
                Menu {
                    id: deckMenu
                    MenuItem { text: qsTr("Set Start of Version Here"); enabled: root.loaded; onTriggered: root.call("deck_set_point", { kind: "start" }) }
                    MenuItem { text: qsTr("Set End of Version Here"); enabled: root.loaded; onTriggered: root.call("deck_set_point", { kind: "end" }) }
                    MenuItem { text: qsTr("Set Fade-Out Point Here"); enabled: root.loaded; onTriggered: root.call("deck_set_point", { kind: "fade" }) }
                    MenuItem { text: qsTr("Clear Start / End / Fade"); enabled: root.loaded; onTriggered: root.call("deck_set_point", { kind: "clear" }) }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Reset Tempo"); enabled: root.loaded; onTriggered: root.call("deck_tempo_reset") }
                    MenuItem {
                        text: qsTr("Play Next After Current Song")
                        enabled: !!root.d && !!root.d.versionId && !root.d.entryId
                        onTriggered: Session.addToSet([root.d.versionId], "next")
                    }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Eject"); enabled: root.loaded && !root.d.audible; onTriggered: root.call("deck_action", { action: "unload" }) }
                }
            }
        }

        // The song.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing
            Rectangle {
                Layout.preferredWidth: 52
                Layout.preferredHeight: 52
                radius: 4
                color: root.song ? Qt.alpha(root.song.color, 0.3) : Theme.system.alternateBase
                border.color: Theme.system.mid
                Image {
                    id: art
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectCrop
                    visible: status === Image.Ready
                    property string forRecording: ""
                    Connections {
                        target: root
                        function onDChanged() {
                            const id = root.d ? root.d.recordingId : null;
                            if (id === art.forRecording) return;
                            art.forRecording = id || "";
                            art.source = "";
                            if (id) Backend.call("recording_artwork", { id: id }).then(a => { if (art.forRecording === id && a) art.source = a; });
                        }
                    }
                }
                Icon {
                    anchors.centerIn: parent
                    visible: !art.visible && !(root.song && root.song.label)
                    glyph: "music"
                    sourceSize.width: 24; sourceSize.height: 24
                    opacity: Theme.faint
                }
                Label {
                    anchors.centerIn: parent
                    visible: !art.visible && !!root.song && !!root.song.label
                    text: root.song ? root.song.label || "" : ""
                    font.bold: true
                    elide: Text.ElideRight
                    width: parent.width - 4
                    horizontalAlignment: Text.AlignHCenter
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.pointSize: Theme.largePointSize
                    font.bold: true
                    text: root.song ? root.song.title : root.d && root.d.status === "loading" ? qsTr("Loading…") : qsTr("Deck %1 is empty").arg(root.letter)
                    opacity: root.song ? 1 : Theme.dim
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        opacity: Theme.dim
                        text: root.song ? root.song.artist + (root.song.versionLabel ? " · " + root.song.versionLabel : "")
                            : Session.settings && Session.settings.autoLoadNext ? qsTr("Drag a song here — the set's next song loads automatically") : qsTr("Drag a song here")
                    }
                    // Its custom label (e.g. a line dance's name), else its styles; the
                    // set entry's tags; and whether it streams (it needs the network).
                    StyleBadge {
                        visible: root.customLabel !== ""
                        style: root.song && root.song.styles.length ? root.song.styles[0] : null
                        label: root.customLabel
                    }
                    Repeater {
                        model: root.song && root.customLabel === "" ? root.song.styles : []
                        delegate: StyleBadge { required property var modelData; style: modelData }
                    }
                    Repeater {
                        model: root.song ? root.song.tags : []
                        delegate: Label {
                            required property var modelData
                            text: modelData.name
                            color: modelData.color
                            font.pointSize: Theme.smallPointSize
                        }
                    }
                    Label {
                        visible: !!root.song && root.song.spotify
                        text: qsTr("Spotify")
                        color: Theme.spotify
                        font.pointSize: Theme.smallPointSize
                        font.bold: true
                        ToolTip.visible: spotifyHover.hovered
                        ToolTip.text: qsTr("Streams from Spotify: it needs the internet connection")
                        HoverHandler { id: spotifyHover }
                    }
                }
                Label {
                    visible: !!root.d && !!root.d.error
                    text: root.d && root.d.error ? root.d.error : ""
                    color: Theme.error
                }
            }
            // Time left, and played.
            ColumnLayout {
                visible: root.loaded
                spacing: 0
                Label {
                    Layout.alignment: Qt.AlignRight
                    text: root.loaded ? "−" + Session.fmtTime(root.d.remainingMs) : ""
                    font.pointSize: Theme.hugePointSize
                    font.bold: true
                    font.features: { "tnum": 1 }
                    // Amber when the song is about to end.
                    color: root.d && root.d.nearEnd ? Theme.warn : Theme.system.text
                }
                Label {
                    Layout.alignment: Qt.AlignRight
                    text: root.loaded ? Session.fmtTime(root.d.elapsedMs) : ""
                    font.features: { "tnum": 1 }
                    opacity: Theme.dim
                }
            }
        }

        Waveform {
            Layout.fillWidth: true
            recordingId: root.d && root.d.recordingId ? root.d.recordingId : ""
            durationMs: root.d ? root.d.durationMs : 0
            positionMs: root.d ? root.d.positionMs : 0
            startMs: root.d ? root.d.startMs : 0
            endMs: root.d ? root.d.endMs : null
            fadeAtMs: root.d ? root.d.fadeAtMs : null
            cueMs: root.loaded ? root.d.cueMs : null
            hotCues: root.song ? root.song.hotCues : []
            accent: root.accent
            interactive: root.loaded
            guarded: !!root.d && root.d.protected
            onSeeked: (ms, alt) => root.seek(ms, alt)
            onScrubbed: (ms, alt) => { if (!root.d.protected || alt) root.call("deck_scrub", { ms: Math.round(ms), force: alt }); }
        }

        // Transport.
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            Button {
                text: qsTr("Cue"); Layout.fillWidth: true; Layout.minimumWidth: 44; Layout.maximumWidth: 100
                enabled: root.loaded
                highlighted: root.loaded && !root.playing && Math.abs(root.d.positionMs - root.d.cueMs) < 60
                onPressed: root.call("deck_cue", { down: true })
                onReleased: root.call("deck_cue", { down: false })
                ToolTip.visible: hovered
                ToolTip.text: qsTr("While playing, back to the cue point; while paused, set it here (hold to preview)")
            }
            Button {
                icon.source: Theme.icon(root.playing ? "pause" : "play")
                Layout.fillWidth: true; Layout.minimumWidth: 40; Layout.maximumWidth: 70
                enabled: root.loaded && root.d.status !== "loading"
                onClicked: root.call("deck_action", { action: "toggle" })
                ToolTip.visible: hovered
                ToolTip.text: root.playing ? qsTr("Pause") : root.d && root.d.onAir ? qsTr("Play (the room hears this deck)") : qsTr("Play in headphones")
            }
            Repeater {
                model: 4
                delegate: Button {
                    required property int index
                    readonly property int slot: index + 1
                    readonly property bool isSet: !!root.song && root.song.hotCues.some(c => c.slot === slot)
                    text: slot
                    enabled: root.loaded
                    Layout.fillWidth: true; Layout.minimumWidth: 26; Layout.maximumWidth: 40
                    palette.button: isSet ? Theme.cueColors[index] : undefined
                    down: pad.pressed
                    ToolTip.visible: hovered
                    ToolTip.text: isSet ? qsTr("Hot cue %1: jump (Alt-click or right-click to clear)").arg(slot) : qsTr("Set hot cue %1 here").arg(slot)
                    // A mouse area for the modifiers: Alt-click (or right-click) clears.
                    MouseArea {
                        id: pad
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: m => {
                            if (m.button === Qt.RightButton || (m.modifiers & Qt.AltModifier)) {
                                root.call("deck_hotcue", { slot: slot, action: "clear" });
                                return;
                            }
                            if (isSet && root.d.protected) {
                                Session.toast(qsTr("Deck %1 is on air: its hot cues don't jump. Turn off protection in Settings → General to use them.").arg(root.letter), "info");
                                return;
                            }
                            root.call("deck_hotcue", { slot: slot, action: "trigger" });
                        }
                    }
                }
            }
            Item { Layout.fillWidth: true }
            Button {
                visible: !!root.d && root.d.audible && root.playing
                enabled: !!root.d && !root.d.fading
                text: qsTr("Fade")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Fade out (%1 s)").arg(root.d ? root.d.fadeSecs : 0)
                onClicked: root.call("deck_action", { action: "fadeOut" })
            }
            Button {
                visible: !(root.d && root.d.audible && root.playing)
                enabled: root.loaded
                text: qsTr("Go Live"); Layout.minimumWidth: 60
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Put this deck on air (the other deck fades out first)")
                onClicked: root.call("deck_go_live").then(r => {
                    if (r === "armed") Session.toast(qsTr("A song is playing — press Go Live again within 3 s to fade it out."), "info");
                })
            }
            Button {
                visible: !!root.d && root.d.fading
                flat: true
                text: qsTr("Cancel")
                onClicked: root.call("deck_action", { action: "cancelFade" })
            }
        }

        // Stems and tempo.
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: root.d && root.d.stems ? 4 : 0
                delegate: Button {
                    required property int index
                    text: root.stemShort[index]
                    checkable: true
                    checked: root.d.stems[index]
                    Layout.preferredWidth: 44
                    palette.button: checked ? Theme.stemColors[index] : undefined
                    ToolTip.visible: hovered
                    ToolTip.text: root.stemNames[index]
                    onClicked: root.call("deck_stem", { index: index })
                }
            }
            Label {
                visible: !!root.d && !root.d.stems && !!root.stemJob && (root.stemJob.state === "running" || root.stemJob.state === "queued")
                text: root.stemJob && root.stemJob.state === "running" ? qsTr("Stems %1%").arg(Math.round(root.stemJob.percent)) : qsTr("Stems queued")
                opacity: Theme.dim
            }
            Button {
                visible: !!root.song && root.song.hasFile && !root.d.stems && !(root.stemJob && (root.stemJob.state === "running" || root.stemJob.state === "queued"))
                flat: true
                enabled: root.loaded
                text: root.stemJob && root.stemJob.state === "failed" ? qsTr("Retry Stems") : qsTr("Prepare Stems")
                ToolTip.visible: hovered
                ToolTip.text: root.stemJob && root.stemJob.state === "failed" ? qsTr("Failed: %1").arg(root.stemJob.message || "") : qsTr("Separate vocals, drums, bass and the rest, to mute them live (takes a minute or two)")
                onClicked: root.call("deck_stem", { index: 0 })
            }
            Label {
                visible: root.loaded
                ToolTip.visible: bpmHover.hovered && !!root.song && !!root.song.style && !!root.song.style.sweet
                ToolTip.text: root.song && root.song.style && root.song.style.sweet ? qsTr("%1: %2–%3 BPM").arg(root.song.style.name).arg(root.song.style.sweet[0]).arg(root.song.style.sweet[1]) : ""
                HoverHandler { id: bpmHover }
                textFormat: Text.StyledText
                color: !root.d ? Theme.system.text : root.d.tempoFit === "outside" ? Theme.warn : root.d.tempoFit === "sweet" ? Theme.ok : Theme.system.text
                text: "<b>" + Session.fmtBpm(root.d ? root.d.bpm : null) + "</b> BPM"
                    + (root.song && root.song.baseBpm && Math.abs(root.d.tempo - 1) > 0.0005 ? " · " + qsTr("orig %1").arg(Session.fmtBpm(root.song.baseBpm)) : "")
            }
            Slider {
                id: tempo
                Layout.fillWidth: true
                from: 1 - root.range; to: 1 + root.range; stepSize: 0.001
                value: root.d ? root.d.tempo : 1
                enabled: root.loaded
                onMoved: root.call("deck_set", { tempo: value })
                TapHandler { acceptedButtons: Qt.LeftButton; onDoubleTapped: root.call("deck_tempo_reset") }
            }
            ToolButton {
                enabled: root.loaded
                contentItem: Label {
                    text: Session.fmtPct(tempo.value)
                    color: root.d && root.d.tooSlow ? Theme.warn : Theme.system.buttonText
                    font.features: { "tnum": 1 }
                    horizontalAlignment: Text.AlignHCenter
                }
                ToolTip.visible: hovered
                ToolTip.text: root.d && root.d.tooSlow ? qsTr("Slowed more than the club allows — may sound warbly") : qsTr("Reset to the version's tempo")
                onClicked: root.call("deck_tempo_reset")
            }
        }
    }
}
