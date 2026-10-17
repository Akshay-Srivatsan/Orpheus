import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Whether a chime plays before the announcement, and its sound. With
// `inherits`, each can be left to the default (what's chosen above:
// `inheritedOn`, `inheritedSound`); `edited(value)` gives the new
// { enabled, sound } (a key left out = the default). Without, it's your own
// setting: a check box and a sound. The sound shows while it chimes.
ColumnLayout {
    id: root
    property var value: ({})
    property bool inherits: true
    property bool inheritedOn: false
    property string inheritedSound: ""
    /// The sound shows even while it doesn't chime (a set can choose the
    /// sound for the songs that do).
    property bool soundAlways: false
    signal edited(var value)

    readonly property bool chosenOn: value.enabled !== undefined && value.enabled !== null
    readonly property bool chosenSound: value.sound !== undefined && value.sound !== null
    readonly property bool on: chosenOn ? value.enabled : inheritedOn
    readonly property string sound: chosenSound ? value.sound : inheritedSound

    function soundName(s) { return s ? s.replace(/^.*[\\/]/, "") : qsTr("Built-in chime"); }
    function set(patch) {
        const v = Object.assign({}, value, patch);
        for (const k of Object.keys(v)) if (v[k] === null || v[k] === undefined) delete v[k];
        edited(v);
    }
    function chooseSound() {
        Session.chooseFile({ title: qsTr("Choose a chime"), nameFilters: [qsTr("Audio files (*.wav *.flac *.ogg *.opus *.mp3 *.m4a *.aac)"), qsTr("All files (*)")] }, path => { if (path) root.set({ sound: path }); });
    }

    CheckBox {
        visible: !root.inherits
        text: qsTr("Play a chime before each announcement")
        checked: root.on
        onToggled: root.set({ enabled: checked })
    }
    ComboBox {
        visible: root.inherits
        readonly property var values: [null, true, false]
        implicitContentWidthPolicy: ComboBox.WidestText
        model: [root.inheritedOn ? qsTr("Default (chime)") : qsTr("Default (no chime)"), qsTr("Chime"), qsTr("No chime")]
        currentIndex: root.chosenOn ? (root.value.enabled ? 1 : 2) : 0
        onActivated: index => root.set({ enabled: values[index] })
    }
    RowLayout {
        visible: root.on || root.soundAlways
        Layout.fillWidth: true
        ComboBox {
            id: soundBox
            Layout.fillWidth: true
            Layout.minimumWidth: 80
            // The default (if it inherits), the built-in chime, the file chosen
            // here (if any), and a way to choose one.
            readonly property string file: root.chosenSound ? root.value.sound : (!root.inherits ? root.sound : "")
            readonly property var items: (root.inherits ? [{ label: qsTr("Default (%1)").arg(root.soundName(root.inheritedSound)), sound: null }] : [])
                .concat([{ label: qsTr("Built-in chime"), sound: "" }])
                .concat(file ? [{ label: root.soundName(file), sound: file }] : [])
                .concat([{ label: qsTr("Choose File…"), sound: undefined }])
            model: items.map(i => i.label)
            currentIndex: root.inherits && !root.chosenSound ? 0 : items.findIndex(i => i.sound === root.sound)
            onActivated: index => {
                const s = items[index].sound;
                if (s === undefined) {
                    currentIndex = Qt.binding(() => root.inherits && !root.chosenSound ? 0 : items.findIndex(i => i.sound === root.sound));
                    root.chooseSound();
                } else {
                    root.set({ sound: s });
                }
            }
            ToolTip.visible: hovered && !!file
            ToolTip.text: file
        }
        ToolButton {
            icon.source: Theme.icon("play")
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Play the chime")
            onClicked: Session.run("chime_preview", { sound: root.sound })
        }
    }
}
