import QtQuick
import QtQuick.Controls
import Orpheus

// What's being dragged, as a label following the pointer: an in-window drag
// (Drag.Internal). Songs and set entries only move within the window, and a
// platform drag on Wayland could leave the app waiting on the compositor
// (the window stopped responding). `payload`: format → text, read with
// `Session.dropText`; `handler`: the DragHandler that starts it.
Label {
    id: root
    property var payload: ({})
    property DragHandler handler: null
    /// Where the pointer is on the label (it sits just below and right of it).
    readonly property real offset: Theme.spacing * 2

    parent: Overlay.overlay
    z: 1000
    visible: Drag.active
    padding: 4
    color: palette.toolTipText
    background: Rectangle { color: palette.toolTipBase; border.color: palette.mid; radius: 3 }
    x: handler ? handler.centroid.scenePosition.x + offset : 0
    y: handler ? handler.centroid.scenePosition.y + offset : 0

    Drag.dragType: Drag.Internal
    Drag.keys: Object.keys(payload)
    Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
    Drag.hotSpot.x: -offset
    Drag.hotSpot.y: -offset

    // It starts and ends with the handler (dropping where it ends). Views keep
    // their rows meanwhile (`Session.dragging`).
    Connections {
        target: root.handler
        function onActiveChanged() {
            if (root.handler.active) {
                Session.dragging = true;
                root.Drag.active = true;
            } else {
                root.Drag.drop();
                Session.dragging = false;
            }
        }
    }
}
