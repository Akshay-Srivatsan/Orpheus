pragma Singleton
import QtQuick

// Colours and sizes the interface uses beyond the system palette.
QtObject {
    /// Deck A cues in headphones by default (blue), deck B is the room (orange).
    readonly property var deckColors: ["#0a84ff", "#ff9f0a"]
    readonly property color onAir: "#ff453a"
    readonly property color ok: "#30d158"
    readonly property color warn: "#ffd60a"
    readonly property color error: "#ff453a"
    readonly property color spotify: "#1db954"
    /// Hot cue pads 1–4.
    readonly property var cueColors: ["#ff453a", "#ff9f0a", "#30d158", "#64d2ff"]
    /// Stems: vocals, drums, bass, other.
    readonly property var stemColors: ["#ff6482", "#ffd60a", "#30d158", "#64d2ff"]
    /// Styles without a colour.
    readonly property color neutral: "#8e8e93"

    readonly property int spacing: 8
    readonly property int rowHeight: 26
    readonly property int waveformHeight: 58
    readonly property int decksHeight: 262
    readonly property int sidebarWidth: 220
    readonly property int rightPanelWidth: 380
    /// A bundled icon (used when the system's icon theme lacks one).
    function icon(name) { return "qrc:/icons/" + name + ".svg"; }

    /// Toasts stay this long (ms): errors longer, and longer still with a button.
    readonly property int toastMs: 3500
    readonly property int toastErrorMs: 6000
    readonly property int toastActionMs: 10000
}
