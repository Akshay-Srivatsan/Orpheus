import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A form row's label: at the top of its row, level with the first field's text
// (as QFormLayout places them).
Label {
    Layout.alignment: Qt.AlignLeft | Qt.AlignTop
    topPadding: Math.max(0, (Theme.fieldHeight - implicitContentHeight) / 2)
}
