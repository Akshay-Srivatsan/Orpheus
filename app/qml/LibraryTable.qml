import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The library view's songs: a row per version (folded under its song where
// the view lists songs), sortable by column. Double-click loads the free deck
// (or fills the slot being chosen); drag rows to the set, a deck, a crate or a style.
FocusScope {
    id: root

    /// The view (`library_view`).
    property var view: null
    readonly property var rows: view ? view.rows : []
    readonly property bool positioned: !!view && (view.sort === "position" || Session.view.kind === "setlist" || Session.view.kind === "history" || (Session.view.kind === "crate" && !view.foldable))
    /// Shift-click selects from here.
    property int anchorIndex: -1

    signal combine(var songs, var recordings)

    // Columns. Title, artist and version share what the others leave (by
    // `weights`, down to `minimums`) until a header's edge is dragged, which fixes
    // that column's width. Hidden columns and dragged widths are saved on this
    // computer; when the table is too narrow, columns go in `dropOrder`.
    readonly property var optional: ["artist", "version", "style", "bpm", "time", "meter", "plays"]
    readonly property var names: ({ artist: qsTr("Artist"), version: qsTr("Version"), style: qsTr("Style"), bpm: qsTr("BPM"), time: qsTr("Time"), meter: qsTr("Meter"), plays: qsTr("Plays") })
    readonly property var dropOrder: ["plays", "meter", "version", "style", "artist", "time"]
    readonly property var defaults: ({ position: 34, style: 92, bpm: 88, time: 52, meter: 56, plays: 52, played: 124 })
    readonly property var weights: ({ title: 2.2, artist: 1.4, version: 0.8 })
    readonly property var minimums: ({ title: 140, artist: 90, version: 60 })
    readonly property int wActions: 56
    readonly property string playsKey: Session.view.kind === "history" ? "played" : "plays"
    readonly property var saved: Session.settings && Session.settings.libraryColumns ? Session.settings.libraryColumns : ({ hidden: [], widths: {} })
    /// Widths being dragged (saved when the drag ends).
    property var dragged: ({})
    function fixedWidth(k) {
        if (dragged[k] !== undefined) return dragged[k];
        if (saved.widths[k] !== undefined) return saved.widths[k];
        return defaults[k];
    }
    function isFlex(k) { return weights[k] !== undefined && fixedWidth(k) === undefined; }
    readonly property var shownColumns: {
        let shown = optional.filter(k => !saved.hidden.includes(k));
        const need = cols => (positioned ? fixedWidth("position") : 0) + wActions + (isFlex("title") ? minimums.title : fixedWidth("title"))
            + cols.reduce((n, k) => n + (isFlex(k) ? minimums[k] : fixedWidth(k === "plays" ? playsKey : k)), 0);
        for (const k of dropOrder) {
            if (need(shown) <= list.width) break;
            shown = shown.filter(x => x !== k);
        }
        return shown;
    }
    function shows(k) { return shownColumns.includes(k === "played" ? "plays" : k); }
    readonly property var flex: {
        const keys = ["title"].concat(["artist", "version"].filter(k => shows(k)));
        let free = list.width - wActions - (positioned ? fixedWidth("position") : 0);
        for (const k of shownColumns) if (weights[k] === undefined) free -= fixedWidth(k === "plays" ? playsKey : k);
        let weight = 0;
        for (const k of keys) {
            if (isFlex(k)) weight += weights[k];
            else free -= fixedWidth(k);
        }
        const out = {};
        for (const k of keys) out[k] = isFlex(k) ? Math.max(minimums[k], free * weights[k] / weight) : fixedWidth(k);
        return out;
    }
    readonly property real wTitle: flex.title
    function colWidth(key) {
        if (key !== "title" && key !== "position" && !shows(key)) return 0;
        return flex[key] !== undefined ? flex[key] : fixedWidth(key);
    }
    function resize(key, dx) {
        const d = Object.assign({}, dragged);
        d[key] = Math.max(minimums[key] || 32, colWidth(key) + dx);
        dragged = d;
    }
    function saveColumns(patch) {
        const next = { hidden: saved.hidden, widths: Object.assign({}, saved.widths, dragged) };
        Object.assign(next, patch || {});
        dragged = {};
        Session.saveSettings({ libraryColumns: next });
    }
    function toggleColumn(k) {
        saveColumns({ hidden: saved.hidden.includes(k) ? saved.hidden.filter(x => x !== k) : saved.hidden.concat([k]) });
    }

    // Before every title: the fold control (views that fold songs) and where it plays from.
    readonly property int wFold: 34
    readonly property int wMarker: 14
    readonly property int wIndent: 16

    function sortBy(key) {
        if (Session.sort === key || (Session.sort === "" && view && view.sort === key)) Session.descending = !Session.descending;
        else { Session.sort = key; Session.descending = false; }
    }
    function sortMark(key) {
        return view && view.sort === key ? (Session.descending ? "down" : "up") : "";
    }

    // A cell's text.
    component Cell: Label {
        height: Theme.rowHeight
        leftPadding: 6
        rightPadding: 6
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    component NumberCell: Cell {
        horizontalAlignment: Text.AlignRight
        font.features: { "tnum": 1 }
    }

    function select(index, modifiers) {
        const id = rows[index].versionId;
        const sel = Session.selected;
        if (modifiers & (Qt.ControlModifier | Qt.MetaModifier)) {
            Session.selected = sel.includes(id) ? sel.filter(x => x !== id) : sel.concat([id]);
            anchorIndex = index;
        } else if ((modifiers & Qt.ShiftModifier) && anchorIndex >= 0) {
            const a = Math.min(anchorIndex, index), b = Math.max(anchorIndex, index);
            Session.selected = rows.slice(a, b + 1).map(r => r.versionId);
        } else {
            Session.selected = [id];
            anchorIndex = index;
        }
        Session.inspectTrack = rows[index].trackId;
        Session.inspectEntry = "";
    }

    /// Double-click / Enter: fill the slot being chosen, add the edit being
    /// chosen, else load onto the free deck (paused).
    function choose(row) {
        if (Session.fillingSlot) Session.fillSlot(row.versionId);
        else if (Session.pickingEdit) Session.addEdit({ recording: row.recordingId });
        else Session.loadDeck(Session.freeDeck, row.versionId, null);
    }

    function toggleFold(trackId) {
        const t = Session.toggled;
        Session.toggled = t.includes(trackId) ? t.filter(x => x !== trackId) : t.concat([trackId]);
    }

    function selectedRows() {
        return rows.filter(r => Session.selected.includes(r.versionId));
    }

    // A controller's browse knob and buttons. Its press loads (or fills the slot);
    // it never plays, so choosing songs stays silent.
    Connections {
        target: Backend
        function onEvent(name, p) {
            if (name !== "midi-ui") return;
            const i = list.currentIndex;
            const row = i >= 0 && i < root.rows.length ? root.rows[i] : null;
            if (p.action === "browse.scroll") root.move(p.delta || 0);
            else if (p.action === "browse.audition" && row) root.choose(row);
            else if (p.action === "deck.load" && p.deck !== undefined && row) Session.loadDeck(p.deck, row.versionId, null);
            else if (p.action === "browse.queueNext" && Session.selected.length) Session.addToSet(Session.selected, "next");
            else if (p.action === "browse.queueEnd" && Session.selected.length) Session.addToSet(Session.selected, "end");
        }
    }

    Keys.onDeletePressed: {
        const c = Session.view.kind === "crate" ? Session.crates.find(x => x.id === Session.view.id) : null;
        if (c && !c.smart && Session.selected.length) Session.run("crate_remove", { id: c.id, versionIds: Session.selected });
    }
    Keys.onUpPressed: move(-1)
    Keys.onDownPressed: move(1)
    Keys.onReturnPressed: { const i = list.currentIndex; if (i >= 0) choose(rows[i]); }
    Keys.onEscapePressed: { Session.fillingSlot = ""; Session.pickingEdit = ""; }
    function move(delta) {
        if (!rows.length) return;
        const i = Math.max(0, Math.min(rows.length - 1, list.currentIndex + delta));
        list.currentIndex = i;
        select(i, 0);
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header. Right-click it to choose the columns.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Theme.rowHeight
            color: Theme.system.button
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.system.mid }
            TapHandler { acceptedButtons: Qt.RightButton; onTapped: columnMenu.popup() }
            Row {
                anchors.fill: parent
                HeaderCell {
                    visible: root.positioned; width: root.colWidth("position"); text: "#"; alignment: Qt.AlignRight
                    sorted: root.sortMark("position"); onClicked: root.sortBy("position"); onContextMenu: columnMenu.popup()
                }
                HeaderCell {
                    width: root.wTitle
                    text: qsTr("Title")
                    indent: (root.view && root.view.foldable ? root.wFold : 0) + root.wMarker
                    sorted: root.sortMark("title")
                    onClicked: root.sortBy("title")
                    onContextMenu: columnMenu.popup()
                    resizable: true
                    onResized: dx => root.resize("title", dx)
                    onResizeFinished: root.saveColumns()
                    // Fold or unfold every song's versions.
                    ToolButton {
                        visible: !!root.view && root.view.foldable
                        x: 2
                        width: root.wFold - 4
                        height: parent.height
                        icon.source: Theme.icon(Session.fold ? "chevron-right" : "chevron-down")
                        icon.width: 12; icon.height: 12
                        ToolTip.visible: hovered
                        ToolTip.text: Session.fold ? qsTr("Unfold all: show every version of every song") : qsTr("Fold all: show each song once, its other versions under it")
                        onClicked: { Session.fold = !Session.fold; Session.toggled = []; }
                    }
                }
                Repeater {
                    model: root.optional
                    delegate: HeaderCell {
                        required property string modelData
                        readonly property string key: modelData === "plays" ? root.playsKey : modelData
                        visible: root.shows(modelData)
                        width: root.colWidth(key)
                        text: key === "played" ? qsTr("Played") : root.names[modelData]
                        alignment: key === "bpm" || key === "time" || key === "plays" ? Qt.AlignRight : Qt.AlignLeft
                        tip: key === "meter" ? qsTr("Triple (3/4, 6/8) or duple (2/4, 4/4)") : ""
                        sorted: root.sortMark(key)
                        onClicked: root.sortBy(key)
                        onContextMenu: columnMenu.popup()
                        resizable: true
                        onResized: dx => root.resize(key, dx)
                        onResizeFinished: root.saveColumns()
                    }
                }
            }
            Menu {
                id: columnMenu
                popupType: Popup.Native
                MenuItem {
                    text: qsTr("Fold Versions")
                    checkable: true
                    checked: Session.fold
                    enabled: !!root.view && root.view.foldable
                    onTriggered: { Session.fold = !Session.fold; Session.toggled = []; }
                }
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
                    onObjectAdded: (i, o) => columnMenu.insertItem(i + 2, o)
                    onObjectRemoved: (i, o) => columnMenu.removeItem(o)
                }
                MenuSeparator {}
                MenuItem {
                    text: qsTr("Reset Column Widths")
                    enabled: Object.keys(root.saved.widths).length > 0
                    onTriggered: { root.dragged = {}; Session.saveSettings({ libraryColumns: { hidden: root.saved.hidden, widths: {} } }); }
                }
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            focus: true
            model: root.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            reuseItems: true

            delegate: Item {
                id: rowItem
                required property var modelData
                required property int index
                readonly property var r: modelData
                readonly property bool selected: Session.selected.includes(r.versionId)
                readonly property bool onDeck: Session.decks.some(d => d && d.versionId === r.versionId)
                /// The room hears it (else, on a deck, it's previewed).
                readonly property bool audible: Session.decks.some(d => d && d.versionId === r.versionId && d.audible)
                /// The first of the songs suggested by tempo (a dashed line above them).
                readonly property bool firstSuggested: r.suggested && (index === 0 || !root.rows[index - 1].suggested)
                width: list.width
                height: Theme.rowHeight

                readonly property color fg: selected ? Theme.system.highlightedText : Theme.system.text

                Rectangle {
                    anchors.fill: parent
                    color: rowItem.selected ? Theme.system.highlight
                        : hover.hovered ? Qt.alpha(Theme.system.highlight, 0.12)
                        : rowItem.index % 2 ? Theme.system.alternateBase : Theme.system.base
                }
                HoverHandler { id: hover }
                // On a deck: a bar in its colour (red while the room hears it).
                Rectangle {
                    visible: rowItem.onDeck
                    width: 3; height: parent.height
                    color: rowItem.audible ? Theme.onAir : Theme.deckColors[0]
                }
                Row {
                    visible: rowItem.firstSuggested
                    width: parent.width
                    spacing: 4
                    Repeater {
                        model: Math.ceil(rowItem.width / 8)
                        delegate: Rectangle { width: 4; height: 1; color: Theme.system.mid }
                    }
                }
                HoverHandler { id: suggestedHover; enabled: rowItem.r.suggested }
                ToolTip.visible: suggestedHover.hovered && rowItem.r.suggested
                ToolTip.delay: 800
                ToolTip.text: qsTr("Not labelled %1 yet — its tempo fits").arg(Session.styleFilter.length === 1 ? Session.styleName(Session.styleFilter[0]) : "")

                Row {
                    anchors.fill: parent
                    spacing: 0
                    opacity: rowItem.r.playedInSet || rowItem.r.suggested || rowItem.r.availability === "missing" ? 0.55 : 1

                    NumberCell { visible: root.positioned; width: root.colWidth("position"); text: rowItem.r.position || ""; color: rowItem.fg; opacity: Theme.dim }
                    // Title: a tree (a song's other versions indented under it). Every row
                    // keeps the fold and source slots, so titles line up.
                    Item {
                        width: root.wTitle
                        height: parent.height
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 6
                            Item { visible: rowItem.r.child; width: root.wIndent; height: 1 }
                            Item {
                                visible: !!root.view && root.view.foldable
                                width: root.wFold - (rowItem.r.child ? root.wIndent : 0)
                                height: parent.height
                                AbstractButton {
                                    visible: rowItem.r.foldCount > 1
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Math.min(parent.width, implicitWidth)
                                    height: parent.height - 4
                                    padding: 0
                                    hoverEnabled: true
                                    contentItem: Row {
                                        spacing: 1
                                        Icon {
                                            anchors.verticalCenter: parent.verticalCenter
                                            glyph: rowItem.r.foldOpen ? "chevron-down" : "chevron-right"
                                            sourceSize.width: 12; sourceSize.height: 12
                                            color: rowItem.fg
                                        }
                                        Label {
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: rowItem.r.foldCount
                                            font.pointSize: Theme.smallPointSize
                                            color: rowItem.fg
                                            opacity: Theme.dim
                                        }
                                    }
                                    ToolTip.visible: hovered
                                    ToolTip.text: rowItem.r.foldOpen ? qsTr("Fold its versions") : qsTr("Show all %1 versions").arg(rowItem.r.foldCount)
                                    onClicked: root.toggleFold(rowItem.r.trackId)
                                }
                            }
                            // Where it plays from: nothing for a file, ○ Spotify only, ● a file and Spotify; ⚠ missing.
                            Item {
                                width: root.wMarker
                                height: parent.height
                                Rectangle {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 7; height: 7; radius: 4
                                    visible: rowItem.r.availability === "spotifyOnly" || rowItem.r.availability === "fileAndSpotify"
                                    color: rowItem.r.availability === "fileAndSpotify" ? Theme.spotify : "transparent"
                                    border.color: Theme.spotify
                                    border.width: 1.5
                                }
                                Icon {
                                    visible: rowItem.r.availability === "missing"
                                    anchors.verticalCenter: parent.verticalCenter
                                    glyph: "warning"
                                    sourceSize.width: 12; sourceSize.height: 12
                                    color: Theme.warn
                                }
                            }
                            Label {
                                width: parent.width - x - 6
                                height: parent.height
                                verticalAlignment: Text.AlignVCenter
                                text: rowItem.r.title
                                elide: Text.ElideRight
                                // On a deck: in its colour (as the bar).
                                color: rowItem.onDeck && !rowItem.selected ? (rowItem.audible ? Theme.onAir : Theme.deckColors[0]) : rowItem.fg
                                font.italic: rowItem.r.suggested
                                opacity: rowItem.r.child ? Theme.dim : 1
                            }
                        }
                    }
                    Cell { visible: root.shows("artist"); width: root.colWidth("artist"); text: rowItem.r.artist; color: rowItem.fg }
                    Cell { visible: root.shows("version"); width: root.colWidth("version"); text: rowItem.r.versionLabel; color: rowItem.fg; opacity: Theme.dim }
                    Item {
                        visible: root.shows("style")
                        width: root.colWidth("style")
                        height: parent.height
                        clip: true
                        Row {
                            x: 6
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3
                            Label { visible: rowItem.r.suggested; text: qsTr("tempo fits"); color: rowItem.fg; opacity: Theme.dim; font.italic: true }
                            Repeater {
                                model: rowItem.r.suggested ? [] : rowItem.r.styles
                                delegate: StyleBadge { required property var modelData; style: modelData }
                            }
                        }
                    }
                    NumberCell {
                        visible: root.shows("bpm")
                        width: root.colWidth("bpm")
                        color: rowItem.selected ? rowItem.fg : rowItem.r.tempoFit === "sweet" ? Theme.ok : rowItem.r.tempoFit === "outside" ? Theme.warn : rowItem.fg
                        text: (rowItem.r.analyzing ? "… " : "") + Session.fmtBpm(rowItem.r.bpm) + (Math.abs(rowItem.r.rate - 1) > 0.001 ? "  " + Session.fmtPct(rowItem.r.rate) : "")
                    }
                    NumberCell { visible: root.shows("time"); width: root.colWidth("time"); text: Session.fmtTime(rowItem.r.durationMs); color: rowItem.r.long && !rowItem.selected ? Theme.warn : rowItem.fg }
                    Cell { visible: root.shows("meter"); width: root.colWidth("meter"); text: Session.meterName(rowItem.r.meter); color: rowItem.fg; opacity: Theme.dim }
                    NumberCell {
                        visible: root.shows("plays")
                        width: root.colWidth(root.playsKey)
                        horizontalAlignment: rowItem.r.playedAt ? Text.AlignLeft : Text.AlignRight
                        color: rowItem.fg
                        opacity: Theme.dim
                        text: rowItem.r.playedAt ? new Date(rowItem.r.playedAt).toLocaleString(Qt.locale(), "MMM d, h:mm ap") : rowItem.r.plays || ""
                    }
                    Row {
                        width: root.wActions
                        height: parent.height
                        spacing: 0
                        ToolButton {
                            icon.source: Theme.icon("headphones")
                            icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                            icon.color: rowItem.fg
                            width: root.wActions / 2
                            height: parent.height
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Preview on the free deck")
                            onClicked: Session.preview(rowItem.r.versionId, null)
                        }
                        ToolButton {
                            icon.source: Theme.icon(Session.fillingSlot ? "check" : "plus")
                            icon.width: Theme.smallIconSize; icon.height: Theme.smallIconSize
                            icon.color: rowItem.fg
                            width: root.wActions / 2
                            height: parent.height
                            ToolTip.visible: hovered
                            ToolTip.text: Session.fillingSlot ? qsTr("Use this song for the slot") : qsTr("Play next")
                            onClicked: Session.fillingSlot ? Session.fillSlot(rowItem.r.versionId) : Session.addToSet([rowItem.r.versionId], "next")
                        }
                    }
                }

                MouseArea {
                    id: mouse
                    anchors.fill: parent
                    anchors.rightMargin: root.wActions
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    drag.target: dragProxy
                    onPressed: m => {
                        root.forceActiveFocus();
                        list.currentIndex = rowItem.index;
                        if (m.button === Qt.RightButton) {
                            if (!rowItem.selected) root.select(rowItem.index, 0);
                            menu.rows = root.selectedRows();
                            menu.historyRow = rowItem.r;
                            menu.popup();
                        } else if (!rowItem.selected || m.modifiers) {
                            root.select(rowItem.index, m.modifiers);
                        }
                    }
                    onReleased: m => {
                        dragProxy.Drag.drop();
                        dragProxy.x = 0; dragProxy.y = 0;
                    }
                    onDoubleClicked: root.choose(rowItem.r)
                }
                Item {
                    id: dragProxy
                    Drag.active: mouse.drag.active
                    Drag.dragType: Drag.Automatic
                    Drag.supportedActions: Qt.CopyAction
                    Drag.keys: ["application/x-orpheus-versions"]
                    Drag.mimeData: ({ "application/x-orpheus-versions": JSON.stringify(Session.selected.length ? Session.selected : [rowItem.r.versionId]) })
                }
            }
        }
    }

    // When nothing's here, why.
    ColumnLayout {
        anchors.centerIn: parent
        visible: !!root.view && !root.rows.length
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

    SongMenu {
        id: menu
        onCombine: (songs, recordings) => root.combine(songs, recordings)
    }
}
