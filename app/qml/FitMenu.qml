import QtQuick
import QtQuick.Controls
import Orpheus

// A menu as wide as its widest item (up to `maximumWidth`): Fusion's are
// 200 px whatever they hold, cutting longer items short.
Menu {
    id: root
    /// The widest it gets (longer items are cut short there).
    property real maximumWidth: 480

    width: {
        let w = 0;
        for (let i = 0; i < count; i++) {
            const it = itemAt(i);
            if (it && it.visible) w = Math.max(w, it.implicitWidth);
        }
        return Math.min(maximumWidth, Math.max(implicitWidth, w + leftPadding + rightPadding));
    }
}
