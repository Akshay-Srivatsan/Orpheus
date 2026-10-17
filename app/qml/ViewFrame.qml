import QtQuick
import QtQuick.Controls
import Orpheus

// The frame around a list or table: its base colour fills the view (also below
// the last row), with the style's border, as item views have.
Frame {
    padding: 1
    background: Rectangle {
        color: Theme.system.base
        border.color: Theme.system.mid
    }
}
