import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Orpheus

ApplicationWindow {
    id: window
    width: 1440
    height: 900
    minimumWidth: 1100
    minimumHeight: 700
    visible: true
    title: Session.set ? qsTr("%1 — Orpheus").arg(Session.set.setlist.name) : "Orpheus"

    Component.onCompleted: Session.start()
    // Closing the main window quits (the projector and settings with it).
    onClosing: Qt.quit()

    // ------------------------------------------------------------ menus

    menuBar: MenuBar {
        Menu {
            title: qsTr("&File")
            Action { text: qsTr("&New Setlist…"); shortcut: StandardKey.New; onTriggered: newSetlistDialog.open() }
            Action { text: qsTr("&Import Music…"); onTriggered: importMusic.open() }
            MenuSeparator {}
            Action { text: qsTr("&Settings…"); shortcut: "Ctrl+,"; onTriggered: Session.settingsPage = "general" }
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
        }
        Menu {
            title: qsTr("&Play")
            Action { text: qsTr("Play &Next"); shortcut: "Ctrl+Return"; onTriggered: Session.run("perform_play_next") }
            Action { text: qsTr("&Skip Next"); onTriggered: Session.run("perform_skip_next") }
            Action {
                text: qsTr("&Auto Mode")
                checkable: true
                checked: !!Session.perform && Session.perform.autoAdvance
                onTriggered: Session.run("perform_set_auto_advance", { value: checked })
            }
            MenuSeparator {}
            Action { text: qsTr("&Fade Out Deck A"); shortcut: "Ctrl+."; onTriggered: Session.run("deck_action", { deck: 0, action: "fadeOut" }) }
            Action { text: qsTr("Play / Pause the &Free Deck"); shortcut: "Space"; onTriggered: Session.run("deck_action", { deck: Session.freeDeck, action: "toggle" }) }
        }
        Menu {
            title: qsTr("&View")
            Action { text: qsTr("&Find"); shortcut: StandardKey.Find; onTriggered: { library.searchField.forceActiveFocus(); library.searchField.selectAll(); } }
            Action { text: qsTr("Song &Info"); shortcut: "Ctrl+I"; onTriggered: Session.rightTab = "info" }
            Action { text: qsTr("&Set"); onTriggered: Session.rightTab = "set" }
        }
    }

    // ------------------------------------------------------------ layout

    header: TopBar {
        onOpenProjector: (screen, fullscreen) => projector.showOn(screen, fullscreen)
        onCloseProjector: projector.close()
    }

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

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.decksHeight
            Layout.maximumHeight: Theme.decksHeight
            Layout.margins: Theme.spacing
            spacing: Theme.spacing
            Deck { deck: 0; Layout.fillWidth: true; Layout.fillHeight: true }
            Transport { Layout.preferredWidth: 240; Layout.fillHeight: true }
            Deck { deck: 1; Layout.fillWidth: true; Layout.fillHeight: true }
        }

        SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 200
            orientation: Qt.Horizontal

            Sidebar {
                SplitView.preferredWidth: Theme.sidebarWidth
                SplitView.minimumWidth: 160
                onNewSetlist: newSetlistDialog.open()
                onEditSmartCrate: id => smartCrate.edit(id)
            }
            LibraryPane {
                id: library
                SplitView.fillWidth: true
                SplitView.minimumWidth: 400
                onCombine: (songs, recordings) => combineDialog.show(songs, recordings)
                onEditSmartCrate: id => smartCrate.edit(id)
            }
            Pane {
                SplitView.preferredWidth: Theme.rightPanelWidth
                SplitView.minimumWidth: 300
                padding: 0
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    TabBar {
                        Layout.fillWidth: true
                        currentIndex: Session.rightTab === "info" ? 1 : 0
                        TabButton { text: qsTr("Set"); onClicked: Session.rightTab = "set" }
                        TabButton { text: qsTr("Info"); onClicked: Session.rightTab = "info" }
                    }
                    StackLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        currentIndex: Session.rightTab === "info" ? 1 : 0
                        SetPanel { onNewSetlist: newSetlistDialog.open() }
                        Inspector {}
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
        Rectangle {
            visible: fileDrop.containsDrag
            anchors.fill: parent
            anchors.margins: Theme.spacing
            radius: 6
            color: Qt.alpha(Theme.system.highlight, 0.12)
            border.color: Theme.system.highlight
            border.width: 2
            Label {
                anchors.centerIn: parent
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

    Toasts {
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: Theme.spacing * 2
    }

    // ------------------------------------------------------------ dialogs

    NewSetlistDialog { id: newSetlistDialog }
    CombineDialog { id: combineDialog }
    SmartCrateDialog { id: smartCrate }
    ProjectorWindow { id: projector }
    SettingsWindow {}

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
