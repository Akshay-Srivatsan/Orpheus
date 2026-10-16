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

    // Column widths (px); the title takes what's left.
    readonly property int wPosition: 32
    readonly property int wArtist: Math.max(100, width * 0.18)
    readonly property int wVersion: Math.max(70, width * 0.11)
    readonly property int wStyle: 100
    readonly property int wBpm: 84
    readonly property int wTime: 48
    readonly property int wMeter: 52
    readonly property int wPlays: Session.view.kind === "history" ? 110 : 48
    readonly property int wActions: 52
    readonly property int wTitle: Math.max(120, width - 4 - (positioned ? wPosition : 0) - wArtist - wVersion - wStyle - wBpm - wTime - wMeter - wPlays - wActions)

    function sortBy(key) {
        if (Session.sort === key || (Session.sort === "" && view && view.sort === key)) Session.descending = !Session.descending;
        else { Session.sort = key; Session.descending = false; }
    }
    function sortMark(key) {
        return view && view.sort === key ? (Session.descending ? " ▼" : " ▲") : "";
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
            color: palette.button
            Row {
                anchors.fill: parent
                anchors.leftMargin: 4
                spacing: 0
                HeaderCell { visible: root.positioned; width: root.wPosition; text: "#" + root.sortMark("position"); onClicked: root.sortBy("position") }
                HeaderCell { width: root.wTitle; text: qsTr("Title") + root.sortMark("title"); onClicked: root.sortBy("title") }
                HeaderCell { width: root.wArtist; text: qsTr("Artist") + root.sortMark("artist"); onClicked: root.sortBy("artist") }
                HeaderCell {
                    width: root.wVersion
                    text: qsTr("Version") + root.sortMark("version") + (root.view && root.view.foldable ? (Session.fold ? "  ▸" : "  ▾") : "")
                    onClicked: root.sortBy("version")
                    tip: root.view && root.view.foldable ? qsTr("Right-click: fold or unfold all songs' versions") : ""
                    onContextMenu: { Session.fold = !Session.fold; Session.toggled = []; }
                }
                HeaderCell { width: root.wStyle; text: qsTr("Style") + root.sortMark("style"); onClicked: root.sortBy("style") }
                HeaderCell { width: root.wBpm; text: qsTr("BPM") + root.sortMark("bpm"); onClicked: root.sortBy("bpm") }
                HeaderCell { width: root.wTime; text: qsTr("Time") + root.sortMark("time"); onClicked: root.sortBy("time") }
                HeaderCell { width: root.wMeter; text: qsTr("Meter") + root.sortMark("meter"); onClicked: root.sortBy("meter"); tip: qsTr("Triple (3/4, 6/8) or duple (2/4, 4/4)") }
                HeaderCell {
                    width: root.wPlays
                    text: (Session.view.kind === "history" ? qsTr("Played") : qsTr("Plays")) + root.sortMark(Session.view.kind === "history" ? "played" : "plays")
                    onClicked: root.sortBy(Session.view.kind === "history" ? "played" : "plays")
                }
                Item { width: root.wActions }
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

                Rectangle {
                    anchors.fill: parent
                    color: rowItem.selected ? palette.highlight : rowItem.index % 2 ? palette.alternateBase : palette.base
                    opacity: rowItem.selected ? 0.55 : 1
                }
                Rectangle {
                    visible: rowItem.onDeck
                    width: 3; height: parent.height
                    color: Session.decks.some(d => d && d.versionId === rowItem.r.versionId && d.audible) ? Theme.onAir : Theme.deckColors[0]
                }

                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 4
                    spacing: 0
                    opacity: rowItem.r.playedInSet || rowItem.r.suggested ? 0.55 : 1

                    Label { visible: root.positioned; width: root.wPosition; height: parent.height; verticalAlignment: Text.AlignVCenter; text: rowItem.r.position || ""; opacity: 0.6 }
                    // Title: a tree (a song's other versions indented under it).
                    RowLayout {
                        width: root.wTitle
                        height: parent.height
                        spacing: 4
                        Item { visible: rowItem.r.child; Layout.preferredWidth: 14 }
                        ToolButton {
                            visible: !!root.view && root.view.foldable
                            enabled: rowItem.r.foldCount > 1
                            opacity: enabled ? 1 : 0
                            Layout.preferredWidth: 30
                            Layout.preferredHeight: Theme.rowHeight - 2
                            text: rowItem.r.foldCount > 1 ? (rowItem.r.foldOpen ? "▾" : "▸") + rowItem.r.foldCount : ""
                            font.pixelSize: 10
                            onClicked: root.toggleFold(rowItem.r.trackId)
                        }
                        // Where it plays from: ○ Spotify only, ● a file and Spotify.
                        Rectangle {
                            Layout.preferredWidth: 7
                            Layout.preferredHeight: 7
                            radius: 4
                            visible: rowItem.r.availability === "spotifyOnly" || rowItem.r.availability === "fileAndSpotify"
                            color: rowItem.r.availability === "fileAndSpotify" ? Theme.spotify : "transparent"
                            border.color: Theme.spotify
                            border.width: 1.5
                        }
                        Label { visible: rowItem.r.availability === "missing"; text: "⚠"; color: Theme.warn }
                        Label { Layout.fillWidth: true; text: rowItem.r.title; elide: Text.ElideRight }
                    }
                    Label { width: root.wArtist; height: parent.height; verticalAlignment: Text.AlignVCenter; text: rowItem.r.artist; elide: Text.ElideRight }
                    Label { width: root.wVersion; height: parent.height; verticalAlignment: Text.AlignVCenter; text: rowItem.r.versionLabel; elide: Text.ElideRight; opacity: 0.6 }
                    Item {
                        width: root.wStyle
                        height: parent.height
                        clip: true
                        Row {
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3
                            Label { visible: rowItem.r.suggested; text: qsTr("tempo fits"); opacity: 0.7; font.italic: true }
                            Repeater {
                                model: rowItem.r.suggested ? [] : rowItem.r.styles
                                delegate: StyleBadge { required property var modelData; style: modelData }
                            }
                        }
                    }
                    Label {
                        width: root.wBpm
                        height: parent.height
                        verticalAlignment: Text.AlignVCenter
                        color: rowItem.r.tempoFit === "sweet" ? Theme.ok : rowItem.r.tempoFit === "outside" ? Theme.warn : palette.text
                        text: Session.fmtBpm(rowItem.r.bpm) + (Math.abs(rowItem.r.rate - 1) > 0.001 ? "  " + Session.fmtPct(rowItem.r.rate) : "") + (rowItem.r.analyzing ? " …" : "")
                    }
                    Label { width: root.wTime; height: parent.height; verticalAlignment: Text.AlignVCenter; text: Session.fmtTime(rowItem.r.durationMs); color: rowItem.r.long ? Theme.warn : palette.text }
                    Label { width: root.wMeter; height: parent.height; verticalAlignment: Text.AlignVCenter; text: Session.meterName(rowItem.r.meter); opacity: 0.6 }
                    Label {
                        width: root.wPlays
                        height: parent.height
                        verticalAlignment: Text.AlignVCenter
                        opacity: 0.6
                        text: rowItem.r.playedAt ? new Date(rowItem.r.playedAt).toLocaleString(Qt.locale(), "MMM d, h:mm ap") : rowItem.r.plays || ""
                    }
                    Row {
                        width: root.wActions
                        height: parent.height
                        spacing: 0
                        ToolButton {
                            icon.source: Theme.icon("headphones")
                            icon.width: 14; icon.height: 14
                            width: root.wActions / 2
                            height: parent.height
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Preview on the free deck")
                            onClicked: Session.preview(rowItem.r.versionId, null)
                        }
                        ToolButton {
                            icon.source: Theme.icon(Session.fillingSlot ? "check" : "plus")
                            icon.width: 14; icon.height: 14
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
