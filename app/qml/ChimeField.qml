import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Whether a chime plays before the announcement, and which: a version of a
// song (played as it's set: start, end, fade, gain). With `inherits`, each
// can be left to the default (what's chosen above: `inheritedOn`,
// `inheritedSound`); `edited(value)` gives the new { enabled, sound } (a key
// left out = the default). Without, it's your own setting: a check box and
// a version. `versions` ({ id, label }) are offered first (the song's own);
// any other is chosen in the library (`title` names what it's for).
ColumnLayout {
    id: root
    property var value: ({})
    property bool inherits: true
    property bool inheritedOn: false
    property var inheritedSound: null
    property var versions: []
    property string title: ""
    /// The version shows even while it doesn't chime (a set can choose the
    /// version for the songs that do).
    property bool soundAlways: false
    /// Called before choosing in the library, and after (e.g. to make way for it).
    property var beforePicking: null
    property var afterPicking: null
    signal edited(var value)

    readonly property bool chosenOn: value.enabled !== undefined && value.enabled !== null
    readonly property bool chosenSound: !!value.sound
    readonly property bool on: chosenOn ? value.enabled : inheritedOn
    readonly property var sound: chosenSound ? value.sound : inheritedSound

    // Names of the versions chosen here and above.
    property var labels: ({})
    function label(id) { return labels[id] || qsTr("…"); }
    function loadLabels() {
        const ids = [value.sound, inheritedSound].filter(x => !!x && !labels[x]);
        if (ids.length) Backend.call("chime_labels", { versionIds: ids }).then(l => {
            const next = Object.assign({}, labels);
            ids.forEach((id, i) => next[id] = l[i]);
            labels = next;
        });
    }
    onValueChanged: loadLabels()
    onInheritedSoundChanged: loadLabels()
    Component.onCompleted: loadLabels()

    function set(patch) {
        const v = Object.assign({}, value, patch);
        for (const k of Object.keys(v)) if (v[k] === null || v[k] === undefined) delete v[k];
        edited(v);
    }
    function pick() {
        if (beforePicking) beforePicking();
        Session.pickChime(title, id => { root.set({ sound: id }); if (root.afterPicking) root.afterPicking(); });
    }

    CheckBox {
        visible: !root.inherits
        text: qsTr("Play a chime before each announcement")
        checked: root.on
        onToggled: root.set({ enabled: checked })
    }
    ComboBox {
        id: onBox
        visible: root.inherits
        readonly property var values: [null, true, false]
        readonly property int chosen: root.chosenOn ? (root.value.enabled ? 1 : 2) : 0
        implicitContentWidthPolicy: ComboBox.WidestText
        model: [root.inheritedOn ? qsTr("Default (chime)") : qsTr("Default (no chime)"), qsTr("Chime"), qsTr("No chime")]
        currentIndex: chosen
        // A new list resets the current item: show the choice again.
        onModelChanged: currentIndex = Qt.binding(() => onBox.chosen)
        onActivated: index => root.set({ enabled: values[index] })
    }
    RowLayout {
        visible: root.on || root.soundAlways
        Layout.fillWidth: true
        ComboBox {
            id: soundBox
            Layout.fillWidth: true
            Layout.minimumWidth: 80
            // The default (if it inherits), the versions offered, the one
            // chosen (if another), and a way to choose any.
            readonly property var others: root.chosenSound && !root.versions.some(v => v.id === root.value.sound) ? [{ id: root.value.sound, label: root.label(root.value.sound) }] : []
            readonly property var items: (root.inherits ? [{ label: root.inheritedSound ? qsTr("Default (%1)").arg(root.label(root.inheritedSound)) : qsTr("Default (none chosen)"), id: null }] : [])
                .concat(root.versions).concat(others)
                .concat([{ label: qsTr("Choose in the Library…"), id: undefined }])
            readonly property int chosen: root.chosenSound ? items.findIndex(i => i.id === root.value.sound) : root.inherits ? 0 : -1
            model: items.map(i => i.label)
            currentIndex: chosen
            onModelChanged: currentIndex = Qt.binding(() => soundBox.chosen)
            displayText: chosen >= 0 ? items[chosen].label : qsTr("No version chosen")
            onActivated: index => {
                const id = items[index].id;
                currentIndex = Qt.binding(() => soundBox.chosen);
                if (id === undefined) root.pick();
                else root.set({ sound: id });
            }
        }
        Button {
            // An icon's width (Fusion's buttons are otherwise at least 80 px).
            implicitWidth: implicitContentWidth + leftPadding + rightPadding
            icon.color: palette.buttonText
            icon.source: Theme.icon("play")
            enabled: !!root.sound
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Play the chime")
            onClicked: Session.run("chime_preview", { versionId: root.sound })
        }
    }
    Label {
        visible: root.on && !root.sound
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.warn
        font.pointSize: Theme.smallPointSize
        text: qsTr("Choose the version of a song to play as the chime (e.g. a short excerpt: set its start and end).")
    }
}
