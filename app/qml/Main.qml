import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Orpheus

ApplicationWindow {
    id: window
    width: 1440
    height: 900
    minimumWidth: 640
    minimumHeight: 480
    visible: true
    title: Session.set ? qsTr("%1 — Orpheus").arg(Session.set.setlist.name) : "Orpheus"

    Component.onCompleted: Session.start()

    // --audit-layout: after each resize (and every few seconds), list items
    // spilling out of their containers, in every open window.
    LayoutAudit { id: layoutAudit }
    Timer {
        running: Qt.application.arguments.includes("--audit-layout")
        interval: 3000
        repeat: true
        onTriggered: {
            layoutAudit.run(window.contentItem, "main " + window.width + "×" + window.height);
            if (settingsWindow.visible) layoutAudit.run(settingsWindow.contentItem, "settings " + Session.settingsPage + " " + settingsWindow.width + "×" + settingsWindow.height);
            if (projector.visible) layoutAudit.run(projector.contentItem, "projector");
            for (const d of [newSetlistDialog, combineDialog, smartCrate])
                if (d.visible) layoutAudit.run(d.contentItem, "dialog " + d.title);
        }
    }
    // Closing the main window quits (the projector and settings with it).
    onClosing: Qt.quit()

    /// Wide enough for both decks with the transport between them (else it goes below).
    readonly property bool wide: width >= 1150
    /// Too narrow for the decks side by side: one above the other.
    readonly property bool single: width < 760
    /// The side panels: shown unless you hide them (the sidebar starts hidden when narrow).
    property var sidebarChoice: null
    property bool setPanelShown: true
    readonly property bool sidebarShown: sidebarChoice === null ? width >= 1000 : sidebarChoice

    // ------------------------------------------------------------ menus

    component DeckMenu: Menu {
        required property int deck
        Action { text: qsTr("&Play / Pause"); onTriggered: Session.run("deck_action", { deck: deck, action: "toggle" }) }
        Action { text: qsTr("&Go Live"); onTriggered: Session.run("deck_go_live", { deck: deck }) }
        Action { text: qsTr("&Fade Out"); onTriggered: Session.run("deck_action", { deck: deck, action: "fadeOut" }) }
        Action { text: qsTr("Reset &Tempo"); onTriggered: Session.run("deck_tempo_reset", { deck: deck }) }
        MenuSeparator {}
        Action { text: qsTr("&Eject"); onTriggered: Session.run("deck_action", { deck: deck, action: "unload" }) }
    }

    menuBar: MenuBar {
        Menu {
            title: qsTr("&File")
            Action { text: qsTr("&New Setlist…"); shortcut: StandardKey.New; onTriggered: newSetlistDialog.open() }
            Action { text: qsTr("New &Crate"); onTriggered: Session.newCrateWith([]) }
            Action { text: qsTr("New S&mart Crate…"); onTriggered: smartCrate.edit("new") }
            MenuSeparator {}
            Action { text: qsTr("&Import Music…"); shortcut: StandardKey.Open; onTriggered: importMusic.open() }
            Action { text: qsTr("Import a Se&tlist…"); onTriggered: sidebar.importSet() }
            Action { text: qsTr("Import a C&rate…"); onTriggered: sidebar.importCrate() }
            Menu {
                title: qsTr("&Export the Set")
                enabled: !!Session.set
                Action { text: qsTr("Set &File…"); onTriggered: setPanel.exportSet() }
                Action { text: qsTr("&CSV…"); onTriggered: setPanel.exportCsv() }
            }
            Action { text: qsTr("&Print the Set…"); shortcut: StandardKey.Print; enabled: !!Session.set; onTriggered: setPanel.printSet() }
            MenuSeparator {}
            Action { text: qsTr("&Settings…"); shortcut: StandardKey.Preferences; onTriggered: Session.settingsPage = "general" }
            MenuSeparator {}
            Action { text: qsTr("&Quit"); shortcut: StandardKey.Quit; onTriggered: Qt.quit() }
        }
        Menu {
            title: qsTr("&Edit")
            Action {
                text: Session.undos.length ? qsTr("&Undo: %1").arg(Session.undos[Session.undos.length - 1].text) : qsTr("&Undo")
                enabled: Session.undos.length > 0
                shortcut: StandardKey.Undo
                onTriggered: Session.undo()
            }
            MenuSeparator {}
            Action { text: qsTr("Select &All Songs"); shortcut: StandardKey.SelectAll; onTriggered: library.table.selectAll() }
            Action { text: qsTr("&Find…"); shortcut: StandardKey.Find; onTriggered: { library.searchField.forceActiveFocus(); library.searchField.selectAll(); } }
        }
        Menu {
            title: qsTr("&Library")
            Action { text: qsTr("Play &Next"); enabled: Session.selected.length > 0; onTriggered: Session.addToSet(Session.selected, "next") }
            Action { text: qsTr("Add to &End of Set"); enabled: Session.selected.length > 0; onTriggered: Session.addToSet(Session.selected, "end") }
            Action { text: qsTr("&Preview in Headphones"); enabled: Session.selected.length > 0; onTriggered: Session.preview(Session.selected[0], null) }
            MenuSeparator {}
            Action { text: qsTr("&Combine…"); enabled: Session.selected.length > 1; onTriggered: library.table.combineSelected() }
            MenuSeparator {}
            Action { text: qsTr("&All Songs"); onTriggered: Session.show({ kind: "all" }) }
            Action { text: qsTr("&History"); onTriggered: Session.show({ kind: "history" }) }
            Action { text: qsTr("&Files"); onTriggered: Session.show({ kind: "files" }) }
            MenuSeparator {}
            Action { text: qsTr("&Rescan Music Folders"); enabled: !(Session.scan && Session.scan.running); onTriggered: Session.run("library_rescan") }
            Action { text: qsTr("Music &Folders…"); onTriggered: Session.settingsPage = "library" }
        }
        Menu {
            title: qsTr("&Play")
            Action { text: qsTr("Play &Next"); shortcut: "Ctrl+Return"; onTriggered: Session.run("perform_play_next") }
            Action { text: qsTr("&Skip the Next Song"); onTriggered: Session.run("perform_skip_next") }
            Action {
                text: qsTr("&Auto Mode")
                checkable: true
                checked: !!Session.perform && Session.perform.autoAdvance
                onTriggered: Session.run("perform_set_auto_advance", { value: checked })
            }
            Action { text: qsTr("Announce the Next &Dance"); enabled: !!Session.tts && Session.tts.state === "ready"; onTriggered: Session.run("announce_next") }
            MenuSeparator {}
            DeckMenu { deck: 0; title: qsTr("Deck &A") }
            DeckMenu { deck: 1; title: qsTr("Deck &B") }
            Action { text: qsTr("Play / Pause the &Free Deck"); shortcut: "Space"; onTriggered: Session.run("deck_action", { deck: Session.freeDeck, action: "toggle" }) }
            MenuSeparator {}
            Action { text: qsTr("&Center the Crossfader"); onTriggered: Session.run("levels_set", { master: null, cue: null, crossfader: 0.5 }) }
        }
        Menu {
            title: qsTr("&View")
            Action { text: qsTr("&Sidebar"); checkable: true; checked: window.sidebarShown; onTriggered: window.sidebarChoice = checked }
            Action { text: qsTr("Set &Panel"); checkable: true; checked: window.setPanelShown; onTriggered: window.setPanelShown = checked }
            MenuSeparator {}
            Action { text: qsTr("The S&et"); shortcut: "Ctrl+1"; onTriggered: { window.setPanelShown = true; Session.rightTab = "set"; } }
            Action { text: qsTr("Song &Info"); shortcut: "Ctrl+I"; onTriggered: { window.setPanelShown = true; Session.rightTab = "info"; } }
            MenuSeparator {}
            Action { text: qsTr("E&xpand All Songs"); onTriggered: library.table.expandAll() }
            Action { text: qsTr("C&ollapse All Songs"); onTriggered: library.table.collapseAll() }
            MenuSeparator {}
            Action { text: qsTr("&Projector in a Window"); onTriggered: projector.showOn(-1, false) }
            Action { text: qsTr("Close the P&rojector"); onTriggered: projector.close() }
            MenuSeparator {}
            Action {
                text: qsTr("&Full Screen")
                shortcut: StandardKey.FullScreen
                checkable: true
                checked: window.visibility === Window.FullScreen
                onTriggered: window.visibility = checked ? Window.FullScreen : Window.Windowed
            }
        }
        Menu {
            title: qsTr("&Help")
            Action { text: qsTr("&About Orpheus"); onTriggered: about.open() }
        }
    }

    // ------------------------------------------------------------ layout

    header: TopBar {
        onOpenProjector: (screen, fullscreen) => projector.showOn(screen, fullscreen)
        onCloseProjector: projector.close()
    }
    footer: StatusBar {}

    Label {
        visible: !!Backend.failure
        anchors.centerIn: parent
        width: parent.width * 0.6
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
        text: Backend.failure
        color: Theme.error
    }

    ColumnLayout {
        visible: !Backend.failure
        anchors.fill: parent
        spacing: 0

        // The decks, and the transport between them (below them when narrow).
        GridLayout {
            Layout.fillWidth: true
            // As tall as the decks need (the library gets the rest).
            Layout.fillHeight: false
            Layout.margins: Theme.spacing
            Layout.bottomMargin: 0
            columns: window.wide ? 3 : window.single ? 1 : 2
            columnSpacing: Theme.spacing
            rowSpacing: Theme.spacing
            Deck {
                deck: 0
                Layout.row: 0; Layout.column: 0
                Layout.minimumWidth: 300
                Layout.fillWidth: true
                Layout.fillHeight: true
            }
            Transport {
                compact: !window.wide
                Layout.row: window.wide ? 0 : window.single ? 2 : 1
                Layout.column: window.wide ? 1 : 0
                Layout.columnSpan: window.wide || window.single ? 1 : 2
                Layout.preferredWidth: window.wide ? Math.max(250, implicitWidth) : -1
                Layout.minimumWidth: window.wide ? implicitWidth : -1
                Layout.fillWidth: !window.wide
                Layout.fillHeight: true
            }
            Deck {
                deck: 1
                Layout.row: window.single ? 1 : 0; Layout.column: window.wide ? 2 : window.single ? 0 : 1
                Layout.minimumWidth: 300
                Layout.fillWidth: true
                Layout.fillHeight: true
            }
        }

        SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 200
            orientation: Qt.Horizontal

            Sidebar {
                id: sidebar
                visible: window.sidebarShown
                SplitView.preferredWidth: Theme.sidebarWidth
                SplitView.minimumWidth: 150
                onNewSetlist: newSetlistDialog.open()
                onEditSmartCrate: id => smartCrate.edit(id)
            }
            LibraryPane {
                id: library
                SplitView.fillWidth: true
                SplitView.minimumWidth: 300
                onCombine: (songs, recordings) => combineDialog.show(songs, recordings)
                onEditSmartCrate: id => smartCrate.edit(id)
            }
            // The set and Info: tabs over a framed page, as a tab widget.
            Pane {
                visible: window.setPanelShown
                SplitView.preferredWidth: Theme.rightPanelWidth
                SplitView.minimumWidth: 260
                padding: Theme.spacing
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    TabBar {
                        Layout.fillWidth: true
                        currentIndex: Session.rightTab === "info" ? 1 : 0
                        TabButton { text: qsTr("Set"); onClicked: Session.rightTab = "set" }
                        TabButton { text: qsTr("Info"); onClicked: Session.rightTab = "info" }
                    }
                    Frame {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        padding: 0
                        StackLayout {
                            anchors.fill: parent
                            currentIndex: Session.rightTab === "info" ? 1 : 0
                            SetPanel { id: setPanel; onNewSetlist: newSetlistDialog.open() }
                            Inspector {}
                        }
                    }
                }
            }
        }
    }

    // Music dropped on the window joins the library.
    DropArea {
        id: fileDrop
        anchors.fill: parent
        keys: ["text/uri-list"]
        // While files are dragged over the window: what dropping does.
        Pane {
            visible: fileDrop.containsDrag
            anchors.centerIn: parent
            Label {
                text: qsTr("Drop music files or folders to add them to your library")
                font.pointSize: Theme.titlePointSize
                font.bold: true
            }
        }
        onDropped: d => {
            const paths = d.urls.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, "")));
            if (!paths.length) return;
            Session.run("library_import", { paths: paths });
            Session.toast(qsTr("Importing %1 item(s)…").arg(paths.length), "info");
        }
    }

    // ------------------------------------------------------------ dialogs

    NewSetlistDialog { id: newSetlistDialog }
    CombineDialog { id: combineDialog }
    SmartCrateDialog { id: smartCrate }
    ProjectorWindow { id: projector }
    SettingsWindow { id: settingsWindow }

    MessageDialog {
        id: about
        title: qsTr("About Orpheus")
        text: qsTr("Orpheus")
        informativeText: qsTr("DJ software for social dancing: dance styles, club guidelines, live and planned sets, a projector for the dancers.\n\nFree software under the GNU GPL, version 3 or later.")
        buttons: MessageDialog.Ok
    }

    FileDialog {
        id: importMusic
        title: qsTr("Import music")
        fileMode: FileDialog.OpenFiles
        onAccepted: Session.run("library_import", { paths: selectedFiles.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, ""))) })
    }

    // Confirmations and files components ask for (see Session).
    MessageDialog {
        id: confirm
        property var callback
        buttons: MessageDialog.Ok | MessageDialog.Cancel
        onButtonClicked: (button, role) => { if (callback) callback(role === MessageDialog.AcceptRole); callback = null; }
    }
    FolderDialog {
        id: folders
        property var callback
        onAccepted: { if (callback) callback(decodeURIComponent(String(selectedFolder).replace(/^file:\/\//, ""))); callback = null; }
        onRejected: callback = null
    }
    FileDialog {
        id: files
        property var callback
        onAccepted: { if (callback) callback(decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""))); callback = null; }
        onRejected: callback = null
    }
    Connections {
        target: Session
        function onConfirmRequested(text, okLabel, callback) {
            confirm.text = text;
            confirm.callback = callback;
            confirm.open();
        }
        function onFileRequested(options, callback) {
            if (options.folder) {
                folders.title = options.title || "";
                folders.callback = callback;
                folders.open();
                return;
            }
            files.title = options.title || "";
            files.nameFilters = options.nameFilters || [];
            files.fileMode = options.save ? FileDialog.SaveFile : FileDialog.OpenFile;
            files.selectedFile = options.name ? "file:///" + options.name : "";
            files.callback = callback;
            files.open();
        }
    }
}
