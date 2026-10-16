import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Controllers")

    readonly property var s: Session.settings
    property var midi: null
    /// The control being learned (-1: a new one; null: none).
    property var learning: null
    Component.onCompleted: load()
    function load() { Backend.call("midi_info").then(m => midi = m); }

    readonly property var mappings: midi ? midi.mappings : []
    /// The mapping in use; with automatic selection, the one chosen for the connected controller.
    readonly property var mapping: s ? mappings.find(m => m.id === (s.midi.mappingId || (midi ? midi.activeMapping : ""))) || mappings[0] : null
    readonly property var kinds: ["button", "hold", "absolute", "relative"]

    function hex(n) { return "0x" + n.toString(16).toUpperCase().padStart(2, "0"); }
    function setMidi(patch) { Session.saveSettings({ midi: Object.assign({}, s.midi, patch) }); }
    function setControl(index, control) {
        Session.run("midi_control_set", { mappingId: mapping.id, index: index, control: control }).then(() => page.load());
    }
    function learn(index) {
        learning = index === null ? -1 : index;
        Session.run("midi_learn").then(msg => {
            learning = null;
            if (!msg) { Session.toast(qsTr("No MIDI message received"), "info"); return; }
            const base = index === null ? { action: "perform.playNext", invert: false } : mapping.controls[index];
            setControl(index, Object.assign({}, base, { status: msg.status, data1: msg.data1, kind: index === null ? msg.kind : base.kind }));
        });
    }

    GridLayout {
        visible: !!page.s
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        Label { text: qsTr("MIDI") }
        CheckBox { text: qsTr("Use MIDI controllers"); checked: !!page.s && page.s.midi.enabled; onToggled: page.setMidi({ enabled: checked }) }

        Label { text: qsTr("Connected"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Repeater {
                model: page.midi ? page.midi.ports : []
                delegate: StatusButton { required property var modelData; text: modelData.name; status: modelData.connected ? "ok" : ""; flat: true; enabled: false }
            }
            Label { visible: !!page.midi && !page.midi.ports.length; text: qsTr("No MIDI devices found. Plug one in; it connects automatically."); opacity: 0.6 }
        }

        Label { text: qsTr("Mapping") }
        RowLayout {
            ComboBox {
                Layout.preferredWidth: 300
                model: [qsTr("Automatic") + (page.s && !page.s.midi.mappingId && page.mapping ? " (" + page.mapping.name + ")" : "")].concat(page.mappings.map(m => m.name))
                currentIndex: page.s ? page.mappings.findIndex(m => m.id === page.s.midi.mappingId) + 1 : 0
                onActivated: index => page.setMidi({ mappingId: index === 0 ? "" : page.mappings[index - 1].id })
            }
            Button {
                visible: !!page.mapping && !page.mapping.builtin && !!page.s && page.s.midi.mappingId === page.mapping.id
                text: qsTr("Delete Mapping")
                onClicked: Session.confirm(qsTr("Delete the mapping “%1”?").arg(page.mapping.name), qsTr("Delete"),
                    ok => { if (ok) Session.run("midi_mapping_delete", { mappingId: page.mapping.id }).then(() => page.load()); })
            }
        }
    }

    Label {
        visible: !!page.mapping
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        font.pixelSize: 11
        opacity: 0.7
        text: qsTr("Left = deck A, right = deck B. SYNC = Go Live (on the deck the room hears: fade out). Pads: Cues mode = hot cues (hold PAD MODE to clear), Sampler mode = stems (vocals / drums / bass / other). Tempo faders use soft takeover; the crossfader always follows the hardware.")
            + (page.mapping && page.mapping.builtin ? " " + qsTr("Editing a built-in mapping creates a custom copy.") : "")
    }

    Repeater {
        model: page.mapping ? page.mapping.controls : []
        delegate: RowLayout {
            required property var modelData
            required property int index
            Layout.fillWidth: true
            Label { Layout.preferredWidth: 90; text: page.hex(modelData.status) + " " + page.hex(modelData.data1); font.family: "monospace" }
            ComboBox {
                model: [qsTr("Button"), qsTr("Button (press & release)"), qsTr("Fader/knob"), qsTr("Encoder/jog")]
                currentIndex: Math.max(0, page.kinds.indexOf(modelData.kind))
                onActivated: i => page.setControl(index, Object.assign({}, modelData, { kind: page.kinds[i] }))
            }
            CheckBox { text: qsTr("invert"); checked: !!modelData.invert; onToggled: page.setControl(index, Object.assign({}, modelData, { invert: checked })) }
            ComboBox {
                Layout.fillWidth: true
                model: page.midi ? page.midi.actions.map(a => a[1]) : []
                currentIndex: page.midi ? page.midi.actions.findIndex(a => a[0] === modelData.action) : -1
                onActivated: i => page.setControl(index, Object.assign({}, modelData, { action: page.midi.actions[i][0] }))
            }
            Button {
                text: page.learning === index ? qsTr("Move a control…") : qsTr("Learn")
                enabled: page.learning === null
                onClicked: page.learn(index)
            }
            ToolButton {
                icon.source: Theme.icon("x")
                onClicked: Session.run("midi_control_remove", { mappingId: page.mapping.id, index: index }).then(() => page.load())
            }
        }
    }
    RowLayout {
        visible: !!page.mapping
        Button {
            text: page.learning === -1 ? qsTr("Press or move a control on your controller…") : qsTr("Learn New Control")
            enabled: page.learning === null
            onClicked: page.learn(null)
        }
        Button { visible: page.learning !== null; flat: true; text: qsTr("Cancel"); onClicked: Session.run("midi_learn_cancel") }
    }
}
