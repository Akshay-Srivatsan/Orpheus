import QtQuick
import QtQuick.Controls
import Orpheus

// A number, or empty for "the default" (`placeholderText` says what that is).
// `committed(value)` gives the new one (null: emptied) when editing ends:
// Return, leaving the field, or editing finished. (A TextField's own
// editingFinished needs acceptable input, which empty isn't, so clearing a
// field was never saved.)
TextField {
    id: root
    /// The value shown (null or undefined: empty).
    property var value: null
    property real minimum: -1e9
    signal committed(var value)

    text: value === null || value === undefined ? "" : String(value)
    validator: DoubleValidator { bottom: root.minimum }

    function commit() {
        const v = text.trim() === "" ? null : Number(text);
        if (v !== null && (isNaN(v) || v < minimum)) return;
        if (v !== (value === undefined ? null : value)) committed(v);
    }
    onEditingFinished: commit()
    onActiveFocusChanged: if (!activeFocus) commit()
    Keys.onReturnPressed: event => { commit(); event.accepted = false; }
    Keys.onEnterPressed: event => { commit(); event.accepted = false; }
}
