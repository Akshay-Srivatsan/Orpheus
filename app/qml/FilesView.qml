pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Qt.labs.qmlmodels
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
            Label { text: qsTr("Show") }
            ComboBox {
                implicitContentWidthPolicy: ComboBox.WidestText
                readonly property var values: ["all", "in", "out"]
                model: [qsTr("All files"), qsTr("In the library"), qsTr("Not in the library (%1)").arg(root.outCount)]
                currentIndex: values.indexOf(root.show)
                onActivated: index => root.show = values[index]
            }
            Button {
                enabled: root.show === "out" && root.outCount > 0
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

        // The files: a table (name, folder, its songs, what you can do).
        ViewFrame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: Theme.spacing
            Layout.rightMargin: Theme.spacing
            Layout.bottomMargin: Theme.spacing
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                HorizontalHeaderView {
                    Layout.fillWidth: true
                    syncView: fileTable
                    clip: true
                    resizableColumns: true
                    model: [qsTr("File"), qsTr("Folder"), qsTr("In the Library As"), ""]
                    // Labels over their columns' text (as in the library).
                    delegate: HorizontalHeaderViewDelegate {
                        id: section
                        contentItem: Label { text: section.model.modelData; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter }
                    }
                }
                TreeView {
                    id: fileTable
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    alternatingRows: true
                    boundsBehavior: Flickable.StopAtBounds
                    selectionBehavior: TableView.SelectionDisabled
                    ScrollBar.vertical: ScrollBar {}
                    readonly property var shares: [0.3, 0.22, 0.32, 0.16]
                    columnWidthProvider: c => Math.max(80, Math.floor(width * shares[c]))
                    onWidthChanged: forceLayout()
                    // Each row is its index into `shown`, once per column (rows have mixed types;
                    // the model wants one type, and one property, per column).
                    model: TreeModel {
                        TableModelColumn { display: "k0" }
                        TableModelColumn { display: "k1" }
                        TableModelColumn { display: "k2" }
                        TableModelColumn { display: "k3" }
                        rows: root.shown.map((f, i) => ({ k0: i, k1: i, k2: i, k3: i }))
                    }
                    delegate: TreeViewDelegate {
                        id: cell
                        required property int column
                        readonly property var f: model.display !== undefined ? root.shown[model.display] : null
                        contentItem: Loader {
                            sourceComponent: cell.column === 3 ? actions : cell.column === 2 ? songs : plain
                        }
                        Component {
                            id: plain
                            Label {
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideMiddle
                                opacity: !cell.f || cell.f.out || cell.column === 1 ? Theme.dim : 1
                                text: !cell.f ? "" : cell.column === 0 ? cell.f.name
                                    : cell.f.folder === null || cell.f.folder === undefined ? qsTr("(added from elsewhere)") : cell.f.folder
                                ToolTip.visible: tip.hovered
                                ToolTip.delay: 600
                                ToolTip.text: !cell.f ? "" : cell.column === 0 ? cell.f.path : cell.f.root ? qsTr("In the library folder %1").arg(cell.f.root) : ""
                                HoverHandler { id: tip }
                            }
                        }
                        // Its songs are links (they open in Info); a removed file songs
                        // still play is a warning.
                        Component {
                            id: songs
                            Label {
                                readonly property var f: cell.f
                                readonly property string links: f ? f.songTitles.map(s => "<a href=\"" + s.id + "\">" + s.label.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</a>").join(", ") : ""
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                                textFormat: Text.StyledText
                                linkColor: f && f.removed ? Theme.warn : palette.link
                                color: f && f.removed && f.songTitles.length ? Theme.warn : Theme.system.text
                                opacity: !f || (f.removed && !f.songTitles.length) || !f.recording ? Theme.dim : 1
                                text: !f ? "" : f.removed
                                    ? qsTr("Removed") + (f.songTitles.length ? " · " + qsTr("still in %1").arg(links) : "")
                                    : !f.recording ? qsTr("Not added") : links
                                onLinkActivated: id => { Session.inspectTrack = id; Session.rightTab = "info"; }
                                HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
                            }
                        }
                        Component {
                            id: actions
                            // One button, as the file is: out of the library, Add; in it, Remove….
                            Button {
                                readonly property bool out: !!cell.f && cell.f.out
                                enabled: out || (!!cell.f && !!cell.f.recording)
                                icon.source: out ? Theme.icon("plus") : ""
                                text: out ? qsTr("Add to Library") : qsTr("Remove…")
                                onClicked: out ? root.add([cell.f.path]) : root.remove(cell.f)
                            }
                        }
                    }
                    Label {
                        anchors.centerIn: parent
                        visible: !!root.view && !root.shown.length
                        opacity: Theme.dim
                        text: root.show === "out" ? qsTr("Every file in your library folders is in the library.") : qsTr("No audio files yet: add a music folder in Settings → Library, or Add Files…")
                    }
                }
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
