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
    /// In the simple interface: double-click opens an entry's options, songs
    /// are found and added below the set, and there's no deck to load.
    property bool simple: false

    readonly property var v: Session.set
    /// This panel's interface is the one shown: only then does it make rows.
    property bool active: true

    // The list holds the entries' ids, changed in place as the set changes
    // (rows keep their delegates, and the list where it's scrolled to); each
    // row reads its data from `rowById`.
    property var rowById: ({})
    ListModel { id: rowKeys }
    onVChanged: syncRows()
    function syncRows() {
        const rows = v ? v.rows : [];
        const m = {};
        for (const r of rows) m[r.entry.id] = r;
        // Gone first (their rows still read the old data), then the new data, then order.
        for (let i = rowKeys.count - 1; i >= 0; i--) if (!m[rowKeys.get(i).key]) rowKeys.remove(i);
        rowById = m;
        for (let i = 0; i < rows.length; i++) {
            const k = rows[i].entry.id;
            if (i < rowKeys.count && rowKeys.get(i).key === k) continue;
            let j = -1;
            for (let q = i + 1; q < rowKeys.count; q++) if (rowKeys.get(q).key === k) { j = q; break; }
            if (j >= 0) rowKeys.move(j, i, 1);
            else rowKeys.insert(i, { key: k });
        }
    }
    readonly property var set: v ? v.setlist : null
    readonly property var p: Session.perform
    property bool showCheck: false
    property bool renaming: false

    /// The entry a Shift-click extends the selection from (the last clicked).
    property string anchor: ""
    /// A click on an entry: selects it; with Ctrl (⌘ on a Mac) adds or removes
    /// it; with Shift, selects from the last entry clicked to it. The list
    /// takes the keyboard (Delete, Ctrl+A).
    function select(entry, modifiers) {
        const sel = Session.selectedEntries;
        const ids = set.entries.map(e => e.id);
        entries.forceActiveFocus();
        if ((modifiers & Qt.ShiftModifier) && ids.includes(anchor)) {
            const a = ids.indexOf(anchor), b = ids.indexOf(entry.id);
            const range = ids.slice(Math.min(a, b), Math.max(a, b) + 1);
            Session.selectedEntries = (modifiers & (Qt.ControlModifier | Qt.MetaModifier)) ? [...new Set(sel.concat(range))] : range;
        } else if (modifiers & (Qt.ControlModifier | Qt.MetaModifier)) {
            Session.selectedEntries = sel.includes(entry.id) ? sel.filter(x => x !== entry.id) : sel.concat([entry.id]);
            anchor = entry.id;
        } else {
            Session.selectedEntries = [entry.id];
            anchor = entry.id;
        }
        Session.inspectEntry = entry.id;
        const row = v.rows.find(r => r.entry.id === entry.id);
        Session.inspectTrack = row && row.trackId ? row.trackId : "";
        // Selecting an empty slot starts choosing its song.
        if (!entry.versionId && entry.kind !== "pause") Session.startFillingSlot(entry);
    }

    /// Double-click: load onto the free deck (paused); on a slot, choose its song.
    function choose(entry) {
        if (!entry.versionId && !simple) { Session.startFillingSlot(entry); return; }
        if (simple) {
            const row = v.rows.find(r => r.entry.id === entry.id);
            Session.showSongInfo(row && row.trackId ? row.trackId : "", entry.id);
            return;
        }
        Session.run("deck_load", { deck: null, versionId: entry.versionId, entryId: entry.id, play: false });
    }

    /// Selects every entry (Ctrl+A in the set).
    function selectAll() {
        if (!set) return;
        Session.selectedEntries = set.entries.map(e => e.id);
        entries.forceActiveFocus();
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
            text: root.simple ? qsTr("A set is the songs you'll play, in order: make one to plan it, then play it with ▶.")
                : qsTr("Live DJing: press + on a song (or Play Next) — a live set is created for you.\nPreset DJing: plan a setlist ahead of time.")
        }
        Button { Layout.alignment: Qt.AlignHCenter; highlighted: true; text: root.simple ? qsTr("New Set…") : qsTr("New Setlist…"); onClicked: root.newSetlist() }
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
                icon.color: palette.buttonText
                icon.source: Theme.icon("dots")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("More for this set")
                onClicked: setMenu.popup()
                FitMenu {
                    id: setMenu
                    MenuItem { text: qsTr("Rename"); onTriggered: root.renaming = true }
                    MenuItem { text: qsTr("Duplicate"); onTriggered: Session.run("setlist_duplicate", { id: root.set.id, name: qsTr("%1 (copy)").arg(root.set.name) }) }
                    MenuItem { text: qsTr("Settings for This Set…"); onTriggered: chimeDialog.open() }
                    MenuItem {
                        text: qsTr("Use the Club's Newer Guidelines")
                        enabled: !!root.v && !!root.v.club && !!root.v.club.newerVersion
                        onTriggered: Session.run("setlist_update", { id: root.set.id, patch: { club: { id: root.v.club.id, version: root.v.club.newerVersion } } })
                    }
                    MenuItem {
                        text: qsTr("Fit All Tempos to Their Styles")
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("E.g. after changing the club: songs playing outside their style's tempo get the nearest that fits; the others stay as they are")
                        onTriggered: Session.fitTempos(root.v.rows.map(r => r.entry.id))
                    }
                    MenuItem {
                        text: qsTr("Reset Played / Skipped")
                        onTriggered: Session.confirm(qsTr("Mark every song in this set as not played?"), qsTr("Reset"), ok => { if (ok) Session.run("setlist_reset", { id: root.set.id }); })
                    }
                    MenuSeparator {}
                    FitMenu {
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

        // The set's own pauses and chime (over its songs', its club's and yours;
        // a song's own choice in the set is over all).
        AppDialog {
            id: chimeDialog
            title: qsTr("Settings for This Set")
            standardButtons: Dialog.Close
            readonly property var announce: Session.settings ? Session.settings.announce : null
            function update(patch) { Session.run("setlist_update", { id: root.set.id, patch: patch }); }
            GridLayout {
                anchors.fill: parent
                columns: 2
                columnSpacing: Theme.spacing
                rowSpacing: Theme.spacing
                Label {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    Layout.maximumWidth: 420
                    wrapMode: Text.WordWrap
                    text: qsTr("For every song in this set, unless the song's own settings in the set say otherwise. Left empty, each takes its default: the club's or yours for the pause, each song's (and version's) or yours for announcements.")
                }
                FormLabel { text: qsTr("Pause between songs") }
                SecondsField {
                    value: root.set ? root.set.pauseSecs : null
                    placeholder: root.v ? root.v.defaultPauseSecs : 0
                    onEdited: v => chimeDialog.update({ pauseSecs: v })
                }
                FormLabel { text: qsTr("With announcements") }
                RowLayout {
                    Label { text: qsTr("Pause") }
                    SecondsField {
                        value: root.set ? root.set.announcePauseBefore : null
                        placeholder: chimeDialog.announce ? chimeDialog.announce.pauseBeforeSecs : 0
                        onEdited: v => chimeDialog.update({ announcePauseBefore: v })
                    }
                    Label { text: qsTr("announcement, pause") }
                    SecondsField {
                        value: root.set ? root.set.announcePauseAfter : null
                        placeholder: chimeDialog.announce ? chimeDialog.announce.pauseAfterSecs : 0
                        onEdited: v => chimeDialog.update({ announcePauseAfter: v })
                    }
                }
                FormLabel { text: qsTr("Chime") }
                ChimeField {
                    Layout.fillWidth: true
                    soundAlways: true
                    value: root.set && root.set.chime ? root.set.chime : ({})
                    inheritedOn: !!chimeDialog.announce && chimeDialog.announce.chime
                    inheritedSound: chimeDialog.announce ? chimeDialog.announce.chimeSound || null : null
                    title: root.set ? qsTr("the set “%1”").arg(root.set.name) : ""
                    beforePicking: () => chimeDialog.close()
                    afterPicking: () => chimeDialog.open()
                    onEdited: v => chimeDialog.update({ chime: v })
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
                implicitContentWidthPolicy: ComboBox.WidestText
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
                Layout.preferredWidth: 100
                readonly property var choices: [{ label: qsTr("No club"), ref: null }].concat(Session.clubs.map(c => ({ label: c.name, ref: { id: c.id, version: c.versions[0].id } })))
                model: choices.map(c => c.label).concat(root.v && root.v.club && root.v.club.newerVersion ? [root.v.club.label] : [])
                currentIndex: !root.v || !root.v.club ? 0 : root.v.club.newerVersion ? model.length - 1 : Math.max(0, choices.findIndex(c => c.ref && c.ref.id === root.v.club.id))
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Club: whose guidelines (styles, tempos, what a set should contain) this set follows")
                onActivated: index => {
                    if (index < choices.length) Session.run("setlist_update", { id: root.set.id, patch: { club: choices[index].ref } });
                }
            }
        }

        // The entries. Songs dropped below them (or on an empty set) go at the end.
        ViewFrame {
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
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
                    if (Session.dropHas(d, "application/x-orpheus-entries")) root.moveEntries(JSON.parse(Session.dropText(d, "application/x-orpheus-entries")), at);
                    else Session.droppedVersions(d, false).then(ids => { if (ids.length) root.insertSongs(ids, at); });
                }
            }
            ListView {
                id: entries
                anchors.fill: parent
                clip: true
                focus: true
                model: root.active ? rowKeys : null
                ScrollBar.vertical: ScrollBar {}
                property int dropIndex: -1

                // Bring what's playing into view when it changes (not on every
                // update: you may have scrolled away to plan).
                property string shownPlaying: ""
                Connections {
                    target: root
                    function onPChanged() {
                        if (!root.p || !root.v || root.p.currentEntryId === entries.shownPlaying) return;
                        entries.shownPlaying = root.p.currentEntryId || "";
                        const i = root.v.rows.findIndex(r => r.entry.id === root.p.currentEntryId);
                        if (i >= 0) entries.positionViewAtIndex(i, ListView.Contain);
                    }
                }

                delegate: SetEntry {
                    required property string key
                    required property int index
                    // Its data: updated as the set changes; a row being removed
                    // keeps its last until it goes.
                    property var modelData: root.rowById[key]
                    Component.onCompleted: modelData = root.rowById[key]
                    Connections {
                        target: root
                        function onRowByIdChanged() { const r = root.rowById[key]; if (r) modelData = r; }
                    }
                    width: entries.width
                    row: modelData
                    rowIndex: index
                    current: !!root.p && modelData.entry.id === root.p.currentEntryId
                    next: !!root.p && modelData.entry.id === root.p.nextEntryId
                    selected: !!Session.selectedEntriesSet[modelData.entry.id]
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
                    onMoveNext: Session.run("entries_move_next", { setlistId: root.set.id, ids: Session.selectedEntries.includes(modelData.entry.id) ? Session.selectedEntries : [modelData.entry.id] })
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

        // Totals, on a line of their own (the buttons below take the width).
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.topMargin: 4
            elide: Text.ElideRight
            opacity: Theme.dim
            text: !root.v ? "" : qsTr("%1 songs · %2").arg(root.v.rows.filter(r => r.number).length).arg(Session.fmtTime(root.v.totalMs))
                + (root.v.remainingMs && root.p && root.p.startedAt ? " · " + qsTr("%1 left").arg(Session.fmtTime(root.v.remainingMs)) : "")
        }
        // Add slots and pauses; the club's guidelines.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            Layout.topMargin: 4
            Button {
                icon.source: Theme.icon("plus"); text: qsTr("Style Slot")
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Add a placeholder for a dance style; choose the song live")
                onClicked: slotMenu.popup()
                FitMenu {
                    id: slotMenu
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

            Button {
                enabled: !!root.v && !!root.v.check
                checkable: true
                checked: root.showCheck
                onToggled: root.showCheck = checked
                text: root.v && root.v.warnings ? qsTr("%1 to Check").arg(root.v.warnings) : qsTr("Guidelines")
                icon.source: Theme.icon(root.v && root.v.warnings ? "warning" : "check")
                icon.color: !enabled ? palette.buttonText : root.v && root.v.warnings ? Theme.warn : Theme.ok
                ToolTip.visible: hovered
                ToolTip.text: enabled ? qsTr("How the set follows the club's guidelines") : qsTr("The set has no club (its guidelines would be checked here)")
            }
            Item { Layout.fillWidth: true }
        }
    }
}
