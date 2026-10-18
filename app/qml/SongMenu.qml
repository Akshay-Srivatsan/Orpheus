import QtQuick
import QtQuick.Controls
import Orpheus

// The library's context menu for the selected songs (`rows`).
FitMenu {
    id: root

    property var rows: []
    /// The clicked song's values the table has no room for: { name, value }.
    property var details: []
    /// The row right-clicked (history rows can be removed from history).
    property var historyRow: null

    readonly property var ids: rows.map(r => r.versionId)
    readonly property var songs: [...new Set(rows.map(r => r.trackId))]
    readonly property var recordings: [...new Set(rows.map(r => r.recordingId))]
    readonly property var crate: Session.view.kind === "crate" ? Session.crates.find(c => c.id === Session.view.id) : null
    readonly property bool plainCrate: !!crate && !crate.smart
    readonly property var editFor: Session.pickingEdit

    signal combine(var songs, var recordings)

    function allHave(key) { return rows.length > 0 && rows.every(r => r.styles.some(s => s.key === key)); }

    // What the table had no room to show.
    Instantiator {
        model: root.details
        delegate: MenuItem {
            required property var modelData
            enabled: false
            text: modelData.name + ": " + modelData.value
        }
        onObjectAdded: (i, o) => root.insertItem(i, o)
        onObjectRemoved: (i, o) => root.removeItem(o)
    }
    MenuSeparator { visible: root.details.length > 0 }
    MenuItem {
        visible: !!root.historyRow && !!root.historyRow.historyEntryId
        height: visible ? implicitHeight : 0
        text: qsTr("Remove from History")
        onTriggered: Session.run("history_remove", { entryIds: root.rows.filter(r => r.historyEntryId).map(r => r.historyEntryId) })
    }
    MenuItem {
        visible: !!Session.fillingSlot
        height: visible ? implicitHeight : 0
        text: qsTr("Use for the Slot")
        onTriggered: Session.fillSlot(root.ids[0])
    }
    MenuItem {
        visible: !!Session.chimePick
        height: visible ? implicitHeight : 0
        text: qsTr("Use as the Chime")
        onTriggered: Session.chimePicked(root.ids[0])
    }
    MenuItem {
        visible: !!root.editFor
        height: visible ? implicitHeight : 0
        text: qsTr("Add Its Audio to “%1”").arg(Session.pickingEditTitle)
        onTriggered: Session.addEdit({ recording: root.rows[0].recordingId })
    }
    MenuItem { text: Session.addAfterText; onTriggered: Session.addToSet(root.ids, "next") }
    MenuItem {
        readonly property bool unlisted: root.rows.length > 0 && root.rows.every(r => r.unlisted)
        text: unlisted ? qsTr("Show in the Library") : qsTr("Hide from the Library")
        onTriggered: Session.setUnlisted(root.ids, !unlisted)
    }
    MenuItem { text: qsTr("Add to End of Set"); onTriggered: Session.addToSet(root.ids, "end") }
    MenuItem { text: qsTr("Preview in Headphones (free deck)"); onTriggered: Session.preview(root.ids[0], null) }
    FitMenu {
        title: qsTr("Load onto")
        MenuItem { text: qsTr("Deck A"); enabled: !(Session.decks[0] && Session.decks[0].audible); onTriggered: Session.loadDeck(0, root.ids[0], null) }
        MenuItem { text: qsTr("Deck B"); enabled: !(Session.decks[1] && Session.decks[1].audible); onTriggered: Session.loadDeck(1, root.ids[0], null) }
    }
    MenuSeparator {}
    FitMenu {
        id: crateMenu
        title: qsTr("Add to Crate")
        Instantiator {
            model: Session.crates.filter(c => !c.smart)
            delegate: MenuItem {
                required property var modelData
                text: modelData.name
                onTriggered: Session.run("crate_add", { id: modelData.id, versionIds: root.ids })
            }
            onObjectAdded: (i, o) => crateMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => crateMenu.removeItem(o)
        }
        MenuSeparator {}
        MenuItem { text: qsTr("New Crate…"); onTriggered: Session.newCrateWith(root.ids) }
    }
    MenuItem {
        visible: root.plainCrate
        height: visible ? implicitHeight : 0
        text: qsTr("Remove from Crate")
        onTriggered: Session.run("crate_remove", { id: root.crate.id, versionIds: root.ids })
    }
    FitMenu {
        id: styleMenu
        title: qsTr("Dance Styles")
        Instantiator {
            model: Session.myStyles
            delegate: MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: root.allHave(modelData.key)
                onTriggered: root.allHave(modelData.key)
                    ? Session.run("style_remove", { versionIds: root.ids, style: modelData.key })
                    : Session.run("style_add", { style: modelData.key, versionIds: root.ids })
            }
            onObjectAdded: (i, o) => styleMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => styleMenu.removeItem(o)
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Remove All")
            enabled: root.rows.some(r => r.styles.length)
            onTriggered: Session.run("style_remove", { versionIds: root.ids, style: null })
        }
        MenuItem {
            text: Session.myStyles.length ? qsTr("More in Info…") : qsTr("Create Dance Styles…")
            onTriggered: {
                if (!Session.myStyles.length) { Session.settingsPage = "styles"; return; }
                Session.inspectTrack = root.rows[0].trackId;
                Session.rightTab = "info";
            }
        }
    }
    FitMenu {
        id: tagMenu
        title: qsTr("Tags")
        enabled: Session.myTags.length > 0
        Instantiator {
            model: Session.myTags
            delegate: MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                // Ticked when every chosen song has it; choosing it then removes it.
                checked: root.rows.length > 0 && root.rows.every(r => r.tags.includes(modelData.key))
                onTriggered: Session.run("tag_set", { tag: modelData.key, versionIds: root.ids, on: !root.rows.every(r => r.tags.includes(modelData.key)) })
            }
            onObjectAdded: (i, o) => tagMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => tagMenu.removeItem(o)
        }
    }
    MenuSeparator {}
    MenuItem {
        visible: root.songs.length > 1 || root.recordings.length > 1
        height: visible ? implicitHeight : 0
        text: qsTr("Combine…")
        onTriggered: root.combine(root.songs, root.recordings)
    }
    FitMenu {
        title: qsTr("Versions")
        enabled: root.songs.length === 1
        MenuItem {
            text: qsTr("New Version…")
            onTriggered: { Session.inspectTrack = root.rows[0].trackId; Session.rightTab = "info"; }
        }
        MenuItem {
            text: qsTr("Make Primary Version")
            enabled: root.rows.length === 1 && root.rows[0].versionCount > 1 && !root.rows[0].isPrimary
            onTriggered: Session.run("track_update", { id: root.rows[0].trackId, patch: { primary: root.rows[0].versionId } })
        }
        MenuItem {
            text: root.rows.length > 1 ? qsTr("Separate into Their Own Song") : qsTr("Separate into Its Own Song")
            enabled: root.songs.length === 1 && root.rows.length > 0 && root.rows[0].versionCount > root.rows.length
            onTriggered: Session.undoable(root.rows.length > 1 ? qsTr("Separated into their own song") : qsTr("Separated into its own song"),
                () => Session.run("versions_separate", { versionIds: root.ids }))
        }
    }
    FitMenu {
        title: qsTr("Audio")
        MenuItem { text: qsTr("Re-analyze"); onTriggered: Session.run("recordings_analyze", { ids: root.recordings }) }
        MenuItem {
            text: qsTr("Find on Spotify")
            enabled: !!Session.spotify && Session.spotify.connected && root.rows.some(r => r.availability === "file" || r.availability === "missing")
            onTriggered: {
                const recs = [...new Set(root.rows.filter(r => r.availability === "file" || r.availability === "missing").map(r => r.recordingId))];
                Session.run("recordings_find_spotify", { ids: recs }).then(r => {
                    if (r !== undefined) Session.toast(recs.length === 1 ? qsTr("Looking for it on Spotify…") : qsTr("Looking for %1 songs on Spotify…").arg(recs.length), "info");
                });
            }
        }
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Remove from Library…")
        onTriggered: Session.confirm(qsTr("Remove %1 song(s) from the library? Sets that play them keep them (until you remove them there too); your music files aren't touched.").arg(root.songs.length),
            qsTr("Remove"), ok => { if (ok) Session.run("tracks_delete", { ids: root.songs }); })
    }
}
