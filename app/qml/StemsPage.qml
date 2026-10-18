import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Orpheus

SettingsPage {
    id: page
    title: qsTr("Stems")
    intro: qsTr("Orpheus can split songs into vocals, drums, bass and other (HT-Demucs), so you can mute parts of a song, e.g. drop the vocals for a teaching track. Separation takes a minute or two per song and runs in the background; prepared stems are stored on this computer and work offline. On a controller, pads in Sampler mode toggle stems. Only local files can be separated (not Spotify songs).")

    readonly property var st: Session.stems
    readonly property var jobs: st ? Object.keys(st.jobs).map(k => ({ id: k, job: st.jobs[k] })) : []
    property var ready: []
    Component.onCompleted: load()
    function load() { Backend.call("stems_ready").then(r => ready = r); }
    Connections { target: Session; function onStemsChanged() { page.load(); } }

    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: Theme.spacing * 2
        rowSpacing: Theme.spacing

        FormLabel { text: qsTr("Separation model"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            RowLayout {
                visible: !!page.st && page.st.model.state === "ready"
                Label { text: qsTr("Ready") }
                Button {
                    text: qsTr("Remove Model…")
                    enabled: !page.jobs.some(j => j.job.state === "running")
                    onClicked: Session.confirm(qsTr("Delete the stem separation model (210 MB)? Prepared stems keep working; preparing new ones downloads it again."), qsTr("Delete"),
                        ok => { if (ok) Session.run("stems_remove_model"); })
                }
            }
            Label {
                visible: !!page.st && page.st.model.state === "downloading"
                text: page.st && page.st.model.state === "downloading" ? qsTr("Downloading… %1%").arg(page.st.model.total ? Math.round(page.st.model.done / page.st.model.total * 100) : 0) : ""
            }
            Button {
                enabled: !!page.st && (page.st.model.state === "missing" || page.st.model.state === "error")
                highlighted: true
                text: qsTr("Download Model (about 210 MB)")
                onClicked: Session.run("stems_download_model")
            }
            Label { visible: !!page.st && page.st.model.state === "error"; text: page.st && page.st.model.message ? page.st.model.message : ""; color: Theme.error }
        }

        FormLabel { text: qsTr("Prepare"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Button {
                text: qsTr("Prepare Stems for the Current Set")
                enabled: !!page.st && page.st.model.state === "ready" && !!Session.set
                onClicked: {
                    const ids = [...new Set(Session.set.rows.filter(r => r.recordingId).map(r => r.recordingId))].filter(id => !page.ready.includes(id));
                    Session.run("stems_prepare", { recordingIds: ids }).then(n => {
                        if (n !== undefined) Session.toast(qsTr("Preparing stems for %1 song(s) in “%2”").arg(n).arg(Session.set.setlist.name), "info");
                    });
                }
            }
            Label { visible: page.jobs.length > 0; text: qsTr("%1 song(s) in progress").arg(page.jobs.filter(j => j.job.state !== "failed").length) }
            Repeater {
                model: page.jobs.filter(j => j.job.state === "failed")
                delegate: Label { required property var modelData; text: modelData.id + ": " + (modelData.job.message || ""); color: Theme.error }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                font.pointSize: Theme.smallPointSize
                opacity: 0.7
                text: qsTr("Separation uses a lot of CPU; Orpheus runs it at low priority, but preparing ahead of an event is best.")
            }
        }

        FormLabel { text: qsTr("Storage"); Layout.alignment: Qt.AlignTop }
        ColumnLayout {
            Label { text: qsTr("%1 GB for %2 song(s) (about 85 MB per 3-minute song)").arg(((page.st ? page.st.cacheBytes : 0) / 1e9).toFixed(2)).arg(page.ready.length) }
            TextField { Layout.fillWidth: true; readOnly: true; text: page.st ? page.st.cacheDir : "" }
            Button {
                text: qsTr("Delete All Stems…")
                enabled: page.ready.length > 0
                onClicked: Session.confirm(qsTr("Delete the prepared stems of all %1 song(s)?").arg(page.ready.length), qsTr("Delete"),
                    ok => { if (ok) Session.run("stems_delete_all").then(() => page.load()); })
            }
        }
    }
}
