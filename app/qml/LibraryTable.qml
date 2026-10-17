pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml.Models
import Qt.labs.qmlmodels
import Orpheus

// The library view's songs: a tree table (a song's other versions fold under
// the row standing for it, where the view lists songs), sortable by column.
// Double-click (or Enter) loads the free deck, or fills the slot being chosen;
// drag rows to the set, a deck, a crate or a style.
FocusScope {
    id: root

    /// The view (`library_view`).
    property var view: null
    readonly property bool positioned: !!view && (view.sort === "position" || Session.view.kind === "setlist" || Session.view.kind === "history" || (Session.view.kind === "crate" && !view.foldable))

    signal combine(var songs, var recordings)

    // ------------------------------------------------------------ columns

    /// The columns you can choose and order (Library Columns…): most important
    /// first. Title comes first (the tree, folding, is in the first column; in
    /// ordered views the title cell starts with the position) and the row's
    /// buttons last. When the table is too narrow, the last chosen columns give
    /// way: their values show at the top of the row's menu (right-click).
    readonly property var optional: ["artist", "version", "style", "bpm", "time", "meter", "plays"]
    readonly property var saved: Session.settings && Session.settings.libraryColumns ? Session.settings.libraryColumns : ({ order: [], hidden: [], widths: {} })
    /// Your order (columns it lacks go at the end, in the default order).
    readonly property var order: {
        const mine = (saved.order || []).filter(k => optional.includes(k));
        return mine.concat(optional.filter(k => !mine.includes(k)));
    }
    readonly property var chosen: order.filter(k => !saved.hidden.includes(k))
    /// The model's columns, in order (all of them have the same data, so any order works).
    readonly property var keys: ["title"].concat(order).concat(["actions"])
    readonly property string playsKey: Session.view.kind === "history" ? "played" : "plays"
    readonly property var names: ({
        title: positioned ? qsTr("#  Title") : qsTr("Title"), artist: qsTr("Artist"), version: qsTr("Version"), style: qsTr("Style"),
        bpm: qsTr("BPM"), time: qsTr("Time"), meter: qsTr("Meter"), plays: Session.view.kind === "history" ? qsTr("Played") : qsTr("Plays"), actions: ""
    })
    /// Widths (px): fixed ones, and the share of the rest the flexible ones take.
    readonly property var defaults: ({ style: 100, bpm: 90, time: 60, meter: 64, plays: 60, played: 130, actions: 96 })
    readonly property var weights: ({ title: 2.2, artist: 1.4, version: 0.8 })
    readonly property var minimums: ({ title: 120, artist: 90, version: 70 })

    function widthOf(k) {
        const key = k === "plays" ? playsKey : k;
        return saved.widths[key] !== undefined ? saved.widths[key] : defaults[key];
    }
    function minimumOf(k) { return widthOf(k) !== undefined ? widthOf(k) : minimums[k]; }
    /// The chosen columns that fit, in your order (the first that doesn't fit, and those after it, don't show).
    readonly property var shownColumns: {
        let room = tree.width - widthOf("actions") - minimumOf("title");
        const shown = [];
        for (const k of chosen) {
            if (minimumOf(k) > room) break;
            shown.push(k);
            room -= minimumOf(k);
        }
        return shown;
    }
    /// The chosen columns that don't fit: shown in each row's menu.
    readonly property var overflow: chosen.filter(k => !shownColumns.includes(k))
    function shows(k) {
        if (k === "title" || k === "actions") return true;
        return shownColumns.includes(k);
    }
    /// The flexible columns' widths: shares (by `weights`) of what the fixed ones
    /// leave, any below its minimum held at it and the rest shared again, so they
    /// add up to the room there is.
    readonly property var flexWidths: {
        let free = tree.width;
        let flex = [];
        for (const k of keys) {
            if (!shows(k)) continue;
            if (widthOf(k) !== undefined) free -= widthOf(k);
            else flex.push(k);
        }
        const out = {};
        for (;;) {
            const weight = flex.reduce((n, k) => n + weights[k], 0);
            const low = flex.filter(k => free * weights[k] / weight < minimums[k]);
            if (!low.length) break;
            for (const k of low) { out[k] = minimums[k]; free -= minimums[k]; }
            flex = flex.filter(k => !low.includes(k));
        }
        const weight = flex.reduce((n, k) => n + weights[k], 0);
        for (const k of flex) out[k] = Math.floor(free * weights[k] / weight);
        return out;
    }
    /// A column's width: hidden 0, fixed, or its share of what's left.
    function columnWidth(c) {
        const k = keys[c];
        if (!shows(k)) return 0;
        return widthOf(k) !== undefined ? widthOf(k) : flexWidths[k];
    }
    onFlexWidthsChanged: tree.forceLayout()
    onShownColumnsChanged: tree.forceLayout()
    onSavedChanged: tree.forceLayout()
    // Dragged widths belong to positions: a new order starts from the saved ones.
    onKeysChanged: { tree.clearColumnWidths(); tree.forceLayout(); }

    /// Saves the widths dragged in the header (and `patch`: order, hidden).
    function saveColumns(patch) {
        const widths = Object.assign({}, saved.widths);
        for (let c = 0; c < keys.length; c++) {
            const w = tree.explicitColumnWidth(c);
            if (w > 0 && keys[c] !== "actions") widths[keys[c] === "plays" ? playsKey : keys[c]] = w;
        }
        Session.saveSettings({ libraryColumns: Object.assign({ order: order, hidden: saved.hidden, widths: widths }, patch || {}) });
    }
    function resetColumns() {
        tree.clearColumnWidths();
        Session.saveSettings({ libraryColumns: { order: order, hidden: saved.hidden, widths: {} } });
    }
    /// The columns dialog (from the header's menu or the View menu).
    signal configureColumns()

    /// A row's menu (selecting it, unless it's in the selection), at the pointer or
    /// below `button`. It starts with the row's values the table has no room for.
    function openMenu(row, r, button) {
        const index = tree.index(row, 0);
        if (!selection.isRowSelected(row, index.parent))
            selection.setCurrentIndex(index, ItemSelectionModel.ClearAndSelect | ItemSelectionModel.Rows);
        menu.rows = selectedRows();
        menu.historyRow = r;
        menu.details = overflow.map(k => ({ name: names[k], value: cellText(r, k) })).filter(d => d.value !== "");
        if (button) menu.popup(button, 0, button.height);
        else menu.popup();
    }

    /// A row's value in a column, as text.
    function cellText(r, key) {
        if (!r) return "";
        switch (key) {
        case "artist": return r.artist;
        case "version": return r.versionLabel;
        case "bpm": return (r.analyzing ? "… " : "") + Session.fmtBpm(r.bpm) + (Math.abs(r.rate - 1) > 0.001 ? "  " + Session.fmtPct(r.rate) : "");
        case "time": return Session.fmtTime(r.durationMs);
        case "meter": return Session.meterName(r.meter);
        case "plays": return r.playedAt ? new Date(r.playedAt).toLocaleString(Qt.locale(), "MMM d, h:mm ap") : r.plays ? String(r.plays) : "";
        case "style": return stylesText(r, true);
        }
        return "";
    }
    /// Its styles, in their colours (`plain`: without).
    function stylesText(r, plain) {
        if (!r) return "";
        if (r.suggested) return qsTr("tempo fits");
        return r.styles.map(s => plain ? s.short || s.name : "<font color='" + Theme.readable(s.color) + "'>" + (s.short || s.name) + "</font>").join(", ");
    }

    // ------------------------------------------------------------ rows

    // Every column gives the row's key (an index into `flat`): the model needs
    // one type per column, and rows have mixed nulls and numbers.
    TreeModel {
        id: songs
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
        TableModelColumn { display: "k" }
    }
    // Delegates find their row's data through `flat` (by the `k` the model
    // gives as its edit role), never through model indexes, which a reload
    // invalidates.
    property var flat: []
    // A reload keeps the selection (and doesn't change what Info shows).
    property bool restoring: false
    onViewChanged: {
        restoring = true;
        const all = [];
        const keyed = rows => rows.map(r => {
            const o = Object.assign({}, r, { k: all.length });
            all.push(r);
            if (r.rows) o.rows = keyed(r.rows);
            return o;
        });
        const rows = keyed(view ? view.rows : []);
        flat = all;
        songs.rows = rows;
        if (Session.unfolded.length) {
            for (let r = 0; r < tree.rows; r++) {
                const row = rowAt(r);
                if (row && Session.unfolded.includes(row.trackId)) tree.expand(r);
            }
        }
        restoreSelection();
        restoring = false;
    }

    function rowAt(r) { return r >= 0 && r < tree.rows ? flat[songs.data(tree.index(r, 0), Qt.DisplayRole)] : null; }

    ItemSelectionModel {
        id: selection
        model: songs
        onSelectionChanged: root.syncSelection()
        onCurrentChanged: root.syncSelection()
    }
    /// The selected songs (else the current row) are Session's; Info shows the
    /// current row (the one clicked), else the last selected.
    function syncSelection() {
        if (restoring) return;
        const rows = selectedRows();
        const current = selection.currentIndex.valid ? flat[songs.data(selection.currentIndex, Qt.DisplayRole)] : null;
        Session.selected = (rows.length ? rows : current ? [current] : []).map(r => r.versionId);
        const shown = current || rows[rows.length - 1];
        // While a chime is chosen, Info keeps showing what it's for.
        if (shown && !Session.chimePick) {
            Session.inspectTrack = shown.trackId;
            Session.inspectEntry = "";
        }
    }
    /// After a reload, the songs that were selected stay selected.
    function restoreSelection() {
        const keep = Session.selected;
        if (!keep.length) return;
        for (let r = 0; r < tree.rows; r++) {
            const row = rowAt(r);
            if (row && keep.includes(row.versionId)) selection.select(tree.index(r, 0), ItemSelectionModel.Select | ItemSelectionModel.Rows);
        }
    }
    function selectedRows() {
        return selection.selectedIndexes.filter(i => i.column === 0).map(i => flat[songs.data(i, Qt.DisplayRole)]);
    }

    /// Double-click / Enter: fill the slot being chosen, add the edit being
    /// chosen, else load onto the free deck (paused).
    function choose(row) {
        if (!row) return;
        if (Session.chimePick) Session.chimePicked(row.versionId);
        else if (Session.fillingSlot) Session.fillSlot(row.versionId);
        else if (Session.pickingEdit) Session.addEdit({ recording: row.recordingId });
        else Session.loadDeck(null, row.versionId, null);
    }

    function sortBy(key) {
        if (key === "actions") return;
        if (Session.sort === key || (Session.sort === "" && view && view.sort === key)) Session.descending = !Session.descending;
        else { Session.sort = key; Session.descending = false; }
    }

    function selectAll() {
        for (let r = 0; r < tree.rows; r++) selection.select(tree.index(r, 0), ItemSelectionModel.Select | ItemSelectionModel.Rows);
    }
    function combineSelected() {
        const rows = selectedRows();
        root.combine([...new Set(rows.map(r => r.trackId))], [...new Set(rows.map(r => r.recordingId))]);
    }
    function expandAll() { tree.expandRecursively(); Session.unfolded = (view ? view.rows : []).map(r => r.trackId); }
    function collapseAll() { tree.collapseRecursively(); Session.unfolded = []; }

    // A controller's browse knob and buttons. Its press loads (or fills the slot);
    // it never plays, so choosing songs stays silent.
    Connections {
        target: Backend
        function onEvent(name, p) {
            if (name !== "midi-ui") return;
            const row = root.rowAt(tree.currentRow);
            if (p.action === "browse.scroll") root.move(p.delta || 0);
            else if (p.action === "browse.audition" && row) root.choose(row);
            else if (p.action === "deck.load" && p.deck !== undefined && row) Session.loadDeck(p.deck, row.versionId, null);
            else if (p.action === "browse.queueNext" && Session.selected.length) Session.addToSet(Session.selected, "next");
            else if (p.action === "browse.queueEnd" && Session.selected.length) Session.addToSet(Session.selected, "end");
        }
    }
    function move(delta) {
        if (!tree.rows) return;
        const r = Math.max(0, Math.min(tree.rows - 1, tree.currentRow + delta));
        selection.setCurrentIndex(tree.index(r, 0), ItemSelectionModel.ClearAndSelect | ItemSelectionModel.Rows);
        tree.positionViewAtRow(r, TableView.Contain);
    }

    // Delete or Backspace (the Mac's Delete key) removes songs from a crate;
    // Return or Enter loads (or fills the slot); Escape stops choosing.
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
            const c = Session.view.kind === "crate" ? Session.crates.find(x => x.id === Session.view.id) : null;
            if (c && !c.smart && Session.selected.length) Session.run("crate_remove", { id: c.id, versionIds: Session.selected });
            event.accepted = true;
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            root.choose(root.rowAt(tree.currentRow));
            event.accepted = true;
        } else if (event.key === Qt.Key_Escape) {
            Session.fillingSlot = "";
            Session.pickingEdit = "";
            Session.chimePick = null;
        }
    }

    // ------------------------------------------------------------ the table

    // The header and rows, in a view frame.
    ViewFrame {
        anchors.fill: parent
        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            HorizontalHeaderView {
                id: header
                Layout.fillWidth: true
                syncView: tree
                clip: true
                resizableColumns: true
                // A list of the columns' keys (each delegate gets one as `model.modelData`).
                model: root.keys
                delegate: HorizontalHeaderViewDelegate {
                    id: section
                    readonly property string column: model.modelData
                    readonly property string key: column === "plays" ? root.playsKey : column
                    readonly property bool sorted: !!root.view && root.view.sort === key
                    contentItem: RowLayout {
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            text: root.names[section.column] || ""
                            elide: Text.ElideRight
                            horizontalAlignment: section.key === "bpm" || section.key === "time" || section.key === "plays" ? Text.AlignRight : Text.AlignLeft
                        }
                        Icon {
                            visible: section.sorted
                            glyph: Session.descending ? "sort-down" : "sort-up"
                            sourceSize.width: 12; sourceSize.height: 12
                        }
                    }
                    TapHandler { onTapped: root.sortBy(section.key) }
                    TapHandler { acceptedButtons: Qt.RightButton; onTapped: columnMenu.popup() }
                }
            }

            TreeView {
                id: tree
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                focus: true
                model: songs
                selectionModel: selection
                selectionBehavior: TableView.SelectRows
                selectionMode: TableView.ExtendedSelection
                alternatingRows: true
                boundsBehavior: Flickable.StopAtBounds
                columnWidthProvider: c => root.columnWidth(c)
                onWidthChanged: forceLayout()
                ScrollBar.vertical: ScrollBar {}
                // Widths dragged in the header are saved once the drag pauses.
                onLayoutChanged: saveWidths.restart()
                Timer { id: saveWidths; interval: 800; onTriggered: { if (tree.columnWidthsChanged()) root.saveColumns(); } }
                function columnWidthsChanged() {
                    for (let c = 0; c < root.keys.length; c++) {
                        const w = explicitColumnWidth(c);
                        const k = root.keys[c] === "plays" ? root.playsKey : root.keys[c];
                        if (w > 0 && root.saved.widths[k] !== w) return true;
                    }
                    return false;
                }
                onExpanded: (row, depth) => { const r = root.rowAt(row); if (r && !Session.unfolded.includes(r.trackId)) Session.unfolded = Session.unfolded.concat([r.trackId]); }
                onCollapsed: (row, recursively) => { const r = root.rowAt(row); if (r) Session.unfolded = Session.unfolded.filter(t => t !== r.trackId); }

                delegate: TreeViewDelegate {
                    id: cell
                    required property int column
                    readonly property string key: root.keys[column]
                    readonly property var r: model.display !== undefined ? root.flat[model.display] : null
                    readonly property bool onDeck: !!r && Session.decks.some(d => d && d.versionId === r.versionId)
                    readonly property bool audible: !!r && Session.decks.some(d => d && d.versionId === r.versionId && d.audible)
                    /// Dimmed: played in the open set, suggested by tempo, missing, or hidden from the library.
                    readonly property bool dim: !!r && (r.playedInSet || r.suggested || r.availability === "missing" || r.unlisted)
                    readonly property color fg: highlighted ? palette.highlightedText : palette.text

                    contentItem: Loader {
                        sourceComponent: cell.key === "title" ? titleCell
                            : cell.key === "style" ? styleCell
                            : cell.key === "actions" ? actionsCell
                            : textCell
                    }
                    Component {
                        id: textCell
                        Label {
                            verticalAlignment: Text.AlignVCenter
                            opacity: cell.dim || ["version", "meter", "plays"].includes(cell.key) ? Theme.dim : 1
                            elide: Text.ElideRight
                            font.features: { "tnum": 1 }
                            horizontalAlignment: ["bpm", "time", "plays"].includes(cell.key) && !(cell.r && cell.r.playedAt) ? Text.AlignRight : Text.AlignLeft
                            color: cell.highlighted || !cell.r ? cell.fg
                                : cell.key === "bpm" ? (cell.r.tempoFit === "sweet" ? Theme.ok : cell.r.tempoFit === "outside" ? Theme.warn : cell.fg)
                                : cell.key === "time" && cell.r.long ? Theme.warn
                                : cell.fg
                            text: root.cellText(cell.r, cell.key)
                            ToolTip.visible: tipHover.hovered && !!ToolTip.text
                            ToolTip.delay: 600
                            ToolTip.text: !cell.r ? "" : cell.key === "version" ? cell.r.comment || ""
                                : cell.key === "meter" && cell.r.meter ? (cell.r.meterByYou ? qsTr("Set by you") : qsTr("Detected")) : ""
                            HoverHandler { id: tipHover }
                        }
                    }
                    // The title, after where it plays from: nothing for a file, ○ Spotify
                    // only, ● a file and Spotify, a warning when missing. On a deck, in the
                    // deck's colour (red while the room hears it).
                    Component {
                        id: titleCell
                        RowLayout {
                            spacing: 4
                            Label {
                                visible: root.positioned
                                Layout.preferredWidth: 24
                                horizontalAlignment: Text.AlignRight
                                text: cell.r && cell.r.position ? cell.r.position : ""
                                color: cell.fg
                                opacity: Theme.dim
                                font.features: { "tnum": 1 }
                            }
                            Item {
                                implicitWidth: 12
                                implicitHeight: 12
                                Rectangle {
                                    anchors.centerIn: parent
                                    width: 7; height: 7; radius: 4
                                    visible: !!cell.r && (cell.r.availability === "spotifyOnly" || cell.r.availability === "fileAndSpotify")
                                    color: cell.r && cell.r.availability === "fileAndSpotify" ? Theme.spotify : "transparent"
                                    border.color: Theme.spotify
                                    border.width: 1.5
                                }
                                Icon {
                                    anchors.centerIn: parent
                                    visible: !!cell.r && cell.r.availability === "missing"
                                    glyph: "warning"
                                    sourceSize.width: 12; sourceSize.height: 12
                                    color: Theme.warn
                                }
                            }
                            Label {
                                Layout.fillWidth: true
                                text: cell.r ? cell.r.title : ""
                                elide: Text.ElideRight
                                font.italic: !!cell.r && cell.r.suggested
                                opacity: cell.dim || (cell.r && cell.r.child) ? Theme.dim : 1
                                color: cell.onDeck && !cell.highlighted ? (cell.audible ? Theme.onAir : Theme.deckColors[0]) : cell.fg
                            }
                        }
                    }
                    Component {
                        id: styleCell
                        Label {
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                            textFormat: Text.StyledText
                            opacity: cell.dim ? Theme.dim : 1
                            color: cell.fg
                            font.italic: !!cell.r && cell.r.suggested
                            text: root.stylesText(cell.r, cell.highlighted)
                            ToolTip.visible: styleHover.hovered && !!cell.r && cell.r.styles.length > 0
                            ToolTip.text: cell.r ? cell.r.styles.map(s => [s.name].concat(s.problems).join(": ")).join("\n") : ""
                            ToolTip.delay: 600
                            HoverHandler { id: styleHover }
                        }
                    }
                    Component {
                        id: actionsCell
                        RowLayout {
                            spacing: 0
                            ToolButton {
                                icon.source: Theme.icon("headphones")
                                icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                                icon.color: cell.fg
                                implicitHeight: 22
                                ToolTip.visible: hovered; ToolTip.text: qsTr("Preview on the free deck")
                                onClicked: Session.preview(cell.r.versionId, null)
                            }
                            ToolButton {
                                icon.source: Theme.icon(Session.fillingSlot ? "check" : "plus")
                                icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                                icon.color: cell.fg
                                implicitHeight: 22
                                ToolTip.visible: hovered
                                ToolTip.text: Session.fillingSlot ? qsTr("Use this song for the slot") : qsTr("Play next")
                                onClicked: Session.fillingSlot ? Session.fillSlot(cell.r.versionId) : Session.addToSet([cell.r.versionId], "next")
                            }
                            ToolButton {
                                id: moreButton
                                icon.source: Theme.icon("dots")
                                icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                                icon.color: cell.fg
                                implicitHeight: 22
                                ToolTip.visible: hovered
                                ToolTip.text: root.overflow.length ? qsTr("More: its other details, and what you can do with it") : qsTr("What you can do with it")
                                onClicked: root.openMenu(cell.row, cell.r, moreButton)
                            }
                        }
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onDoubleTapped: root.choose(cell.r)
                    }
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: root.openMenu(cell.row, cell.r, null)
                    }
                    // Drag the selected songs (or this one) to the set, a deck, a crate or a style.
                    DragHandler {
                        id: dragging
                        target: null
                        onActiveChanged: {
                            if (active) { Session.dragging = true; return; }
                            dragProxy.Drag.drop();
                            // No platform drag came of it: nothing to wait for.
                            if (!dragProxy.started) Session.dragging = false;
                        }
                    }
                    Item {
                        id: dragProxy
                        property bool started: false
                        Drag.onDragStarted: started = true
                        Drag.onDragFinished: { started = false; Session.dragging = false; }
                        Drag.active: dragging.active
                        Drag.dragType: Drag.Automatic
                        Drag.supportedActions: Qt.CopyAction
                        Drag.mimeData: ({ "application/x-orpheus-versions": JSON.stringify(Session.selected.length && cell.r && Session.selected.includes(cell.r.versionId) ? Session.selected : cell.r ? [cell.r.versionId] : []) })
                    }
                }
            }
        }
    }

    // When nothing's here, why.
    ColumnLayout {
        anchors.centerIn: parent
        visible: !!root.view && !root.view.rows.length
        width: parent.width * 0.6
        spacing: Theme.spacing
        readonly property var crate: Session.view.kind === "crate" ? Session.crates.find(c => c.id === Session.view.id) : null
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.bold: !!root.view && root.view.librarySongs === 0
            text: !root.view ? "" : root.view.librarySongs === 0 ? qsTr("Your library is empty.")
                : Session.search ? qsTr("No songs match “%1”.").arg(Session.search)
                : Session.styleFilter.length === 1 && Session.styleFilter[0] !== "__none"
                    ? (Session.suggestByTempo ? qsTr("Nothing here is labelled %1, and no unlabelled songs fit its tempo.") : qsTr("Nothing here is labelled %1.")).arg(Session.styleName(Session.styleFilter[0]))
                : Session.styleFilter.length ? qsTr("No songs here match the style filter.")
                : parent.crate && parent.crate.smart ? qsTr("No songs match this smart crate's rules yet.")
                : qsTr("Nothing here yet.")
        }
        Label {
            visible: !!root.view && (root.view.librarySongs === 0 || Session.styleFilter.length > 0)
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            opacity: Theme.dim
            text: root.view && root.view.librarySongs === 0 ? qsTr("Add a music folder, drag files onto this window, or connect Spotify.")
                : qsTr("Drag songs onto a style above to label them.")
        }
        Button {
            visible: !!root.view && root.view.librarySongs === 0
            Layout.alignment: Qt.AlignHCenter
            highlighted: true
            text: qsTr("Add Music Folder…")
            onClicked: Session.settingsPage = "library"
        }
    }

    // The header's menu: the columns to show, folding, widths.
    Menu {
        id: columnMenu
        MenuItem { text: qsTr("Expand All Songs"); enabled: !!root.view && root.view.foldable; onTriggered: root.expandAll() }
        MenuItem { text: qsTr("Collapse All Songs"); enabled: !!root.view && root.view.foldable; onTriggered: root.collapseAll() }
        MenuSeparator {}
        Instantiator {
            model: root.optional
            delegate: MenuItem {
                required property string modelData
                text: root.names[modelData]
                checkable: true
                checked: !root.saved.hidden.includes(modelData)
                onTriggered: root.toggleColumn(modelData)
            }
            onObjectAdded: (i, o) => columnMenu.insertItem(i + 3, o)
            onObjectRemoved: (i, o) => columnMenu.removeItem(o)
        }
        MenuSeparator {}
        MenuItem { text: qsTr("Reset Column Widths"); enabled: Object.keys(root.saved.widths).length > 0; onTriggered: root.resetColumns() }
    }

    SongMenu {
        id: menu
        onCombine: (songs, recordings) => root.combine(songs, recordings)
    }
}
