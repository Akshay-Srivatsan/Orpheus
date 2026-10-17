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
    // The club's styles in a list; the selected one's details in a form below.
    Heading { text: qsTr("Styles") }
    ViewFrame {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.min(styleList.contentHeight + 2, 8 * Theme.rowHeight + 2)
        visible: styleList.count > 0
        ListView {
            id: styleList
            anchors.fill: parent
            clip: true
            model: root.c ? root.editor.entries(root.c.style) : []
            ScrollBar.vertical: ScrollBar {}
            delegate: ItemDelegate {
                id: styleItem
                required property var modelData
                readonly property var st: modelData.v
                width: ListView.view.width
                highlighted: styleDetails.sid === modelData.id
                onClicked: root.editor.openStyle = modelData.id
                contentItem: RowLayout {
                    spacing: Theme.spacing
                    Rectangle { implicitWidth: 12; implicitHeight: 12; radius: 6; color: styleItem.st.color || Theme.neutral; border.color: Theme.system.mid }
                    Label { Layout.fillWidth: true; text: styleItem.st.name; elide: Text.ElideRight; color: styleItem.highlighted ? palette.highlightedText : palette.text }
                    Label {
                        opacity: Theme.dim
                        color: styleItem.highlighted ? palette.highlightedText : palette.text
                        text: [(styleItem.st.meter || []).length ? qsTr("meter %1").arg(styleItem.st.meter.join(", ")) : "",
                            styleItem.st.tempo && (styleItem.st.tempo.min != null || styleItem.st.tempo.max != null) ? (styleItem.st.tempo.min ?? "") + "–" + (styleItem.st.tempo.max ?? "") + " BPM" : "",
                            root.editor.songsOf(styleItem.st).length ? qsTr("%n song(s)", "", root.editor.songsOf(styleItem.st).length) : ""].filter(x => x).join(" · ")
                    }
                }
            }
        }
    }
    RowLayout {
        Button { icon.source: Theme.icon("plus"); text: qsTr("Add Style"); onClicked: root.editor.addItem("style") }
        Button {
            text: qsTr("Remove Style")
            enabled: !!styleDetails.st
            onClicked: { const sid = styleDetails.sid; root.edit(d => delete d.style[sid]); root.editor.openStyle = ""; }
        }
    }
    GroupBox {
        id: styleBox
        visible: !!styleDetails.st
        Layout.fillWidth: true
        title: styleDetails.st ? styleDetails.st.name : ""
        ColumnLayout {
            // The selected style (the first, until you choose one).
            id: styleDetails
            readonly property var all: root.c ? root.editor.entries(root.c.style) : []
            readonly property string sid: all.some(e => e.id === root.editor.openStyle) ? root.editor.openStyle : all.length ? all[0].id : ""
            readonly property var st: sid && root.c ? root.c.style[sid] : null
            readonly property var songs: st ? root.editor.songsOf(st) : []
            anchors.fill: parent
            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Theme.spacing * 2
                FormLabel { text: qsTr("Name") }
                TextField {
                    Layout.fillWidth: true
                    text: styleDetails.st ? styleDetails.st.name : ""
                    ToolTip.visible: hovered; ToolTip.text: qsTr("id: %1").arg(styleDetails.sid)
                    onEditingFinished: if (text !== styleDetails.st.name) { const sid = styleDetails.sid; root.edit(d => d.style[sid].name = text); }
                }
                FormLabel { text: qsTr("Short name") }
                TextField {
                    implicitWidth: 120
                    text: styleDetails.st ? styleDetails.st.short || "" : ""
                    onEditingFinished: { const sid = styleDetails.sid; root.edit(d => d.style[sid].short = text || null); }
                }
                FormLabel { text: qsTr("Colour") }
                Button {
                    text: qsTr("Choose…")
                    icon.source: Theme.icon("dot")
                    icon.color: styleDetails.st && styleDetails.st.color ? styleDetails.st.color : Theme.neutral
                    onClicked: { const sid = styleDetails.sid; root.editor.pickColor(styleDetails.st.color, col => root.edit(d => d.style[sid].color = col)); }
                }
                FormLabel { text: qsTr("Meter"); ToolTip.visible: meterHover.hovered; ToolTip.text: qsTr("Beats per bar, e.g. 3 or 2, 4"); HoverHandler { id: meterHover } }
                TextField {
                    implicitWidth: 120
                    placeholderText: qsTr("any")
                    text: styleDetails.st ? (styleDetails.st.meter || []).join(", ") : ""
                    onEditingFinished: { const sid = styleDetails.sid; root.edit(d => d.style[sid].meter = root.editor.meters(text)); }
                }
                FormLabel { text: qsTr("Tempo (BPM)") }
                RangeInput { value: styleDetails.st ? styleDetails.st.tempo : null; onEdited: r => { const sid = styleDetails.sid; root.edit(d => d.style[sid].tempo = r); } }
                FormLabel { text: qsTr("Preferred tempo") }
                RangeInput { value: styleDetails.st ? styleDetails.st.preferred : null; onEdited: r => { const sid = styleDetails.sid; root.edit(d => d.style[sid].preferred = r); } }
                FormLabel { text: qsTr("Songs per set") }
                RangeInput { value: styleDetails.st ? styleDetails.st.count : null; onEdited: r => { const sid = styleDetails.sid; root.edit(d => d.style[sid].count = r); } }
                FormLabel { text: qsTr("Length") }
                RangeInput { kind: "time"; value: styleDetails.st ? styleDetails.st.length : null; onEdited: r => { const sid = styleDetails.sid; root.edit(d => d.style[sid].length = r); } }
                FormLabel { text: qsTr("Share of the set") }
                RangeInput { kind: "pct"; value: styleDetails.st ? styleDetails.st.share : null; onEdited: r => { const sid = styleDetails.sid; root.edit(d => d.style[sid].share = r); } }
                FormLabel { text: qsTr("In a row, at most") }
                TextField {
                    implicitWidth: 60
                    text: styleDetails.st && styleDetails.st.max_in_a_row != null ? String(styleDetails.st.max_in_a_row) : ""
                    onEditingFinished: { const sid = styleDetails.sid; root.edit(d => d.style[sid].max_in_a_row = root.editor.num(text)); }
                }
                FormLabel { text: qsTr("Where in the set") }
                RowLayout {
                    TextField {
                        Layout.fillWidth: true
                        Layout.maximumWidth: 220
                        placeholderText: qsTr("e.g. 0–40, 90–100 (%)")
                        text: styleDetails.st ? root.editor.whenText(styleDetails.st.when) : ""
                        onEditingFinished: { const sid = styleDetails.sid; root.edit(d => d.style[sid].when = root.editor.whenParse(text)); }
                    }
                    CheckBox { text: qsTr("First song"); checked: !!styleDetails.st && !!styleDetails.st.first; onToggled: { const sid = styleDetails.sid; root.edit(d => d.style[sid].first = checked); } }
                    CheckBox { text: qsTr("Last song"); checked: !!styleDetails.st && !!styleDetails.st.last; onToggled: { const sid = styleDetails.sid; root.edit(d => d.style[sid].last = checked); } }
                }
                FormLabel { text: qsTr("Note") }
                TextField {
                    Layout.fillWidth: true
                    text: styleDetails.st ? styleDetails.st.note || "" : ""
                    onEditingFinished: { const sid = styleDetails.sid; root.edit(d => d.style[sid].note = text || null); }
                }
            }
            // The songs the club suggests for it.
            Item {
                id: styleRow
                visible: false
                readonly property string sid: styleDetails.sid
                readonly property var st: styleDetails.st || ({})
                readonly property var songs: styleDetails.songs
            }
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
                            text: songRow.here && songRow.here.have ? qsTr("In your library") : qsTr("Not in your library")
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
                            icon.color: palette.buttonText
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

    // ------------------------------------------------------------ groups
    Heading { text: qsTr("Groups") }
    Label { text: qsTr("Styles counted together (e.g. all waltzes)."); opacity: Theme.dim }
    Repeater {
        model: root.c ? root.editor.entries(root.c.group) : []
        // Each group: its name, its styles (ticked), and how many it wants.
        delegate: GroupBox {
            id: groupRow
            required property var modelData
            readonly property string gid: modelData.id
            readonly property var g: modelData.v
            Layout.fillWidth: true
            title: g.name || qsTr("Group")
            GridLayout {
                anchors.fill: parent
                columns: 2
                columnSpacing: Theme.spacing * 2
                FormLabel { text: qsTr("Name") }
                TextField {
                    Layout.fillWidth: true
                    text: groupRow.g.name
                    onEditingFinished: root.edit(d => d.group[groupRow.gid].name = text)
                }
                FormLabel { text: qsTr("Styles") }
                Flow {
                    Layout.fillWidth: true
                    spacing: Theme.spacing
                    Repeater {
                        model: root.editor.entries(root.c.style)
                        delegate: CheckBox {
                            required property var modelData
                            text: modelData.v.name
                            checked: groupRow.g.styles.includes(modelData.id)
                            onToggled: root.edit(d => d.group[groupRow.gid].styles = checked
                                ? d.group[groupRow.gid].styles.concat([modelData.id])
                                : d.group[groupRow.gid].styles.filter(x => x !== modelData.id))
                        }
                    }
                }
                FormLabel { text: qsTr("Songs per set") }
                RangeInput { value: groupRow.g.count; onEdited: r => root.edit(d => d.group[groupRow.gid].count = r) }
                FormLabel { text: qsTr("Share of the set") }
                RangeInput { kind: "pct"; value: groupRow.g.share; onEdited: r => root.edit(d => d.group[groupRow.gid].share = r) }
                Item { implicitWidth: 1 }
                Button { text: qsTr("Remove Group"); onClicked: root.edit(d => delete d.group[groupRow.gid]) }
            }
        }
    }
    Button { icon.source: Theme.icon("plus"); text: qsTr("Add Group"); enabled: !!root.c && Object.keys(root.c.style).length > 0; onClicked: root.editor.addItem("group") }

    // ------------------------------------------------------------ rules
    Heading { text: qsTr("Rules for Part of the Set") }
    Repeater {
        model: root.c ? root.editor.entries(root.c.rule) : []
        // Each rule: songs of a length, how many per set.
        delegate: GroupBox {
            id: ruleRow
            required property var modelData
            readonly property string rid: modelData.id
            readonly property var r: modelData.v
            Layout.fillWidth: true
            title: r.name || qsTr("Rule")
            GridLayout {
                anchors.fill: parent
                columns: 2
                columnSpacing: Theme.spacing * 2
                FormLabel { text: qsTr("Name") }
                TextField { Layout.fillWidth: true; text: ruleRow.r.name; onEditingFinished: root.edit(d => d.rule[ruleRow.rid].name = text) }
                FormLabel { text: qsTr("Songs per set") }
                RangeInput { value: ruleRow.r.songs; onEdited: v => root.edit(d => d.rule[ruleRow.rid].songs = v) }
                FormLabel { text: qsTr("Each this long") }
                RangeInput { kind: "time"; value: ruleRow.r.length; onEdited: v => root.edit(d => d.rule[ruleRow.rid].length = v) }
                Item { implicitWidth: 1 }
                Button { text: qsTr("Remove Rule"); onClicked: root.edit(d => delete d.rule[ruleRow.rid]) }
            }
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
        delegate: GroupBox {
            id: tagRow
            required property var modelData
            readonly property string tid: modelData.id
            readonly property var t: modelData.v
            Layout.fillWidth: true
            title: t.name || qsTr("Tag")
            GridLayout {
                anchors.fill: parent
                columns: 2
                columnSpacing: Theme.spacing * 2
                FormLabel { text: qsTr("Name") }
                TextField { Layout.fillWidth: true; text: tagRow.t.name; onEditingFinished: root.edit(d => d.tag[tagRow.tid].name = text) }
                FormLabel { text: qsTr("Colour") }
                Button {
                    text: qsTr("Choose…")
                    icon.source: Theme.icon("dot")
                    icon.color: tagRow.t.color || Theme.neutral
                    onClicked: root.editor.pickColor(tagRow.t.color, col => root.edit(d => d.tag[tagRow.tid].color = col))
                }
                FormLabel { text: qsTr("Note") }
                TextField { Layout.fillWidth: true; text: tagRow.t.note || ""; onEditingFinished: root.edit(d => d.tag[tagRow.tid].note = text || null) }
                Item { implicitWidth: 1 }
                CheckBox { text: qsTr("Show on the projector"); checked: !!tagRow.t.announce; onToggled: root.edit(d => d.tag[tagRow.tid].announce = checked) }
                Item { implicitWidth: 1 }
                Button { text: qsTr("Remove Tag"); onClicked: root.edit(d => delete d.tag[tagRow.tid]) }
            }
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
