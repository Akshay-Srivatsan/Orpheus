import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Library")
    intro: qsTr("Your library (songs, versions, dance styles, setlists, crates, club guidelines) is stored as plain text files in one folder. Put it in a synced folder or a git repository to share it between computers or DJs. Music files themselves are never modified.")

    readonly property var s: Session.settings
    property var info: null
    Component.onCompleted: load()
    function load() { Backend.call("library_info").then(v => info = v); }
    function set(patch) { Session.saveSettings(patch); }

    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Library folder"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            TextField { Layout.fillWidth: true; readOnly: true; text: page.info ? page.info.libraryDir : "" }
            RowLayout {
                Button {
                    text: qsTr("Choose…")
                    onClicked: Session.chooseFile({ folder: true, title: qsTr("Choose the library folder (e.g. a synced folder)") }, dir => { page.set({ libraryDir: dir }); page.load(); })
                }
                Button { visible: !!page.s && !!page.s.libraryDir; flat: true; text: qsTr("Use default"); onClicked: { page.set({ libraryDir: null }); page.load(); } }
                Button {
                    text: qsTr("Reload")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Read the library folder again, e.g. after it was synced")
                    onClicked: Session.run("library_reload").then(errs => {
                        page.load();
                        if (errs !== undefined) Session.toast(errs.length ? qsTr("Reloaded with %1 problem(s)").arg(errs.length) : qsTr("Library reloaded"), errs.length ? "error" : "info");
                    })
                }
            }
        }

        FormLabel { text: qsTr("Music folders"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            Repeater {
                model: page.s ? page.s.libraryFolders : []
                delegate: RowLayout {
                    required property string modelData
                    Layout.fillWidth: true
                    Label { Layout.fillWidth: true; text: modelData; elide: Text.ElideMiddle }
                    ToolButton {
                        icon.source: Theme.icon("x")
                        ToolTip.visible: hovered; ToolTip.text: qsTr("Remove")
                        onClicked: page.set({ libraryFolders: page.s.libraryFolders.filter(f => f !== modelData) })
                    }
                }
            }
            Label { visible: !!page.s && !page.s.libraryFolders.length; text: qsTr("No folders yet."); opacity: 0.6 }
            RowLayout {
                Button {
                    text: qsTr("Add Folder…")
                    onClicked: Session.chooseFile({ folder: true, title: qsTr("Add a music folder") }, dir => {
                        if (!page.s.libraryFolders.includes(dir)) page.set({ libraryFolders: page.s.libraryFolders.concat([dir]) });
                    })
                }
                Button { text: qsTr("Rescan"); enabled: !(Session.scan && Session.scan.running); onClicked: Session.run("library_rescan") }
                Label {
                    visible: !!Session.scan
                    opacity: 0.7
                    font.pixelSize: 11
                    text: !Session.scan ? "" : Session.scan.running ? Session.scan.phase + "…"
                        : qsTr("Last scan: %1 new, %2 moved, %3 gone").arg(Session.scan.added).arg(Session.scan.relinked).arg(Session.scan.removed)
                }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pixelSize: 11
                opacity: 0.7
                text: qsTr("Files are recognised by their audio content, so renaming or reorganising folders keeps everything linked.")
            }
        }

        FormLabel { text: qsTr("This computer"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            TextField { Layout.fillWidth: true; readOnly: true; text: page.info ? page.info.dataDir : "" }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pixelSize: 11
                opacity: 0.7
                text: qsTr("File locations, waveforms, settings and the Spotify login stay here (not shared).")
            }
        }

        FormLabel { visible: !!page.info && page.info.loadErrors.length > 0; text: qsTr("Problems"); color: Theme.warn; Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            visible: !!page.info && page.info.loadErrors.length > 0
            Layout.fillWidth: true
            Repeater {
                model: page.info ? page.info.loadErrors : []
                delegate: Label { required property string modelData; Layout.fillWidth: true; wrapMode: Text.WordWrap; text: modelData; color: Theme.error }
            }
        }
    }
}
