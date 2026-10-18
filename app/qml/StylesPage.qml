import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Orpheus

// Your dance styles (what songs are tagged with), your tags, and per club
// what its styles are called ("what I call X, the club may call Y"; the same
// name needs no rule).
SettingsPage {
    id: page
    title: qsTr("Dance Styles & Tags")
    intro: qsTr("Your dance styles are what you tag songs with; a song can have several. In a club's set, songs show the club's styles they fit (by meter, tempo and length, or because the club suggests the song). When a song fits several, your styles on it pick which show by default: what the club calls them (At a Club, below), or all of them. That never makes a song a style it doesn't fit; choose any style for a song in the set with right-click → Show As. Without a club, your own styles are shown.")

    property var info: null
    property string clubId: Session.activeClub ? Session.activeClub.id : ""
    Component.onCompleted: load()
    onClubIdChanged: load()
    Connections { target: Session; function onStylesChanged() { page.load(); } }
    function load() { Backend.call("styles_page", { clubId: clubId || null }).then(v => info = v); }

    readonly property var lib: info ? { styles: info.styles, tags: info.tags || [], clubRules: info.clubRules || {}, pronunciations: info.pronunciations || [] } : null

    /// Saves the library with `change` applied to a copy.
    function update(change) {
        const next = JSON.parse(JSON.stringify(lib));
        change(next);
        Session.run("styles_save", { library: next });
    }
    function clubStyleName(id) { const s = info.clubStyles.find(c => c.key === id); return s ? s.name : id; }
    function myName(key) { const s = info.styles.find(m => m.key === key); return s ? s.name : key; }

    ColorDialog {
        id: colors
        property var apply
        onAccepted: if (apply) apply(String(selectedColor))
    }
    function pickColor(current, apply) { colors.selectedColor = current; colors.apply = apply; colors.open(); }

    // ------------------------------------------------------------ your styles
    Label { text: qsTr("Your styles"); font.bold: true; font.pointSize: Theme.titlePointSize }
    Repeater {
        model: page.info ? page.info.styles : []
        delegate: RowLayout {
            required property var modelData
            required property int index
            Layout.fillWidth: true
            ToolButton {
                icon.source: Theme.icon("dot")
                icon.color: modelData.color || Theme.neutral
                ToolTip.visible: hovered; ToolTip.text: qsTr("Colour…")
                onClicked: page.pickColor(modelData.color, c => page.update(l => l.styles[index].color = c))
            }
            TextField {
                Layout.preferredWidth: 180
                text: modelData.name
                onEditingFinished: if (text.trim() && text.trim() !== modelData.name) page.update(l => l.styles[index].name = text.trim())
            }
            TextField {
                Layout.preferredWidth: 80
                text: modelData.short
                placeholderText: modelData.name
                ToolTip.visible: hovered; ToolTip.text: qsTr("Short name for tight spaces (empty = the full name)")
                onEditingFinished: if (text.trim() !== modelData.short) page.update(l => l.styles[index].short = text.trim())
            }
            Label { Layout.preferredWidth: 50; text: page.info.usage[modelData.key] || 0; opacity: 0.6; horizontalAlignment: Text.AlignRight }
            ComboBox {
                readonly property var others: page.info.styles.filter(o => o.key !== modelData.key)
                Layout.preferredWidth: 160
                model: [qsTr("Merge into…")].concat(others.map(o => o.name))
                onActivated: idx => {
                    currentIndex = 0;
                    if (idx === 0) return;
                    const into = others[idx - 1];
                    Session.confirm(qsTr("Merge “%1” into “%2”? Songs tagged %1 become %2.").arg(modelData.name).arg(into.name), qsTr("Merge"),
                        ok => { if (ok) Session.run("styles_rekey", { from: modelData.key, to: into.key }); });
                }
            }
            ToolButton { icon.color: palette.buttonText; icon.source: Theme.icon("up"); ToolTip.visible: hovered; ToolTip.text: qsTr("Move up"); enabled: index > 0; onClicked: page.update(l => l.styles.splice(index - 1, 0, ...l.styles.splice(index, 1))) }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("trash")
                onClicked: {
                    const n = page.info.usage[modelData.key] || 0;
                    Session.confirm(n ? qsTr("Delete “%1”? It is removed from %2 song version(s); the songs stay.").arg(modelData.name).arg(n) : qsTr("Delete “%1”?").arg(modelData.name),
                        qsTr("Delete"), ok => { if (ok) Session.run("styles_rekey", { from: modelData.key, to: null }); });
                }
            }
        }
    }
    Label { visible: !!page.info && !page.info.styles.length; text: qsTr("No styles yet."); opacity: 0.6 }
    Flow {
        visible: !!page.info && page.info.undefinedStyles.length > 0
        Layout.fillWidth: true
        spacing: 4
        Label { text: qsTr("Songs are tagged with styles that aren't in your list:"); color: Theme.warn }
        Repeater {
            model: page.info ? page.info.undefinedStyles : []
            delegate: Button {
                required property var modelData
                text: modelData.name + " (" + modelData.count + ")"
                onClicked: Session.run("style_create", { name: modelData.name, key: modelData.key })
            }
        }
    }
    RowLayout {
        TextField { id: newStyle; Layout.preferredWidth: 220; placeholderText: qsTr("New style name"); onAccepted: addStyle.clicked() }
        Button {
            id: addStyle
            icon.source: Theme.icon("plus"); text: qsTr("Add Style")
            enabled: newStyle.text.trim().length > 0
            onClicked: { Session.run("style_create", { name: newStyle.text.trim() }); newStyle.text = ""; }
        }
        Button {
            enabled: !!page.info && !!page.info.club && page.info.importable.length > 0
            text: page.info && page.info.club ? qsTr("Add %1's %2 other style(s)").arg(page.info.club.label).arg(page.info.importable.length) : ""
            ToolTip.visible: hovered
            ToolTip.text: page.info ? page.info.importable.map(c => c.name).join(", ") : ""
            onClicked: Session.run("styles_import_club", { clubId: page.info.club.id })
        }
    }

    // ------------------------------------------------------------ your tags
    Label { text: qsTr("Your tags"); font.bold: true; font.pointSize: Theme.titlePointSize; Layout.topMargin: Theme.spacing }
    Label { text: qsTr("Labels for songs that aren't dance styles, e.g. “Classic”, “Teaching”, “Requests”. Smart crates can use them."); opacity: 0.7; wrapMode: Text.WordWrap; Layout.fillWidth: true }
    Repeater {
        model: page.info ? page.info.tags || [] : []
        delegate: RowLayout {
            required property var modelData
            required property int index
            ToolButton {
                icon.source: Theme.icon("dot")
                icon.color: modelData.color || Theme.neutral
                ToolTip.visible: hovered; ToolTip.text: qsTr("Colour…")
                onClicked: page.pickColor(modelData.color, c => page.update(l => l.tags[index].color = c))
            }
            TextField {
                Layout.preferredWidth: 180
                text: modelData.name
                onEditingFinished: if (text.trim() && text.trim() !== modelData.name) page.update(l => l.tags[index].name = text.trim())
            }
            Label { Layout.preferredWidth: 50; text: page.info.tagUsage[modelData.key] || 0; opacity: 0.6; horizontalAlignment: Text.AlignRight }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("trash")
                onClicked: {
                    const n = page.info.tagUsage[modelData.key] || 0;
                    Session.confirm(n ? qsTr("Delete the tag “%1”? It is removed from %2 song version(s).").arg(modelData.name).arg(n) : qsTr("Delete the tag “%1”?").arg(modelData.name),
                        qsTr("Delete"), ok => { if (ok) Session.run("tags_rekey", { from: modelData.key, to: null }); });
                }
            }
        }
    }
    Flow {
        visible: !!page.info && page.info.undefinedTags.length > 0
        Layout.fillWidth: true
        spacing: 4
        Label { text: qsTr("Songs carry tags that aren't in your list:"); color: Theme.warn }
        Repeater {
            model: page.info ? page.info.undefinedTags : []
            delegate: Button {
                required property var modelData
                text: modelData.name + " (" + modelData.count + ")"
                onClicked: Session.run("tag_create", { name: modelData.name, key: modelData.key })
            }
        }
    }
    RowLayout {
        TextField { id: newTag; Layout.preferredWidth: 220; placeholderText: qsTr("New tag name"); onAccepted: addTag.clicked() }
        Button {
            id: addTag
            icon.source: Theme.icon("plus"); text: qsTr("Add Tag")
            enabled: newTag.text.trim().length > 0
            onClicked: { Session.run("tag_create", { name: newTag.text.trim() }); newTag.text = ""; }
        }
    }

    // ------------------------------------------------------------ at a club
    RowLayout {
        visible: Session.clubs.length > 0
        Layout.topMargin: Theme.spacing
        Label { text: qsTr("At a club"); font.bold: true; font.pointSize: Theme.titlePointSize }
        ComboBox {
            Layout.preferredWidth: 220
            model: Session.clubs.map(c => c.name)
            currentIndex: page.info && page.info.club ? Session.clubs.findIndex(c => c.id === page.info.club.id) : 0
            onActivated: index => page.clubId = Session.clubs[index].id
        }
    }
    Label {
        visible: !!page.info && !!page.info.club
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        opacity: 0.7
        text: page.info && page.info.club ? qsTr("When a song fits several of %1's styles, the ones your styles on it are called there show. Styles of the same name are paired without a rule.").arg(page.info.club.label) : ""
    }
    Repeater {
        model: page.info ? page.info.rules : []
        delegate: RowLayout {
            required property var modelData
            readonly property bool gone: !page.info.styles.some(m => m.key === modelData.mine) || !page.info.clubStyles.some(c => c.key === modelData.club)
            Label {
                textFormat: Text.StyledText
                text: qsTr("What I call <b>%1</b>, %2 may call <b>%3</b>").arg(page.myName(modelData.mine)).arg(page.info.club.label).arg(page.clubStyleName(modelData.club))
                    + (gone ? " · " + qsTr("not a style any more") : "")
            }
            ToolButton {
                icon.color: palette.buttonText
                icon.source: Theme.icon("trash")
                onClicked: Session.run("club_rule_set", { clubId: page.info.club.id, mine: modelData.mine, theirs: modelData.club, on: false })
            }
        }
    }
    Repeater {
        model: page.info ? page.info.sameNamed.filter(p => !page.info.rules.some(r => r.mine === p.mine && r.club === p.club)) : []
        delegate: Label {
            required property var modelData
            textFormat: Text.StyledText
            opacity: 0.6
            text: qsTr("What I call <b>%1</b>, %2 calls <b>%3</b> · same name").arg(page.myName(modelData.mine)).arg(page.info.club.label).arg(page.clubStyleName(modelData.club))
        }
    }
    Label {
        visible: !!page.info && !!page.info.club && !page.info.rules.length && !page.info.sameNamed.length
        opacity: 0.6
        text: page.info && page.info.club ? qsTr("No rules yet: songs that fit several of %1's styles show all of them.").arg(page.info.club.label) : ""
    }
    RowLayout {
        visible: !!page.info && !!page.info.club && page.info.clubStyles.length > 0
        Label { text: qsTr("What I call") }
        ComboBox { id: ruleMine; Layout.preferredWidth: 160; model: page.info ? page.info.styles.map(m => m.name) : [] }
        Label { text: page.info && page.info.club ? qsTr("%1 may call").arg(page.info.club.label) : "" }
        ComboBox { id: ruleClub; Layout.preferredWidth: 160; model: page.info ? page.info.clubStyles.map(c => c.name) : [] }
        Button {
            icon.source: Theme.icon("plus"); text: qsTr("Add Rule")
            enabled: ruleMine.currentIndex >= 0 && ruleClub.currentIndex >= 0
            onClicked: Session.run("club_rule_set", { clubId: page.info.club.id, mine: page.info.styles[ruleMine.currentIndex].key, theirs: page.info.clubStyles[ruleClub.currentIndex].key, on: true })
        }
    }
    Label {
        visible: !!page.info && !!page.info.club && !page.info.clubStyles.length
        opacity: 0.6
        text: page.info && page.info.club ? qsTr("%1 has no styles yet.").arg(page.info.club.label) : ""
    }
}
