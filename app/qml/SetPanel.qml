import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The open set: its entries in order (songs, style slots and pauses), what's
// playing and what's next, and how it follows its club's guidelines.
Pane {
    id: root
    padding: 0

    signal newSetlist()

    readonly property var v: Session.set
    readonly property var set: v ? v.setlist : null
    readonly property var p: Session.perform
    property bool showCheck: false
    property bool renaming: false

    function select(entry, modifiers) {
        const sel = Session.selectedEntries;
        if (modifiers & (Qt.ControlModifier | Qt.MetaModifier)) {
            Session.selectedEntries = sel.includes(entry.id) ? sel.filter(x => x !== entry.id) : sel.concat([entry.id]);
        } else if ((modifiers & Qt.ShiftModifier) && sel.length) {
            const ids = set.entries.map(e => e.id);
            const a = ids.indexOf(sel[0]), b = ids.indexOf(entry.id);
            Session.selectedEntries = ids.slice(Math.min(a, b), Math.max(a, b) + 1);
        } else {
            Session.selectedEntries = [entry.id];
        }
        Session.inspectEntry = entry.id;
        const row = v.rows.find(r => r.entry.id === entry.id);
        Session.inspectTrack = row && row.trackId ? row.trackId : "";
        // Selecting an empty slot starts choosing its song.
        if (!entry.versionId && entry.kind !== "pause") Session.startFillingSlot(entry);
    }

    /// Double-click: load onto the free deck (paused); on a slot, choose its song.
    function choose(entry) {
        if (!entry.versionId) { Session.startFillingSlot(entry); return; }
        Session.run("deck_load", { deck: Session.freeDeck, versionId: entry.versionId, entryId: entry.id, play: false });
    }

    function remove(ids) {
        if (Session.decks.some(d => d && d.audible && d.entryId && ids.includes(d.entryId))) {
            Session.toast(qsTr("That song is playing right now."));
            return;
        }
        Session.run("entries_delete", { setlistId: set.id, ids: ids });
        Session.selectedEntries = [];
    }

    function insertSongs(ids, at) {
        Session.run("entries_insert", { setlistId: set.id, position: at, entries: ids.map(id => ({ versionId: id })) });
    }

    function moveEntries(ids, at) {
        const order = set.entries.map(e => e.id);
        const before = order.slice(0, at).filter(id => !ids.includes(id));
        const after = order.slice(at).filter(id => !ids.includes(id));
        Session.run("entries_reorder", { setlistId: set.id, ids: before.concat(ids, after) });
    }

    function addSlot(style) {
        const club = root.v && root.v.club;
        Session.run("entries_insert", { setlistId: set.id, position: null, entries: [club ? { clubStyles: [style] } : { styles: [style] }] });
    }

    function addPause() {
        Session.run("entries_insert", { setlistId: set.id, position: null, entries: [{ kind: "pause", pauseBefore: 30 }] }).then(ids => {
            if (!ids || !ids[0]) return;
            Session.selectedEntries = [ids[0]];
            Session.inspectEntry = ids[0];
            Session.inspectTrack = "";
            Session.rightTab = "info";
        });
    }

    function exportSet() {
        Session.chooseFile({ title: qsTr("Export “%1”").arg(set.name), nameFilters: [qsTr("Orpheus set (*.toml)")], save: true, name: set.name + ".orpheus-set.toml" }, path => {
            Session.run("setlist_export", { id: set.id, path: path }).then(r => { if (r !== undefined) Session.toast(qsTr("Exported “%1”.").arg(set.name), "info"); });
        });
    }
    function exportCsv() {
        Session.chooseFile({ title: qsTr("Export “%1” as CSV").arg(set.name), nameFilters: [qsTr("CSV (*.csv)")], save: true, name: set.name + ".csv" }, path => {
            Session.run("setlist_export_csv", { id: set.id, path: path });
        });
    }

    function printSet() {
        Session.run("setlist_print", { id: set.id }).then(path => {
            if (!path) return;
            Qt.openUrlExternally("file://" + path);
            Session.toast(qsTr("Opened “%1” in your browser to print.").arg(set.name), "info");
        });
    }

    // Delete, or Backspace (the Mac's Delete key), removes the selected entries.
    Keys.onPressed: event => {
        if ((event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) && Session.selectedEntries.length) {
            remove(Session.selectedEntries);
            event.accepted = true;
        }
    }

    // No set open.
    ColumnLayout {
        visible: !root.set
        anchors.centerIn: parent
        width: parent.width - Theme.spacing * 4
        spacing: Theme.spacing
        Label { Layout.alignment: Qt.AlignHCenter; text: qsTr("No set open"); font.bold: true }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            opacity: Theme.dim
            text: qsTr("Live DJing: press + on a song (or Play Next) — a live set is created for you.\nPreset DJing: plan a setlist ahead of time.")
        }
        Button { Layout.alignment: Qt.AlignHCenter; highlighted: true; text: qsTr("New Setlist…"); onClicked: root.newSetlist() }
        Heading { visible: Session.setlists.length > 0; text: qsTr("Recent") }
        Repeater {
            model: Session.setlists.slice(0, 6)
            delegate: ItemDelegate {
                required property var modelData
                Layout.fillWidth: true
                text: modelData.name + "  (" + modelData.entryCount + ")"
                onClicked: Session.run("perform_open", { setlistId: modelData.id })
            }
        }
    }

    ColumnLayout {
        visible: !!root.set
        anchors.fill: parent
        spacing: 0

        // Name and actions.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            Layout.bottomMargin: 4
            Loader {
                Layout.fillWidth: true
                sourceComponent: root.renaming ? nameField : nameLabel
                Component {
                    id: nameLabel
                    Label {
                        text: root.set ? root.set.name : ""
                        font.bold: true
                        font.pointSize: Theme.titlePointSize
                        elide: Text.ElideRight
                        TapHandler { onDoubleTapped: root.renaming = true }
                    }
                }
                Component {
                    id: nameField
                    TextField {
                        text: root.set.name
                        Component.onCompleted: { forceActiveFocus(); selectAll(); }
                        onAccepted: { Session.run("setlist_update", { id: root.set.id, patch: { name: text } }); root.renaming = false; }
                        onEditingFinished: if (root.renaming) { Session.run("setlist_update", { id: root.set.id, patch: { name: text } }); root.renaming = false; }
                        Keys.onEscapePressed: root.renaming = false
                    }
                }
            }
            ToolButton {
                icon.source: Theme.icon("dots")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("More for this set")
                onClicked: setMenu.popup()
                Menu {
                    id: setMenu
                    popupType: Popup.Native
                    MenuItem { text: qsTr("Rename"); onTriggered: root.renaming = true }
                    MenuItem { text: qsTr("Duplicate"); onTriggered: Session.run("setlist_duplicate", { id: root.set.id, name: qsTr("%1 (copy)").arg(root.set.name) }) }
                    MenuItem {
                        text: qsTr("Reset Played / Skipped")
                        onTriggered: Session.confirm(qsTr("Mark every song in this set as not played?"), qsTr("Reset"), ok => { if (ok) Session.run("setlist_reset", { id: root.set.id }); })
                    }
                    MenuSeparator {}
                    Menu {
                        title: qsTr("Export")
                        MenuItem { text: qsTr("Set File…"); onTriggered: root.exportSet() }
                        MenuItem { text: qsTr("CSV…"); onTriggered: root.exportCsv() }
                        MenuItem { text: qsTr("Print…"); onTriggered: root.printSet() }
                    }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Close Set"); onTriggered: Session.run("perform_open", { setlistId: null }) }
                    MenuItem {
                        text: qsTr("Delete Set…")
                        onTriggered: Session.confirm(qsTr("Delete “%1”?").arg(root.set.name), qsTr("Delete"), ok => { if (ok) Session.run("setlist_delete", { id: root.set.id }); })
                    }
                }
            }
        }

        // Live or preset, and the club.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.bottomMargin: 6
            ComboBox {
                readonly property var modes: ["live", "preset"]
                model: [qsTr("Live"), qsTr("Preset")]
                currentIndex: root.set && root.set.mode === "preset" ? 1 : 0
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Live: choose songs as you go. Preset: a setlist planned ahead of time.")
                onActivated: index => Session.run("setlist_update", { id: root.set.id, patch: { mode: modes[index] } })
            }
            ComboBox {
                id: clubBox
                Layout.fillWidth: true
                readonly property var choices: [{ label: qsTr("No club"), ref: null }].concat(Session.clubs.map(c => ({ label: c.name, ref: { id: c.id, version: c.versions[0].id } })))
                model: choices.map(c => c.label).concat(root.v && root.v.club && root.v.club.newerVersion ? [root.v.club.label] : [])
                currentIndex: !root.v || !root.v.club ? 0 : root.v.club.newerVersion ? model.length - 1 : Math.max(0, choices.findIndex(c => c.ref && c.ref.id === root.v.club.id))
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Club: whose guidelines (styles, tempos, what a set should contain) this set follows")
                onActivated: index => {
                    if (index < choices.length) Session.run("setlist_update", { id: root.set.id, patch: { club: choices[index].ref } });
                }
            }
            Button {
                visible: !!root.v && !!root.v.club && !!root.v.club.newerVersion
                flat: true
                text: qsTr("Use newer guidelines")
                onClicked: Session.run("setlist_update", { id: root.set.id, patch: { club: { id: root.v.club.id, version: root.v.club.newerVersion } } })
            }
        }

        // The entries. Songs dropped below them (or on an empty set) go at the end.
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            DropArea {
                anchors.fill: parent
                keys: Session.songKeys.concat(["application/x-orpheus-entries"])
                onEntered: entries.dropIndex = root.v ? root.v.rows.length : 0
                onExited: entries.dropIndex = -1
                onDropped: d => {
                    const at = root.v.rows.length;
                    entries.dropIndex = -1;
                    if (d.formats.includes("application/x-orpheus-entries")) root.moveEntries(JSON.parse(d.getDataAsString("application/x-orpheus-entries")), at);
                    else Session.droppedVersions(d, false).then(ids => { if (ids.length) root.insertSongs(ids, at); });
                }
            }
            ListView {
                id: entries
                anchors.fill: parent
                clip: true
                focus: true
                model: root.v ? root.v.rows : []
                ScrollBar.vertical: ScrollBar {}
                property int dropIndex: -1

                // Keep what's playing in view.
                Connections {
                    target: root
                    function onPChanged() {
                        if (!root.p || !root.v) return;
                        const i = root.v.rows.findIndex(r => r.entry.id === root.p.currentEntryId);
                        if (i >= 0) entries.positionViewAtIndex(i, ListView.Contain);
                    }
                }

                delegate: SetEntry {
                    required property var modelData
                    required property int index
                    width: entries.width
                    row: modelData
                    rowIndex: index
                    current: !!root.p && modelData.entry.id === root.p.currentEntryId
                    next: !!root.p && modelData.entry.id === root.p.nextEntryId
                    selected: Session.selectedEntries.includes(modelData.entry.id)
                    dropAbove: entries.dropIndex === index
                    onPicked: modifiers => { entries.forceActiveFocus(); root.select(modelData.entry, modifiers); }
                    onChosen: root.choose(modelData.entry)
                    onDropHover: i => entries.dropIndex = i
                    onSongsDropped: (ids, at, slot) => {
                        entries.dropIndex = -1;
                        if (slot) Session.fillSlot(ids[0], modelData.entry.id);
                        else root.insertSongs(ids, at);
                    }
                    onEntriesDropped: (ids, at) => { entries.dropIndex = -1; root.moveEntries(ids, at); }
                    onRemove: root.remove(Session.selectedEntries.includes(modelData.entry.id) ? Session.selectedEntries : [modelData.entry.id])
                    onMoveNext: Session.run("entries_move_next", { setlistId: root.set.id, ids: Session.selectedEntries.length ? Session.selectedEntries : [modelData.entry.id] })
                    club: root.v ? root.v.club : null
                }

                footer: Item {
                    width: entries.width
                    height: Theme.rowHeight * 3
                    Rectangle { visible: entries.dropIndex === (root.v ? root.v.rows.length : 0); width: parent.width; height: 2; color: Theme.system.highlight }
                    Label {
                        visible: !!root.v && !root.v.rows.length
                        anchors.centerIn: parent
                        width: parent.width - Theme.spacing * 4
                        wrapMode: Text.WordWrap
                        horizontalAlignment: Text.AlignHCenter
                        opacity: Theme.dim
                        text: qsTr("Drag songs here, press + on a song, or add style slots below.")
                    }
                }
            }

        }

        // How the set follows its club's guidelines.
        Pane {
            visible: root.showCheck && !!root.v && !!root.v.check
            Layout.fillWidth: true
            Layout.maximumHeight: 220
            padding: Theme.spacing
            background: Rectangle { color: Theme.system.alternateBase; Rectangle { width: parent.width; height: 1; color: Theme.system.mid } }
            ScrollView {
                anchors.fill: parent
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        Repeater {
                            model: root.v && root.v.check ? root.v.check.tallies : []
                            delegate: Label {
                                required property var modelData
                                textFormat: Text.StyledText
                                color: modelData.ok ? Theme.system.text : Theme.warn
                                text: modelData.name + " <b>" + modelData.have + "</b> / " + modelData.want
                                ToolTip.visible: tallyHover.hovered
                                ToolTip.text: qsTr("%1 expects %2").arg(root.v && root.v.club ? root.v.club.name : qsTr("The club")).arg(modelData.want)
                                HoverHandler { id: tallyHover }
                            }
                        }
                    }
                    Repeater {
                        model: root.v && root.v.check ? root.v.check.issues : []
                        delegate: ItemDelegate {
                            required property var modelData
                            Layout.fillWidth: true
                            enabled: modelData.entries.length > 0
                            text: modelData.text
                            contentItem: RowLayout {
                                spacing: Theme.spacing
                                Icon {
                                    Layout.alignment: Qt.AlignTop
                                    glyph: modelData.level === "warn" ? "warning" : "info"
                                    color: modelData.level === "warn" ? Theme.warn : Theme.system.text
                                }
                                Label { Layout.fillWidth: true; text: modelData.text; wrapMode: Text.WordWrap }
                            }
                            onClicked: Session.selectedEntries = modelData.entries
                        }
                    }
                    RowLayout {
                        visible: !!root.v && !!root.v.check && !root.v.check.issues.length
                        Icon { glyph: "check"; color: Theme.ok }
                        Label { text: qsTr("Follows %1's guidelines.").arg(root.v && root.v.club ? root.v.club.name : ""); color: Theme.ok }
                    }
                }
            }
        }

        // Add slots and pauses; totals.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            Button {
                icon.source: Theme.icon("plus"); text: qsTr("Style Slot")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Add a placeholder for a dance style; choose the song live")
                onClicked: slotMenu.popup()
                Menu {
                    id: slotMenu
                    popupType: Popup.Native
                    Instantiator {
                        model: root.v && root.v.club ? root.v.club.styles : Session.myStyles
                        delegate: MenuItem {
                            required property var modelData
                            text: modelData.name
                            onTriggered: root.addSlot(modelData.key)
                        }
                        onObjectAdded: (i, o) => slotMenu.insertItem(i, o)
                        onObjectRemoved: (i, o) => slotMenu.removeItem(o)
                    }
                    MenuItem {
                        visible: !(root.v && root.v.club ? root.v.club.styles.length : Session.myStyles.length)
                        text: root.v && root.v.club ? qsTr("%1 has no styles (Settings → Clubs)").arg(root.v.club.name) : qsTr("Add dance styles in Settings → Dance Styles")
                        onTriggered: Session.settingsPage = root.v && root.v.club ? "clubs" : "styles"
                    }
                }
            }
            Button {
                icon.source: Theme.icon("pause")
                text: qsTr("Pause")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Add a pause without a song: a break, an extra pause, an announcement or a stop for auto mode")
                onClicked: root.addPause()
            }
            Item { Layout.fillWidth: true }
            Button {
                visible: !!root.v && !!root.v.check
                checkable: true
                checked: root.showCheck
                onToggled: root.showCheck = checked
                text: root.v && root.v.warnings ? qsTr("%1 to Check").arg(root.v.warnings) : qsTr("Guidelines")
                icon.source: Theme.icon(root.v && root.v.warnings ? "warning" : "check")
                icon.color: root.v && root.v.warnings ? Theme.warn : Theme.ok
                ToolTip.visible: hovered
                ToolTip.text: qsTr("How the set follows the club's guidelines")
            }
            Label {
                opacity: Theme.dim
                text: !root.v ? "" : qsTr("%1 songs · %2").arg(root.v.rows.filter(r => r.number).length).arg(Session.fmtTime(root.v.totalMs))
                    + (root.v.remainingMs && root.p && root.p.startedAt ? " · " + qsTr("%1 left").arg(Session.fmtTime(root.v.remainingMs)) : "")
            }
        }
    }
}
