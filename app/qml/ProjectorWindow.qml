import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Orpheus

// What dancers see on the big screen: the dance now (or next, during the
// pause), recently played and coming up (live layout), or the whole setlist
// (preset layout), and any announcement. F toggles full screen; Esc leaves it.
Window {
    id: root
    title: qsTr("Orpheus — Projector")
    width: 1280
    height: 720
    color: bg

    property var payload: null
    readonly property var s: payload ? payload.settings : null
    readonly property bool light: !!s && s.theme === "light"
    readonly property real scale: s ? s.fontScale : 1
    /// One unit ≈ 1% of the screen width (bounded by height), times the text size setting.
    readonly property real u: Math.min(width / 100, height / 56.25) * scale
    readonly property color bg: light ? "#f5f5f7" : "#07080a"
    readonly property color card: light ? "#ffffff" : "#121318"
    readonly property color line: light ? "#d6d6dc" : "#2a2c33"
    readonly property color text1: light ? "#0b0b0d" : "#ffffff"
    readonly property color text2: light ? "#3a3b40" : "#d9dbe1"
    readonly property color text3: light ? "#6b6d75" : "#9a9da6"

    readonly property bool gap: !!payload && payload.inGap && !payload.playing
    readonly property bool revealNext: gap && !!s && s.revealNextInGap && !!payload.next
    readonly property bool idle: !payload || (!payload.current && !payload.next && payload.entries.length === 0)
    readonly property var deck: payload ? Session.decks[payload.liveDeck] : null
    /// How far the current song is (0–1), when the deck plays it.
    readonly property var progress: {
        const d = deck;
        if (!d || !payload || !payload.current || d.entryId !== payload.current.id) return null;
        if (d.status === "empty" || d.status === "loading" || d.status === "error") return null;
        const end = d.endMs !== null && d.endMs !== undefined ? d.endMs : d.durationMs;
        return end > d.startMs ? Math.min(1, Math.max(0, (d.positionMs - d.startMs) / (end - d.startMs))) : null;
    }

    /// Shows it on screen `index` (full screen), or in a window (-1).
    function showOn(index, fullscreen) {
        if (index >= 0 && index < Qt.application.screens.length) screen = Qt.application.screens[index];
        visibility = fullscreen ? Window.FullScreen : Window.Windowed;
        show();
        raise();
    }

    Component.onCompleted: Backend.call("projector_state").then(p => payload = p)
    Connections {
        target: Backend
        function onEvent(name, p) { if (name === "projector") root.payload = p; }
    }

    Shortcut { sequence: "F"; onActivated: root.visibility = root.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen }
    Shortcut { sequences: [StandardKey.Cancel]; onActivated: root.visibility = Window.Windowed }

    // The cursor hides after a moment without movement.
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: hideCursor.running ? Qt.ArrowCursor : Qt.BlankCursor
        onPositionChanged: hideCursor.restart()
        onDoubleClicked: root.visibility = root.visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen
        Timer { id: hideCursor; interval: 2000; running: true }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 2.4 * root.u
        spacing: 2 * root.u

        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                Layout.preferredHeight: 4 * root.u
                text: root.payload ? root.payload.title : "Orpheus"
                color: root.text2
                font.pixelSize: 3 * root.u
                font.bold: true
                fontSizeMode: Text.Fit
                minimumPixelSize: 10
                elide: Text.ElideRight
            }
            Text {
                visible: !root.s || root.s.showClock
                text: clockTimer.now.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
                color: root.text2
                font.pixelSize: 3 * root.u
                font.bold: true
                Timer { id: clockTimer; property date now: new Date(); interval: 1000; running: true; repeat: true; onTriggered: now = new Date() }
            }
        }

        // Before anything plays.
        ColumnLayout {
            visible: root.idle
            Layout.fillWidth: true
            Layout.fillHeight: true
            Item { Layout.fillHeight: true }
            Text { Layout.alignment: Qt.AlignHCenter; text: root.payload ? root.payload.title : "Orpheus"; color: root.text1; font.pixelSize: 6 * root.u; font.bold: true }
            Text { Layout.alignment: Qt.AlignHCenter; text: qsTr("The dancing will begin shortly"); color: root.text3; font.pixelSize: 3 * root.u }
            Item { Layout.fillHeight: true }
        }

        Loader {
            visible: !root.idle
            Layout.fillWidth: true
            Layout.fillHeight: true
            sourceComponent: root.payload && root.payload.mode === "preset" ? presetView : liveView
        }
    }

    // An announcement across the bottom.
    Rectangle {
        visible: !!root.payload && !!root.payload.message
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 2.4 * root.u
        height: messageText.implicitHeight + 3 * root.u
        radius: 1.6 * root.u
        color: root.light ? "#0b0b0d" : "#ffffff"
        Text {
            id: messageText
            anchors.centerIn: parent
            width: parent.width - 4 * root.u
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            text: root.payload && root.payload.message ? root.payload.message : ""
            color: root.light ? "#ffffff" : "#0b0b0d"
            font.pixelSize: 4 * root.u
            font.bold: true
        }
    }

    // ------------------------------------------------------------ live layout
    Component {
        id: liveView
        RowLayout {
            id: live
            spacing: 2.4 * root.u
            readonly property var upcoming: {
                const n = root.s ? root.s.upcomingCount || 0 : 0;
                if (!n || !root.payload) return [];
                const anchor = root.revealNext ? root.payload.next.id : root.payload.current ? root.payload.current.id : null;
                const i = anchor ? root.payload.entries.findIndex(e => e.id === anchor) : -1;
                return root.payload.entries.slice(i + 1).filter(e => e.status === "pending").slice(0, n);
            }
            readonly property var upNext: !upcoming.length && root.s && root.s.showNext && !root.gap && root.payload.next ? root.payload.next : null
            readonly property var history: {
                if (!root.payload) return [];
                // During the pause the main card shows the last song (unless revealing the next): skip it here.
                const showingLast = root.gap && !root.revealNext;
                const h = showingLast && root.payload.current ? root.payload.history.filter(e => e.id !== root.payload.current.id) : root.payload.history;
                return h.slice(0, root.s ? root.s.historyCount : 0);
            }

            NowCard {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1.75
                projector: root
            }
            ColumnLayout {
                visible: !!live.upNext || live.upcoming.length > 0 || live.history.length > 0
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                spacing: 2.4 * root.u
                ProjectorSection {
                    visible: !!live.upNext
                    Layout.fillWidth: true
                    projector: root
                    heading: qsTr("Up next")
                    entries: live.upNext ? [live.upNext] : []
                    big: true
                }
                ProjectorSection {
                    visible: live.upcoming.length > 0
                    Layout.fillWidth: true
                    projector: root
                    heading: qsTr("Coming up")
                    entries: live.upcoming
                }
                ProjectorSection {
                    visible: live.history.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    projector: root
                    heading: qsTr("Recently played")
                    entries: live.history
                }
                Item { Layout.fillHeight: true }
            }
        }
    }

    // ------------------------------------------------------------ preset layout
    Component {
        id: presetView
        ColumnLayout {
            id: preset
            spacing: 2 * root.u
            /// Columns on screen, and rows per column (fewer than MIN_ROWS look sparse).
            readonly property int columns: 3
            readonly property int maxRows: 18
            readonly property int minRows: 8
            readonly property int rows: Math.min(maxRows, Math.max(minRows, Math.ceil(root.payload.entries.length / columns)))
            readonly property var banner: root.revealNext ? root.payload.next : root.gap ? null : root.payload.current
            readonly property var numbered: {
                let n = 0;
                return root.payload.entries.map(e => ({ e: e, n: e.status === "skipped" || e.kind === "pause" ? null : ++n }));
            }

            Rectangle {
                visible: !!preset.banner
                Layout.fillWidth: true
                Layout.preferredHeight: 10 * root.u
                radius: 1.6 * root.u
                color: root.card
                border.color: root.line
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 1.6 * root.u
                    spacing: 2 * root.u
                    Text { text: root.revealNext ? qsTr("Next dance") : qsTr("Now dancing"); color: root.text3; font.pixelSize: 1.8 * root.u; font.bold: true; font.capitalization: Font.AllUppercase }
                    Text {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        verticalAlignment: Text.AlignVCenter
                        text: preset.banner ? preset.banner.dance : ""
                        color: preset.banner ? preset.banner.color : root.text1
                        font.pixelSize: 5 * root.u
                        font.bold: true
                        fontSizeMode: Text.Fit
                        minimumPixelSize: 12
                    }
                    Text {
                        text: !preset.banner ? "" : root.revealNext ? qsTr("Find a partner!") : preset.banner.title
                        color: root.text2
                        font.pixelSize: 2.6 * root.u
                        elide: Text.ElideRight
                        Layout.maximumWidth: 40 * root.u
                    }
                }
                Rectangle {
                    visible: !root.gap && root.progress !== null
                    anchors.left: parent.left
                    anchors.bottom: parent.bottom
                    anchors.margins: 2
                    height: 0.5 * root.u
                    radius: height / 2
                    width: (parent.width - 4) * (root.progress || 0)
                    color: preset.banner ? preset.banner.color : root.text1
                }
            }

            ListView {
                id: cols
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: ListView.Horizontal
                clip: true
                spacing: 2 * root.u
                interactive: false
                model: Math.ceil(preset.numbered.length / preset.rows)
                readonly property var view: preset
                // Keep the current song's column on screen.
                currentIndex: Math.floor(Math.max(0, root.payload.entries.findIndex(e => e.id === (root.payload.current ? root.payload.current.id : root.payload.next ? root.payload.next.id : ""))) / preset.rows)
                highlightRangeMode: ListView.ApplyRange
                preferredHighlightBegin: width / view.columns
                preferredHighlightEnd: width * 2 / view.columns
                delegate: Column {
                    required property int index
                    width: (cols.width - cols.spacing * (cols.view.columns - 1)) / cols.view.columns
                    spacing: 0.4 * root.u
                    Repeater {
                        model: cols.view.numbered.slice(parent.index * cols.view.rows, (parent.index + 1) * cols.view.rows)
                        delegate: Rectangle {
                            id: row
                            required property var modelData
                            readonly property bool current: !root.gap && !!root.payload.current && modelData.e.id === root.payload.current.id
                            width: parent.width
                            height: (cols.height - (cols.view.rows - 1) * 0.4 * root.u) / cols.view.rows
                            radius: 0.8 * root.u
                            color: current ? Qt.alpha(modelData.e.color, 0.25) : "transparent"
                            opacity: modelData.e.status === "played" ? 0.45 : modelData.e.status === "skipped" ? 0.3 : 1
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 0.6 * root.u
                                spacing: 0.8 * root.u
                                Text { Layout.preferredWidth: 3 * root.u; text: row.modelData.n || ""; color: root.text3; font.pixelSize: 1.8 * root.u; horizontalAlignment: Text.AlignRight }
                                Rectangle { Layout.preferredWidth: 0.45 * root.u; Layout.fillHeight: true; Layout.margins: 0.4 * root.u; radius: width / 2; color: row.modelData.e.color }
                                Text {
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    verticalAlignment: Text.AlignVCenter
                                    text: (row.modelData.e.dance || "—") + (row.modelData.e.title ? "  " + row.modelData.e.title : "")
                                    color: root.text1
                                    font.pixelSize: 2.2 * root.u
                                    font.bold: row.current
                                    font.strikeout: row.modelData.e.status === "skipped"
                                    fontSizeMode: Text.Fit
                                    minimumPixelSize: 9
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
