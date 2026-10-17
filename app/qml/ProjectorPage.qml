import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Projector")

    readonly property var s: Session.settings
    readonly property var p: s ? s.projector : null
    function setProj(patch) { Session.saveSettings({ projector: Object.assign({}, p, patch) }); }

    GridLayout {
        visible: !!page.p
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        Label { text: qsTr("Title") }
        TextField {
            Layout.preferredWidth: 320
            placeholderText: qsTr("(setlist name)")
            text: page.p ? page.p.title : ""
            onEditingFinished: page.setProj({ title: text })
        }

        Label { text: qsTr("Layout") }
        TabBar {
            readonly property var values: ["auto", "live", "preset"]
            currentIndex: page.p ? Math.max(0, values.indexOf(page.p.displayMode)) : 0
            TabButton { text: qsTr("Follow set mode"); width: implicitWidth; onClicked: page.setProj({ displayMode: "auto" }) }
            TabButton { text: qsTr("Now playing"); width: implicitWidth; onClicked: page.setProj({ displayMode: "live" }) }
            TabButton { text: qsTr("Full setlist"); width: implicitWidth; onClicked: page.setProj({ displayMode: "preset" }) }
        }

        Label { text: qsTr("Show"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            CheckBox { text: qsTr("Next dance during the pause (so dancers can find a partner)"); checked: !!page.p && page.p.revealNextInGap; onToggled: page.setProj({ revealNextInGap: checked }) }
            CheckBox { text: qsTr("“Up next” while a song plays"); checked: !!page.p && page.p.showNext; onToggled: page.setProj({ showNext: checked }) }
            CheckBox { text: qsTr("Artist"); checked: !!page.p && page.p.showArtist; onToggled: page.setProj({ showArtist: checked }) }
            CheckBox { text: qsTr("Tempo (BPM)"); checked: !!page.p && page.p.showBpm; onToggled: page.setProj({ showBpm: checked }) }
            CheckBox { text: qsTr("Clock"); checked: !!page.p && page.p.showClock; onToggled: page.setProj({ showClock: checked }) }
            CheckBox { text: qsTr("Cover art"); checked: !!page.p && page.p.showArtwork; onToggled: page.setProj({ showArtwork: checked }) }
        }

        Label { text: qsTr("Recently played") }
        RowLayout {
            SpinBox { from: 0; to: 12; value: page.p ? page.p.historyCount : 0; onValueModified: page.setProj({ historyCount: value }) }
            Label { text: qsTr("songs") }
        }
        Label { text: qsTr("Coming up") }
        RowLayout {
            SpinBox { from: 0; to: 8; value: page.p ? page.p.upcomingCount || 0 : 0; onValueModified: page.setProj({ upcomingCount: value }) }
            Label { text: qsTr("songs or slots (live layout; 0 = just “Up next”)"); opacity: 0.7 }
        }
        Label { text: qsTr("Text size") }
        RowLayout {
            Slider {
                id: textScale
                from: 0.6; to: 1.6; stepSize: 0.05
                value: page.p ? page.p.fontScale : 1
                onMoved: page.setProj({ fontScale: value })
            }
            Label { text: Math.round(textScale.value * 100) + "%" }
        }
        Label { text: qsTr("Theme") }
        TabBar {
            currentIndex: page.p && page.p.theme === "light" ? 1 : 0
            TabButton { text: qsTr("Dark"); width: implicitWidth; onClicked: page.setProj({ theme: "dark" }) }
            TabButton { text: qsTr("Light"); width: implicitWidth; onClicked: page.setProj({ theme: "light" }) }
        }
    }
}
