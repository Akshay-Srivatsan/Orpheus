import QtQuick
import QtQuick.Layouts
import Orpheus

// The projector's main card: the dance now, the next dance during the pause
// (so dancers can find a partner), or the one just played.
Rectangle {
    id: root
    property var projector
    readonly property var p: projector.payload
    readonly property var s: projector.s
    readonly property var entry: !p ? null : projector.revealNext ? p.next : p.current
    readonly property string label: projector.revealNext ? qsTr("Next dance") : projector.gap ? qsTr("Just played") : qsTr("Now dancing")
    readonly property real u: projector.u

    radius: 1.6 * u
    color: projector.card
    border.color: projector.line
    clip: true

    Image {
        id: art
        visible: status === Image.Ready
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.rightMargin: 3 * root.u
        width: Math.min(parent.height * 0.6, parent.width * 0.3)
        height: width
        fillMode: Image.PreserveAspectCrop
        property string forRecording: ""
        Connections {
            target: root
            function onEntryChanged() {
                const id = root.entry && !root.projector.revealNext && root.s && root.s.showArtwork ? root.entry.recordingId : null;
                if (id === art.forRecording) return;
                art.forRecording = id || "";
                art.source = "";
                if (id) Backend.call("recording_artwork", { id: id }).then(a => { if (art.forRecording === id && a) art.source = a; });
            }
        }
    }

    ColumnLayout {
        visible: !!root.entry
        anchors.left: parent.left
        anchors.right: art.visible ? art.left : parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.margins: 3 * root.u
        spacing: 1 * root.u
        opacity: root.projector.gap && !root.projector.revealNext ? 0.6 : 1

        Text {
            text: (root.projector.gap ? "" : "● ") + root.label
            color: root.entry ? Theme.readable(root.entry.color, !root.projector.light) : root.projector.text3
            font.pixelSize: 2 * root.u
            font.bold: true
            font.capitalization: Font.AllUppercase
            font.letterSpacing: 0.15 * root.u
        }
        Text {
            visible: !!root.entry && !!root.entry.dance
            Layout.fillWidth: true
            Layout.preferredHeight: 18 * root.u
            text: root.entry ? root.entry.dance : ""
            color: root.entry ? Theme.readable(root.entry.color, !root.projector.light) : root.projector.text1
            font.pixelSize: 9 * root.u
            font.bold: true
            fontSizeMode: Text.Fit
            minimumPixelSize: 14
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            verticalAlignment: Text.AlignVCenter
        }
        Text {
            visible: !!root.entry && root.entry.tags.length > 0
            text: root.entry ? root.entry.tags.map(t => t.name).join(" · ") : ""
            color: root.projector.text2
            font.pixelSize: 2.6 * root.u
            font.bold: true
        }
        Text {
            visible: root.projector.revealNext
            text: qsTr("Find a partner!")
            color: root.projector.text2
            font.pixelSize: 3.4 * root.u
        }
        Text {
            visible: !root.projector.revealNext && !!root.entry && !!root.entry.title
            Layout.fillWidth: true
            Layout.preferredHeight: 8 * root.u
            text: root.entry ? root.entry.title : ""
            color: root.projector.text1
            font.pixelSize: 4 * root.u
            fontSizeMode: Text.Fit
            minimumPixelSize: 12
            wrapMode: Text.WordWrap
            maximumLineCount: 2
        }
        Text {
            visible: !root.projector.revealNext && !!root.s && root.s.showArtist && !!root.entry && !!root.entry.artist
            Layout.fillWidth: true
            text: root.entry ? root.entry.artist : ""
            color: root.projector.text2
            font.pixelSize: 3 * root.u
            elide: Text.ElideRight
        }
        Text {
            visible: !root.projector.revealNext && !!root.s && root.s.showBpm && !!root.entry && !!root.entry.bpm
            text: root.entry && root.entry.bpm ? Math.round(root.entry.bpm) + " BPM" : ""
            color: root.projector.text3
            font.pixelSize: 2.4 * root.u
        }
    }

    // Progress through the song.
    Rectangle {
        visible: !root.projector.gap && root.projector.progress !== null && !!root.entry
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 1.2 * root.u
        height: 0.6 * root.u
        radius: height / 2
        width: (parent.width - 2.4 * root.u) * (root.projector.progress || 0)
        color: root.entry ? root.entry.color : root.projector.text1
    }
}
