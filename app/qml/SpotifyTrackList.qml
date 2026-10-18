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
    /// The selected tracks (by URI), and the row a Shift-click extends from.
    property var selectedUris: []
    property int anchor: -1

    spacing: 0

    onTracksChanged: { refreshKnown(); selectedUris = selectedUris.filter(u => tracks.some(t => t.uri === u)); }

    /// A click on a row: selects it; with Ctrl (⌘ on a Mac) adds or removes
    /// it; with Shift, selects from the last row clicked to it.
    function click(index, modifiers) {
        const uri = tracks[index].uri;
        if ((modifiers & Qt.ShiftModifier) && anchor >= 0 && anchor < tracks.length) {
            const range = tracks.slice(Math.min(anchor, index), Math.max(anchor, index) + 1).map(t => t.uri);
            selectedUris = (modifiers & Qt.ControlModifier) ? [...new Set(selectedUris.concat(range))] : range;
            return;
        }
        if (modifiers & Qt.ControlModifier) selectedUris = selectedUris.includes(uri) ? selectedUris.filter(u => u !== uri) : selectedUris.concat([uri]);
        else selectedUris = [uri];
        anchor = index;
    }
    /// What a row's menu or drag acts on: the selection, if the row is in it, else the row.
    function picked(t) { return selectedUris.includes(t.uri) ? tracks.filter(x => selectedUris.includes(x.uri)) : [t]; }
    Connections { target: Session; function onLibraryChanged() { root.refreshKnown(); } }
    function refreshKnown() {
        if (!tracks.length) { inLibrary = []; return; }
        Backend.call("spotify_in_library", { uris: tracks.map(t => t.uri) }).then(v => root.inLibrary = v);
    }

    /// The tracks as versions to play (`library`: they join the library too).
    function versions(list, library) { return Session.run("spotify_versions", { tracks: list, inLibrary: library }); }
    function preview(t) { versions([t], false).then(v => { if (v && v[0]) Session.preview(v[0], null); }); }
    function load(t) { versions([t], false).then(v => { if (v && v[0]) Session.loadDeck(null, v[0], null); }); }
    function toSet(list, where) { versions(list, false).then(v => { if (v && v.length) Session.addToSet(v, where); }); }
    function toLibrary(list) {
        versions(list, true).then(v => {
            if (v) Session.toast(list.length === 1 ? qsTr("Added “%1” to your library").arg(list[0].title) : qsTr("Added %n song(s) to your library", "", list.length), "info");
        });
    }
    function tagStyle(list, style) { versions(list, true).then(v => { if (v && v.length) Session.run("style_add", { style: style, versionIds: v }); }); }

    Repeater {
        model: root.tracks
        delegate: ItemDelegate {
            id: item
            required property var modelData
            required property int index
            Layout.fillWidth: true
            highlighted: root.selectedUris.includes(modelData.uri)
            onDoubleClicked: root.load(modelData)
            TapHandler {
                id: tap
                acceptedButtons: Qt.LeftButton
                onTapped: root.click(item.index, tap.point.modifiers)
            }

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
                ToolButton { icon.color: palette.buttonText; icon.source: Theme.icon("headphones"); ToolTip.visible: hovered; ToolTip.text: qsTr("Preview in headphones"); onClicked: root.preview(item.modelData) }
                Button { text: qsTr("Next"); ToolTip.visible: hovered; ToolTip.text: Session.addAfterText + qsTr(" (not to your library)"); onClicked: root.toSet([item.modelData], "next") }
                Button { text: qsTr("End"); ToolTip.visible: hovered; ToolTip.text: qsTr("Add to the End of the Set") + qsTr(" (not to your library)"); onClicked: root.toSet([item.modelData], "end") }
                // In the library (a check), or a button to add it: one slot, so rows line up.
                Item {
                    implicitWidth: addButton.implicitWidth
                    implicitHeight: addButton.implicitHeight
                    Icon {
                        anchors.centerIn: parent
                        visible: !!root.inLibrary[item.index]
                        glyph: "check"
                        color: Theme.ok
                        ToolTip.visible: inLibHover.hovered
                        ToolTip.text: qsTr("In your library")
                        HoverHandler { id: inLibHover }
                    }
                    ToolButton {
                        id: addButton
                        icon.color: palette.buttonText
                        anchors.fill: parent
                        enabled: !root.inLibrary[item.index]
                        icon.source: Theme.icon("plus")
                        ToolTip.visible: hovered; ToolTip.text: qsTr("Add to library")
                        onClicked: root.toLibrary([item.modelData])
                    }
                }
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: {
                    if (!root.selectedUris.includes(item.modelData.uri)) root.click(item.index, 0);
                    trackMenu.popup();
                }
            }
            // Drag it to the set, a deck or a style.
            DragHandler { id: dragging; target: null }
            DragLabel {
                readonly property var tracks: root.picked(item.modelData)
                handler: dragging
                text: tracks.length > 1 ? qsTr("%n songs", "", tracks.length) : item.modelData.title
                payload: ({ "application/x-orpheus-spotify": JSON.stringify(tracks) })
            }
            FitMenu {
                id: trackMenu
                MenuItem { text: qsTr("Load onto the Off-Air Deck"); onTriggered: root.load(item.modelData) }
                MenuItem { text: qsTr("Preview in Headphones"); onTriggered: root.preview(item.modelData) }
                MenuItem { text: Session.addAfterText; onTriggered: root.toSet(root.picked(item.modelData), "next") }
                MenuItem { text: qsTr("Add to the End of the Set"); onTriggered: root.toSet(root.picked(item.modelData), "end") }
                MenuSeparator {}
                MenuItem {
                    // Those not in the library yet.
                    readonly property var adding: root.picked(item.modelData).filter(t => !root.inLibrary[root.tracks.indexOf(t)])
                    enabled: adding.length > 0
                    text: adding.length > 1 ? qsTr("Add %1 to Library").arg(adding.length) : qsTr("Add to Library")
                    onTriggered: root.toLibrary(adding)
                }
                FitMenu {
                    id: asStyle
                    title: qsTr("Add to Library as")
                    enabled: Session.myStyles.length > 0
                    Instantiator {
                        model: Session.myStyles
                        delegate: MenuItem {
                            required property var modelData
                            text: modelData.name
                            onTriggered: root.tagStyle(root.picked(item.modelData), modelData.key)
                        }
                        onObjectAdded: (i, o) => asStyle.insertItem(i, o)
                        onObjectRemoved: (i, o) => asStyle.removeItem(o)
                    }
                }
            }
        }
    }
}
