import QtQuick
import QtQuick.Controls
import Orpheus

// A set entry's context menu.
FitMenu {
    id: root

    property var row
    property var club: null
    /// It's the entry playing now.
    property bool current: false

    readonly property var e: row ? row.entry : null
    readonly property var ids: Session.selectedEntries.length && e && Session.selectedEntries.includes(e.id) ? Session.selectedEntries : (e ? [e.id] : [])
    readonly property var shownKeys: row ? row.showAs.filter(x => x.on).map(x => x.key) : []

    signal remove()
    signal moveNext()

    /// The entries it acts on (the selection, if it's part of it), as the set shows them.
    readonly property var rows: Session.set ? ids.map(id => Session.set.rows.find(r => r.entry.id === id)).filter(r => !!r) : (row ? [row] : [])

    /// Changes each entry it acts on: `patch` is the change, or makes it from a row.
    function update(patch) {
        for (const r of rows) {
            const p = typeof patch === "function" ? patch(r) : patch;
            if (p) Session.run("entry_update", { id: r.entry.id, patch: p });
        }
    }

    /// Shows or stops showing one style (on all of them, as it is on the one
    /// clicked): any of the club's in a club's set, else one of the version's
    /// own (all of them, or none chosen: as it is).
    function toggleStyle(key) {
        const on = shownKeys.includes(key);
        update(r => {
            const shown = r.showAs.filter(x => x.on).map(x => x.key);
            if (club) {
                const next = on ? shown.filter(k => k !== key) : shown.includes(key) ? shown : shown.concat([key]);
                return { clubStyles: next.length ? next : null };
            }
            const all = r.showAs.map(x => x.key);
            if (!all.includes(key)) return null;
            const next = all.filter(k => k === key ? !on : shown.includes(k));
            return { styles: next.length === all.length || !next.length ? null : next };
        });
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
    FitMenu {
        title: qsTr("Load onto")
        enabled: !!root.e && !!root.e.versionId
        MenuItem { text: qsTr("Deck A"); enabled: !(Session.decks[0] && Session.decks[0].audible); onTriggered: Session.run("deck_load", { deck: 0, versionId: root.e.versionId, entryId: root.e.id, play: false }) }
        MenuItem { text: qsTr("Deck B"); enabled: !(Session.decks[1] && Session.decks[1].audible); onTriggered: Session.run("deck_load", { deck: 1, versionId: root.e.versionId, entryId: root.e.id, play: false }) }
    }
    MenuItem { text: Session.moveAfterText; enabled: !root.current && !(Session.insertAfterRow && root.ids.length === 1 && root.ids[0] === Session.insertAfterRow.entry.id); onTriggered: root.moveNext() }
    MenuItem {
        text: qsTr("Fit Tempo to Its Style")
        enabled: root.rows.some(r => !!r.trackId)
        ToolTip.visible: hovered
        ToolTip.text: qsTr("Those playing outside their style's tempo get the nearest that fits; the others stay as they are")
        onTriggered: Session.fitTempos(root.ids)
    }
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
        // (Not the one playing.)
        onTriggered: root.update(r => Session.perform && r.entry.id === Session.perform.currentEntryId ? null : { status: "skipped" })
    }
    MenuItem {
        visible: !!root.e && root.e.status === "played"
        height: visible ? implicitHeight : 0
        text: qsTr("Mark as Not Played")
        onTriggered: root.update({ status: "pending" })
    }
    MenuSeparator {}
    FitMenu {
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
    FitMenu {
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
                    // On all of them, as it is on the one clicked.
                    const on = (root.e.tags || []).includes(modelData.key);
                    root.update(r => {
                        const tags = r.entry.tags || [];
                        return { tags: on ? tags.filter(t => t !== modelData.key) : tags.includes(modelData.key) ? tags : tags.concat([modelData.key]) };
                    });
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
