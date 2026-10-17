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

        FormLabel { text: qsTr("Title") }
        TextField {
            Layout.preferredWidth: 320
            placeholderText: qsTr("(setlist name)")
            text: page.p ? page.p.title : ""
            onEditingFinished: page.setProj({ title: text })
        }

        FormLabel { text: qsTr("Layout") }
        ComboBox {
            readonly property var values: ["auto", "live", "preset"]
            model: [qsTr("Follow the set's mode"), qsTr("Now playing"), qsTr("Full setlist")]
            currentIndex: page.p ? Math.max(0, values.indexOf(page.p.displayMode)) : 0
            onActivated: index => page.setProj({ displayMode: values[index] })
        }

        FormLabel { text: qsTr("Show"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            CheckBox { text: qsTr("Next dance during the pause (so dancers can find a partner)"); checked: !!page.p && page.p.revealNextInGap; onToggled: page.setProj({ revealNextInGap: checked }) }
            CheckBox { text: qsTr("“Up next” while a song plays"); checked: !!page.p && page.p.showNext; onToggled: page.setProj({ showNext: checked }) }
            CheckBox { text: qsTr("Artist"); checked: !!page.p && page.p.showArtist; onToggled: page.setProj({ showArtist: checked }) }
            CheckBox { text: qsTr("Tempo (BPM)"); checked: !!page.p && page.p.showBpm; onToggled: page.setProj({ showBpm: checked }) }
            CheckBox { text: qsTr("Clock"); checked: !!page.p && page.p.showClock; onToggled: page.setProj({ showClock: checked }) }
            CheckBox { text: qsTr("Cover art"); checked: !!page.p && page.p.showArtwork; onToggled: page.setProj({ showArtwork: checked }) }
        }

        FormLabel { text: qsTr("Recently played") }
        RowLayout {
            SpinBox { from: 0; to: 12; value: page.p ? page.p.historyCount : 0; onValueModified: page.setProj({ historyCount: value }) }
            Label { text: qsTr("songs") }
        }
        FormLabel { text: qsTr("Coming up") }
        RowLayout {
            SpinBox { from: 0; to: 8; value: page.p ? page.p.upcomingCount || 0 : 0; onValueModified: page.setProj({ upcomingCount: value }) }
            Label { text: qsTr("songs or slots (live layout; 0 = just “Up next”)"); opacity: 0.7 }
        }
        FormLabel { text: qsTr("Text size") }
        RowLayout {
            Slider {
                id: textScale
                from: 0.6; to: 1.6; stepSize: 0.05
                value: page.p ? page.p.fontScale : 1
                onMoved: page.setProj({ fontScale: value })
            }
            Label { text: Math.round(textScale.value * 100) + "%" }
        }
        FormLabel { text: qsTr("Theme") }
        ComboBox {
            model: [qsTr("Dark"), qsTr("Light")]
            currentIndex: page.p && page.p.theme === "light" ? 1 : 0
            onActivated: index => page.setProj({ theme: index === 1 ? "light" : "dark" })
        }
    }
}
