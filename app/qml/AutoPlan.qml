import QtQuick
import QtQuick.Controls
import Orpheus

// What happens next, worked out for the panels that show it (the crossfader
// panel, the simple interface's player bar): the up-next entry, the pause,
// auto mode's steps to the next song with their times, and what will go
// wrong there. It ticks while there's a countdown.
Item {
    id: root
    visible: false

    readonly property var p: Session.perform
    /// Something is audible in the room.
    readonly property bool playing: Session.decks.some(d => d && d.audible)
    readonly property var next: Session.set && p ? Session.set.rows.find(r => r.entry.id === p.nextEntryId) || null : null
    /// The next entry's custom label (not just its styles' short names).
    readonly property string nextLabel: next && !next.isPause && next.label && !next.styles.some(s => s.short === next.label) ? next.label : ""
    readonly property var nextDeck: p ? Session.decks.find(d => d && d.entryId && d.entryId === p.nextEntryId) || null : null
    readonly property bool inGap: !!p && !!p.gapStartedAt && !playing
    property real now: Date.now()
    readonly property real gapMs: inGap ? now - p.gapStartedAt : 0
    /// Expected pause: the general pause, or with announcements both pauses and the speech.
    readonly property real gapTarget: p && p.autoAdvanceAt && p.gapStartedAt ? Math.max(1000, p.autoAdvanceAt - p.gapStartedAt) : (p ? p.gapSeconds : 8) * 1000

    /// The deck the room hears (its song's time left starts the steps while it plays).
    readonly property var liveDeck: p ? Session.decks[p.liveDeck] : null
    /// Auto mode's way to the next song, in order: { kind, from, to, estimated }
    /// (epoch ms), the song playing first (see the engine's `AutoStep`).
    readonly property var steps: {
        if (!p || !p.autoSteps || !p.autoSteps.length) return [];
        // In the pause the song stays listed, done (the list keeps its length).
        if (!p.stepsAfterSong) return [{ kind: "song", from: p.gapStartedAt, to: p.gapStartedAt, estimated: false }].concat(p.autoSteps);
        const end = now + (liveDeck && liveDeck.audible ? liveDeck.remainingMs : 0);
        return [{ kind: "song", from: now, to: end, estimated: false }]
            .concat(p.autoSteps.map(s => Object.assign({}, s, { from: s.from + end, to: s.to + end })));
    }
    readonly property real nextAt: steps.length ? steps[steps.length - 1].to : 0
    readonly property bool estimated: steps.some(s => s.estimated)
    /// What will go wrong at the next entry: { text, stops } (stops: auto mode can't go on there).
    readonly property var problems: p && p.nextProblems ? p.nextProblems : []
    readonly property bool stops: problems.some(x => x.stops)
    function stepName(kind) {
        return { song: qsTr("The song"), pause: qsTr("Pause"), chime: qsTr("Chime"), announcement: qsTr("Announcement"), pauseAfter: qsTr("Pause") }[kind] || kind;
    }
    /// Text a bit lighter than the rest (steps done), for styled text.
    readonly property color dimText: Qt.tint(Theme.system.text, Qt.rgba(Theme.system.window.r, Theme.system.window.g, Theme.system.window.b, 1 - Theme.dim))
    readonly property string stepsText: steps.map(st => {
        const t = stepName(st.kind) + " " + stepTime(st);
        return st.to <= now ? "<font color='" + dimText + "'><s>" + t + "</s></font>" : st.from <= now ? "<b>" + t + "</b>" : t;
    }).concat(["<b><font color='" + (stops ? Theme.warn : Theme.ok) + "'>"
        + (stops ? qsTr("Auto mode stops") : qsTr("Next song")) + " " + (estimated ? qsTr("in about %1") : qsTr("in %1")).arg(secs(nextAt - now))
        + "</font></b>"]).join(" → ")
    function secs(ms) { return ms < 60000 ? qsTr("%1 s").arg(Math.max(0, Math.ceil(ms / 1000))) : Session.fmtTime(ms); }
    /// A step's time: done, left (the current one), or how long (estimated: about).
    function stepTime(st) {
        if (st.to <= now) return "✓";
        if (st.from <= now) return qsTr("%1 left").arg(secs(st.to - now));
        return (st.estimated ? qsTr("about %1") : "%1").arg(secs(st.to - st.from));
    }

    Timer { interval: 200; running: root.inGap || root.steps.length > 0; repeat: true; onTriggered: root.now = Date.now() }

    function playNext() {
        Session.run("perform_play_next").then(r => {
            if (r === "armed") Session.toast(qsTr("A song is playing — press Play Next again within 3 s to fade it out and continue."), "info");
        });
    }
}
