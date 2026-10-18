import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The middle of the window: the view's title and search, the style filter,
// and its songs (`library_view`, asked for again when anything it shows changes).
Pane {
    id: root
    padding: 0

    property var view: null
    /// In the simple interface (see LibraryTable.simple).
    property bool simple: false
    property alias searchField: search
    /// The songs table (the Library menu acts on its selection).
    property alias table: table

    signal combine(var songs, var recordings)
    signal editSmartCrate(string id)

    readonly property bool spotifyView: Session.view.kind === "spotify-search" || Session.view.kind === "spotify-playlist"
    readonly property bool filesView: Session.view.kind === "files"
    readonly property var crate: Session.view.kind === "crate" ? Session.crates.find(c => c.id === Session.view.id) : null
    readonly property string title: {
        const v = Session.view;
        switch (v.kind) {
        case "all": return qsTr("All Songs");
        case "crate": return crate ? crate.name : qsTr("Crate");
        case "setlist": { const s = Session.setlists.find(x => x.id === v.id); return s ? s.name : qsTr("Setlist"); }
        case "history": return qsTr("History");
        case "missing": return qsTr("Missing Files");
        case "club-songs": return Session.activeClub ? qsTr("%1's Songs").arg(Session.activeClub.name) : qsTr("Club Songs");
        case "files": return qsTr("Files");
        }
        return "";
    }

    function query() {
        const kind = Session.view.kind;
        const v = kind === "crate" || kind === "setlist" ? { kind: kind, id: Session.view.id } : { kind: kind };
        return {
            view: v,
            styleFilter: Session.styleFilter,
            search: Session.search,
            sort: Session.sort || null,
            descending: Session.descending,
            suggestByTempo: Session.suggestByTempo,
            // A chime is often a version kept out of the lists.
            showUnlisted: Session.showUnlisted || !!Session.chimePick,
            rules: Session.filterConditions.length ? { match: "all", conditions: Session.filterConditions } : null,
        };
    }
    /// Club songs: how many of the club's suggested songs are here.
    property var clubSongCount: null
    /// A smart crate's rules, described.
    property string rulesText: ""
    function reloadHeader() {
        clubSongCount = null;
        rulesText = "";
        if (Session.view.kind === "club-songs" && Session.activeClub)
            Backend.call("club_song_status", { clubId: Session.activeClub.id, versionId: Session.activeClub.version }).then(m => {
                const all = Object.values(m);
                root.clubSongCount = { have: all.filter(x => x.length).length, total: all.length };
            }, () => {});
        if (crate && crate.smart)
            Backend.call("smart_describe", { rules: crate.smart }).then(d => root.rulesText = d.join(crate.smart.match === "any" ? "\n" + qsTr("or") + " " : "\n" + qsTr("and") + " "));
    }
    onCrateChanged: reloadHeader()

    /// A reload waits while something is dragged (see `Session.dragging`).
    property bool missed: false
    function reload() {
        missed = false;
        // Not shown (the other interface's): it catches up when it is.
        if (Session.dragging || !visible) { missed = true; return; }
        reloadHeader();
        if (spotifyView || filesView) return;
        const q = query();
        Backend.call("library_view", { query: q }).then(v => { if (Session.dragging) root.missed = true; else root.view = v; }, e => Session.toast(String(e)));
    }

    // Anything the view depends on: ask again (once, shortly after).
    Timer { id: later; interval: 30; onTriggered: root.reload() }
    Connections {
        target: Session
        function onViewChanged() { Session.sort = ""; Session.descending = false; later.restart(); }
        function onStyleFilterChanged() { later.restart(); }
        function onSearchChanged() { later.restart(); }
        function onSortChanged() { later.restart(); }
        function onDescendingChanged() { later.restart(); }
        function onSuggestByTempoChanged() { later.restart(); }
        function onShowUnlistedChanged() { later.restart(); }
        function onFilterConditionsChanged() { later.restart(); }
        function onDraggingChanged() { if (!Session.dragging && root.missed) later.restart(); }
        function onChimePickChanged() { later.restart(); }
        function onLibraryChanged() { later.restart(); }
        function onSetlistChanged(id) { if (Session.view.kind === "setlist" || Session.view.kind === "history") later.restart(); }
        function onHistoryChanged() { if (Session.view.kind === "history") later.restart(); }
    }
    // What the rows take from the performance and the club (songs played in
    // the open set are dimmed; the club's styles): reload when that changes,
    // not on every change to them (each action in the set changes them).
    readonly property string performKey: !Session.perform ? "" : Session.perform.setlistId + "|" + (Session.set
        ? Session.set.rows.filter(r => r.entry.status === "played" || r.entry.status === "playing").map(r => r.entry.versionId).join(",") : "")
    readonly property string clubKey: JSON.stringify(Session.activeClub)
    onPerformKeyChanged: later.restart()
    onClubKeyChanged: later.restart()
    Component.onCompleted: reload()
    onVisibleChanged: if (visible && missed) later.restart()

    SpotifyBrowser {
        visible: root.spotifyView
        anchors.fill: parent
    }

    ColumnLayout {
        visible: !root.spotifyView
        anchors.fill: parent
        spacing: 4

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            Layout.bottomMargin: 0
            Icon { visible: !!root.crate && !!root.crate.smart; glyph: "auto" }
            Label { Layout.fillWidth: true; Layout.minimumWidth: 60; elide: Text.ElideRight; text: root.title; font.bold: true; font.pointSize: Theme.titlePointSize }
            Label {
                visible: !!root.crate && !!root.crate.smart
                text: qsTr("smart crate")
                opacity: Theme.dim
                ToolTip.visible: rulesHover.hovered && !!root.rulesText
                ToolTip.text: root.rulesText
                HoverHandler { id: rulesHover }
            }
            Label {
                visible: Session.view.kind === "club-songs" && !!root.clubSongCount
                text: root.clubSongCount ? qsTr("%1 of %2 suggested songs here").arg(root.clubSongCount.have).arg(root.clubSongCount.total) : ""
                opacity: Theme.dim
            }
            Button {
                enabled: !!root.crate && !!root.crate.smart
                text: qsTr("Edit Rules…")
                onClicked: root.editSmartCrate(root.crate.id)
            }
            Button {
                enabled: Session.view.kind === "club-songs"
                text: qsTr("Get the club's other songs…")
                onClicked: Session.settingsPage = "clubs"
            }
            SearchField {
                id: search
                Layout.fillWidth: true
                Layout.minimumWidth: 120
                Layout.maximumWidth: 280
                placeholderText: qsTr("Search title, artist, style…")
                text: Session.search
                onTextEdited: Session.search = text
                onClearButtonPressed: Session.search = ""
                Keys.onEscapePressed: { Session.search = ""; focus = false; }
            }
        }

        FilesView {
            visible: root.filesView
            Layout.fillWidth: true
            Layout.fillHeight: true
        }

        StyleFilterBar {
            visible: !root.filesView
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            view: root.view
        }

        // Choosing a song for a style slot, or audio for an edit.
        Pane {
            visible: !!Session.fillingSlot || !!Session.pickingEdit || !!Session.chimePick
            Layout.fillWidth: true
            padding: 6
            background: Rectangle { color: Qt.alpha(Theme.system.highlight, 0.15); Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.system.highlight } }
            RowLayout {
                anchors.fill: parent
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    text: Session.chimePick ? qsTr("Choosing the chime for %1: double-click a version of a song.").arg(Session.chimePick.title)
                        : Session.fillingSlot
                        ? (Session.fillingRow ? qsTr("Choosing a song for slot %1 (%2): double-click a song, press its check button, or drag it onto the slot.")
                            .arg(Session.fillingRow.number || "").arg(Session.fillingRow.styles.map(s => s.name).join(" / ") || Session.fillingRow.title)
                            : qsTr("Choosing a song for the slot: double-click a song, press its check button, or drag it onto the slot."))
                        : qsTr("Choosing audio for a new version of “%1”: double-click a song to add its audio.").arg(Session.pickingEditTitle)
                }
                Button { text: qsTr("Show All Songs"); enabled: !!Session.fillingSlot && Session.styleFilter.length > 0; onClicked: Session.styleFilter = [] }
                Button { text: qsTr("Cancel"); onClicked: { Session.fillingSlot = ""; Session.pickingEdit = ""; Session.chimePick = null; } }
            }
        }

        LibraryTable {
            id: table
            simple: root.simple
            visible: !root.filesView
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.bottomMargin: Theme.spacing
            view: root.view
            onCombine: (songs, recordings) => root.combine(songs, recordings)
        }

        SpotifySearchSection {
            visible: !root.filesView && !!Session.spotify && Session.spotify.connected && query.trim().length >= minLength
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.bottomMargin: Theme.spacing
            query: Session.search
        }
    }
}
