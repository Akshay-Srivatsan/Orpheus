import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The window's toolbar: the club, the time (and the set's), projector
// announcements, the room and headphone volumes, and what's connected.
// When narrow, buttons show only their icons.
ToolBar {
    id: root

    signal openProjector(int screen, bool fullscreen)
    signal closeProjector()

    property real now: Date.now()
    Timer { interval: 1000; running: true; repeat: true; onTriggered: root.now = Date.now() }

    readonly property var p: Session.perform
    readonly property var startedAt: p ? p.startedAt : null
    readonly property var setMinutes: Session.activeClub ? Session.activeClub.setMinutes : null
    readonly property bool audioOk: !!Session.outputs && !!Session.outputs.master && !Session.outputs.errors.length
    readonly property var midiConnected: Session.midiPorts.filter(m => m.connected)
    readonly property bool narrow: width < 1000
    readonly property int display: narrow ? AbstractButton.IconOnly : AbstractButton.TextBesideIcon

    function hm(ms) {
        const m = Math.floor(ms / 60000);
        return Math.floor(m / 60) + ":" + String(m % 60).padStart(2, "0");
    }

    // A connection's status: a dot in its colour, and its name.
    component Status: ToolButton {
        property string status: ""
        property string tip: ""
        display: root.display
        icon.source: Theme.icon("dot")
        icon.color: status === "ok" ? Theme.ok : status === "warn" ? Theme.warn : status === "bad" ? Theme.error : Theme.neutral
        ToolTip.visible: hovered && !!tip
        ToolTip.text: (root.narrow ? text + ": " : "") + tip
    }
    // A level meter: green, red when clipping.
    component Level: ProgressBar {
        Layout.preferredWidth: 36
        Layout.preferredHeight: 8
        from: 0; to: 1
        palette.highlight: value > 0.985 ? Theme.error : Theme.ok
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing
        spacing: Theme.spacing / 2

        Label {
            Layout.fillWidth: true
            Layout.minimumWidth: 40
            elide: Text.ElideRight
            text: Session.activeClub ? Session.activeClub.label : qsTr("No club")
            opacity: Session.activeClub ? 1 : Theme.dim
            ToolTip.visible: clubHover.hovered && !!Session.activeClub
            ToolTip.text: qsTr("Club guidelines for this set (styles, tempos, what a set should contain)")
            HoverHandler { id: clubHover }
        }
        Label { text: new Date(root.now).toLocaleTimeString(Qt.locale(), Locale.ShortFormat); font.bold: true }
        Label {
            visible: !!root.startedAt
            text: qsTr("Set %1").arg(root.hm(root.now - root.startedAt)) + (root.setMinutes ? qsTr(" of %1").arg(root.hm(root.setMinutes * 60000)) : "")
            color: root.setMinutes && root.now - root.startedAt > root.setMinutes * 60000 ? Theme.warn : Theme.system.text
            opacity: Theme.dim
        }
        Item { Layout.fillWidth: true }

        ToolButton {
            id: announce
            icon.color: palette.buttonText
            display: root.display
            icon.source: Theme.icon("mic")
            text: qsTr("Announce")
            checked: !!root.p && !!root.p.message
            onClicked: announceMenu.popup(announce, 0, announce.height)
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Show an announcement on the projector")
            Menu {
                id: announceMenu
                MenuItem {
                    text: root.p && root.p.message ? qsTr("Clear “%1”").arg(root.p.message) : qsTr("Clear")
                    enabled: !!root.p && !!root.p.message
                    onTriggered: Session.run("projector_message", { message: null })
                }
                MenuSeparator {}
                Instantiator {
                    model: Session.activeClub ? Session.activeClub.announcements : []
                    delegate: MenuItem {
                        required property var modelData
                        text: modelData
                        onTriggered: Session.run("projector_message", { message: modelData })
                    }
                    // After "Clear" and its separator.
                    onObjectAdded: (i, o) => announceMenu.insertItem(i + 2, o)
                    onObjectRemoved: (i, o) => announceMenu.removeItem(o)
                }
                MenuSeparator {}
                MenuItem { text: qsTr("Custom Message…"); onTriggered: customMessage.open() }
            }
        }
        ToolButton {
            id: projectorButton
            icon.color: palette.buttonText
            display: root.display
            icon.source: Theme.icon("projector")
            text: qsTr("Projector")
            onClicked: projectorMenu.popup(projectorButton, 0, projectorButton.height)
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Projector display for the dancers")
            Menu {
                id: projectorMenu
                Instantiator {
                    model: Qt.application.screens
                    delegate: MenuItem {
                        required property var modelData
                        required property int index
                        text: qsTr("Full Screen on %1 (%2×%3)").arg(modelData.name).arg(modelData.width).arg(modelData.height)
                        onTriggered: root.openProjector(index, true)
                    }
                    onObjectAdded: (i, o) => projectorMenu.insertItem(i, o)
                    onObjectRemoved: (i, o) => projectorMenu.removeItem(o)
                }
                MenuSeparator {}
                MenuItem { text: qsTr("Open in a Window"); onTriggered: root.openProjector(-1, false) }
                MenuItem { text: qsTr("Close Projector"); onTriggered: root.closeProjector() }
                MenuSeparator {}
                MenuItem { text: qsTr("Projector Settings…"); onTriggered: Session.settingsPage = "projector" }
            }
        }
        ToolSeparator {}

        // Volumes: the room, then the headphones (slider positions are square roots of the gain).
        Icon { glyph: "speaker"; ToolTip.visible: masterHover.hovered; ToolTip.text: qsTr("Room volume"); HoverHandler { id: masterHover } }
        Slider {
            Layout.fillWidth: true
            Layout.minimumWidth: 50
            Layout.maximumWidth: 110
            from: 0; to: 1; stepSize: 0.01
            value: Session.levels ? Math.sqrt(Session.levels.masterVolume) : 0
            onMoved: Session.run("levels_set", { master: value * value, cue: null, crossfader: null })
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Room volume")
        }
        Level { value: Session.levels ? Math.min(1, Math.sqrt(Session.levels.master)) : 0 }
        // Levelling songs to one loudness (Settings → General has the target).
        ToolButton {
            icon.color: palette.buttonText
            icon.source: Theme.icon("levels")
            checkable: true
            checked: !!Session.settings && Session.settings.autoGain
            ToolTip.visible: hovered
            ToolTip.text: Session.settings ? (checked ? qsTr("Songs are levelled to %1 LUFS (their peaks kept clear): click to play them as they are") : qsTr("Level songs to %1 LUFS (their peaks kept clear)")).arg(Session.settings.targetLoudnessDb) + qsTr(" — the target is in Settings → General") : ""
            onToggled: Session.saveSettings({ autoGain: checked })
        }
        // Evening out dynamics (Settings → General has how strongly).
        ToolButton {
            icon.color: palette.buttonText
            icon.source: Theme.icon("compress")
            checkable: true
            checked: !!Session.settings && Session.settings.compress
            ToolTip.visible: hovered
            ToolTip.text: checked ? qsTr("Songs' dynamics are evened out (quiet passages brought up): click to play them as they are")
                : qsTr("Even out songs' dynamics: bring quiet passages up towards the song's main level") + qsTr(" — how strongly is in Settings → General")
            onToggled: Session.saveSettings({ compress: checked })
        }
        Icon { Layout.leftMargin: Theme.spacing; glyph: "headphones"; ToolTip.visible: cueHover.hovered; ToolTip.text: qsTr("Headphone volume"); HoverHandler { id: cueHover } }
        Slider {
            Layout.fillWidth: true
            Layout.minimumWidth: 50
            Layout.maximumWidth: 110
            from: 0; to: 1; stepSize: 0.01
            value: Session.levels ? Math.sqrt(Session.levels.cueVolume) : 0
            onMoved: Session.run("levels_set", { master: null, cue: value * value, crossfader: null })
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Headphone volume")
        }
        Level { value: Session.levels ? Math.min(1, Math.sqrt(Session.levels.cue)) : 0 }
        ToolSeparator {}

        Status {
            text: qsTr("Audio")
            status: root.audioOk ? "ok" : "bad"
            tip: root.audioOk ? qsTr("Audio: %1").arg(Session.outputs.master.device) : (Session.outputs && Session.outputs.errors.length ? Session.outputs.errors.join("\n") : qsTr("No audio output"))
            onClicked: Session.settingsPage = "audio"
        }
        Status {
            text: qsTr("MIDI")
            status: root.midiConnected.length ? "ok" : ""
            tip: root.midiConnected.length ? root.midiConnected.map(m => m.name).join("\n") : qsTr("No MIDI controller connected")
            onClicked: Session.settingsPage = "midi"
        }
        Status {
            id: spotifyStatus
            text: qsTr("Spotify")
            status: Session.spotify && Session.spotify.connected ? "ok" : Session.spotify && Session.spotify.connecting ? "warn" : ""
            tip: Session.spotify && Session.spotify.connected ? qsTr("Connected as %1").arg(Session.spotify.username) : qsTr("Not connected")
            onClicked: spotifyMenu.popup(spotifyStatus, 0, spotifyStatus.height)
            Menu {
                id: spotifyMenu
                MenuItem {
                    text: qsTr("Refresh the Session")
                    enabled: !!Session.spotify && Session.spotify.hasCachedCredentials
                    onTriggered: Session.run("spotify_refresh").then(r => { if (r) Session.toast(qsTr("Spotify session refreshed."), "info"); })
                }
                MenuItem { text: qsTr("Spotify Settings…"); onTriggered: Session.settingsPage = "spotify" }
            }
        }
        ToolButton {
            icon.color: palette.buttonText
            icon.source: Theme.icon("gear")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Settings (%1)").arg(Qt.platform.os === "osx" ? "⌘," : "Ctrl+,")
            onClicked: Session.settingsPage = "general"
        }
    }

    AppDialog {
        id: customMessage
        title: qsTr("Show on the Projector")
        standardButtons: Dialog.Ok | Dialog.Cancel
        TextField { id: messageField; implicitWidth: 320; placeholderText: qsTr("Custom message"); onAccepted: customMessage.accept() }
        onOpened: { messageField.text = ""; messageField.forceActiveFocus(); }
        onAccepted: if (messageField.text.trim()) Session.run("projector_message", { message: messageField.text.trim() })
    }
}
