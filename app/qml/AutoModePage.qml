import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// Starting songs automatically, and spoken announcements of the next dance
// (Kokoro text-to-speech). All off by default.
SettingsPage {
    id: page
    title: qsTr("Auto Mode")
    intro: qsTr("Auto mode starts the next song by itself after the pause between songs, and can announce the next dance first: the song ends, a pause, the announcement, another pause, then the next song. Mark songs in a setlist with Auto Mode Stops Before This Song (right-click) to start those yourself with Play Next. Everything here is off unless you turn it on.")

    readonly property var s: Session.settings
    readonly property var a: s ? s.announce : null
    readonly property var tts: Session.tts
    readonly property string voice: tts && tts.voice ? tts.voice : a ? a.voice : ""
    property var voices: null
    property string voicesError: ""
    property string language: ""
    readonly property var prons: Session.styles && Session.styles.pronunciations ? Session.styles.pronunciations : []
    property var phonemes: ({})

    Component.onCompleted: Backend.call("tts_voices").then(v => {
        voices = v;
        const mine = v.find(x => x.key === page.voice);
        language = mine ? mine.language : "";
    }, e => voicesError = String(e))

    readonly property var languages: voices ? [...new Set(voices.map(v => v.language))] : []
    readonly property var shownVoices: voices ? voices.filter(v => !language || v.language === language) : []

    function setAnnounce(patch) { Session.saveSettings({ announce: Object.assign({}, a, patch) }); }
    function saveProns(list) { phonemes = {}; Session.run("pronunciations_save", { list: list }); }

    GridLayout {
        visible: !!page.a
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Next song"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            RadioButton { text: qsTr("Manual: start each song with Play Next"); checked: !!page.s && !page.s.autoAdvance; onToggled: if (checked) Session.saveSettings({ autoAdvance: false }) }
            RadioButton { text: qsTr("Automatic (auto mode)"); checked: !!page.s && !!page.s.autoAdvance; onToggled: if (checked) Session.saveSettings({ autoAdvance: true }) }
            Label { text: qsTr("The Auto button under Play Next switches it for the current session."); font.pointSize: Theme.smallPointSize; opacity: 0.7 }
        }

        FormLabel { text: qsTr("Announcements"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            CheckBox { text: qsTr("Announce the next dance before starting it"); checked: !!page.a && page.a.enabled; onToggled: page.setAnnounce({ enabled: checked }) }
            Label {
                visible: !!page.a && !page.a.enabled
                text: qsTr("Without announcements, auto mode waits for the “pause between songs” (Settings → General).")
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
            }
        }

        FormLabel { text: qsTr("Chime"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            ChimeField {
                Layout.maximumWidth: 420
                inherits: false
                soundAlways: true
                value: page.a ? { enabled: page.a.chime, sound: page.a.chimeSound } : ({})
                title: qsTr("your announcements")
                beforePicking: () => Session.settingsPage = ""
                afterPicking: () => Session.settingsPage = "auto"
                onEdited: v => page.setAnnounce({ chime: !!v.enabled, chimeSound: v.sound || null })
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Unless something more specific chooses otherwise — first a song's place in a set, then the set, the version, the song, and last this setting. Songs and versions can also have their own words (Info panel).")
            }
        }

        FormLabel { text: qsTr("Timing") }
        RowLayout {
            Label { text: qsTr("Pause") }
            TextField {
                implicitWidth: 56
                text: page.a ? String(page.a.pauseBeforeSecs) : ""
                validator: DoubleValidator { bottom: 0 }
                onEditingFinished: page.setAnnounce({ pauseBeforeSecs: Number(text) })
            }
            Label { text: qsTr("s, announcement, pause") }
            TextField {
                implicitWidth: 56
                text: page.a ? String(page.a.pauseAfterSecs) : ""
                validator: DoubleValidator { bottom: 0 }
                onEditingFinished: page.setAnnounce({ pauseAfterSecs: Number(text) })
            }
            Label { text: qsTr("s, next song") }
        }

        FormLabel { text: qsTr("What to say"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("One is picked at random each time (not the same twice in a row). {dance} becomes the next dance, as the projector names it.")
            }
            Repeater {
                model: page.a ? page.a.patterns : []
                delegate: RowLayout {
                    required property string modelData
                    required property int index
                    Layout.fillWidth: true
                    TextField {
                        Layout.fillWidth: true
                        text: modelData
                        onEditingFinished: {
                            const patterns = page.a.patterns.slice();
                            patterns[index] = text;
                            page.setAnnounce({ patterns: patterns });
                        }
                    }
                    ToolButton { icon.source: Theme.icon("x"); onClicked: page.setAnnounce({ patterns: page.a.patterns.filter((_, j) => j !== index) }) }
                }
            }
            Button { icon.source: Theme.icon("plus"); text: qsTr("Add Sentence"); onClicked: page.setAnnounce({ patterns: page.a.patterns.concat([qsTr("Next up: {dance}.")]) }) }
        }

        FormLabel { text: qsTr("Voice"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            RowLayout {
                visible: !!page.voices
                ComboBox {
                    implicitContentWidthPolicy: ComboBox.WidestText
                    model: [qsTr("All languages")].concat(page.languages)
                    currentIndex: page.languages.indexOf(page.language) + 1
                    onActivated: index => page.language = index === 0 ? "" : page.languages[index - 1]
                }
                ComboBox {
                    Layout.preferredWidth: 260
                    model: page.shownVoices.map(v => v.name + " (" + v.gender + ")" + (page.tts && page.tts.installedVoices.includes(v.key) ? qsTr(" (downloaded)") : ""))
                    currentIndex: page.shownVoices.findIndex(v => v.key === page.voice)
                    onActivated: index => page.setAnnounce({ voice: page.shownVoices[index].key })
                }
            }
            Label { visible: !!page.voicesError; text: qsTr("Couldn't load Kokoro's voice list (%1). Voice: %2").arg(page.voicesError).arg(page.voice); color: Theme.warn; wrapMode: Text.WordWrap; Layout.fillWidth: true }
            Label { visible: !page.voices && !page.voicesError; text: qsTr("Loading voices…"); opacity: 0.6 }
            RowLayout {
                Label {
                    visible: !!page.tts && page.tts.state === "installing"
                    text: page.tts && page.tts.state === "installing" ? qsTr("Downloading %1… %2").arg(page.tts.what || "").arg(page.tts.total ? Math.round(page.tts.done / page.tts.total * 100) + "%" : (page.tts.done / 1e6).toFixed(0) + " MB") : ""
                }
                Icon { visible: !!page.tts && page.tts.state === "ready"; glyph: "check"; color: Theme.ok }
                Label { visible: !!page.tts && page.tts.state === "ready"; text: qsTr("Ready"); color: Theme.ok }
                Button {
                    visible: !!page.tts && page.tts.state === "ready"
                    text: qsTr("Remove…")
                    onClicked: Session.confirm(qsTr("Delete the announcer's model and voices from this computer?"), qsTr("Delete"), ok => { if (ok) Session.run("tts_remove"); })
                }
                Button {
                    visible: !page.tts || (page.tts.state !== "ready" && page.tts.state !== "installing")
                    highlighted: true
                    text: page.tts && page.tts.installedVoices.length ? qsTr("Download This Voice") : qsTr("Download the Voice (%1 MB)").arg(page.a && page.a.model === "fp32" ? 325 : 160)
                    onClicked: Session.run("tts_install")
                }
                Label { visible: !!page.tts && page.tts.state === "error"; text: page.tts && page.tts.message ? page.tts.message : ""; color: Theme.error }
            }
            RowLayout {
                Label { text: qsTr("Quality") }
                ComboBox {
                    implicitContentWidthPolicy: ComboBox.WidestText
                    model: [qsTr("Standard (160 MB)"), qsTr("Full precision (325 MB)")]
                    currentIndex: page.a && page.a.model === "fp32" ? 1 : 0
                    onActivated: index => page.setAnnounce({ model: index === 1 ? "fp32" : "fp16" })
                }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Announcements are spoken by Kokoro, an open-source text-to-speech model that runs inside Orpheus on this computer. Downloading needs the internet once.")
            }
        }

        FormLabel { text: qsTr("Pronunciation"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("How announcements say words they get wrong, e.g. dance names. Write it the way it sounds (“shot-EESH”), or give phonemes between slashes (/ʃɑtˈiʃ/); “phonemes” shows what the voice reads now. Saved with your library.")
            }
            Repeater {
                model: page.prons
                delegate: ColumnLayout {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    RowLayout {
                        Layout.fillWidth: true
                        TextField {
                            Layout.preferredWidth: 180
                            text: modelData.word
                            placeholderText: qsTr("Word or phrase")
                            onEditingFinished: page.saveProns(page.prons.map((p, j) => j === index ? Object.assign({}, p, { word: text.trim() }) : p))
                        }
                        TextField {
                            Layout.fillWidth: true
                            text: modelData.say
                            placeholderText: qsTr("Say as")
                            onEditingFinished: page.saveProns(page.prons.map((p, j) => j === index ? Object.assign({}, p, { say: text }) : p))
                        }
                        ToolButton {
                            icon.source: Theme.icon("speaker")
                            enabled: !!page.tts && page.tts.state === "ready" && !!modelData.word
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Say it with this pronunciation")
                            onClicked: Session.run("tts_say", { text: modelData.word })
                        }
                        Button {
                            flat: true
                            text: qsTr("phonemes")
                            enabled: !!page.tts && page.tts.state === "ready" && !!modelData.word
                            onClicked: Session.run("tts_phonemes", { text: modelData.word }).then(p => {
                                if (p === undefined) return;
                                const next = Object.assign({}, page.phonemes);
                                next[index] = p;
                                page.phonemes = next;
                            })
                        }
                        ToolButton { icon.source: Theme.icon("x"); onClicked: page.saveProns(page.prons.filter((_, j) => j !== index)) }
                    }
                    Label { visible: !!page.phonemes[index]; text: page.phonemes[index] || ""; font.family: "monospace"; opacity: 0.8 }
                }
            }
            RowLayout {
                TextField { id: newWord; Layout.preferredWidth: 180; placeholderText: qsTr("Word or phrase"); onAccepted: addPron.clicked() }
                Button {
                    id: addPron
                    icon.source: Theme.icon("plus"); text: qsTr("Add Pronunciation")
                    enabled: newWord.text.trim().length > 0
                    onClicked: { page.saveProns(page.prons.concat([{ word: newWord.text.trim(), say: "" }])); newWord.text = ""; }
                }
            }
        }

        FormLabel { text: qsTr("Sound"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Layout.fillWidth: true
            RowLayout {
                Label { text: qsTr("Speed") }
                Slider { from: 0.7; to: 1.4; stepSize: 0.05; value: page.a ? page.a.speed || 1 : 1; onMoved: page.setAnnounce({ speed: value }) }
                Label { text: qsTr("Volume") }
                Slider { id: vol; from: 0; to: 1.5; stepSize: 0.05; value: page.a ? page.a.volume : 1; onMoved: page.setAnnounce({ volume: value }) }
                Label { text: Math.round(vol.value * 100) + "%"; opacity: 0.7 }
            }
            RowLayout {
                Layout.fillWidth: true
                TextField { id: sample; Layout.fillWidth: true; placeholderText: qsTr("Try a sentence…") }
                Button {
                    text: qsTr("Say It")
                    enabled: !!page.tts && page.tts.state === "ready"
                    onClicked: {
                        const pattern = page.a.patterns.find(p => p.trim()) || qsTr("The next dance is a {dance}.");
                        Session.run("tts_say", { text: sample.text.trim() || pattern.replace("{dance}", qsTr("Rotary Waltz")) });
                    }
                }
            }
            Label { text: qsTr("Announcements play in the room and in your headphones."); font.pointSize: Theme.smallPointSize; opacity: 0.7 }
        }
    }
}
