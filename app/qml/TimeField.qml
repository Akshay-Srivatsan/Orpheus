import QtQuick
import QtQuick.Controls
import Orpheus

// A time as m:ss.s; empty = none (inherit). `edited(ms)` with null when cleared.
TextField {
    id: root
    property var value: null
    signal edited(var ms)

    implicitWidth: 72
    text: value === null || value === undefined ? "" : Session.fmtTime(value, true)

    function parse(s) {
        const t = s.trim();
        if (!t) return null;
        const parts = t.split(":").map(Number);
        if (parts.some(p => isNaN(p))) return undefined;
        const secs = parts.length === 2 ? parts[0] * 60 + parts[1] : parts[0];
        return Math.round(secs * 1000);
    }
    onEditingFinished: {
        const v = parse(text);
        if (v === undefined) text = value === null || value === undefined ? "" : Session.fmtTime(value, true);
        else if (v !== value) edited(v);
    }
}
