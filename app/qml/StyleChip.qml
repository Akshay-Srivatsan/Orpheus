import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A style filter: a toggle button with a dot in the style's colour, its name
// and how many songs have it. Songs dropped on it are given the style (`dropped(ids)`).
Button {
    id: root
    property int count: -1
    property color color: "transparent"
    property string tip: ""

    signal dropped(var versionIds)
    /// `add`: Ctrl or Shift was held (add to the filter instead of replacing it).
    signal chosen(bool add)

    flat: !checked && !drop.containsDrag
    down: area.pressed
    topPadding: 3
    bottomPadding: 3
    leftPadding: 8
    rightPadding: 8

    contentItem: RowLayout {
        spacing: 5
        Rectangle { visible: root.color.a > 0; implicitWidth: 8; implicitHeight: 8; radius: 4; color: root.color }
        Label { text: root.text; color: Theme.system.buttonText }
        Label { visible: root.count >= 0; text: root.count; color: Theme.system.buttonText; opacity: Theme.dim; font.features: { "tnum": 1 } }
    }

    // A mouse area for the modifiers (the button still shows hover and press).
    MouseArea {
        id: area
        anchors.fill: parent
        onClicked: mouse => root.chosen((mouse.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.MetaModifier)) !== 0)
    }
    Keys.onSpacePressed: root.chosen(false)

    DropArea {
        id: drop
        anchors.fill: parent
        keys: Session.songKeys
        // Spotify songs given a style join the library.
        onDropped: d => Session.droppedVersions(d, true).then(ids => root.dropped(ids))
    }

    ToolTip.visible: hovered && !!tip
    ToolTip.text: tip
    ToolTip.delay: 600
}
