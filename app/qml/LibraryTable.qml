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

    // Column widths (px). Title, artist and version share what the others leave
    // (by `weights`, down to `minimums`) until a header's edge is dragged, which
    // fixes that column's width (in `widths`).
    property var widths: ({ position: 34, style: 92, bpm: 88, time: 52, meter: 56, plays: 52, played: 124 })
    readonly property var weights: ({ title: 2.2, artist: 1.4, version: 0.8 })
    readonly property var minimums: ({ title: 140, artist: 90, version: 60 })
    readonly property int wActions: 56
    readonly property string playsKey: Session.view.kind === "history" ? "played" : "plays"
    readonly property var flex: {
        const fixed = (positioned ? widths.position : 0) + widths.style + widths.bpm + widths.time + widths.meter + widths[playsKey] + wActions;
        let free = list.width - fixed;
        let weight = 0;
        for (const k of ["title", "artist", "version"]) {
            if (widths[k] !== undefined) free -= widths[k];
            else weight += weights[k];
        }
        const out = {};
        for (const k of ["title", "artist", "version"])
            out[k] = widths[k] !== undefined ? widths[k] : Math.max(minimums[k], free * weights[k] / weight);
        return out;
    }
    readonly property real wTitle: flex.title
    function colWidth(key) { return flex[key] !== undefined ? flex[key] : widths[key]; }
    function resize(key, dx) {
        const w = Object.assign({}, widths);
        w[key] = Math.max(minimums[key] || 32, colWidth(key) + dx);
        widths = w;
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

        // Header.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Theme.rowHeight
            color: Theme.system.button
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.system.mid }
            Row {
                anchors.fill: parent
                HeaderCell {
                    visible: root.positioned; width: root.widths.position; text: "#"; alignment: Qt.AlignRight
                    sorted: root.sortMark("position"); onClicked: root.sortBy("position")
                }
                HeaderCell {
                    id: titleHeader
                    width: root.wTitle
                    resizable: true
                    onResized: dx => root.resize("title", dx)
                    text: qsTr("Title")
                    indent: (root.view && root.view.foldable ? root.wFold : 0) + root.wMarker
                    sorted: root.sortMark("title")
                    onClicked: root.sortBy("title")
                    tip: root.view && root.view.foldable ? qsTr("Right-click: fold or unfold every song's versions") : ""
                    onContextMenu: if (root.view && root.view.foldable) { Session.fold = !Session.fold; Session.toggled = []; }
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
                HeaderCell { width: root.colWidth("artist"); text: qsTr("Artist"); sorted: root.sortMark("artist"); onClicked: root.sortBy("artist"); resizable: true; onResized: dx => root.resize("artist", dx) }
                HeaderCell { width: root.colWidth("version"); text: qsTr("Version"); sorted: root.sortMark("version"); onClicked: root.sortBy("version"); resizable: true; onResized: dx => root.resize("version", dx) }
                HeaderCell { width: root.widths.style; text: qsTr("Style"); sorted: root.sortMark("style"); onClicked: root.sortBy("style"); resizable: true; onResized: dx => root.resize("style", dx) }
                HeaderCell { width: root.widths.bpm; text: qsTr("BPM"); alignment: Qt.AlignRight; sorted: root.sortMark("bpm"); onClicked: root.sortBy("bpm"); resizable: true; onResized: dx => root.resize("bpm", dx) }
                HeaderCell { width: root.widths.time; text: qsTr("Time"); alignment: Qt.AlignRight; sorted: root.sortMark("time"); onClicked: root.sortBy("time"); resizable: true; onResized: dx => root.resize("time", dx) }
                HeaderCell {
                    width: root.widths.meter; text: qsTr("Meter"); sorted: root.sortMark("meter"); onClicked: root.sortBy("meter")
                    tip: qsTr("Triple (3/4, 6/8) or duple (2/4, 4/4)"); resizable: true; onResized: dx => root.resize("meter", dx)
                }
                HeaderCell {
                    width: root.widths[root.playsKey]
                    text: Session.view.kind === "history" ? qsTr("Played") : qsTr("Plays")
                    alignment: Session.view.kind === "history" ? Qt.AlignLeft : Qt.AlignRight
                    sorted: root.sortMark(root.playsKey)
                    onClicked: root.sortBy(root.playsKey)
                    resizable: true
                    onResized: dx => root.resize(root.playsKey, dx)
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
                    color: Session.decks.some(d => d && d.versionId === rowItem.r.versionId && d.audible) ? Theme.onAir : Theme.deckColors[0]
                }

                Row {
                    anchors.fill: parent
                    spacing: 0
                    opacity: rowItem.r.playedInSet || rowItem.r.suggested ? 0.55 : 1

                    NumberCell { visible: root.positioned; width: root.widths.position; text: rowItem.r.position || ""; color: rowItem.fg; opacity: Theme.dim }
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
                                            font.pixelSize: 11
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
                                color: rowItem.fg
                                opacity: rowItem.r.child ? Theme.dim : 1
                            }
                        }
                    }
                    Cell { width: root.colWidth("artist"); text: rowItem.r.artist; color: rowItem.fg }
                    Cell { width: root.colWidth("version"); text: rowItem.r.versionLabel; color: rowItem.fg; opacity: Theme.dim }
                    Item {
                        width: root.widths.style
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
                        width: root.widths.bpm
                        color: rowItem.selected ? rowItem.fg : rowItem.r.tempoFit === "sweet" ? Theme.ok : rowItem.r.tempoFit === "outside" ? Theme.warn : rowItem.fg
                        text: (rowItem.r.analyzing ? "… " : "") + Session.fmtBpm(rowItem.r.bpm) + (Math.abs(rowItem.r.rate - 1) > 0.001 ? "  " + Session.fmtPct(rowItem.r.rate) : "")
                    }
                    NumberCell { width: root.widths.time; text: Session.fmtTime(rowItem.r.durationMs); color: rowItem.r.long && !rowItem.selected ? Theme.warn : rowItem.fg }
                    Cell { width: root.widths.meter; text: Session.meterName(rowItem.r.meter); color: rowItem.fg; opacity: Theme.dim }
                    NumberCell {
                        width: root.widths[root.playsKey]
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
    Label {
        anchors.centerIn: parent
        visible: !!root.view && !root.rows.length
        width: parent.width * 0.6
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        opacity: 0.7
        text: !root.view ? "" : root.view.librarySongs === 0 ? qsTr("Your library is empty.\nAdd a music folder in Settings → Library, drag files onto this window, or connect Spotify.")
            : Session.search ? qsTr("No songs match “%1”.").arg(Session.search)
            : Session.styleFilter.length ? qsTr("No songs here match the style filter. Drag songs onto a style above to label them.")
            : qsTr("Nothing here yet.")
    }

    SongMenu {
        id: menu
        onCombine: (songs, recordings) => root.combine(songs, recordings)
    }
}
