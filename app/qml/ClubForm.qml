import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

// A club's guidelines as a form: the club, the set, styles (with their
// suggested songs), groups, rules for part of the set, tags, announcements.
// Changes go through the clubs page (`editor.edit(change)`).
ColumnLayout {
    id: root
    property var editor
    readonly property var c: editor.draft
    spacing: Theme.spacing

    function edit(change) { editor.edit(change); }
    function heading(t) { return t; }

    // ------------------------------------------------------------ the club
    GridLayout {
        visible: !!root.c
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: 4
        FormLabel { text: qsTr("Name") }
        TextField { Layout.fillWidth: true; text: root.c ? root.c.name : ""; onEditingFinished: if (text !== root.c.name) root.edit(d => d.name = text) }
        FormLabel { text: qsTr("Description") }
        TextArea { Layout.fillWidth: true; wrapMode: TextEdit.Wrap; text: root.c ? root.c.description : ""; onEditingFinished: if (text !== root.c.description) root.edit(d => d.description = text) }
        FormLabel { text: qsTr("Guidelines page") }
        TextField { Layout.fillWidth: true; placeholderText: qsTr("https://… (for people)"); text: root.c && root.c.website ? root.c.website : ""; onEditingFinished: root.edit(d => d.website = text || null) }
        FormLabel { text: qsTr("File's web address"); ToolTip.visible: urlHover.hovered; ToolTip.text: qsTr("Where this file is published; Check for Updates fetches it"); HoverHandler { id: urlHover } }
        TextField { Layout.fillWidth: true; placeholderText: "https://…/club.toml"; text: root.c && root.c.url ? root.c.url : ""; onEditingFinished: root.edit(d => d.url = text || null) }
    }

    // ------------------------------------------------------------ the set
    Heading { text: qsTr("The Set") }
    GridLayout {
        visible: !!root.c
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: 4
        FormLabel { text: qsTr("Minutes") }
        RangeInput { value: root.c ? root.c.set.minutes : null; onEdited: r => root.edit(d => d.set.minutes = r) }
        FormLabel { text: qsTr("Songs") }
        RangeInput { value: root.c ? root.c.set.songs : null; onEdited: r => root.edit(d => d.set.songs = r) }
        FormLabel { text: qsTr("Pause (seconds)") }
        RowLayout {
            TextField { implicitWidth: 60; placeholderText: qsTr("target"); text: root.c && root.c.set.pause.target != null ? String(root.c.set.pause.target) : ""; onEditingFinished: root.edit(d => d.set.pause.target = root.editor.num(text)) }
            Label { text: qsTr("between"); opacity: 0.6 }
            RangeInput {
                value: root.c ? { min: root.c.set.pause.min, max: root.c.set.pause.max } : null
                onEdited: r => root.edit(d => { d.set.pause.min = r ? r.min : null; d.set.pause.max = r ? r.max : null; })
            }
        }
        FormLabel { text: qsTr("Fade-out (seconds)") }
        TextField { implicitWidth: 60; text: root.c && root.c.set.fade_out != null ? String(root.c.set.fade_out) : ""; onEditingFinished: root.edit(d => d.set.fade_out = root.editor.num(text)) }
        FormLabel { text: qsTr("Tempo change allowed") }
        RowLayout {
            Label { text: qsTr("faster") }
            TextField {
                implicitWidth: 50
                placeholderText: "%"
                text: root.c && root.c.set.tempo_change ? String(root.c.set.tempo_change.faster) : ""
                onEditingFinished: root.edit(d => {
                    const v = root.editor.num(text);
                    d.set.tempo_change = v === null && !(d.set.tempo_change && d.set.tempo_change.slower) ? null : { faster: v || 0, slower: d.set.tempo_change ? d.set.tempo_change.slower : 0 };
                })
            }
            Label { text: qsTr("%, slower") }
            TextField {
                implicitWidth: 50
                placeholderText: "%"
                text: root.c && root.c.set.tempo_change ? String(root.c.set.tempo_change.slower) : ""
                onEditingFinished: root.edit(d => {
                    const v = root.editor.num(text);
                    d.set.tempo_change = v === null && !(d.set.tempo_change && d.set.tempo_change.faster) ? null : { faster: d.set.tempo_change ? d.set.tempo_change.faster : 0, slower: v || 0 };
                })
            }
            Label { text: qsTr("% (empty: your tempo slider range)"); opacity: 0.6 }
        }
        FormLabel { text: qsTr("Same style in a row") }
        RowLayout {
            Label { text: qsTr("at most") }
            TextField { implicitWidth: 50; text: root.c && root.c.set.max_in_a_row != null ? String(root.c.set.max_in_a_row) : ""; onEditingFinished: root.edit(d => d.set.max_in_a_row = root.editor.num(text)) }
        }
        FormLabel { text: qsTr("Auto mode") }
        ComboBox {
            implicitContentWidthPolicy: ComboBox.WidestText
            model: [qsTr("Not specified"), qsTr("Manual (Play Next)"), qsTr("Automatic")]
            currentIndex: !root.c || root.c.set.auto_advance === null || root.c.set.auto_advance === undefined ? 0 : root.c.set.auto_advance ? 2 : 1
            onActivated: index => root.edit(d => d.set.auto_advance = index === 0 ? null : index === 2)
        }
    }

    // ------------------------------------------------------------ styles
    Heading { text: qsTr("Styles") }
    RowLayout {
        spacing: 4
        Item { Layout.preferredWidth: 26 }
        Label { Layout.preferredWidth: 160; text: qsTr("Name"); opacity: 0.6 }
        Label { Layout.preferredWidth: 70; text: qsTr("Short"); opacity: 0.6 }
        Label { Layout.preferredWidth: 60; text: qsTr("Meter"); opacity: 0.6; ToolTip.visible: meterHover.hovered; ToolTip.text: qsTr("Beats per bar, e.g. 3 or 2, 4"); HoverHandler { id: meterHover } }
        Label { Layout.preferredWidth: 130; text: qsTr("Tempo (BPM)"); opacity: 0.6 }
        Label { Layout.preferredWidth: 130; text: qsTr("Preferred"); opacity: 0.6 }
        Label { Layout.preferredWidth: 130; text: qsTr("Per set"); opacity: 0.6 }
    }
    Repeater {
        model: root.c ? root.editor.entries(root.c.style) : []
        delegate: ColumnLayout {
            id: styleRow
            required property var modelData
            readonly property string sid: modelData.id
            readonly property var st: modelData.v
            readonly property var songs: root.editor.songsOf(st)
            Layout.fillWidth: true
            spacing: 2
            RowLayout {
                spacing: 4
                Rectangle {
                    Layout.preferredWidth: 22; Layout.preferredHeight: 22; radius: 4
                    color: styleRow.st.color || Theme.neutral
                    border.color: palette.mid
                    MouseArea { anchors.fill: parent; onClicked: root.editor.pickColor(styleRow.st.color, col => root.edit(d => d.style[styleRow.sid].color = col)) }
                }
                TextField { Layout.preferredWidth: 160; text: styleRow.st.name; ToolTip.visible: hovered; ToolTip.text: qsTr("id: %1").arg(styleRow.sid); onEditingFinished: if (text !== styleRow.st.name) root.edit(d => d.style[styleRow.sid].name = text) }
                TextField { Layout.preferredWidth: 70; text: styleRow.st.short || ""; onEditingFinished: root.edit(d => d.style[styleRow.sid].short = text || null) }
                TextField { Layout.preferredWidth: 60; placeholderText: qsTr("any"); text: (styleRow.st.meter || []).join(", "); onEditingFinished: root.edit(d => d.style[styleRow.sid].meter = root.editor.meters(text)) }
                RangeInput { value: styleRow.st.tempo; onEdited: r => root.edit(d => d.style[styleRow.sid].tempo = r) }
                RangeInput { value: styleRow.st.preferred; onEdited: r => root.edit(d => d.style[styleRow.sid].preferred = r) }
                RangeInput { value: styleRow.st.count; onEdited: r => root.edit(d => d.style[styleRow.sid].count = r) }
                ToolButton {
                    text: styleRow.songs.length ? "♪" + styleRow.songs.length : "⋯"
                    checked: root.editor.openStyle === styleRow.sid
                    ToolTip.visible: hovered; ToolTip.text: qsTr("More: length, share, where in the set, notes, suggested songs")
                    onClicked: root.editor.openStyle = root.editor.openStyle === styleRow.sid ? "" : styleRow.sid
                }
                ToolButton { icon.source: Theme.icon("x"); onClicked: root.edit(d => delete d.style[styleRow.sid]) }
            }
            // More about the style, and its suggested songs.
            ColumnLayout {
                visible: root.editor.openStyle === styleRow.sid
                Layout.fillWidth: true
                Layout.leftMargin: 30
                RowLayout {
                    Label { text: qsTr("Length") }
                    RangeInput { kind: "time"; value: styleRow.st.length; onEdited: r => root.edit(d => d.style[styleRow.sid].length = r) }
                    Label { text: qsTr("Share") }
                    RangeInput { kind: "pct"; value: styleRow.st.share; onEdited: r => root.edit(d => d.style[styleRow.sid].share = r) }
                    Label { text: qsTr("In a row, at most") }
                    TextField { implicitWidth: 44; text: styleRow.st.max_in_a_row != null ? String(styleRow.st.max_in_a_row) : ""; onEditingFinished: root.edit(d => d.style[styleRow.sid].max_in_a_row = root.editor.num(text)) }
                }
                RowLayout {
                    Label { text: qsTr("Where in the set") }
                    TextField { Layout.preferredWidth: 200; placeholderText: qsTr("e.g. 0–40, 90–100 (%)"); text: root.editor.whenText(styleRow.st.when); onEditingFinished: root.edit(d => d.style[styleRow.sid].when = root.editor.whenParse(text)) }
                    CheckBox { text: qsTr("first song"); checked: !!styleRow.st.first; onToggled: root.edit(d => d.style[styleRow.sid].first = checked) }
                    CheckBox { text: qsTr("last song"); checked: !!styleRow.st.last; onToggled: root.edit(d => d.style[styleRow.sid].last = checked) }
                }
                TextField { Layout.fillWidth: true; placeholderText: qsTr("Note"); text: styleRow.st.note || ""; onEditingFinished: root.edit(d => d.style[styleRow.sid].note = text || null) }
                Label {
                    text: qsTr("Suggested songs") + (root.editor.hasCriteria(styleRow.st) ? "" : qsTr(" (this style has no meter, tempo or length: only these songs fit it)"))
                    opacity: 0.7
                }
                Repeater {
                    model: styleRow.songs
                    delegate: ColumnLayout {
                        id: songRow
                        required property var modelData
                        required property int index
                        readonly property bool own: index < (styleRow.st.song || []).length
                        readonly property var here: (root.editor.songStatus[styleRow.sid] || [])[index]
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                text: (songRow.modelData.title || qsTr("Untitled")) + (songRow.modelData.artist ? " — " + songRow.modelData.artist : "")
                                    + (songRow.modelData.length ? " · " + root.editor.secs(songRow.modelData.length) : "") + (songRow.modelData.note ? " · " + songRow.modelData.note : "")
                            }
                            Label {
                                opacity: 0.6
                                text: [songRow.modelData.fingerprint ? "fingerprint" : "", songRow.modelData.spotify ? "Spotify" : "", songRow.modelData.hash ? "hash" : ""].filter(x => x).join(" · ")
                            }
                            Label {
                                text: songRow.here && songRow.here.have ? qsTr("✓ in your library") : qsTr("not in your library")
                                color: songRow.here && songRow.here.have ? Theme.ok : palette.text
                                opacity: songRow.here && songRow.here.have ? 1 : 0.6
                                ToolTip.visible: haveHover.hovered && !!songRow.here && songRow.here.have
                                ToolTip.text: songRow.here ? songRow.here.songs : ""
                                HoverHandler { id: haveHover }
                            }
                            Button {
                                visible: !(songRow.here && songRow.here.have) && !!songRow.modelData.spotify
                                flat: true
                                text: qsTr("Add from Spotify")
                                onClicked: Session.run("spotify_resolve", { input: songRow.modelData.spotify }).then(found => {
                                    if (found && found.length) Session.run("spotify_import", { tracks: found, inLibrary: true }).then(() => Session.toast(qsTr("Added to your library from Spotify."), "info"));
                                })
                            }
                            Repeater {
                                model: songRow.here && songRow.here.have ? [] : (songRow.modelData.download || [])
                                delegate: Button {
                                    required property string modelData
                                    required property int index
                                    flat: true
                                    enabled: !root.editor.busy && !!songRow.modelData.hash
                                    text: qsTr("Download") + ((songRow.modelData.download || []).length > 1 ? " (" + (index + 1) + ")" : "")
                                    ToolTip.visible: hovered; ToolTip.text: modelData
                                    onClicked: {
                                        root.editor.busy = true;
                                        Session.run("club_song_download", { url: modelData, hash: songRow.modelData.hash }).then(p => {
                                            root.editor.busy = false;
                                            if (p) Session.toast(qsTr("Downloaded and added to your library (%1).").arg(p), "info");
                                        });
                                    }
                                }
                            }
                            Label { visible: !songRow.own; text: qsTr("song library: %1").arg(songRow.modelData.id || ""); opacity: 0.6 }
                            ToolButton {
                                icon.source: Theme.icon("x")
                                ToolTip.visible: hovered
                                ToolTip.text: songRow.own ? qsTr("Remove") : qsTr("Don't suggest it for this style")
                                onClicked: songRow.own ? root.edit(d => d.style[styleRow.sid].song.splice(songRow.index, 1))
                                    : root.edit(d => d.style[styleRow.sid].suggested = (d.style[styleRow.sid].suggested || []).filter(x => x !== songRow.modelData.id))
                            }
                        }
                        TextArea {
                            visible: songRow.own
                            Layout.fillWidth: true
                            placeholderText: qsTr("Download links, one per line (need the file's hash)")
                            text: (songRow.modelData.download || []).join("\n")
                            onEditingFinished: root.edit(d => d.style[styleRow.sid].song[songRow.index].download = text.split("\n").map(l => l.trim()).filter(l => l))
                        }
                    }
                }
                Button {
                    icon.source: Theme.icon("plus"); text: qsTr("Add the Selected Song")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("The song selected in the library (its fingerprint, file hash and Spotify link)")
                    onClicked: {
                        const vid = Session.selected[Session.selected.length - 1];
                        if (!vid) { Session.toast(qsTr("Select a song in the library first."), "info"); return; }
                        Session.run("club_song_for_version", { versionId: vid }).then(song => {
                            if (song) root.edit(d => d.style[styleRow.sid].song = (d.style[styleRow.sid].song || []).concat([song]));
                        });
                    }
                }
            }
        }
    }
    Button { icon.source: Theme.icon("plus"); text: qsTr("Add Style"); onClicked: root.editor.addItem("style") }

    // ------------------------------------------------------------ groups
    Label { text: qsTr("GROUPS (styles counted together)"); font.pointSize: Theme.smallPointSize; font.bold: true; opacity: 0.6; Layout.topMargin: Theme.spacing }
    Repeater {
        model: root.c ? root.editor.entries(root.c.group) : []
        delegate: RowLayout {
            id: groupRow
            required property var modelData
            Layout.fillWidth: true
            TextField { Layout.preferredWidth: 150; text: groupRow.modelData.v.name; onEditingFinished: root.edit(d => d.group[groupRow.modelData.id].name = text) }
            Flow {
                Layout.fillWidth: true
                spacing: 3
                Repeater {
                    model: root.editor.entries(root.c.style)
                    delegate: Button {
                        required property var modelData
                        readonly property bool on: groupRow.modelData.v.styles.includes(modelData.id)
                        text: modelData.v.short || modelData.v.name
                        checkable: true
                        checked: on
                        implicitHeight: 24
                        onClicked: root.edit(d => d.group[groupRow.modelData.id].styles = on ? d.group[groupRow.modelData.id].styles.filter(x => x !== modelData.id) : d.group[groupRow.modelData.id].styles.concat([modelData.id]))
                    }
                }
            }
            Label { text: qsTr("per set") }
            RangeInput { value: groupRow.modelData.v.count; onEdited: r => root.edit(d => d.group[groupRow.modelData.id].count = r) }
            Label { text: qsTr("share") }
            RangeInput { kind: "pct"; value: groupRow.modelData.v.share; onEdited: r => root.edit(d => d.group[groupRow.modelData.id].share = r) }
            ToolButton { icon.source: Theme.icon("x"); onClicked: root.edit(d => delete d.group[groupRow.modelData.id]) }
        }
    }
    Button { icon.source: Theme.icon("plus"); text: qsTr("Add Group"); enabled: !!root.c && Object.keys(root.c.style).length > 0; onClicked: root.editor.addItem("group") }

    // ------------------------------------------------------------ rules
    Heading { text: qsTr("Rules for Part of the Set") }
    Repeater {
        model: root.c ? root.editor.entries(root.c.rule) : []
        delegate: RowLayout {
            id: ruleRow
            required property var modelData
            TextField { Layout.preferredWidth: 150; text: ruleRow.modelData.v.name; onEditingFinished: root.edit(d => d.rule[ruleRow.modelData.id].name = text) }
            Label { text: qsTr("songs") }
            RangeInput { value: ruleRow.modelData.v.songs; onEdited: r => root.edit(d => d.rule[ruleRow.modelData.id].songs = r) }
            Label { text: qsTr("each") }
            RangeInput { kind: "time"; value: ruleRow.modelData.v.length; onEdited: r => root.edit(d => d.rule[ruleRow.modelData.id].length = r) }
            Label { text: qsTr("long") }
            ToolButton { icon.source: Theme.icon("x"); onClicked: root.edit(d => delete d.rule[ruleRow.modelData.id]) }
        }
    }
    RowLayout {
        Button { icon.source: Theme.icon("plus"); text: qsTr("Add Rule"); onClicked: root.editor.addItem("rule") }
        Label { text: qsTr("Rules can also count styles in part of the set (edit the file: styles = […], count = …)."); opacity: 0.6; font.pointSize: Theme.smallPointSize }
    }

    // ------------------------------------------------------------ tags
    Heading { text: qsTr("Tags") }
    Repeater {
        model: root.c ? root.editor.entries(root.c.tag) : []
        delegate: RowLayout {
            id: tagRow
            required property var modelData
            Rectangle {
                Layout.preferredWidth: 22; Layout.preferredHeight: 22; radius: 4
                color: tagRow.modelData.v.color || Theme.neutral
                border.color: palette.mid
                MouseArea { anchors.fill: parent; onClicked: root.editor.pickColor(tagRow.modelData.v.color, col => root.edit(d => d.tag[tagRow.modelData.id].color = col)) }
            }
            TextField { Layout.preferredWidth: 150; text: tagRow.modelData.v.name; onEditingFinished: root.edit(d => d.tag[tagRow.modelData.id].name = text) }
            CheckBox { text: qsTr("on the projector"); checked: !!tagRow.modelData.v.announce; onToggled: root.edit(d => d.tag[tagRow.modelData.id].announce = checked) }
            TextField { Layout.fillWidth: true; placeholderText: qsTr("Note"); text: tagRow.modelData.v.note || ""; onEditingFinished: root.edit(d => d.tag[tagRow.modelData.id].note = text || null) }
            ToolButton { icon.source: Theme.icon("x"); onClicked: root.edit(d => delete d.tag[tagRow.modelData.id]) }
        }
    }
    Button { icon.source: Theme.icon("plus"); text: qsTr("Add Tag"); onClicked: root.editor.addItem("tag") }

    // ------------------------------------------------------------ announcements
    Heading { text: qsTr("Announcements") }
    TextArea {
        Layout.fillWidth: true
        placeholderText: qsTr("One per line: reminders to announce (Announce menu)")
        text: root.c ? root.c.announcements.join("\n") : ""
        onEditingFinished: root.edit(d => d.announcements = text.split("\n").map(l => l.trim()).filter(l => l))
    }
}
