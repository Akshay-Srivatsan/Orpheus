import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A min–max range (either end may be empty), as numbers, lengths (m:ss,
// `kind: "time"`) or percentages (`kind: "pct"`). `edited(range)`: null when both are empty.
RowLayout {
    id: root
    property var value: null
    property string kind: "num"
    signal edited(var range)

    spacing: 2

    function show(v) {
        if (v === null || v === undefined) return "";
        if (kind === "time") return Math.floor(v / 60) + ":" + String(Math.round(v % 60)).padStart(2, "0");
        return String(Math.round(v * 10) / 10);
    }
    function parse(t) {
        const s = t.trim().replace(/%$/, "");
        if (!s) return null;
        if (kind === "time" && s.includes(":")) {
            const [m, sec] = s.split(":");
            const n = Number(m) * 60 + Number(sec);
            return isFinite(n) ? n : null;
        }
        const n = Number(s);
        return isFinite(n) ? n : null;
    }
    function set(which, t) {
        const r = { min: value ? value.min : null, max: value ? value.max : null };
        r[which] = parse(t);
        edited(r.min === null && r.max === null ? null : r);
    }

    TextField {
        implicitWidth: 56
        horizontalAlignment: Text.AlignHCenter
        placeholderText: root.kind === "time" ? "m:ss" : qsTr("min")
        text: root.show(root.value ? root.value.min : null)
        onEditingFinished: if (text !== root.show(root.value ? root.value.min : null)) root.set("min", text)
    }
    Label { text: "–"; opacity: 0.6 }
    TextField {
        implicitWidth: 56
        horizontalAlignment: Text.AlignHCenter
        placeholderText: root.kind === "time" ? "m:ss" : qsTr("max")
        text: root.show(root.value ? root.value.max : null)
        onEditingFinished: if (text !== root.show(root.value ? root.value.max : null)) root.set("max", text)
    }
    Label { visible: root.kind === "pct"; text: "%"; opacity: 0.6 }
}
