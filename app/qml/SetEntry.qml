import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// One entry of the set panel: a song, a style slot or a pause (`row` from `set_view`).
Item {
    id: root

    property var row
    property int rowIndex: 0
    property bool current: false
    property bool next: false
    property bool selected: false
    property bool dropAbove: false
    /// The set's club (its styles for Show As, its tags).
    property var club: null

    readonly property var e: row.entry
    readonly property bool isSlot: !row.isPause && !row.trackId
    readonly property bool playingNow: current && Session.decks[Session.liveDeck] && Session.decks[Session.liveDeck].status === "playing"

    signal picked(int modifiers)
    signal chosen()
    signal dropHover(int index)
    signal songsDropped(var ids, int at, bool slot)
    signal entriesDropped(var ids, int at)
    signal remove()
    signal moveNext()

    implicitHeight: body.implicitHeight + holdBar.height + 8
    opacity: e.status === "played" ? 0.45 : e.status === "skipped" ? 0.35 : 1

    // Auto mode stops before this entry.
    RowLayout {
        id: holdBar
        visible: !!root.e.hold
        width: parent.width
        height: visible ? 18 : 0
        Label { Layout.leftMargin: 30; text: qsTr("⏸ auto mode stops here"); font.pixelSize: 10; opacity: 0.7 }
        ToolButton { text: "×"; implicitHeight: 16; onClicked: Session.run("entry_update", { id: root.e.id, patch: { hold: false } }) }
        Item { Layout.fillWidth: true }
    }

    Rectangle {
        anchors.fill: parent
        anchors.topMargin: holdBar.height
        color: root.selected ? Qt.alpha(palette.highlight, 0.45) : root.current ? Qt.alpha(Theme.deckColors[1], 0.16) : slotDrop.containsDrag ? Qt.alpha(palette.highlight, 0.25) : "transparent"
        border.color: Session.fillingSlot === root.e.id ? Theme.warn : "transparent"
    }
    Rectangle { visible: root.next; x: 0; y: holdBar.height; width: 2; height: parent.height - holdBar.height; color: Theme.ok }
    Rectangle { visible: root.dropAbove; width: parent.width; height: 2; y: holdBar.height; color: palette.highlight }

    RowLayout {
        id: body
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: holdBar.bottom
        anchors.margins: 4
        spacing: 6

        Label {
            Layout.preferredWidth: 22
            horizontalAlignment: Text.AlignRight
            text: root.playingNow ? "▶" : root.row.number || ""
            opacity: 0.6
            font.pixelSize: 11
        }
        Rectangle {
            Layout.preferredWidth: 4
            Layout.fillHeight: true
            radius: 2
            color: root.row.styles.length ? root.row.styles[0].color : Theme.neutral
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1
            RowLayout {
                Layout.fillWidth: true
                spacing: 4
                Label {
                    visible: !root.row.isPause
                    text: root.row.label || "—"
                    font.bold: true
                    font.italic: root.row.noStyle
                    font.pixelSize: 11
                    color: root.row.styles.length ? root.row.styles[0].color : palette.text
                    ToolTip.visible: labelHover.hovered
                    ToolTip.text: root.row.noStyle && root.club ? qsTr("Not one of %1's styles (right-click: Show As)").arg(root.club.name) : root.row.styles.map(s => s.name).join("\n")
                    HoverHandler { id: labelHover }
                }
                Label {
                    visible: root.row.warnings.length > 0
                    text: "⚠"
                    color: Theme.warn
                    ToolTip.visible: warnHover.hovered
                    ToolTip.text: root.row.warnings.join("\n")
                    HoverHandler { id: warnHover }
                }
                Label {
                    visible: !!root.row.heardAs
                    text: "↺"
                    opacity: 0.6
                    ToolTip.visible: heardHover.hovered
                    ToolTip.text: qsTr("This recording was already played in this set (as “%1”)").arg(root.row.heardAs || "")
                    HoverHandler { id: heardHover }
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: root.row.isPause ? "⏸ " + root.row.title : root.row.title
                    font.italic: root.isSlot
                    font.strikeout: root.e.status === "skipped"
                    opacity: root.row.repeat ? 0.55 : 1
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Label {
                    visible: root.e.announcement !== undefined && root.e.announcement !== null
                    text: root.e.announcement ? "🔊 " + root.e.announcement : qsTr("no announcement")
                    font.pixelSize: 11
                    opacity: 0.7
                    elide: Text.ElideRight
                    Layout.maximumWidth: 160
                }
                Button {
                    visible: root.isSlot
                    flat: true
                    text: Session.fillingSlot === root.e.id ? qsTr("Choosing… (Cancel)") : qsTr("Choose Song…")
                    font.pixelSize: 11
                    onClicked: Session.fillingSlot === root.e.id ? Session.fillingSlot = "" : Session.startFillingSlot(root.e)
                }
                Label {
                    visible: !!root.row.trackId
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.pixelSize: 11
                    opacity: root.row.repeat ? 0.45 : 0.7
                    text: root.row.artist + (root.row.versionLabel ? " · " + root.row.versionLabel : "")
                }
                Repeater {
                    model: root.row.tags
                    delegate: Label {
                        required property var modelData
                        text: modelData.name
                        font.pixelSize: 10
                        color: modelData.color
                    }
                }
                Label {
                    visible: !!root.e.note
                    text: "✎ " + (root.e.note || "")
                    font.pixelSize: 11
                    opacity: 0.7
                    elide: Text.ElideRight
                    Layout.maximumWidth: 120
                }
                Item { Layout.fillWidth: !root.row.trackId }
            }
        }
        ColumnLayout {
            spacing: 0
            Label { Layout.alignment: Qt.AlignRight; text: root.row.bpm ? Session.fmtBpm(root.row.bpm) : ""; font.pixelSize: 11 }
            Label {
                Layout.alignment: Qt.AlignRight
                text: root.row.durationMs ? Session.fmtTime(root.row.durationMs) : ""
                font.pixelSize: 11
                color: root.row.long ? Theme.warn : palette.text
                opacity: 0.8
            }
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        anchors.topMargin: holdBar.height
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        drag.target: dragProxy
        onPressed: m => {
            if (m.button === Qt.RightButton) {
                if (!root.selected) root.picked(0);
                menu.popup();
            } else if (!root.selected || m.modifiers) {
                root.picked(m.modifiers);
            }
        }
        onReleased: { dragProxy.Drag.drop(); dragProxy.x = 0; dragProxy.y = 0; }
        onDoubleClicked: root.chosen()
    }
    Item {
        id: dragProxy
        Drag.active: mouse.drag.active
        Drag.dragType: Drag.Automatic
        Drag.supportedActions: Qt.MoveAction | Qt.CopyAction
        Drag.mimeData: {
            const ids = Session.selectedEntries.includes(root.e.id) ? Session.selectedEntries : [root.e.id];
            const data = { "application/x-orpheus-entries": JSON.stringify(ids) };
            if (root.e.versionId) data["application/x-orpheus-versions"] = JSON.stringify([root.e.versionId]);
            return data;
        }
    }

    // Songs dropped on the middle of an empty slot fill it; elsewhere they go above or below.
    DropArea {
        id: slotDrop
        anchors.fill: parent
        keys: ["application/x-orpheus-versions", "application/x-orpheus-entries"]
        readonly property bool moving: drag.formats ? drag.formats.includes("application/x-orpheus-entries") : false
        function onSlot(y) { return root.isSlot && !moving && y > height * 0.2 && y < height * 0.8; }
        onPositionChanged: d => root.dropHover(onSlot(d.y) ? -1 : d.y > height / 2 ? root.rowIndex + 1 : root.rowIndex)
        onExited: root.dropHover(-1)
        onDropped: d => {
            const at = d.y > height / 2 ? root.rowIndex + 1 : root.rowIndex;
            if (d.formats.includes("application/x-orpheus-entries")) {
                root.entriesDropped(JSON.parse(d.getDataAsString("application/x-orpheus-entries")), at);
                return;
            }
            const ids = JSON.parse(d.getDataAsString("application/x-orpheus-versions") || "[]");
            if (ids.length) root.songsDropped(ids, at, onSlot(d.y));
        }
    }

    EntryMenu {
        id: menu
        row: root.row
        club: root.club
        current: root.current
        onRemove: root.remove()
        onMoveNext: root.moveNext()
    }
}
