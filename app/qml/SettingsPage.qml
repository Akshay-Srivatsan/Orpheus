import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A settings page: a title, an optional introduction, and its content
// (`default` children) in a scrolling column.
ScrollView {
    id: root
    property string title: ""
    property string intro: ""
    default property alias content: column.data

    contentWidth: availableWidth
    padding: Theme.spacing * 2

    ColumnLayout {
        id: column
        width: root.availableWidth
        spacing: Theme.spacing

        Label { text: root.title; font.pointSize: Theme.largePointSize; font.bold: true }
        Label {
            visible: !!root.intro
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            opacity: 0.75
            text: root.intro
        }
    }
}
