pragma Singleton
import QtQuick
import QtQuick.Controls

// Colours and sizes the interface uses beyond the system palette. Status
// colours come in a light and a dark variant, following the system's theme.
QtObject {
    readonly property SystemPalette system: SystemPalette { colorGroup: SystemPalette.Active }
    /// The system's theme is dark (its window colour is).
    readonly property bool dark: system.window.hslLightness < 0.5

    /// Deck A cues in headphones by default (blue), deck B is the room (orange).
    readonly property var deckColors: dark ? ["#0a84ff", "#ff9f0a"] : ["#0064d2", "#c86400"]
    readonly property color onAir: dark ? "#ff453a" : "#d70015"
    readonly property color ok: dark ? "#30d158" : "#1a7f37"
    readonly property color warn: dark ? "#ffd60a" : "#9a6700"
    readonly property color error: dark ? "#ff453a" : "#cf222e"
    readonly property color spotify: dark ? "#1ed760" : "#1a9e48"
    /// Hot cue pads 1–4.
    readonly property var cueColors: ["#ff453a", "#ff9f0a", "#30d158", "#64d2ff"]
    /// Stems: vocals, drums, bass, other.
    readonly property var stemColors: ["#ff6482", "#ffd60a", "#30d158", "#64d2ff"]
    /// Styles without a colour.
    readonly property color neutral: "#8e8e93"
    /// Secondary text (artist, version, counts): the text colour, faded.
    readonly property real dim: 0.65
    readonly property real faint: 0.45

    /// A text field's height in the current style (form labels line up with it).
    readonly property TextField probe: TextField {}
    readonly property real fieldHeight: probe.implicitHeight

    /// Text sizes, relative to the controls' font (points).
    readonly property real basePointSize: probe.font.pointSize > 0 ? probe.font.pointSize : 10
    readonly property real smallPointSize: basePointSize * 0.85
    readonly property real titlePointSize: basePointSize * 1.3
    readonly property real largePointSize: basePointSize * 1.6
    readonly property real hugePointSize: basePointSize * 2

    readonly property int spacing: 8
    readonly property int rowHeight: 26
    readonly property int iconSize: 16
    /// The player's Play / Pause icon (larger: it's the main control).
    readonly property int playIconSize: 24
    readonly property int smallIconSize: 14
    readonly property int waveformHeight: 58
    readonly property int decksHeight: 262
    readonly property int sidebarWidth: 220
    /// The decks' narrowest, and the transport's width between them (from the font size).
    readonly property int deckMinimumWidth: 300
    readonly property int transportWidth: Math.round(fieldHeight * 11)
    readonly property int rightPanelWidth: 380
    /// A colour (e.g. a dance style's) made readable as text: the same hue, lighter
    /// on a dark background, darker on a light one (`onDark`: the window's theme).
    function readable(c, onDark) {
        const col = Qt.color(c || neutral);
        const darkBg = onDark === undefined ? dark : onDark;
        let l = col.hslLightness;
        if (darkBg && l < 0.62) l = 0.62;
        if (!darkBg && l > 0.42) l = 0.42;
        return Qt.hsla(Math.max(0, col.hslHue), col.hslSaturation, l, 1);
    }
    /// A bundled icon (used when the system's icon theme lacks one).
    function icon(name) { return "qrc:/icons/" + name + ".svg"; }

    /// Toasts stay this long (ms): errors longer, and longer still with a button.
    readonly property int toastMs: 3500
    readonly property int toastErrorMs: 6000
    readonly property int toastActionMs: 10000
}
