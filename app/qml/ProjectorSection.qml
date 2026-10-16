import QtQuick
import QtQuick.Layouts
import Orpheus

// A projector side panel: a heading and dances (with their song titles).
Rectangle {
    id: root
    property var projector
    property string heading: ""
    property var entries: []
    /// One dance, large (Up next).
    property bool big: false
    readonly property real u: projector.u

    implicitHeight: column.implicitHeight + 4 * u
    radius: 1.6 * u
    color: projector.card
    border.color: projector.line

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: 2 * root.u
        spacing: 1 * root.u
        Text {
            text: root.heading
            color: root.projector.text2
            font.pixelSize: 1.7 * root.u
            font.bold: true
            font.capitalization: Font.AllUppercase
            font.letterSpacing: 0.2 * root.u
        }
        Repeater {
            model: root.entries
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                spacing: 1 * root.u
                Rectangle { Layout.preferredWidth: 0.45 * root.u; Layout.fillHeight: true; radius: width / 2; color: modelData.color }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Text {
                        Layout.fillWidth: true
                        text: modelData.dance || "—"
                        color: modelData.color
                        font.pixelSize: (root.big ? 4.2 : 2.6) * root.u
                        font.bold: true
                        fontSizeMode: Text.HorizontalFit
                        minimumPixelSize: 10
                        elide: Text.ElideRight
                    }
                    Text {
                        visible: !root.big && !!modelData.title
                        Layout.fillWidth: true
                        text: modelData.title
                        color: root.projector.text3
                        font.pixelSize: 2 * root.u
                        elide: Text.ElideRight
                    }
                    Text {
                        visible: root.big && modelData.tags.length > 0
                        text: modelData.tags.map(t => t.name).join(" · ")
                        color: root.projector.text2
                        font.pixelSize: 2 * root.u
                    }
                }
            }
        }
    }
}
