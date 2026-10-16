import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Orpheus

// Library → Files: the audio files in your library folders. A file removed
// from the library stays on disk, and scanning skips it until it's added
// again here. Files elsewhere can be added too.
Pane {
    id: root
    padding: 0

    property var view: null
    /// "all", "in" (in the library) or "out" (not in it).
    property string show: "all"

    function load() { Backend.call("files_view").then(v => view = v); }
    Component.onCompleted: load()
    Connections {
        target: Session
        function onRecordingsChanged() { root.load(); }
        function onLibraryChanged() { root.load(); }
        function onScanChanged() { if (Session.scan && !Session.scan.running) root.load(); }
    }

    readonly property var shown: {
        if (!view) return [];
        const q = Session.search.trim().toLowerCase();
        return view.files.filter(f => show === "all" || (show === "out") === f.out)
            .filter(f => !q || f.path.toLowerCase().includes(q) || f.songTitles.some(s => s.label.toLowerCase().includes(q)));
    }
    readonly property int outCount: view ? view.files.filter(f => f.out).length : 0

    function add(paths) {
        Session.run("library_add_files", { paths: paths });
        Session.toast(paths.length === 1 ? qsTr("Adding it to the library…") : qsTr("Adding %1 files to the library…").arg(paths.length), "info");
    }
    function remove(f) {
        const titles = f.songTitles.map(s => "“" + s.label + "”").join(", ");
        const msg = f.otherSource
            ? qsTr("Remove “%1” from the library? Its audio stays playable from its other %2. The file stays on disk.").arg(f.name).arg(f.otherSource === "spotify" ? qsTr("source") : qsTr("file"))
            : f.songTitles.length
                ? qsTr("Remove “%1” from the library? It's played by %2: those stay (in sets too), but show as missing and can't play until you add it again, or a file that sounds the same. The file stays on disk.").arg(f.name).arg(titles)
                : qsTr("Remove “%1” from the library? The file stays on disk, and you can add it again here.").arg(f.name);
        Session.confirm(msg, qsTr("Remove"), ok => { if (ok) Session.run("audio_file_remove", { hash: f.hash }); });
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacing

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacing
            TabBar {
                currentIndex: ["all", "in", "out"].indexOf(root.show)
                TabButton { text: qsTr("All"); width: implicitWidth; onClicked: root.show = "all" }
                TabButton { text: qsTr("In the library"); width: implicitWidth; onClicked: root.show = "in" }
                TabButton { text: qsTr("Not in the library (%1)").arg(root.outCount); width: implicitWidth; onClicked: root.show = "out" }
            }
            Button {
                visible: root.show === "out" && root.outCount > 0
                text: qsTr("Add All %1 Back").arg(root.shown.length)
                onClicked: root.add(root.shown.map(f => f.path))
            }
            Item { Layout.fillWidth: true }
            Button {
                text: qsTr("Add Files…")
                onClicked: addFiles.open()
            }
        }

        // Files that sound like audio that songs still play but that's missing.
        ColumnLayout {
            visible: !!root.view && root.view.matches.length > 0
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                textFormat: Text.StyledText
                text: qsTr("<b>Sounds like missing audio</b> — songs still play audio that isn't here; these files sound the same (another encoding, not an edit):")
            }
            Repeater {
                model: root.view ? root.view.matches : []
                delegate: RowLayout {
                    required property var modelData
                    Label {
                        Layout.fillWidth: true
                        textFormat: Text.StyledText
                        text: qsTr("“%1” for <b>%2</b> (%3% lines up, %4% differs)").arg(modelData.foundTitle).arg(modelData.songTitle)
                            .arg(Math.round(modelData.coverage * 100)).arg(Math.round(modelData.error * 100))
                    }
                    Button { text: qsTr("Use It"); onClicked: Session.run("reconnect", { missing: modelData.missing, found: modelData.found }) }
                }
            }
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.shown
            ScrollBar.vertical: ScrollBar {}
            header: RowLayout {
                width: ListView.view.width
                Label { Layout.preferredWidth: parent.width * 0.32; Layout.leftMargin: Theme.spacing; text: qsTr("FILE"); font.pixelSize: 11; font.bold: true; opacity: 0.6 }
                Label { Layout.preferredWidth: parent.width * 0.25; text: qsTr("FOLDER"); font.pixelSize: 11; font.bold: true; opacity: 0.6 }
                Label { Layout.fillWidth: true; text: qsTr("IN THE LIBRARY AS"); font.pixelSize: 11; font.bold: true; opacity: 0.6 }
            }
            delegate: Rectangle {
                id: fileRow
                required property var modelData
                required property int index
                width: ListView.view.width
                height: Theme.rowHeight + 4
                color: index % 2 ? palette.alternateBase : palette.base
                RowLayout {
                    anchors.fill: parent
                    opacity: fileRow.modelData.out ? 0.6 : 1
                    Label { Layout.preferredWidth: parent.width * 0.32; Layout.leftMargin: Theme.spacing; text: fileRow.modelData.name; elide: Text.ElideMiddle }
                    Label {
                        Layout.preferredWidth: parent.width * 0.25
                        elide: Text.ElideMiddle
                        opacity: 0.7
                        text: fileRow.modelData.folder === null || fileRow.modelData.folder === undefined ? qsTr("(added from elsewhere)") : fileRow.modelData.folder
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        text: fileRow.modelData.removed
                            ? qsTr("removed") + (fileRow.modelData.songTitles.length ? " · " + qsTr("still in %1").arg(fileRow.modelData.songTitles.map(s => s.label).join(", ")) : "")
                            : !fileRow.modelData.recording ? qsTr("not added") : fileRow.modelData.songTitles.map(s => s.label).join(", ")
                    }
                    Button {
                        visible: fileRow.modelData.out
                        text: qsTr("Add to Library")
                        onClicked: root.add([fileRow.modelData.path])
                    }
                    Button {
                        visible: !fileRow.modelData.out && !!fileRow.modelData.recording
                        flat: true
                        text: qsTr("Remove")
                        onClicked: root.remove(fileRow.modelData)
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                visible: !!root.view && !root.shown.length
                opacity: 0.6
                text: root.show === "out" ? qsTr("Every file in your library folders is in the library.") : qsTr("No audio files yet: add a music folder in Settings → Library, or Add Files…")
            }
        }
    }

    FileDialog {
        id: addFiles
        title: qsTr("Add audio files to the library")
        fileMode: FileDialog.OpenFiles
        onAccepted: root.add(selectedFiles.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, ""))))
    }
}
