import QtQuick
import Orpheus

// A song's overview waveform (low = red, mid = green, high = blue), with the
// playhead, trimmed parts, fade-out point and cue points over it. Click or drag
// to seek (`seeked(ms, alt)`, `scrubbed(ms, alt)`).
Item {
    id: root

    property string recordingId: ""
    /// Changes when the waveform may have become available (e.g. analysed).
    property var revision
    property real durationMs: 0
    property real positionMs: 0
    property real startMs: 0
    property var endMs: null
    property var fadeAtMs: null
    property var cueMs: null
    property var hotCues: []
    property color accent: Theme.system.highlight
    property bool interactive: true

    signal seeked(real ms, bool alt)
    signal scrubbed(real ms, bool alt)

    implicitHeight: Theme.waveformHeight

    property var bins: []
    property var cache: ({})

    function load() {
        const id = recordingId;
        if (!id) { bins = []; return; }
        if (cache[id]) { bins = cache[id]; return; }
        bins = [];
        Backend.call("recording_waveform", { id: id }).then(w => {
            if (w.length) cache[id] = w;
            if (recordingId === id) bins = w;
        });
    }
    onRecordingIdChanged: load()
    onRevisionChanged: { if (recordingId) delete cache[recordingId]; load(); }
    onBinsChanged: canvas.requestPaint()

    function px(ms) { return durationMs > 0 ? ms / durationMs * width : 0; }
    function msAt(x) { return Math.max(0, Math.min(durationMs, x / width * durationMs)); }

    // A dark well in either theme, so the bands' colours read the same.
    Rectangle {
        anchors.fill: parent
        radius: 3
        color: Theme.dark ? Qt.darker(Theme.system.base, 1.4) : Qt.darker(Theme.system.base, 1.06)
        border.color: Theme.system.mid
        // Without a song: just the line the waveform will stand on.
        Rectangle {
            visible: root.bins.length === 0
            anchors.centerIn: parent
            width: parent.width - 16
            height: 1
            color: Theme.system.mid
        }
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const data = root.bins;
            const n = data.length / 4;
            const mid = height / 2;
            if (n === 0) return;
            for (let x = 0; x < width; x++) {
                const b = Math.min(n - 1, Math.floor(x / width * n));
                const peak = data[b * 4] / 255;
                const lo = data[b * 4 + 1], mi = data[b * 4 + 2], hi = data[b * 4 + 3];
                const m = Math.max(lo, mi, hi, 1);
                const r = 60 + 195 * lo / m, g = 60 + 195 * mi / m * 0.9, bl = 70 + 185 * hi / m;
                const h = Math.max(1, peak * (height / 2 - 2));
                ctx.fillStyle = Qt.rgba(r / 255, g / 255, bl / 255, 1);
                ctx.fillRect(x, mid - h, 1, h * 2);
            }
        }
    }

    // Trimmed parts (before the start, after the end) are shaded; what's played is dimmed.
    Rectangle { visible: root.startMs > 0; x: 0; width: root.px(root.startMs); height: parent.height; color: Qt.rgba(0, 0, 0, 0.55) }
    Rectangle {
        visible: root.endMs !== null && root.endMs < root.durationMs
        x: root.endMs !== null ? root.px(root.endMs) : 0
        width: parent.width - x
        height: parent.height
        color: Qt.rgba(0, 0, 0, 0.55)
    }
    Rectangle {
        visible: root.positionMs > root.startMs
        x: root.px(root.startMs)
        width: Math.max(0, root.px(root.positionMs) - x)
        height: parent.height
        color: Qt.rgba(0.05, 0.05, 0.06, 0.5)
    }
    // Fade-out point.
    Rectangle {
        visible: root.fadeAtMs !== null
        x: root.fadeAtMs !== null ? root.px(root.fadeAtMs) : 0
        width: 1
        height: parent.height
        color: Theme.warn
    }
    // Hot cues: lines with their pad number.
    Repeater {
        model: root.hotCues
        delegate: Item {
            required property var modelData
            x: root.px(modelData.ms)
            height: root.height
            Rectangle { width: 1; height: parent.height; color: Theme.cueColors[(modelData.slot - 1) % 4] }
            Rectangle {
                y: parent.height - 11
                width: 9; height: 11
                color: Theme.cueColors[(modelData.slot - 1) % 4]
                Text { anchors.centerIn: parent; text: modelData.slot; font.pixelSize: 9; font.bold: true; color: "black" }
            }
        }
    }
    // Main cue point.
    Icon {
        visible: root.cueMs !== null
        x: root.cueMs !== null ? root.px(root.cueMs) - width / 2 : 0
        y: -4
        glyph: "sort-down"
        sourceSize.width: 14; sourceSize.height: 14
        color: Theme.deckColors[1]
    }
    // Playhead.
    Rectangle {
        visible: root.durationMs > 0
        x: root.px(root.positionMs) - 1
        width: 2
        height: parent.height
        color: root.accent
    }

    MouseArea {
        anchors.fill: parent
        enabled: root.interactive && root.durationMs > 0
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        property bool alt: false
        onPressed: mouse => { alt = (mouse.modifiers & Qt.AltModifier) !== 0; root.seeked(root.msAt(mouse.x), alt); }
        onPositionChanged: mouse => { if (pressed) root.scrubbed(root.msAt(mouse.x), alt); }
    }
}
