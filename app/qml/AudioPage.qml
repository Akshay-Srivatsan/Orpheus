import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Audio")
    intro: qsTr("The crossfader decides which deck the room hears; the headphone (cue) output plays whichever decks have their headphone button on. With a DJ controller such as the Numark DJ2GO2, choose it for both and use channels 1–2 for the room and 3–4 for headphones.")

    readonly property var s: Session.settings
    property var devices: []
    Component.onCompleted: Backend.call("audio_devices").then(d => devices = d)

    function setAudio(patch) { Session.saveSettings({ audio: Object.assign({}, s.audio, patch) }); }
    /// Stereo channel pairs a device offers: [0,1], [2,3], …
    function pairs(channels) {
        const out = [];
        for (let i = 0; i + 1 < Math.max(2, channels); i += 2) out.push([i, i + 1]);
        return out;
    }
    readonly property var masterDevice: s ? devices.find(d => d.id === s.audio.masterDevice) || devices.find(d => d.isDefault) : null
    readonly property var cueDevice: s ? devices.find(d => d.id === s.audio.cueDevice) : null

    GridLayout {
        visible: !!page.s
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Room (master)") }
        RowLayout {
            ComboBox {
                Layout.preferredWidth: 300
                model: [qsTr("System default")].concat(page.devices.map(d => d.name + (d.isDefault ? qsTr(" (default)") : "")))
                currentIndex: page.s ? page.devices.findIndex(d => d.id === page.s.audio.masterDevice) + 1 : 0
                onActivated: index => page.setAudio({ masterDevice: index === 0 ? null : page.devices[index - 1].id })
            }
            ComboBox {
                implicitContentWidthPolicy: ComboBox.WidestText
                readonly property var choices: page.pairs(page.masterDevice ? page.masterDevice.channels : 2)
                model: choices.map(p => qsTr("Channels %1–%2").arg(p[0] + 1).arg(p[1] + 1))
                currentIndex: page.s ? Math.max(0, choices.findIndex(p => p[0] === page.s.audio.masterChannels[0])) : 0
                onActivated: index => page.setAudio({ masterChannels: choices[index] })
            }
            Button {
                text: qsTr("Test")
                enabled: !Session.decks.some(d => d && d.audible)
                onClicked: Session.run("audio_test", { deck: Session.liveDeck })
            }
        }

        FormLabel { text: qsTr("Headphones (cue)") }
        RowLayout {
            ComboBox {
                Layout.preferredWidth: 300
                model: [qsTr("None")].concat(page.devices.map(d => d.name))
                currentIndex: page.s ? page.devices.findIndex(d => d.id === page.s.audio.cueDevice) + 1 : 0
                onActivated: index => page.setAudio({ cueDevice: index === 0 ? null : page.devices[index - 1].id })
            }
            ComboBox {
                implicitContentWidthPolicy: ComboBox.WidestText
                visible: !!page.s && !!page.s.audio.cueDevice
                readonly property var choices: page.pairs(page.cueDevice ? page.cueDevice.channels : 2)
                model: choices.map(p => qsTr("Channels %1–%2").arg(p[0] + 1).arg(p[1] + 1))
                currentIndex: page.s ? Math.max(0, choices.findIndex(p => p[0] === page.s.audio.cueChannels[0])) : 0
                onActivated: index => page.setAudio({ cueChannels: choices[index] })
            }
            Button {
                visible: !!page.s && !!page.s.audio.cueDevice
                text: qsTr("Test")
                enabled: !(Session.decks[Session.freeDeck] && Session.decks[Session.freeDeck].audible)
                onClicked: Session.run("audio_test", { deck: Session.freeDeck })
            }
        }
        Item { visible: !!Session.outputs && Session.outputs.cueOnMasterChannels; width: 1 }
        Label {
            visible: !!Session.outputs && Session.outputs.cueOnMasterChannels
            color: Theme.warn
            text: qsTr("⚠ Headphones use the same channels as the room: previews will be heard by the dancers.")
        }

        FormLabel { text: qsTr("Buffer") }
        RowLayout {
            SpinBox { from: 20; to: 500; stepSize: 10; value: page.s ? page.s.audio.bufferMs : 100; onValueModified: page.setAudio({ bufferMs: value }) }
            Label { text: qsTr("ms (raise if you hear dropouts)"); opacity: 0.7 }
        }

        FormLabel { text: qsTr("Status"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Label {
                visible: !!Session.outputs && !!Session.outputs.master
                text: Session.outputs && Session.outputs.master ? qsTr("Room: %1 · %2 Hz · %3 ch").arg(Session.outputs.master.device).arg(Session.outputs.master.sampleRate).arg(Session.outputs.master.channels) : ""
            }
            Label {
                visible: !!Session.outputs && !!Session.outputs.cue
                text: Session.outputs && Session.outputs.cue ? qsTr("Headphones: %1 · %2 Hz").arg(Session.outputs.cue.device).arg(Session.outputs.cue.sampleRate) : ""
            }
            Repeater {
                model: Session.outputs ? Session.outputs.errors : []
                delegate: Label { required property string modelData; text: modelData; color: Theme.error }
            }
            Button { text: qsTr("Reconnect Devices"); onClicked: Session.run("audio_reopen").then(o => { if (o) Session.outputs = o; }) }
        }
    }
}
