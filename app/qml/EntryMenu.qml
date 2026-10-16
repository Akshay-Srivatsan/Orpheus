import QtQuick
import QtQuick.Controls
import Orpheus

// A set entry's context menu.
Menu {
    id: root
    popupType: Popup.Native

    property var row
    property var club: null
    /// It's the entry playing now.
    property bool current: false

    readonly property var e: row ? row.entry : null
    readonly property var ids: Session.selectedEntries.length && e && Session.selectedEntries.includes(e.id) ? Session.selectedEntries : (e ? [e.id] : [])
    readonly property var shownKeys: row ? row.showAs.filter(x => x.on).map(x => x.key) : []

    signal remove()
    signal moveNext()

    function update(patch) { return Session.run("entry_update", { id: e.id, patch: patch }); }

    /// Shows or stops showing one style: any of the club's in a club's set,
    /// else one of the version's own (all of them, or none chosen: as it is).
    function toggleStyle(key) {
        const on = shownKeys.includes(key);
        if (club) {
            const next = on ? shownKeys.filter(k => k !== key) : shownKeys.concat([key]);
            update({ clubStyles: next.length ? next : null });
        } else {
            const all = row.showAs.map(x => x.key);
            const next = all.filter(k => k === key ? !on : shownKeys.includes(k));
            update({ styles: next.length === all.length || !next.length ? null : next });
        }
    }

    MenuItem {
        visible: !!root.e && !root.e.versionId && root.e.kind !== "pause"
        height: visible ? implicitHeight : 0
        text: qsTr("Choose Song for This Slot…")
        onTriggered: Session.startFillingSlot(root.e)
    }
    MenuItem {
        text: qsTr("Play Now")
        enabled: !!root.e && !!root.e.versionId && !root.current
        onTriggered: Session.run("perform_play_entry", { entryId: root.e.id })
    }
    MenuItem {
        text: qsTr("Preview in Headphones (free deck)")
        enabled: !!root.e && !!root.e.versionId
        onTriggered: Session.preview(root.e.versionId, root.e.id)
    }
    Menu {
        title: qsTr("Load onto")
        enabled: !!root.e && !!root.e.versionId
        MenuItem { text: qsTr("Deck A"); enabled: !(Session.decks[0] && Session.decks[0].audible); onTriggered: Session.run("deck_load", { deck: 0, versionId: root.e.versionId, entryId: root.e.id, play: false }) }
        MenuItem { text: qsTr("Deck B"); enabled: !(Session.decks[1] && Session.decks[1].audible); onTriggered: Session.run("deck_load", { deck: 1, versionId: root.e.versionId, entryId: root.e.id, play: false }) }
    }
    MenuItem { text: qsTr("Move to Play Next"); enabled: !root.current; onTriggered: root.moveNext() }
    MenuItem {
        text: qsTr("Auto Mode Stops Before This Song")
        checkable: true
        checked: !!root.e && !!root.e.hold
        onTriggered: root.update({ hold: !root.e.hold })
    }
    MenuSeparator {}
    MenuItem {
        visible: !!root.e && root.e.status === "skipped"
        height: visible ? implicitHeight : 0
        text: qsTr("Unskip")
        onTriggered: root.update({ status: "pending" })
    }
    MenuItem {
        visible: !!root.e && root.e.status !== "skipped"
        height: visible ? implicitHeight : 0
        text: qsTr("Skip")
        enabled: !root.current
        onTriggered: root.ids.forEach(id => Session.run("entry_update", { id: id, patch: { status: "skipped" } }))
    }
    MenuItem {
        visible: !!root.e && root.e.status === "played"
        height: visible ? implicitHeight : 0
        text: qsTr("Mark as Not Played")
        onTriggered: root.update({ status: "pending" })
    }
    MenuSeparator {}
    Menu {
        id: showAs
        title: qsTr("Show As")
        enabled: !!root.row && !root.row.isPause
        Instantiator {
            model: root.row ? root.row.showAs : []
            delegate: MenuItem {
                required property var modelData
                text: modelData.fits ? modelData.name : qsTr("%1 (doesn't fit)").arg(modelData.name)
                checkable: true
                checked: modelData.on
                onTriggered: root.toggleStyle(modelData.key)
            }
            onObjectAdded: (i, o) => showAs.insertItem(i, o)
            onObjectRemoved: (i, o) => showAs.removeItem(o)
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Show What It Matches")
            enabled: !!root.e && (!!root.e.clubStyles || !!root.e.styles)
            onTriggered: root.update(root.club ? { clubStyles: null } : { styles: null })
        }
    }
    Menu {
        id: tagMenu
        title: qsTr("Tags")
        enabled: !!root.club && root.club.tags.length > 0
        Instantiator {
            model: root.club ? root.club.tags : []
            delegate: MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: !!root.e && (root.e.tags || []).includes(modelData.key)
                onTriggered: {
                    const tags = root.e.tags || [];
                    root.update({ tags: tags.includes(modelData.key) ? tags.filter(t => t !== modelData.key) : tags.concat([modelData.key]) });
                }
            }
            onObjectAdded: (i, o) => tagMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => tagMenu.removeItem(o)
        }
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Edit Overrides for This Set…")
        onTriggered: { Session.inspectEntry = root.e.id; Session.rightTab = "info"; }
    }
    MenuItem { text: qsTr("Remove from Set"); enabled: !root.current; onTriggered: root.remove() }
}
