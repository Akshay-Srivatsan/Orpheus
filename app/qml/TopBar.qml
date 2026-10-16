import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The window's top: the club, the time (and the set's), projector
// announcements, the room and headphone volumes, and what's connected.
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

    function hm(ms) {
        const m = Math.floor(ms / 60000);
        return Math.floor(m / 60) + ":" + String(m % 60).padStart(2, "0");
    }

    RowLayout {
        anchors.fill: parent
        spacing: Theme.spacing

        Label { text: "Orpheus"; font.bold: true; font.pixelSize: 15 }
        Label {
            text: Session.activeClub ? Session.activeClub.label : ""
            opacity: 0.7
            ToolTip.visible: clubHover.hovered && !!Session.activeClub
            ToolTip.text: qsTr("Club guidelines for this set (styles, tempos, what a set should contain)")
            HoverHandler { id: clubHover }
        }
        Item { Layout.fillWidth: true }
        Label { text: new Date(root.now).toLocaleTimeString(Qt.locale(), Locale.ShortFormat); font.bold: true; font.pixelSize: 15 }
        Label {
            visible: !!root.startedAt
            text: qsTr("Set %1").arg(root.hm(root.now - root.startedAt)) + (root.setMinutes ? qsTr(" of %1").arg(root.hm(root.setMinutes * 60000)) : "")
            color: root.setMinutes && root.now - root.startedAt > root.setMinutes * 60000 ? Theme.warn : palette.text
            opacity: 0.8
        }
        Item { Layout.fillWidth: true }

        Label {
            visible: !!Session.scan && Session.scan.running
            text: Session.scan ? Session.scan.phase + (Session.scan.total ? " " + Session.scan.done + "/" + Session.scan.total : "") : ""
            opacity: 0.7
        }

        ToolButton {
            text: qsTr("Announce")
            checked: !!root.p && !!root.p.message
            onClicked: announceMenu.popup()
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Show an announcement on the projector")
            Menu {
                id: announceMenu
                MenuItem {
                    visible: !!root.p && !!root.p.message
                    height: visible ? implicitHeight : 0
                    text: qsTr("Clear “%1”").arg(root.p && root.p.message ? root.p.message : "")
                    onTriggered: Session.run("projector_message", { message: null })
                }
                Repeater {
                    model: Session.activeClub ? Session.activeClub.announcements : []
                    delegate: MenuItem {
                        required property var modelData
                        text: modelData
                        onTriggered: Session.run("projector_message", { message: modelData })
                    }
                }
                MenuSeparator {}
                MenuItem { text: qsTr("Custom Message…"); onTriggered: customMessage.open() }
            }
        }
        ToolButton {
            text: qsTr("Projector")
            onClicked: projectorMenu.popup()
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Projector display for the dancers")
            Menu {
                id: projectorMenu
                popupType: Popup.Native
                Repeater {
                    model: Qt.application.screens
                    delegate: MenuItem {
                        required property var modelData
                        required property int index
                        text: qsTr("Full Screen on %1 (%2×%3)").arg(modelData.name).arg(modelData.width).arg(modelData.height)
                        onTriggered: root.openProjector(index, true)
                    }
                }
                MenuSeparator {}
                MenuItem { text: qsTr("Open in a Window"); onTriggered: root.openProjector(-1, false) }
                MenuItem { text: qsTr("Close Projector"); onTriggered: root.closeProjector() }
                MenuItem { text: qsTr("Projector Settings…"); onTriggered: Session.settingsPage = "projector" }
            }
        }

        // Volumes: the room, then the headphones (slider positions are square roots of the gain).
        Label { text: "🔈"; ToolTip.visible: masterHover.hovered; ToolTip.text: qsTr("Room volume"); HoverHandler { id: masterHover } }
        Meter { Layout.preferredWidth: 40; value: Session.levels ? Math.min(1, Math.sqrt(Session.levels.master)) : 0 }
        Slider {
            Layout.preferredWidth: 90
            from: 0; to: 1; stepSize: 0.01
            value: Session.levels ? Math.sqrt(Session.levels.masterVolume) : 0
            onMoved: Session.run("levels_set", { master: value * value, cue: null, crossfader: null })
        }
        Label { text: "🎧"; ToolTip.visible: cueHover.hovered; ToolTip.text: qsTr("Headphone volume"); HoverHandler { id: cueHover } }
        Meter { Layout.preferredWidth: 40; value: Session.levels ? Math.min(1, Math.sqrt(Session.levels.cue)) : 0 }
        Slider {
            Layout.preferredWidth: 90
            from: 0; to: 1; stepSize: 0.01
            value: Session.levels ? Math.sqrt(Session.levels.cueVolume) : 0
            onMoved: Session.run("levels_set", { master: null, cue: value * value, crossfader: null })
        }

        StatusButton {
            text: qsTr("Audio")
            status: root.audioOk ? "ok" : "bad"
            tip: root.audioOk ? qsTr("Audio: %1").arg(Session.outputs.master.device) : (Session.outputs && Session.outputs.errors.length ? Session.outputs.errors.join("\n") : qsTr("No audio output"))
            onClicked: Session.settingsPage = "audio"
        }
        StatusButton {
            text: qsTr("MIDI")
            status: root.midiConnected.length ? "ok" : ""
            tip: root.midiConnected.length ? root.midiConnected.map(m => m.name).join("\n") : qsTr("No MIDI controller connected")
            onClicked: Session.settingsPage = "midi"
        }
        StatusButton {
            text: qsTr("Spotify")
            status: Session.spotify && Session.spotify.connected ? "ok" : Session.spotify && Session.spotify.connecting ? "warn" : ""
            tip: Session.spotify && Session.spotify.connected ? qsTr("Spotify: %1").arg(Session.spotify.username) : qsTr("Spotify not connected")
            onClicked: Session.settingsPage = "spotify"
        }
        ToolButton {
            icon.name: "configure"
            icon.source: Theme.icon("gear")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Settings (Ctrl+,)")
            onClicked: Session.settingsPage = "general"
        }
    }

    Dialog {
        id: customMessage
        title: qsTr("Show on the projector")
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: Overlay.overlay
        modal: true
        TextField { id: messageField; width: 320; placeholderText: qsTr("Custom message"); onAccepted: customMessage.accept() }
        onOpened: { messageField.text = ""; messageField.forceActiveFocus(); }
        onAccepted: if (messageField.text.trim()) Session.run("projector_message", { message: messageField.text.trim() })
    }
}
