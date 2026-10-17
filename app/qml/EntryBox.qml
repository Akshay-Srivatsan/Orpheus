import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A set entry's own settings (`view` from `song_view`): which version plays,
// what it's shown as, overrides for this set, and auto mode.
GroupBox {
    id: root
    title: pause ? qsTr("Pause in This Set") : qsTr("In This Set")

    property var view
    /// The version's own target tempo (what the entry inherits).
    property var versionTargetBpm: null
    readonly property var e: view.entry
    readonly property bool pause: view.isPause
    readonly property bool anyStyle: !(Session.set && Session.set.club) && view.showAs.length === 0

    function update(p) { return Session.run("entry_update", { id: e.id, patch: p }); }
    function override(p) {
        const next = Object.assign({}, e.overrides || {}, p);
        for (const k of Object.keys(next)) if (next[k] === null || next[k] === undefined) delete next[k];
        update({ overrides: next });
    }
    function toggleShown(key) {
        const shown = view.showAs.filter(x => x.on).map(x => x.key);
        const on = shown.includes(key);
        if (Session.set && Session.set.club) {
            const next = on ? shown.filter(k => k !== key) : shown.concat([key]);
            update({ clubStyles: next.length ? next : null });
        } else {
            const all = view.showAs.map(x => x.key);
            const next = all.filter(k => k === key ? !on : shown.includes(k));
            update({ styles: next.length === all.length || !next.length ? null : next });
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 4

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Theme.spacing
            rowSpacing: 4

            FormLabel { visible: !root.pause && root.view.versionChoices.length > 1; text: qsTr("Version") }
            ComboBox {
                visible: !root.pause && root.view.versionChoices.length > 1
                Layout.fillWidth: true
                id: versionBox
                model: root.view.versionChoices.map(c => c.label)
                // The entry's version (a new list would otherwise reset it to the first).
                readonly property int chosen: root.view.versionChoices.findIndex(c => c.id === root.e.versionId)
                currentIndex: chosen
                onModelChanged: currentIndex = Qt.binding(() => versionBox.chosen)
                displayText: chosen >= 0 ? root.view.versionChoices[chosen].label : ""
                onActivated: index => Session.run("entry_switch_version", { entryId: root.e.id, versionId: root.view.versionChoices[index].id })
            }

            FormLabel { visible: root.pause; text: qsTr("Name") }
            TextField {
                visible: root.pause
                Layout.fillWidth: true
                text: root.e.label || ""
                placeholderText: qsTr("Pause (e.g. Break, Lesson)")
                onEditingFinished: root.update({ label: text.trim() || null })
            }

            FormLabel { visible: !root.pause; text: root.e.versionId ? qsTr("Show as") : qsTr("Slot") }
            // Without a club, a slot (or a version without styles) takes any of your styles.
            StylePicker {
                visible: !root.pause && root.anyStyle
                Layout.fillWidth: true
                value: root.e.styles || []
                onEdited: keys => root.update({ styles: keys.length ? keys : null })
            }
            Flow {
                visible: !root.pause && !root.anyStyle
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: root.view.showAs.filter(x => x.fits || x.on)
                    delegate: Button {
                        required property var modelData
                        text: modelData.name
                        checkable: true
                        checked: modelData.on
                        implicitHeight: 24
                        onClicked: root.toggleShown(modelData.key)
                    }
                }
                Button {
                    visible: root.view.showAs.some(x => !x.fits && !x.on)
                    icon.source: Theme.icon("plus")
                    icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                    implicitHeight: 24
                    implicitWidth: 32
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Any of the club's styles, even one this song doesn't fit")
                    onClicked: otherStyles.popup()
                    Menu {
                        id: otherStyles
                        Instantiator {
                            model: root.view.showAs.filter(x => !x.fits && !x.on)
                            delegate: MenuItem {
                                required property var modelData
                                text: qsTr("%1 (doesn't fit)").arg(modelData.name)
                                onTriggered: root.toggleShown(modelData.key)
                            }
                            onObjectAdded: (i, o) => otherStyles.insertItem(i, o)
                            onObjectRemoved: (i, o) => otherStyles.removeItem(o)
                        }
                    }
                }
                Button {
                    visible: !!root.e.clubStyles || !!root.e.styles
                    flat: true
                    text: qsTr("Automatic")
                    implicitHeight: 24
                    onClicked: root.update(root.e.clubStyles ? { clubStyles: null } : { styles: null })
                }
            }

            FormLabel { visible: !root.pause; text: qsTr("Label") }
            TextField {
                visible: !root.pause
                Layout.fillWidth: true
                text: root.e.label || ""
                placeholderText: root.view.shownNames || qsTr("e.g. Mixer")
                onEditingFinished: if (text.trim() !== (root.e.label || "")) root.update({ label: text.trim() || null })
            }

            FormLabel { visible: !root.pause; text: qsTr("Tempo") }
            RowLayout {
                visible: !root.pause
                TextField {
                    implicitWidth: 80
                    text: root.e.overrides && root.e.overrides.targetBpm ? String(root.e.overrides.targetBpm) : ""
                    placeholderText: root.versionTargetBpm ? Session.fmtBpm(root.versionTargetBpm) : qsTr("orig.")
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: root.override({ targetBpm: text ? Number(text) : null })
                }
                Label { text: "BPM" }
                Label {
                    visible: !!root.view.playback && !!root.view.playback.target
                    opacity: 0.6
                    text: root.view.playback && root.view.playback.target ? "→ " + Session.fmtBpm(root.view.playback.target) + " (" + ((root.view.playback.rate - 1) * 100).toFixed(1) + "%)" : ""
                }
            }
            FormLabel { visible: !root.pause; text: qsTr("Start / end") }
            RowLayout {
                visible: !root.pause
                TimeField { value: root.e.overrides ? root.e.overrides.cueInMs : null; placeholderText: qsTr("inherit"); onEdited: ms => root.override({ cueInMs: ms }) }
                TimeField { value: root.e.overrides ? root.e.overrides.cueOutMs : null; placeholderText: qsTr("inherit"); onEdited: ms => root.override({ cueOutMs: ms }) }
            }
            FormLabel { visible: !root.pause; text: qsTr("Fade at") }
            TimeField { visible: !root.pause; value: root.e.overrides ? root.e.overrides.fadeOutMs : null; placeholderText: qsTr("inherit"); onEdited: ms => root.override({ fadeOutMs: ms }) }
            FormLabel { visible: !root.pause; text: qsTr("Gain") }
            RowLayout {
                visible: !root.pause
                TextField {
                    implicitWidth: 80
                    text: root.e.overrides && root.e.overrides.gainDb != null ? String(root.e.overrides.gainDb) : ""
                    placeholderText: qsTr("inherit")
                    validator: DoubleValidator {}
                    onEditingFinished: root.override({ gainDb: text ? Number(text) : null })
                }
                Label { text: "dB" }
            }

            FormLabel { text: qsTr("Note") }
            TextField {
                Layout.fillWidth: true
                text: root.e.note || ""
                placeholderText: qsTr("e.g. announce the jam")
                onEditingFinished: if (text !== (root.e.note || "")) root.update({ note: text })
            }
            FormLabel { visible: root.view.tags.length > 0; text: qsTr("Tags") }
            Flow {
                visible: root.view.tags.length > 0
                Layout.fillWidth: true
                spacing: 4
                Repeater {
                    model: root.view.tags
                    delegate: Button {
                        required property var modelData
                        text: modelData.name
                        checkable: true
                        checked: modelData.on
                        implicitHeight: 24
                        ToolTip.visible: hovered && !!modelData.note
                        ToolTip.text: modelData.note
                        onClicked: {
                            const tags = root.e.tags || [];
                            root.update({ tags: modelData.on ? tags.filter(t => t !== modelData.key) : tags.concat([modelData.key]) });
                        }
                    }
                }
            }
        }

        Heading { text: qsTr("Auto Mode") }
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Theme.spacing
            rowSpacing: 4

            FormLabel { text: root.pause ? qsTr("Length") : qsTr("Pause before") }
            RowLayout {
                TextField {
                    implicitWidth: 70
                    text: root.e.pauseBefore != null ? String(root.e.pauseBefore) : ""
                    placeholderText: String(root.view.defaultPauseBefore)
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: root.update({ pauseBefore: text === "" ? null : Number(text) })
                }
                Label { text: "s" }
            }
            FormLabel { text: qsTr("Announce") }
            AnnouncementField {
                Layout.fillWidth: true
                pause: root.pause
                value: root.e.announcement === undefined ? null : root.e.announcement
                defaultLabel: Session.announcementDefault(root.view.inherited.announcement)
                onEdited: v => root.update({ announcement: v })
            }
            FormLabel { visible: root.view.speaks; text: qsTr("Chime") }
            ChimeField {
                visible: root.view.speaks
                Layout.fillWidth: true
                value: root.e.chime || ({})
                inheritedOn: root.view.inherited.chime
                inheritedSound: root.view.inherited.chimeSound
                onEdited: v => root.update({ chime: v })
            }
            FormLabel { visible: root.view.speaks; text: qsTr("Then pause") }
            RowLayout {
                visible: root.view.speaks
                TextField {
                    implicitWidth: 70
                    text: root.e.pauseAfter != null ? String(root.e.pauseAfter) : ""
                    placeholderText: String(root.view.defaultPauseAfter)
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: root.update({ pauseAfter: text === "" ? null : Number(text) })
                }
                Label { text: "s" }
            }
            Item { implicitWidth: 1 }
            CheckBox {
                text: root.pause ? qsTr("Stop before this pause") : qsTr("Stop before this song")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Auto mode waits here: start it with Play Next")
                checked: !!root.e.hold
                onToggled: root.update({ hold: checked })
            }
        }
        Button {
            visible: !!root.e.overrides && Object.keys(root.e.overrides).length > 0
            flat: true
            text: qsTr("Clear overrides")
            onClicked: root.update({ overrides: {} })
        }
    }
}
