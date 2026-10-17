import QtQuick
import QtQuick.Controls
import Orpheus

// A dialog in its own window (modal over the main one), with the standard
// buttons in the platform's order.
Dialog {
    popupType: Popup.Window
    modal: true
    anchors.centerIn: Overlay.overlay
    padding: Theme.spacing * 2
    topPadding: Theme.spacing * 2
}
