import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Under the set (simple interface): find songs to add as you type — your
// library's matches, each with +; Spotify's below them.
ColumnLayout {
    id: root
    /// Library matches shown here (the rest are in Songs).
    readonly property int shownMax: 8
    /// How long typing pauses before searching (ms).
    readonly property int pauseMs: 250
    property var results: []
    spacing: 4

    function search() {
        const q = field.text.trim();
        if (!q) { results = []; return; }
        Backend.call("library_view", { query: { view: { kind: "all" }, search: q } }).then(v => { if (field.text.trim() === q) root.results = v.rows; });
    }
    Timer { id: later; interval: root.pauseMs; onTriggered: root.search() }
    Connections { target: Session; function onLibraryChanged() { if (field.text.trim()) later.restart(); } }

    SearchField {
        id: field
        Layout.fillWidth: true
        placeholderText: qsTr("Add songs: search your library…")
        onTextChanged: later.restart()
    }
    ViewFrame {
        visible: root.results.length > 0
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(root.shownMax, root.results.length) * 40 + 2
        ListView {
            anchors.fill: parent
            clip: true
            model: root.results.slice(0, root.shownMax)
            delegate: ItemDelegate {
                id: hit
                required property var modelData
                width: ListView.view.width
                onDoubleClicked: Session.addToSet([modelData.versionId], "end")
                contentItem: RowLayout {
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Label { Layout.fillWidth: true; elide: Text.ElideRight; text: hit.modelData.title }
                        Label {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            opacity: Theme.dim
                            font.pointSize: Theme.smallPointSize
                            text: [hit.modelData.artist, hit.modelData.styles.map(s => s.name).join(" / "),
                                hit.modelData.bpm ? Session.fmtBpm(hit.modelData.bpm) + " BPM" : "", Session.meterName(hit.modelData.meter)].filter(x => x).join(" · ")
                        }
                    }
                    Button {
                        implicitWidth: implicitContentWidth + leftPadding + rightPadding
                        icon.source: Theme.icon("plus")
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Add to the End of the Set")
                        onClicked: Session.addToSet([hit.modelData.versionId], "end")
                    }
                    Button {
                        implicitWidth: implicitContentWidth + leftPadding + rightPadding
                        icon.source: Theme.icon("dots")
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("More ways to add it")
                        onClicked: hitMenu.popup()
                        FitMenu {
                            id: hitMenu
                            MenuItem { text: Session.addAfterText; onTriggered: Session.addToSet([hit.modelData.versionId], "next") }
                            MenuItem { text: qsTr("Add to the End of the Set"); onTriggered: Session.addToSet([hit.modelData.versionId], "end") }
                            MenuSeparator {}
                            MenuItem { text: qsTr("Song Info…"); onTriggered: Session.showSongInfo(hit.modelData.trackId) }
                        }
                    }
                }
            }
        }
    }
    Button {
        visible: root.results.length > root.shownMax
        text: qsTr("Show All %1 in Songs").arg(root.results.length)
        onClicked: { Session.search = field.text.trim(); Session.show({ kind: "all" }); Session.simplePage = "songs"; }
    }
    SpotifySearchSection {
        Layout.fillWidth: true
        query: field.text
    }
}
