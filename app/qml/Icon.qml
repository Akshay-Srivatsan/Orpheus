import QtQuick
import QtQuick.Controls.impl
import Orpheus

// A bundled icon outside a button, tinted like text (`glyph`: see Theme.icon).
IconImage {
    property string glyph: ""
    source: glyph ? Theme.icon(glyph) : ""
    sourceSize.width: Theme.iconSize
    sourceSize.height: Theme.iconSize
    color: Theme.system.text
}
