import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The audio a version plays (`audio` from `song_view`): BPM, meter and gain,
// its files and Spotify link, stems, and what to copy into a club's file.
ColumnLayout {
    id: root

    property var audio
    /// The versions of the song playing it (Split Off can give one its own).
    property var versions: []
    readonly property var r: audio.recording
    readonly property var job: Session.stems ? Session.stems.jobs[r.id] : undefined
    property var taps: []
    property string linkText: ""

    spacing: 4

    function patch(p) { Session.run("recording_update", { id: r.id, patch: p }); }
    function scaleBpm(f) { if (r.bpm) patch({ bpm: Math.round(r.bpm * f * 10) / 10, bpmSource: "user" }); }

    /// Tapping along: four taps or more within a few seconds set the BPM.
    function tap() {
        const now = Date.now();
        const recent = taps.filter(t => now - t < 2500).concat([now]).slice(-9);
        taps = recent;
        if (recent.length >= 4) {
            const interval = (recent[recent.length - 1] - recent[0]) / (recent.length - 1);
            patch({ bpm: Math.round(60000 / interval * 10) / 10, bpmSource: "tap" });
        }
    }

    function split(source, versionIds, newSong) {
        Session.undoable(newSong ? qsTr("Split off as a song of its own") : qsTr("Split off as audio of its own"),
            () => Session.run("recording_split_source", { id: r.id, source: source, versions: versionIds, newSong: newSong }));
    }

    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing
        rowSpacing: 4

        FormLabel { text: qsTr("BPM") }
        RowLayout {
            SpinBox {
                id: bpm
                from: 0; to: 4000; stepSize: 1
                editable: true
                value: Math.round((root.r.bpm || 0) * 10)
                textFromValue: v => v ? (v / 10).toFixed(1) : ""
                valueFromText: t => Math.round(Number(t) * 10)
                onValueModified: root.patch({ bpm: value ? value / 10 : null })
                implicitWidth: 110
            }
            Button { text: qsTr("Tap"); onClicked: root.tap(); ToolTip.visible: hovered; ToolTip.text: qsTr("Tap along to the beat (4+ taps)") }
            Button {
                text: qsTr("Correct")
                enabled: !!root.r.bpm
                ToolTip.visible: hovered
                ToolTip.text: qsTr("When the BPM counts the wrong beat: halve it, double it…")
                onClicked: correctMenu.popup()
                Menu {
                    id: correctMenu
                    MenuItem { text: qsTr("Halve (½)"); onTriggered: root.scaleBpm(0.5) }
                    MenuItem { text: qsTr("Two Thirds (⅔)"); onTriggered: root.scaleBpm(2 / 3) }
                    MenuItem { text: qsTr("One and a Half (1½)"); onTriggered: root.scaleBpm(1.5) }
                    MenuItem { text: qsTr("Double (2×)"); onTriggered: root.scaleBpm(2) }
                }
            }
        }
        Item { implicitWidth: 1 }
        RowLayout {
            Button {
                visible: root.r.bpmSource !== "detected"
                flat: true
                icon.source: Theme.icon("reload")
                text: qsTr("Analysed") + (root.r.detectedBpm != null ? " (" + Session.fmtBpm(root.r.detectedBpm) + ")" : "")
                onClicked: Session.run("recording_reset_bpm", { id: root.r.id, from: "detected" })
            }
            Button {
                visible: root.r.bpmSource !== "tag" && (root.r.tagBpm != null || root.r.files.length > 0)
                flat: true
                icon.source: Theme.icon("reload")
                text: qsTr("File tag") + (root.r.tagBpm != null ? " (" + Session.fmtBpm(root.r.tagBpm) + ")" : "")
                onClicked: Session.run("recording_reset_bpm", { id: root.r.id, from: "tag" })
            }
        }
        Item { implicitWidth: 1 }
        Label {
            opacity: 0.7
            font.pointSize: Theme.smallPointSize
            text: root.audio.bpmFrom + (root.r.loudnessDb != null ? " · " + root.r.loudnessDb.toFixed(1) + " LUFS" : "") + " · " + Session.fmtTime(root.r.durationMs)
        }
        FormLabel { text: qsTr("Meter") }
        ComboBox {
            readonly property var values: ["auto", 3, 4]
            model: [qsTr("Detected") + (root.r.detectedMeter ? ": " + Session.meterName(root.r.detectedMeter) : root.r.analyzed ? qsTr(": unclear") : ""),
                qsTr("Triple (3/4, 6/8)"), qsTr("Duple (2/4, 4/4)")]
            currentIndex: root.r.meterSource === "user" ? (root.r.beatsPerBar === 3 ? 1 : 2) : 0
            onActivated: index => root.patch({ beatsPerBar: index === 0 ? null : values[index] })
        }
        FormLabel { text: qsTr("Gain") }
        RowLayout {
            SpinBox {
                from: -240; to: 240; stepSize: 5
                editable: true
                value: Math.round((root.r.gainDb || 0) * 10)
                textFromValue: v => (v / 10).toFixed(1)
                valueFromText: t => Math.round(Number(t) * 10)
                onValueModified: root.patch({ gainDb: value / 10 })
            }
            Label { text: "dB" }
        }
    }

    Heading { text: qsTr("Files and Spotify") }
    Repeater {
        model: root.r.sources
        delegate: RowLayout {
            id: source
            required property var modelData
            Layout.fillWidth: true
            readonly property var paths: modelData.kind === "file" ? (root.r.filePaths[modelData.key] || []) : []
            Label { text: modelData.kind === "spotify" ? "Spotify" : "♪" }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                wrapMode: paths.length || modelData.kind === "spotify" ? Text.NoWrap : Text.WordWrap
                color: modelData.kind === "file" && !paths.length ? Theme.warn : palette.text
                text: modelData.kind === "spotify" ? modelData.key
                    : paths.length ? paths.map(p => p.split(/[\/\\]/).pop()).join(", ")
                    : qsTr("File not found on this computer (content id %1…). Add its folder to your library to relink it.").arg(modelData.key.slice(0, 10))
                ToolTip.visible: srcHover.hovered && paths.length > 0
                ToolTip.text: paths.join("\n")
                HoverHandler { id: srcHover }
            }
            Button {
                visible: root.r.sources.length > 1
                flat: true
                text: qsTr("Split off…")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("This is different audio: give it its own (undoes Combine as the same recording)")
                onClicked: splitMenu.popup()
                Menu {
                    id: splitMenu
                    MenuItem { text: qsTr("Split Off as a Song of Its Own"); onTriggered: root.split(modelData, [], true) }
                    Instantiator {
                        model: root.versions
                        delegate: MenuItem {
                            required property var modelData
                            text: qsTr("Split Off for Version “%1”").arg(modelData.name)
                            onTriggered: root.split(source.modelData, [modelData.id], false)
                        }
                        onObjectAdded: (i, o) => splitMenu.insertItem(i + 1, o)
                        onObjectRemoved: (i, o) => splitMenu.removeItem(o)
                    }
                }
            }
            ToolButton {
                visible: root.r.sources.length > 1
                icon.source: Theme.icon("x")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Remove this source")
                onClicked: Session.run("source_remove", { recordingId: root.r.id, source: modelData })
            }
        }
    }
    RowLayout {
        visible: !root.r.sources.some(s => s.kind === "spotify")
        Layout.fillWidth: true
        TextField {
            id: link
            Layout.fillWidth: true
            placeholderText: qsTr("Link a Spotify track (URL or URI) of this audio")
        }
        Button {
            text: qsTr("Link")
            enabled: link.text.trim().length > 0
            onClicked: Session.run("source_link_spotify", { recordingId: root.r.id, uri: link.text }).then(x => { if (x) link.text = ""; })
        }
        Button {
            visible: !!Session.spotify && Session.spotify.connected
            text: qsTr("Find")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Search Spotify (by ISRC, or title and artist) and link the track that sounds the same")
            onClicked: Session.run("recordings_find_spotify", { ids: [root.r.id] }).then(x => { if (x !== undefined) Session.toast(qsTr("Looking on Spotify…"), "info"); })
        }
    }
    RowLayout {
        visible: root.r.sources.some(s => s.kind === "spotify") && root.r.sources.some(s => s.kind === "file")
        Label { text: qsTr("Play from") }
        ComboBox {
            model: [qsTr("The file"), qsTr("Spotify")]
            currentIndex: root.r.preferredSource === "spotify" ? 1 : 0
            onActivated: index => root.patch({ preferredSource: index === 1 ? "spotify" : null })
        }
    }
    RowLayout {
        visible: root.r.files.length > 0
        Label { visible: root.audio.stemsReady; text: qsTr("Stems ready") }
        Button { visible: root.audio.stemsReady; flat: true; text: qsTr("Delete stems"); onClicked: Session.run("stems_delete", { recordingId: root.r.id }) }
        Label {
            visible: !root.audio.stemsReady && !!root.job && root.job.state !== "failed"
            text: root.job && root.job.state === "running" ? qsTr("Preparing stems… %1%").arg(Math.round(root.job.percent)) : qsTr("Preparing stems… (queued)")
        }
        Button {
            visible: !root.audio.stemsReady && !(root.job && root.job.state !== "failed")
            text: qsTr("Prepare Stems")
            onClicked: Session.run("stems_prepare", { recordingIds: [root.r.id] })
        }
        Label { visible: !!root.job && root.job.state === "failed"; text: root.job ? root.job.message || "" : ""; color: Theme.warn }
    }
    RowLayout {
        Label { text: root.r.analyzed ? qsTr("Analysed") : qsTr("Not analysed yet"); opacity: 0.7 }
        Button { flat: true; text: qsTr("Re-analyse"); onClicked: Session.run("recordings_analyze", { ids: [root.r.id] }) }
        Button {
            flat: true
            text: qsTr("Make a Song from It")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("A new song (in your library) playing this audio, e.g. for another line dance")
            onClicked: Session.run("song_from_recording", { id: root.r.id }).then(t => { if (t) Session.inspectTrack = t.id; })
        }
    }
    RowLayout {
        Label { text: qsTr("For a club file:"); opacity: 0.7 }
        Button { flat: true; text: qsTr("Fingerprint"); enabled: !!root.r.fingerprint; onClicked: Session.copy(root.r.fingerprint, qsTr("Fingerprint")) }
        Button {
            visible: root.r.sources.some(s => s.kind === "file")
            flat: true
            text: qsTr("Hash")
            onClicked: Session.copy(root.r.sources.find(s => s.kind === "file").key, qsTr("File hash"))
        }
    }
    Label {
        visible: root.audio.clubSuggests.length > 0
        text: qsTr("Suggested by the club for %1").arg(root.audio.clubSuggests.join(", "))
        wrapMode: Text.WordWrap
        Layout.fillWidth: true
        color: Theme.ok
        ToolTip.visible: suggestHover.hovered
        ToolTip.text: qsTr("The club suggests this audio for these styles")
        HoverHandler { id: suggestHover }
    }
}
