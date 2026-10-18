import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// The library's views, your crates and setlists, and Spotify.
Pane {
    id: root
    padding: Theme.spacing

    property string renamingSet: ""

    signal newSetlist()
    signal editSmartCrate(string id)

    function isView(v) {
        const c = Session.view;
        return c.kind === v.kind && (c.id === undefined || c.id === v.id) && (c.uri === undefined || c.uri === v.uri);
    }

    function importReport(r) {
        const parts = [qsTr("%1 linked to your songs").arg(r.linked)];
        if (r.spotify) parts.push(qsTr("%1 from Spotify").arg(r.spotify));
        if (r.missing) parts.push(qsTr("%1 not here yet (they link up when you add the audio)").arg(r.missing));
        Session.toast(qsTr("Imported: %1.").arg(parts.join(", ")), r.missing ? "error" : "info");
    }

    function importCrate() {
        Session.chooseFile({ title: qsTr("Import a crate"), nameFilters: [qsTr("Orpheus crate (*.toml)")] }, path => {
            Session.run("crate_import", { path: path }).then(r => { if (r) { Session.show({ kind: "crate", id: r.setlistId }); root.importReport(r); } });
        });
    }
    function importSet() {
        Session.chooseFile({ title: qsTr("Import a set"), nameFilters: [qsTr("Orpheus set (*.toml)")] }, path => {
            Session.run("setlist_import", { path: path }).then(r => {
                if (!r) return;
                Session.run("perform_open", { setlistId: r.setlistId });
                root.importReport(r);
            });
        });
    }
    function exportCrate(c) {
        Session.chooseFile({ title: qsTr("Export “%1”").arg(c.name), nameFilters: [qsTr("Orpheus crate (*.toml)")], save: true, name: c.name + ".orpheus-crate.toml" }, path => {
            Session.run("crate_export", { id: c.id, path: path }).then(r => { if (r !== undefined) Session.toast(qsTr("Exported “%1”.").arg(c.name), "info"); });
        });
    }

    // One surface for its rows and section titles.
    ViewFrame {
        anchors.fill: parent
        ScrollView {
            anchors.fill: parent
            contentWidth: availableWidth

            ColumnLayout {
                width: parent.width
                spacing: 2

                SectionTitle { text: qsTr("Library") }
                NavItem {
                    text: qsTr("All Songs"); icon.source: Theme.icon("music")
                    count: Session.summary ? Session.summary.songs : 0
                    selected: root.isView({ kind: "all" })
                    onClicked: Session.show({ kind: "all" })
                }
                NavItem {
                    text: qsTr("History"); icon.source: Theme.icon("history")
                    selected: root.isView({ kind: "history" })
                    onClicked: Session.show({ kind: "history" })
                }
                NavItem {
                    text: qsTr("Files"); icon.source: Theme.icon("disc")
                    selected: root.isView({ kind: "files" })
                    tip: qsTr("The audio files in your library folders: add ones removed from the library, or remove some")
                    onClicked: Session.show({ kind: "files" })
                }
                NavItem {
                    visible: !!Session.summary && Session.summary.missing > 0
                    text: qsTr("Missing Files"); icon.source: Theme.icon("warning")
                    warning: true
                    count: Session.summary ? Session.summary.missing : 0
                    selected: root.isView({ kind: "missing" })
                    onClicked: Session.show({ kind: "missing" })
                }

                SectionTitle {
                    text: qsTr("Crates")
                    actions: [
                        Action { icon.source: Theme.icon("plus"); text: qsTr("New Crate"); onTriggered: crateAddMenu.popup() }
                    ]
                    FitMenu {
                        id: crateAddMenu
                        MenuItem { text: qsTr("New Crate"); onTriggered: Session.newCrateWith([]) }
                        MenuItem { text: qsTr("New Smart Crate…"); onTriggered: root.editSmartCrate("new") }
                        MenuSeparator {}
                        MenuItem { text: qsTr("Import Crate…"); onTriggered: root.importCrate() }
                    }
                }
                NavItem {
                    visible: !!Session.activeClub
                    text: Session.activeClub ? qsTr("%1's Songs").arg(Session.activeClub.name) : ""
                    icon.source: Theme.icon("star")
                    count: Session.summary ? Session.summary.clubSongs : 0
                    tip: Session.activeClub ? qsTr("The songs %1 suggests for its styles that you have (Settings → Clubs lists the others)").arg(Session.activeClub.name) : ""
                    selected: root.isView({ kind: "club-songs" })
                    onClicked: Session.show({ kind: "club-songs" })
                }
                Repeater {
                    model: Session.crates
                    delegate: Loader {
                        id: crateRow
                        required property var modelData
                        Layout.fillWidth: true
                        sourceComponent: Session.renamingCrate === modelData.id ? renameField : crateItem
                        Component {
                            id: crateItem
                            NavItem {
                                text: crateRow.modelData.name
                                icon.source: Theme.icon(crateRow.modelData.smart ? "auto" : "crate")
                                count: Session.summary && Session.summary.crates[crateRow.modelData.id] !== undefined ? Session.summary.crates[crateRow.modelData.id] : 0
                                selected: root.isView({ kind: "crate", id: crateRow.modelData.id })
                                tip: crateRow.modelData.smart ? qsTr("Smart crate — double-click to edit its rules") : ""
                                dropKeys: crateRow.modelData.smart ? [] : ["application/x-orpheus-versions"]
                                onClicked: Session.show({ kind: "crate", id: crateRow.modelData.id })
                                onDoubleClicked: crateRow.modelData.smart ? root.editSmartCrate(crateRow.modelData.id) : Session.renamingCrate = crateRow.modelData.id
                                onDropped: ids => Session.run("crate_add", { id: crateRow.modelData.id, versionIds: ids })
                                onContextMenu: crateMenu.popup()
                                FitMenu {
                                    id: crateMenu
                                    MenuItem { text: qsTr("Rename"); onTriggered: Session.renamingCrate = crateRow.modelData.id }
                                    MenuItem { visible: !!crateRow.modelData.smart; text: qsTr("Edit Rules…"); onTriggered: root.editSmartCrate(crateRow.modelData.id) }
                                    MenuSeparator {}
                                    MenuItem { text: qsTr("Export Crate…"); onTriggered: root.exportCrate(crateRow.modelData) }
                                    MenuItem {
                                        text: qsTr("Delete Crate…")
                                        onTriggered: Session.confirm(qsTr("Delete the crate “%1”? Its songs stay in your library.").arg(crateRow.modelData.name), qsTr("Delete"),
                                            ok => { if (ok) Session.run("crate_delete", { id: crateRow.modelData.id }); })
                                    }
                                }
                            }
                        }
                        Component {
                            id: renameField
                            TextField {
                                text: crateRow.modelData.name
                                Component.onCompleted: { forceActiveFocus(); selectAll(); }
                                function done(save) {
                                    if (save) Session.run("crate_rename", { id: crateRow.modelData.id, name: text });
                                    Session.renamingCrate = "";
                                }
                                onAccepted: done(true)
                                onEditingFinished: if (Session.renamingCrate === crateRow.modelData.id) done(true)
                                Keys.onEscapePressed: done(false)
                            }
                        }
                    }
                }
                Label {
                    visible: !Session.crates.length
                    Layout.fillWidth: true
                    Layout.margins: Theme.spacing
                    wrapMode: Text.WordWrap
                    font.pointSize: Theme.smallPointSize
                    opacity: 0.6
                    text: qsTr("Crates are your own collections (any mix of dance styles). Drag songs onto one to add them; smart crates fill themselves from rules.")
                }

                SectionTitle {
                    text: qsTr("Setlists")
                    actions: [
                        Action { icon.source: Theme.icon("download"); text: qsTr("Import a Setlist…"); onTriggered: root.importSet() },
                        Action { icon.source: Theme.icon("plus"); text: qsTr("New Setlist…"); onTriggered: root.newSetlist() }
                    ]
                }
                Repeater {
                    model: Session.setlists
                    delegate: Loader {
                        id: setRow
                        required property var modelData
                        readonly property bool isOpen: !!Session.perform && Session.perform.setlistId === modelData.id
                        Layout.fillWidth: true
                        sourceComponent: root.renamingSet === modelData.id ? setRename : setItem
                        Component {
                            id: setItem
                            NavItem {
                                text: setRow.modelData.name
                                icon.source: Theme.icon("list")
                                count: setRow.modelData.entryCount
                                marked: setRow.isOpen
                                selected: root.isView({ kind: "setlist", id: setRow.modelData.id })
                                tip: qsTr("Double-click to open in the set panel; right-click for more")
                                onClicked: Session.show({ kind: "setlist", id: setRow.modelData.id })
                                onDoubleClicked: Session.run("perform_open", { setlistId: setRow.modelData.id })
                                onContextMenu: setMenu.popup()
                                FitMenu {
                                    id: setMenu
                                    MenuItem {
                                        text: setRow.isOpen ? qsTr("Close in Set Panel") : qsTr("Open in Set Panel")
                                        onTriggered: Session.run("perform_open", { setlistId: setRow.isOpen ? null : setRow.modelData.id })
                                    }
                                    MenuItem { text: qsTr("Rename"); onTriggered: root.renamingSet = setRow.modelData.id }
                                    MenuItem { text: qsTr("Duplicate"); onTriggered: Session.run("setlist_duplicate", { id: setRow.modelData.id, name: qsTr("%1 (copy)").arg(setRow.modelData.name) }) }
                                    MenuSeparator {}
                                    MenuItem {
                                        text: qsTr("Delete Setlist…")
                                        enabled: !(setRow.isOpen && Session.decks.some(d => d && d.audible && !!d.entryId))
                                        onTriggered: Session.confirm(qsTr("Delete the setlist “%1” (%2 entries)? The songs stay in your library.").arg(setRow.modelData.name).arg(setRow.modelData.entryCount), qsTr("Delete"),
                                            ok => {
                                                if (!ok) return;
                                                Session.run("setlist_delete", { id: setRow.modelData.id });
                                                if (root.isView({ kind: "setlist", id: setRow.modelData.id })) Session.show({ kind: "all" });
                                            })
                                    }
                                }
                            }
                        }
                        Component {
                            id: setRename
                            TextField {
                                text: setRow.modelData.name
                                Component.onCompleted: { forceActiveFocus(); selectAll(); }
                                function done(save) {
                                    if (save) Session.run("setlist_update", { id: setRow.modelData.id, patch: { name: text } });
                                    root.renamingSet = "";
                                }
                                onAccepted: done(true)
                                onEditingFinished: if (root.renamingSet === setRow.modelData.id) done(true)
                                Keys.onEscapePressed: done(false)
                            }
                        }
                    }
                }

                SectionTitle { text: qsTr("Spotify") }
                NavItem {
                    visible: !!Session.spotify && Session.spotify.connected
                    text: qsTr("Search Spotify"); icon.source: Theme.icon("search")
                    selected: root.isView({ kind: "spotify-search" })
                    onClicked: Session.show({ kind: "spotify-search" })
                }
                Repeater {
                    model: Session.spotify && Session.spotify.connected && Session.spotifyPlaylists ? Session.spotifyPlaylists : []
                    delegate: NavItem {
                        required property var modelData
                        Layout.fillWidth: true
                        text: modelData.name
                        icon.source: Theme.icon("spotify")
                        selected: root.isView({ kind: "spotify-playlist", uri: modelData.uri })
                        onClicked: Session.show({ kind: "spotify-playlist", uri: modelData.uri, name: modelData.name })
                    }
                }
                Label {
                    visible: Session.loadingPlaylists
                    Layout.leftMargin: Theme.spacing * 2
                    text: qsTr("Loading playlists…")
                    opacity: Theme.dim
                }
                NavItem {
                    visible: !(Session.spotify && Session.spotify.connected)
                    text: Session.spotify && Session.spotify.connecting ? qsTr("Connecting…") : qsTr("Connect Spotify…")
                    icon.source: Theme.icon("spotify")
                    onClicked: Session.settingsPage = "spotify"
                }
            }
        }
    }
}
