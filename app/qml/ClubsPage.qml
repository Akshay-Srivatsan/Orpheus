import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Orpheus

// Clubs' guidelines (TOML files, see examples/clubs/): their versions,
// importing from a file or the web, and an editor (a form, or the file).
// Sets keep the version they were planned with; saving edits to a version
// that sets use makes a new version.
SettingsPage {
    id: page
    title: qsTr("Clubs")
    intro: qsTr("A club's guidelines: its dance styles (by meter, tempo and length) and what its sets should contain. Clubs can publish them as a file on their website (TOML; see the examples); import it from the web address, and check for updates later. Each set keeps the guidelines it was planned with until you move it to a newer version.")

    property string selId: Session.activeClub ? Session.activeClub.id : ""
    property string versionId: ""
    readonly property var sel: Session.clubs.find(c => c.id === selId) || Session.clubs[0] || null
    readonly property var version: sel ? sel.versions.find(v => v.id === versionId) || sel.versions[0] : null
    property var usage: ({})
    property var songStatus: ({})
    property string text: ""
    property var draft: null
    property string error: ""
    property bool dirty: false
    property bool fileView: false
    property bool busy: false
    property var examples: []
    property string openStyle: ""

    readonly property bool savesNewVersion: !!version && (!!usage[version.id] || version.id !== sel.versions[0].id)
    readonly property var unusedOld: sel ? sel.versions.slice(1).filter(v => !usage[v.id]) : []

    Component.onCompleted: { Backend.call("club_examples").then(e => examples = e); load(); }
    onSelChanged: load()
    onVersionChanged: load()

    function load() {
        dirty = false;
        if (!sel || !version) { draft = null; text = ""; return; }
        const c = sel.id, v = version.id;
        Backend.call("club_usage", { clubId: c }).then(u => usage = u);
        Backend.call("club_song_status", { clubId: c, versionId: v }).then(s => songStatus = s, () => songStatus = {});
        Backend.call("club_get", { clubId: c, versionId: v }).then(r => { text = r.text; draft = r.club; error = r.error || ""; });
    }

    function picked(r, what) {
        selId = r.clubId;
        versionId = r.versionId;
        Session.toast(r.unchanged ? qsTr("%1: no changes since the latest version.").arg(what) : qsTr("%1: added.").arg(what), "info");
    }

    /// Changes the form's draft and keeps the file text in step.
    function edit(change) {
        const next = JSON.parse(JSON.stringify(draft));
        change(next);
        draft = next;
        dirty = true;
        Session.run("club_format", { club: next }).then(t => { if (t !== undefined) { text = t; error = ""; } });
    }
    function addItem(kind) {
        Session.run("club_add_item", { club: draft, kind: kind }).then(c => {
            if (!c) return;
            draft = c;
            dirty = true;
            Session.run("club_format", { club: c }).then(t => { if (t !== undefined) text = t; });
        });
    }
    Timer {
        id: parseLater
        interval: 400
        onTriggered: Backend.call("club_parse", { text: page.text }).then(c => { page.draft = c; page.error = ""; }, e => page.error = String(e))
    }
    function save() {
        if (!sel || !version || error) return;
        Session.run("club_save", { clubId: sel.id, versionId: version.id, text: text }).then(r => {
            if (!r) return;
            dirty = false;
            if (r.versionId !== version.id) Session.toast(qsTr("Saved as a new version: sets planned with the old one keep it."), "info");
            versionId = r.versionId;
        });
    }

    function meters(t) { return t.split(/[ ,\/]+/).map(Number).filter(n => n > 0 && Number.isInteger(n)); }
    function whenText(w) { return (w || []).map(r => (r.min ?? "") + "–" + (r.max ?? "")).join(", "); }
    function whenParse(t) {
        return t.split(/[,;]/).map(p => p.trim().replace(/%/g, "")).filter(p => p).map(p => {
            const [a, b] = p.split(/\s*[–-]\s*/);
            const min = a === "" ? null : Number(a);
            const max = b === undefined ? min : b === "" ? null : Number(b);
            return { min: min, max: max };
        });
    }
    function num(t) { return t.trim() === "" ? null : Number(t); }
    function secs(s) { return Math.floor(s / 60) + ":" + String(Math.round(s % 60)).padStart(2, "0"); }
    function entries(obj) { return obj ? Object.keys(obj).map(k => ({ id: k, v: obj[k] })) : []; }
    function songsOf(st) { return (st.song || []).concat(st.library_songs || []); }
    function hasCriteria(st) { return (st.meter && st.meter.length > 0) || !!st.tempo || !!st.preferred || !!st.length; }

    ColorDialog { id: colors; property var apply; onAccepted: if (apply) apply(String(selectedColor)) }
    function pickColor(c, apply) { colors.selectedColor = c || Theme.neutral; colors.apply = apply; colors.open(); }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Theme.spacing * 2

        // The clubs (above the selected one, so its form gets the page's width).
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Club") }
            ComboBox {
                visible: Session.clubs.length > 0
                Layout.preferredWidth: 280
                model: Session.clubs.map(c => c.name + (Session.settings && Session.settings.defaultClub === c.id ? qsTr("  (default)") : ""))
                currentIndex: page.sel ? Session.clubs.findIndex(c => c.id === page.sel.id) : -1
                onActivated: i => { page.selId = Session.clubs[i].id; page.versionId = ""; }
            }
            Label { visible: !Session.clubs.length; text: qsTr("No clubs yet."); opacity: 0.6 }
            Button {
                text: qsTr("+ Add Club")
                onClicked: addMenu.popup()
                Menu {
                    id: addMenu
                    popupType: Popup.Native
                    MenuItem { text: qsTr("New Club…"); onTriggered: newClub.open() }
                    MenuItem {
                        text: qsTr("Import File…")
                        onTriggered: Session.chooseFile({ title: qsTr("Import club guidelines"), nameFilters: [qsTr("Club guidelines (*.toml)")] },
                            path => Session.run("club_import_file", { path: path }).then(r => { if (r) page.picked(r, qsTr("Club imported")); }))
                    }
                    MenuItem { text: qsTr("Import from a Web Address…"); onTriggered: urlDialog.open() }
                    MenuSeparator {}
                    Menu {
                        id: exampleMenu
                        title: qsTr("Examples")
                        Instantiator {
                            model: page.examples
                            delegate: MenuItem {
                                required property var modelData
                                text: modelData[1]
                                onTriggered: Session.run("club_import_example", { id: modelData[0] }).then(r => { if (r) page.picked(r, modelData[1]); })
                            }
                            onObjectAdded: (i, o) => exampleMenu.insertItem(i, o)
                            onObjectRemoved: (i, o) => exampleMenu.removeItem(o)
                        }
                    }
                }
            }
            Item { Layout.fillWidth: true }
        }

        // The selected club.
        ColumnLayout {
            visible: !!page.sel && !!page.version
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: Theme.spacing

            RowLayout {
                Layout.fillWidth: true
                Label { Layout.fillWidth: true; text: page.sel ? page.sel.name : ""; font.bold: true; font.pixelSize: 16 }
                Button {
                    visible: !!page.sel && !!Session.settings && Session.settings.defaultClub !== page.sel.id
                    text: qsTr("Make Default")
                    onClicked: Session.saveSettings({ defaultClub: page.sel.id })
                }
                Button {
                    text: page.busy ? qsTr("Checking…") : qsTr("Check for Updates")
                    enabled: !page.busy
                    onClicked: { page.busy = true; Session.run("club_update", { clubId: page.sel.id }).then(r => { page.busy = false; if (r) page.picked(r, qsTr("Update")); }); }
                }
                Button {
                    text: qsTr("Export…")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Save this version as one self-contained file (its suggested songs and fingerprints written in), to share or put on the club's website")
                    onClicked: Session.chooseFile({ title: qsTr("Export %1").arg(page.sel.name), nameFilters: [qsTr("Club guidelines (*.toml)")], save: true, name: page.sel.id + ".toml" },
                        path => Session.run("club_export", { clubId: page.sel.id, versionId: page.version.id, path: path }))
                }
                Button {
                    visible: page.unusedOld.length > 0
                    text: qsTr("Delete Unused Old Versions")
                    onClicked: Session.confirm(qsTr("Delete %1 older version(s) of %2 that no set uses?").arg(page.unusedOld.length).arg(page.sel.name), qsTr("Delete"), ok => {
                        if (!ok) return;
                        page.unusedOld.forEach(v => Session.run("club_delete_version", { clubId: page.sel.id, versionId: v.id }));
                        page.versionId = "";
                    })
                }
            }

            // Its versions.
            Repeater {
                model: page.sel ? page.sel.versions : []
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    highlighted: !!page.version && page.version.id === modelData.id
                    onClicked: page.versionId = modelData.id
                    contentItem: RowLayout {
                        Label {
                            Layout.fillWidth: true
                            text: (modelData.name || (index === 0 ? qsTr("Latest (%1)").arg(modelData.label) : modelData.label)) + (modelData.error ? qsTr(" (can't be read)") : "")
                            color: modelData.error ? Theme.error : palette.text
                        }
                        Label {
                            Layout.preferredWidth: 160
                            elide: Text.ElideMiddle
                            opacity: 0.6
                            text: modelData.from === "edited" ? qsTr("edited here") : modelData.from
                        }
                        Label { Layout.preferredWidth: 60; text: qsTr("%1 sets").arg(page.usage[modelData.id] || 0); opacity: 0.6 }
                        ToolButton {
                            icon.source: Theme.icon("edit")
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Name this version")
                            onClicked: { versionName.target = modelData; versionName.open(); }
                        }
                        ToolButton {
                            icon.source: Theme.icon("x")
                            ToolTip.visible: hovered; ToolTip.text: qsTr("Delete this version")
                            onClicked: {
                                const n = page.usage[modelData.id] || 0;
                                const last = page.sel.versions.length === 1;
                                const sets = n ? " " + qsTr("%1 set(s) use it and will have no club.").arg(n) : "";
                                Session.confirm((last ? qsTr("Delete %1?").arg(page.sel.name) : qsTr("Delete this version of %1?").arg(page.sel.name)) + sets, qsTr("Delete"), ok => {
                                    if (ok) Session.run("club_delete_version", { clubId: page.sel.id, versionId: modelData.id }).then(() => page.versionId = "");
                                });
                            }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                TabBar {
                    currentIndex: page.fileView ? 1 : 0
                    TabButton { text: qsTr("Form"); width: implicitWidth; onClicked: page.fileView = false }
                    TabButton { text: qsTr("File (TOML)"); width: implicitWidth; onClicked: page.fileView = true }
                }
                Item { Layout.fillWidth: true }
                Label {
                    visible: page.dirty
                    opacity: 0.7
                    text: page.savesNewVersion ? qsTr("Saving makes a new version (sets keep theirs)") : qsTr("Unsaved changes")
                }
                Button { highlighted: true; text: qsTr("Save"); enabled: page.dirty && !page.error; onClicked: page.save() }
            }
            Label { visible: !!page.error; Layout.fillWidth: true; wrapMode: Text.WordWrap; text: page.error; color: Theme.error }

            // The file.
            TextArea {
                visible: page.fileView || !page.draft
                Layout.fillWidth: true
                Layout.preferredHeight: 420
                font.family: "monospace"
                text: page.text
                wrapMode: TextEdit.NoWrap
                onTextChanged: if (text !== page.text) { page.text = text; page.dirty = true; parseLater.restart(); }
            }

            // The form.
            ClubForm {
                visible: !page.fileView && !!page.draft
                Layout.fillWidth: true
                editor: page
            }
        }

        Label {
            visible: !page.sel
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            wrapMode: Text.WordWrap
            opacity: 0.7
            text: qsTr("Add a club: start from an example, import a club's file, or make your own.")
        }
    }

    Dialog {
        id: newClub
        title: qsTr("New Club")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: Overlay.overlay
        TextField { id: clubName; width: 280; placeholderText: qsTr("Club name"); onAccepted: newClub.accept() }
        onOpened: { clubName.text = ""; clubName.forceActiveFocus(); }
        onAccepted: if (clubName.text.trim()) Session.run("club_new", { name: clubName.text.trim() }).then(r => { if (r) { page.selId = r.clubId; page.versionId = r.versionId; } })
    }
    Dialog {
        id: urlDialog
        title: qsTr("Import from a Web Address")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: Overlay.overlay
        TextField { id: url; width: 360; placeholderText: "https://club.example/orpheus.toml"; onAccepted: urlDialog.accept() }
        onOpened: { url.text = ""; url.forceActiveFocus(); }
        onAccepted: {
            if (!url.text.trim()) return;
            page.busy = true;
            Session.run("club_import_url", { url: url.text.trim() }).then(r => { page.busy = false; if (r) page.picked(r, qsTr("Club imported")); });
        }
    }
    Dialog {
        id: versionName
        property var target: null
        title: qsTr("Name this version")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: Overlay.overlay
        TextField { id: vname; width: 280; placeholderText: qsTr("Empty: by its date"); onAccepted: versionName.accept() }
        onOpened: { vname.text = target ? target.name : ""; vname.forceActiveFocus(); }
        onAccepted: Session.run("club_rename_version", { clubId: page.sel.id, versionId: target.id, name: vname.text.trim() })
    }
}
