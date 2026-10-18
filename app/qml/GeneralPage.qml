import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    title: qsTr("General")
    readonly property var s: Session.settings
    function set(patch) { Session.saveSettings(patch); }

    GridLayout {
        visible: !!s
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Default club") }
        RowLayout {
            ComboBox {
                Layout.preferredWidth: 280
                model: [qsTr("No club (your own styles)")].concat(Session.clubs.map(c => c.name))
                currentIndex: s ? Session.clubs.findIndex(c => c.id === s.defaultClub) + 1 : 0
                onActivated: index => set({ defaultClub: index === 0 ? "" : Session.clubs[index - 1].id })
            }
            Button { flat: true; text: qsTr("Clubs and their guidelines…"); onClicked: Session.settingsPage = "clubs" }
        }

        FormLabel { text: qsTr("Pause between songs") }
        RowLayout {
            NumberField {
                implicitWidth: 70
                minimum: 0
                value: s && s.gapSeconds != null ? s.gapSeconds : null
                placeholderText: "8"
                onCommitted: v => set({ gapSeconds: v })
            }
            Label { text: qsTr("seconds, unless the set or its club says otherwise (a set's own: its ⋯ menu)"); opacity: 0.7 }
        }

        FormLabel { text: qsTr("Next song") }
        RowLayout {
            Label { text: s && s.autoAdvance ? qsTr("Automatic (auto mode)") : qsTr("Manual: start each song with Play Next") }
            Button { flat: true; text: qsTr("Auto mode and announcements…"); onClicked: Session.settingsPage = "auto" }
        }

        FormLabel { text: qsTr("Play Next while playing") }
        ColumnLayout {
            CheckBox { text: qsTr("Require a second press"); checked: !!s && s.confirmNextWhilePlaying; onToggled: set({ confirmNextWhilePlaying: checked }) }
            RowLayout {
                Label { text: qsTr("Fade the current song over") }
                TextField {
                    implicitWidth: 60
                    text: s ? String(s.manualNextFadeMs / 1000) : ""
                    validator: DoubleValidator { bottom: 0 }
                    onEditingFinished: set({ manualNextFadeMs: Math.round(Number(text) * 1000) })
                }
                Label { text: qsTr("s first") }
            }
        }

        FormLabel { text: qsTr("Decks") }
        ColumnLayout {
            CheckBox { text: qsTr("Load the next song in the set onto the free deck automatically"); checked: !!s && s.autoLoadNext; onToggled: set({ autoLoadNext: checked }) }
            CheckBox { text: qsTr("Protect the on-air deck: ignore jog wheels, CUE and cue jumps while the room hears it"); checked: !!s && s.protectOnAir; onToggled: set({ protectOnAir: checked }) }
            CheckBox { text: qsTr("Switch headphones to the free deck when a deck goes on air"); checked: !!s && s.headphonesFollowFreeDeck; onToggled: set({ headphonesFollowFreeDeck: checked }) }
        }

        FormLabel { text: qsTr("Crossfader") }
        ColumnLayout {
            RowLayout {
                Label { text: qsTr("Snaps to the center within ±") }
                SpinBox { from: 0; to: 25; value: s ? Math.round(s.crossfaderSnap * 100) : 0; onValueModified: set({ crossfaderSnap: value / 100 }) }
                Label { text: "%" }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Orpheus never moves the crossfader. Play Next, Go Live and auto mode need it centered, or on the side of the deck they start; otherwise they show an error and the controller's lights flash.")
            }
        }

        FormLabel { text: qsTr("Fade-out length") }
        RowLayout {
            NumberField {
                implicitWidth: 60
                minimum: 0.5
                value: s && s.fadeOutMs != null ? s.fadeOutMs / 1000 : null
                placeholderText: "5"
                onCommitted: v => set({ fadeOutMs: v === null ? null : Math.round(v * 1000) })
            }
            Label { text: qsTr("seconds") }
        }

        FormLabel { text: qsTr("Tempo") }
        ColumnLayout {
            CheckBox { text: qsTr("Keep pitch when changing tempo (key lock)"); checked: !!s && s.keylock; onToggled: set({ keylock: checked }) }
            RowLayout {
                Label { text: qsTr("Time-stretch engine") }
                ComboBox {
                    implicitContentWidthPolicy: ComboBox.WidestText
                    readonly property var values: ["signalsmith", "rubberband", "soundtouch", "bungee"]
                    model: [qsTr("Signalsmith Stretch (default, smooth)"), qsTr("Rubber Band R3 (high quality, more CPU)"), qsTr("SoundTouch (crisp rhythm)"), qsTr("Bungee")]
                    currentIndex: s ? Math.max(0, values.indexOf(s.stretchEngine || "signalsmith")) : 0
                    onActivated: index => set({ stretchEngine: values[index] })
                }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("If slowed-down songs sound smeared or phasey, try Rubber Band or SoundTouch. Changes apply to the next song loaded.")
            }
            RowLayout {
                Label { text: qsTr("Tempo slider range ±") }
                SpinBox { from: 2; to: 50; value: s ? s.tempoRangePct : 10; onValueModified: set({ tempoRangePct: value }) }
                Label { text: "%" }
            }
        }

        FormLabel { text: qsTr("Levels") }
        ColumnLayout {
            RowLayout {
                CheckBox { text: qsTr("Level songs automatically to"); checked: !!s && s.autoGain; onToggled: set({ autoGain: checked }) }
                SpinBox { from: -40; to: 0; value: s ? Math.round(s.targetLoudnessDb) : -14; onValueModified: set({ targetLoudnessDb: value }) }
                Label { text: "LUFS" }
            }
            RowLayout {
                CheckBox { text: qsTr("Even out songs' dynamics"); checked: !!s && s.compress; onToggled: set({ compress: checked }) }
                ComboBox {
                    readonly property var values: ["light", "medium", "strong"]
                    implicitContentWidthPolicy: ComboBox.WidestText
                    enabled: !!s && s.compress
                    model: [qsTr("Light"), qsTr("Medium"), qsTr("Strong")]
                    currentIndex: s ? Math.max(0, values.indexOf(s.compression)) : 1
                    onActivated: index => set({ compression: values[index] })
                }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Evening out dynamics (a slow compressor) brings a song's quiet passages up towards its main level, so an orchestral waltz stays audible; a song that stays at one level is barely changed. It needs the song's loudness (from analysis). Its button is beside the room volume too.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Each song is turned up or down to this loudness (EBU R128), but never boosted past its true peak's room below −1 dBTP. Songs on decks follow at once; one playing moves smoothly. A Spotify song is measured when it's next; if it isn't by the time it starts, it plays as it is. The button beside the room volume turns this on and off.")
            }
            CheckBox { text: qsTr("Skip silence at the start and end of songs"); checked: !!s && s.trimSilence; onToggled: set({ trimSilence: checked }) }
        }
    }
}
