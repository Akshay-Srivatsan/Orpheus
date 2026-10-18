import QtQuick
import QtQuick.Controls
import Orpheus

// The simple interface's Song Info: the Info panel (the song, its versions
// and audio, and — for a song in the set — its options there) in a dialog.
AppDialog {
    id: root
    title: qsTr("Song Info")
    standardButtons: Dialog.Close
    modal: false
    padding: 0
    topPadding: 0
    width: 560
    height: 760

    Inspector {
        anchors.fill: parent
    }
}
