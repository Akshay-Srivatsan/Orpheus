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
    /// Text colour (on the selection's colour when selected).
    readonly property color fg: selected ? Theme.system.highlightedText : Theme.system.text
    /// Played and skipped entries are dimmed (their content, not the selection).
    readonly property real contentOpacity: e.status === "played" ? 0.45 : e.status === "skipped" ? 0.35 : 1

    // Auto mode stops before this entry.
    RowLayout {
        id: holdBar
        visible: !!root.e.hold
        width: parent.width
        height: visible ? 18 : 0
        spacing: 4
        Rectangle { Layout.leftMargin: 8; Layout.preferredWidth: 22; implicitHeight: 1; color: Theme.warn }
        Icon { glyph: "pause"; sourceSize.width: 12; sourceSize.height: 12; color: Theme.warn }
        Label { text: qsTr("Auto mode stops here"); color: Theme.warn; font.pointSize: Theme.smallPointSize }
        ToolButton {
            icon.source: Theme.icon("x")
            icon.width: 10; icon.height: 10
            implicitHeight: 18; implicitWidth: 18
            padding: 0
            ToolTip.visible: hovered; ToolTip.text: qsTr("Don't stop here")
            onClicked: Session.run("entry_update", { id: root.e.id, patch: { hold: false } })
        }
        Rectangle { Layout.fillWidth: true; Layout.rightMargin: 8; implicitHeight: 1; color: Theme.warn }
    }

    Rectangle {
        anchors.fill: parent
        anchors.topMargin: holdBar.height
        color: root.selected ? Theme.system.highlight : root.current ? Qt.alpha(Theme.deckColors[1], 0.16)
            : slotDrop.containsDrag || hover.hovered ? Qt.alpha(Theme.system.highlight, 0.12) : "transparent"
        border.color: Session.fillingSlot === root.e.id ? Theme.warn : "transparent"
    }
    Rectangle { visible: root.next; x: 0; y: holdBar.height; width: 2; height: parent.height - holdBar.height; color: Theme.ok }
    Rectangle { visible: root.dropAbove; width: parent.width; height: 2; y: holdBar.height; color: Theme.system.highlight }
    HoverHandler { id: hover }

    RowLayout {
        id: body
        opacity: root.contentOpacity
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: holdBar.bottom
        anchors.margins: 4
        spacing: 6

        Item {
            Layout.preferredWidth: 22
            Layout.fillHeight: true
            Icon {
                visible: root.playingNow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                glyph: "play"
                sourceSize.width: 12; sourceSize.height: 12
                color: root.selected ? root.fg : Theme.deckColors[1]
            }
            Label {
                visible: !root.playingNow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: root.row.number || ""
                color: root.fg
                opacity: Theme.dim
                font.features: { "tnum": 1 }
            }
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
                    font.pointSize: Theme.smallPointSize
                    color: root.selected || !root.row.styles.length ? root.fg : root.row.styles[0].color
                    ToolTip.visible: labelHover.hovered
                    ToolTip.text: root.row.noStyle && root.club ? qsTr("Not one of %1's styles (right-click: Show As)").arg(root.club.name) : root.row.styles.map(s => s.name).join("\n")
                    HoverHandler { id: labelHover }
                }
                Icon {
                    visible: root.row.warnings.length > 0
                    glyph: "warning"
                    sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize
                    color: root.selected ? root.fg : Theme.warn
                    ToolTip.visible: warnHover.hovered
                    ToolTip.text: root.row.warnings.join("\n")
                    HoverHandler { id: warnHover }
                }
                Icon {
                    visible: !!root.row.heardAs
                    glyph: "reload"
                    sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize
                    color: root.fg
                    opacity: Theme.dim
                    ToolTip.visible: heardHover.hovered
                    ToolTip.text: qsTr("This recording was already played in this set (as “%1”)").arg(root.row.heardAs || "")
                    HoverHandler { id: heardHover }
                }
                Icon {
                    visible: root.row.isPause
                    glyph: "pause"
                    sourceSize.width: Theme.smallIconSize; sourceSize.height: Theme.smallIconSize
                    color: root.fg
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: root.fg
                    text: root.row.title
                    font.italic: root.isSlot
                    font.strikeout: root.e.status === "skipped"
                    opacity: root.row.repeat ? 0.55 : 1
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Icon {
                    visible: root.e.announcement !== undefined && root.e.announcement !== null
                    glyph: "mic"
                    sourceSize.width: 12; sourceSize.height: 12
                    color: root.fg
                    opacity: Theme.dim
                }
                Label {
                    visible: root.e.announcement !== undefined && root.e.announcement !== null
                    text: root.e.announcement ? root.e.announcement : qsTr("no announcement")
                    font.pointSize: Theme.smallPointSize
                    color: root.fg
                    opacity: Theme.dim
                    elide: Text.ElideRight
                    Layout.maximumWidth: 160
                }
                Button {
                    visible: root.isSlot
                    flat: true
                    text: Session.fillingSlot === root.e.id ? qsTr("Choosing… (Cancel)") : qsTr("Choose Song…")
                    font.pointSize: Theme.smallPointSize
                    onClicked: Session.fillingSlot === root.e.id ? Session.fillingSlot = "" : Session.startFillingSlot(root.e)
                }
                Label {
                    visible: !!root.row.trackId
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.pointSize: Theme.smallPointSize
                    color: root.fg
                    opacity: root.row.repeat ? Theme.faint : Theme.dim
                    text: root.row.artist + (root.row.versionLabel ? " · " + root.row.versionLabel : "")
                }
                Repeater {
                    model: root.row.tags
                    delegate: Label {
                        required property var modelData
                        text: modelData.name
                        font.pointSize: Theme.smallPointSize
                        color: modelData.color
                    }
                }
                Icon {
                    visible: !!root.e.note
                    glyph: "edit"
                    sourceSize.width: 12; sourceSize.height: 12
                    color: root.fg
                    opacity: Theme.dim
                }
                Label {
                    visible: !!root.e.note
                    text: root.e.note || ""
                    font.pointSize: Theme.smallPointSize
                    color: root.fg
                    opacity: Theme.dim
                    elide: Text.ElideRight
                    Layout.maximumWidth: 120
                }
                Item { Layout.fillWidth: !root.row.trackId }
            }
        }
        ColumnLayout {
            spacing: 0
            Label { Layout.alignment: Qt.AlignRight; text: root.row.bpm ? Session.fmtBpm(root.row.bpm) : ""; font.pointSize: Theme.smallPointSize; color: root.fg; font.features: { "tnum": 1 } }
            Label {
                Layout.alignment: Qt.AlignRight
                text: root.row.durationMs ? Session.fmtTime(root.row.durationMs) : ""
                font.pointSize: Theme.smallPointSize
                font.features: { "tnum": 1 }
                color: root.row.long && !root.selected ? Theme.warn : root.fg
                opacity: Theme.dim
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
        keys: Session.songKeys.concat(["application/x-orpheus-entries"])
        /// Set entries (not songs) are being dragged over it.
        property bool moving: false
        onEntered: d => moving = d.formats.includes("application/x-orpheus-entries")
        function onSlot(y) { return root.isSlot && !moving && y > height * 0.2 && y < height * 0.8; }
        onPositionChanged: d => root.dropHover(onSlot(d.y) ? -1 : d.y > height / 2 ? root.rowIndex + 1 : root.rowIndex)
        onExited: root.dropHover(-1)
        onDropped: d => {
            const at = d.y > height / 2 ? root.rowIndex + 1 : root.rowIndex;
            if (d.formats.includes("application/x-orpheus-entries")) {
                root.entriesDropped(JSON.parse(d.getDataAsString("application/x-orpheus-entries")), at);
                return;
            }
            const slot = onSlot(d.y);
            Session.droppedVersions(d, false).then(ids => { if (ids.length) root.songsDropped(ids, at, slot); });
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
