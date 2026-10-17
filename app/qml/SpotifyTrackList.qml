import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Spotify search or playlist results. Previewing a song or adding it to the
// set doesn't add it to the library; + (or the menu) does.
ColumnLayout {
    id: root
    property var tracks: []
    property bool compact: false
    /// Which tracks are in the library (by index).
    property var inLibrary: []

    spacing: 0

    onTracksChanged: refreshKnown()
    Connections { target: Session; function onLibraryChanged() { root.refreshKnown(); } }
    function refreshKnown() {
        if (!tracks.length) { inLibrary = []; return; }
        Backend.call("spotify_in_library", { uris: tracks.map(t => t.uri) }).then(v => root.inLibrary = v);
    }

    /// The tracks as versions to play (`library`: they join the library too).
    function versions(list, library) { return Session.run("spotify_versions", { tracks: list, inLibrary: library }); }
    function preview(t) { versions([t], false).then(v => { if (v && v[0]) Session.preview(v[0], null); }); }
    function load(t) { versions([t], false).then(v => { if (v && v[0]) Session.loadDeck(Session.freeDeck, v[0], null); }); }
    function toSet(t, where) { versions([t], false).then(v => { if (v && v.length) Session.addToSet(v, where); }); }
    function toLibrary(t) { versions([t], true).then(v => { if (v) Session.toast(qsTr("Added “%1” to your library").arg(t.title), "info"); }); }
    function tagStyle(t, style) { versions([t], true).then(v => { if (v && v.length) Session.run("style_add", { style: style, versionIds: v }); }); }

    Repeater {
        model: root.tracks
        delegate: ItemDelegate {
            id: item
            required property var modelData
            required property int index
            Layout.fillWidth: true
            onDoubleClicked: root.load(modelData)

            contentItem: RowLayout {
                spacing: Theme.spacing
                Image {
                    Layout.preferredWidth: 36
                    Layout.preferredHeight: 36
                    source: item.modelData.artworkUrl || ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label { Layout.fillWidth: true; text: item.modelData.title; elide: Text.ElideRight }
                    Label {
                        Layout.fillWidth: true
                        opacity: 0.7
                        elide: Text.ElideRight
                        text: item.modelData.artist + (root.compact ? "" : " · " + item.modelData.album)
                    }
                }
                Label { text: Session.fmtTime(item.modelData.durationMs); opacity: 0.7 }
                ToolButton { icon.source: Theme.icon("headphones"); ToolTip.visible: hovered; ToolTip.text: qsTr("Preview in headphones"); onClicked: root.preview(item.modelData) }
                Button { text: qsTr("Next"); ToolTip.visible: hovered; ToolTip.text: qsTr("Play next (not added to your library)"); onClicked: root.toSet(item.modelData, "next") }
                Button { text: qsTr("End"); ToolTip.visible: hovered; ToolTip.text: qsTr("Add to the end of the set (not added to your library)"); onClicked: root.toSet(item.modelData, "end") }
                Label { visible: !!root.inLibrary[item.index]; text: "✓"; color: Theme.ok; ToolTip.visible: inLibHover.hovered; ToolTip.text: qsTr("In your library"); HoverHandler { id: inLibHover } }
                ToolButton {
                    visible: !root.inLibrary[item.index]
                    icon.source: Theme.icon("plus")
                    ToolTip.visible: hovered; ToolTip.text: qsTr("Add to library")
                    onClicked: root.toLibrary(item.modelData)
                }
            }

            TapHandler { acceptedButtons: Qt.RightButton; onTapped: trackMenu.popup() }
            // Drag it to the set, a deck or a style.
            DragHandler { id: dragging; target: null }
            Item {
                Drag.active: dragging.active
                Drag.dragType: Drag.Automatic
                Drag.supportedActions: Qt.CopyAction
                Drag.mimeData: ({ "application/x-orpheus-spotify": JSON.stringify([item.modelData]) })
            }
            Menu {
                id: trackMenu
                MenuItem { text: qsTr("Load onto the Off-Air Deck"); onTriggered: root.load(item.modelData) }
                MenuItem { text: qsTr("Preview in Headphones"); onTriggered: root.preview(item.modelData) }
                MenuItem { text: qsTr("Play Next"); onTriggered: root.toSet(item.modelData, "next") }
                MenuItem { text: qsTr("Add to End of Set"); onTriggered: root.toSet(item.modelData, "end") }
                MenuSeparator {}
                MenuItem { visible: !root.inLibrary[item.index]; height: visible ? implicitHeight : 0; text: qsTr("Add to Library"); onTriggered: root.toLibrary(item.modelData) }
                Menu {
                    id: asStyle
                    title: qsTr("Add to Library as")
                    enabled: Session.myStyles.length > 0
                    Instantiator {
                        model: Session.myStyles
                        delegate: MenuItem {
                            required property var modelData
                            text: modelData.name
                            onTriggered: root.tagStyle(item.modelData, modelData.key)
                        }
                        onObjectAdded: (i, o) => asStyle.insertItem(i, o)
                        onObjectRemoved: (i, o) => asStyle.removeItem(o)
                    }
                }
            }
        }
    }
}
