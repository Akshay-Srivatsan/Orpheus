import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

ApplicationWindow {
    id: window
    width: 1440
    height: 900
    visible: true
    title: "Orpheus"

    property var rows: []

    function reload() {
        Backend.call("library_view", { query: {} }).then(v => rows = v.rows, e => console.warn(e));
    }

    Component.onCompleted: reload()
    Connections {
        target: Backend
        function onEvent(name, payload) { if (name === "library-changed") window.reload(); }
    }

    ListView {
        anchors.fill: parent
        model: window.rows
        delegate: ItemDelegate {
            required property var modelData
            width: ListView.view.width
            text: modelData.title + " — " + modelData.artist
        }
    }
}
